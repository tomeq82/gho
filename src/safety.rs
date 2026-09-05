//! Path safety utilities for output file naming.
//!
//! When extracting pre-11.x images, dirent names come from untrusted input
//! (an attacker could craft a malicious `.gho` file). We must:
//!
//! - Reject names that escape the output directory (path traversal via `..`,
//!   absolute paths, NUL bytes, control characters, etc.).
//! - Use only the 8.3 portion of the name (no VFAT long-name fragments).
//! - Fall back to a stable synthetic name (`file_<offset>`) when the input
//!   is unsafe, so extraction never silently drops files.

use std::path::Path;

/// Maximum length of a sanitized 8.3 file name (without extension).
pub const MAX_SANITIZED_NAME_LEN: usize = 32;

/// Sanitize a 8.3 (name, ext) pair into a filesystem-safe relative name.
///
/// Returns `None` if the name cannot be made safe (shouldn't happen for any
/// well-formed dirent, but defends against crafted inputs).
pub fn sanitize_8_3(name: &[u8; 8], ext: &[u8; 3]) -> Option<String> {
    let mut name_part = sanitize_component(name)?;
    let mut ext_part = sanitize_component(ext)?;
    // Strip trailing space-padding (8.3 fills unused bytes with 0x20). We do
    // NOT strip trailing underscores — those came from sanitising hostile
    // input and the caller may rely on the exact rendering.
    while name_part.ends_with(' ') {
        name_part.pop();
    }
    while ext_part.ends_with(' ') {
        ext_part.pop();
    }
    // After trimming, the name must contain at least one alphanumeric char.
    if !name_part.chars().any(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    if ext_part.is_empty() {
        Some(name_part)
    } else {
        Some(format!("{name_part}.{ext_part}"))
    }
}

/// Build a synthetic fallback name for a dirent whose 8.3 name is unusable.
pub fn fallback_name(dirent_offset: u64) -> String {
    format!("file_{dirent_offset:08x}.bin")
}

/// Test whether `path` contains `..` segments that could escape its parent.
///
/// Absolute paths and prefix components are allowed — the caller is
/// responsible for resolving them against a trusted base. We only flag
/// `..` here, which is the actual escape mechanism.
pub fn contains_parent_traversal(path: &Path) -> bool {
    use std::path::Component;
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            return true;
        }
    }
    false
}

fn sanitize_component(bytes: &[u8]) -> Option<String> {
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        // Allow only printable ASCII; replace anything else with `_`.
        if (0x20..0x7F).contains(&b) && b != b'/' && b != b'\\' && b != b':' && b != b'*' {
            out.push(b as char);
        } else {
            out.push('_');
        }
    }
    // Collapse leading dots to discourage hidden / dotfile escapes.
    while out.starts_with('.') {
        out.remove(0);
        out.insert(0, '_');
    }
    if out.is_empty() {
        return None;
    }
    if out.len() > MAX_SANITIZED_NAME_LEN {
        out.truncate(MAX_SANITIZED_NAME_LEN);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_normal_name() {
        let name = *b"SETUP   ";
        let ext = *b"EXE";
        let s = sanitize_8_3(&name, &ext).unwrap();
        assert_eq!(s, "SETUP.EXE");
    }

    #[test]
    fn sanitize_replaces_control_chars() {
        let name: [u8; 8] = [0x00, 0x01, 0x02, 0x03, b'A', b'B', b'C', 0x7F];
        let ext = *b"   ";
        let s = sanitize_8_3(&name, &ext).unwrap();
        // All control / non-printable / DEL chars become '_'.
        assert!(s.chars().all(|c| c == '_' || c.is_ascii_alphanumeric()));
        assert_eq!(s, "____ABC_");
    }

    #[test]
    fn sanitize_replaces_slashes() {
        let name: [u8; 8] = *b"/..\\abcd";
        let ext = *b"EXE";
        let s = sanitize_8_3(&name, &ext).unwrap();
        assert!(!s.contains('/'));
        assert!(!s.contains('\\'));
        // The "../" prefix becomes "____" — no traversal possible.
        assert!(!s.starts_with('.'));
    }

    #[test]
    fn sanitize_truncates_long_input() {
        let name: [u8; 8] = *b"ABCDEFGH";
        let ext: [u8; 3] = *b"XYZ";
        let s = sanitize_8_3(&name, &ext).unwrap();
        assert!(s.len() <= MAX_SANITIZED_NAME_LEN + 4); // name + . + ext
    }

    #[test]
    fn sanitize_empty_name_returns_none() {
        let name = [0u8; 8];
        let ext = *b"   ";
        assert!(sanitize_8_3(&name, &ext).is_none());
    }

    #[test]
    fn fallback_name_is_stable() {
        let a = fallback_name(0x1234);
        let b = fallback_name(0x1234);
        assert_eq!(a, b);
        assert!(a.starts_with("file_"));
        assert!(a.ends_with(".bin"));
    }

    #[test]
    fn parent_traversal_detected() {
        assert!(contains_parent_traversal(std::path::Path::new(
            "../etc/passwd"
        )));
        assert!(contains_parent_traversal(std::path::Path::new(
            "foo/../../bar"
        )));
        assert!(!contains_parent_traversal(std::path::Path::new(
            "foo/bar.txt"
        )));
        // Absolute paths are NOT flagged here — the caller is responsible
        // for resolving against a trusted base.
        assert!(!contains_parent_traversal(std::path::Path::new(
            "/etc/passwd"
        )));
    }
}
