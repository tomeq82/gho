//! Multi-file logical stream reader for spanned Norton Ghost images.
//!
//! Each physical `.gho`/`.ghs` file starts with a 512-byte file header
//! (`FEEF` magic). When concatenating the physical files into a single
//! logical stream, **the first file's header is kept** (parsers expect it at
//! offset 0) and the **continuation-span headers (file_type == 9) are
//! stripped** at the file boundaries.
//!
//! This matches the algorithm used by the Python
//! `history-recovery/ghost-old-format-2001-survey-full.py::build_logical`.

use crate::error::Result;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::PathBuf;

/// Concatenate the given span files into a single output file.
///
/// The first span's 512-byte file header is preserved at offset 0 of the
/// output. Continuation-span headers (file_type == 9) are stripped from
/// their respective file starts before concatenation.
pub fn concatenate_spans<I, P>(spans: I, out_path: &std::path::Path) -> Result<PathBuf>
where
    I: IntoIterator<Item = P>,
    P: AsRef<std::path::Path>,
{
    use std::io::Write;
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = std::fs::File::create(out_path)?;
    let mut buf = [0u8; 1024 * 1024];
    let mut first = true;
    for span in spans {
        let mut f = BufReader::new(File::open(span.as_ref())?);
        if first {
            // Keep the first file's header intact.
            first = false;
        } else {
            // Skip the 512-byte header of continuation spans.
            f.seek(SeekFrom::Start(512))?;
        }
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
        }
    }
    out.flush()?;
    Ok(out_path.to_path_buf())
}

/// Read the first 512 bytes of a single span file and report whether it
/// looks like a valid Norton Ghost file header (`FEEF` magic + sensible
/// `file_type`).
pub fn looks_like_header(data: &[u8]) -> bool {
    if data.len() < 512 {
        return false;
    }
    let magic = u16::from_le_bytes([data[0], data[1]]);
    magic == 0xEFFE && (data[2] == 1 || data[2] == 9)
}

/// Read the first 512 bytes of a single file.
pub fn read_file_header(path: &std::path::Path) -> Result<[u8; 512]> {
    let mut f = File::open(path)?;
    let mut buf = [0u8; 512];
    use std::io::Read;
    f.read_exact(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_detection_accepts_magic_and_file_type() {
        let mut hdr = [0u8; 512];
        hdr[0] = 0xFE;
        hdr[1] = 0xEF;
        hdr[2] = 1;
        assert!(looks_like_header(&hdr));
        hdr[2] = 9;
        assert!(looks_like_header(&hdr));
        hdr[2] = 5;
        assert!(!looks_like_header(&hdr));
    }

    #[test]
    fn concat_preserves_first_header_strips_continuation_headers() {
        let tmp = tempfile::tempdir().unwrap();
        // File 1: starts with a valid header (file_type=1).
        let f1 = tmp.path().join("a.gho");
        let mut c1 = vec![0u8; 1024];
        c1[0] = 0xFE;
        c1[1] = 0xEF;
        c1[2] = 1;
        for byte in c1.iter_mut().take(1024).skip(512) {
            *byte = 0xAA;
        }
        std::fs::write(&f1, &c1).unwrap();
        // File 2: starts with a valid header (file_type=9, continuation).
        let f2 = tmp.path().join("b.ghs");
        let mut c2 = vec![0u8; 1024];
        c2[0] = 0xFE;
        c2[1] = 0xEF;
        c2[2] = 9;
        for byte in c2.iter_mut().take(1024).skip(512) {
            *byte = 0xBB;
        }
        std::fs::write(&f2, &c2).unwrap();
        let out = tmp.path().join("combined.gho");
        let _ = concatenate_spans([&f1, &f2], &out).unwrap();
        let bytes = std::fs::read(&out).unwrap();
        // Expected: file1 entirely (header + body) + file2 body (no header)
        // = 1024 + 512 = 1536 bytes.
        assert_eq!(bytes.len(), 1536);
        // First 512 bytes are file1's header.
        assert_eq!(&bytes[0..3], &[0xFE, 0xEF, 1]);
        // Next 512 bytes are file1's body (0xAA).
        assert!(bytes[512..1024].iter().all(|&b| b == 0xAA));
        // Last 512 bytes are file2's body (0xBB) — no second header.
        assert!(bytes[1024..1536].iter().all(|&b| b == 0xBB));
    }
}
