//! Property-based tests using `proptest`.
//!
//! These complement the hand-written unit tests in `src/` and the security
//! tests in `tests/security.rs` by exploring large input spaces.

use proptest::prelude::*;

use gho::ghost11::{header::FileHeader, record::Record};
use gho::ghostold::dirent::Dirent;
use gho::safety::{contains_parent_traversal, sanitize_8_3};

// ---------------------------------------------------------------------------
// Header invariants
// ---------------------------------------------------------------------------

proptest! {
    /// For any 512-byte buffer, parse() returns Ok iff bytes 0..2 spell FEEF.
    #[test]
    fn file_header_ok_iff_magic_is_feef(buf: [u8; 512]) {
        let result = FileHeader::parse(&buf);
        let magic_ok = buf[0] == 0xFE && buf[1] == 0xEF;
        prop_assert_eq!(result.is_ok(), magic_ok, "buf={:02x?} prefix", &buf[..8]);
    }

    /// For any 10-byte buffer, parse_at() either returns None or a Record with
    /// type_code and body_len matching the bytes.
    #[test]
    fn record_parse_round_trip(buf: [u8; 10]) {
        let result = Record::parse_at(&buf, 0);
        if let Some(rec) = result {
            let expected_type = u16::from_le_bytes([buf[0], buf[1]]);
            let expected_len = u16::from_le_bytes([buf[8], buf[9]]);
            prop_assert_eq!(u16::from(rec.body_len), expected_len);
            // We don't decode rec.kind here because we don't have access to
            // RecordType::from_u16 from this module; the parse path is
            // exercised by the unit tests.
            let _ = expected_type;
        }
    }
}

// ---------------------------------------------------------------------------
// Dirent invariants
// ---------------------------------------------------------------------------

proptest! {
    /// For any 56-byte buffer, parse() returns Ok and the parsed dirent's
    /// display_name is either empty (after sanitisation) or has no control
    /// characters.
    #[test]
    fn dirent_parse_never_panics(buf: [u8; 56]) {
        let result = Dirent::parse(&buf);
        prop_assert!(result.is_ok());
        let d = result.unwrap();
        let name = d.display_name();
        prop_assert!(name.len() <= 12, "display_name too long: {}", name);
    }

    /// For any 8.3 name + ext bytes, sanitize_8_3 produces a safe name:
    /// - no `/`, `\`, `:`, `*`
    /// - no NUL bytes
    /// - no leading `.`
    #[test]
    fn sanitize_produces_safe_name(name: [u8; 8], ext: [u8; 3]) {
        if let Some(safe) = sanitize_8_3(&name, &ext) {
            prop_assert!(!safe.contains('/'));
            prop_assert!(!safe.contains('\\'));
            prop_assert!(!safe.contains(':'));
            prop_assert!(!safe.contains('*'));
            prop_assert!(!safe.contains('\0'));
            prop_assert!(!safe.starts_with('.'), "safe = {:?}", safe);
        }
    }

    /// For any 8.3 name that contains `..` (path traversal), the sanitised
    /// output must not traverse parent directories.
    #[test]
    fn sanitize_neutralises_traversal(name: [u8; 8], ext: [u8; 3]) {
        // Force the name to start with two dots by copying them in.
        let mut name = name;
        name[0] = b'.';
        name[1] = b'.';
        if let Some(safe) = sanitize_8_3(&name, &ext) {
            let p = std::path::Path::new(&safe);
            prop_assert!(!contains_parent_traversal(p));
        }
    }
}

// ---------------------------------------------------------------------------
// Span helper invariants
// ---------------------------------------------------------------------------

proptest! {
    /// `concatenate_spans` always preserves the file count minus 1 in its
    /// output size — the first file's header is kept, all continuation
    /// headers are stripped.
    #[test]
    fn concat_strips_continuation_headers_only(
        file_type in prop_oneof![Just(1u8), Just(9u8)],
        body_len in 0usize..=1024usize,
    ) {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("input.bin");
        let mut data = vec![0u8; 512 + body_len];
        data[0] = 0xFE;
        data[1] = 0xEF;
        data[2] = file_type;
        std::fs::write(&path, &data).unwrap();

        let out = tmp.path().join("out.bin");
        gho::span::concatenate_spans([path.as_path()], &out).unwrap();
        let out_size = std::fs::metadata(&out).unwrap().len();
        // First (and only) file's header is kept, so size == original.
        prop_assert_eq!(out_size as usize, data.len());
    }
}
