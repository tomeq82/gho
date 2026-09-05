//! Multi-file logical stream reader for spanned Norton Ghost images.
//!
//! Each physical `.gho`/`.ghs` file starts with a 512-byte file header
//! (`FEEF` magic). `SpanReader` transparently skips these whenever they appear
//! in the concatenated logical stream, so callers see one continuous byte
//! stream.
//!
//! For pre-11.x images the embedded header can land **inside** a compressed
//! data block; in that case the caller is responsible for pre-concatenating
//! the physical files and stripping the headers at known offsets before
//! handing the result to `SpanReader`. The 11.x format is naturally tolerant
//! of mid-stream headers.

use crate::error::Result;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::PathBuf;

/// Concatenate the given span files into a single output file, skipping the
/// 512-byte header at the start of each span.
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
    for span in spans {
        let mut f = BufReader::new(File::open(span.as_ref())?);
        // skip 512-byte file header
        f.seek(SeekFrom::Start(512))?;
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
}
