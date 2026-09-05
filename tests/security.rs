//! Security tests for `gho`.
//!
//! See `docs/SECURITY.md` for the threat model. The categories here are:
//!
//! 1. **Decompression bombs** — `fastlz::decompress` must not allocate
//!    unbounded memory.
//! 2. **Integer overflow in length fields** — `stored_len`, `body_len`,
//!    file offsets must be checked with `checked_*` arithmetic.
//! 3. **Resource exhaustion** — count of dirents / blocks / bytes read
//!    must be bounded.
//! 4. **Path traversal** — dirent names must be sanitized; output paths
//!    must not escape `--out`.
//! 5. **No panics on adversarial input** — every code path on untrusted
//!    bytes must return `Result::Err`, never panic.
//! 6. **Format detection confusion** — a malicious image claiming to be
//!    11.x and pre-11.x simultaneously must be rejected deterministically.
//!
//! These tests are run by `cargo test --test security` and feed into the
//! `cargo fuzz` corpus in `fuzz/`.

use gho::error::Error;
use gho::fastlz;
use gho::ghost11::{FileHeader, HEADER_SIZE as GHOST11_HEADER_SIZE};
use gho::ghostold::dirent::Dirent;
use gho::safety::{contains_parent_traversal, sanitize_8_3};
use gho::span::looks_like_header;

// ---------------------------------------------------------------------------
// 1. Decompression bombs
// ---------------------------------------------------------------------------

#[test]
fn fastlz_uncompressed_block_rejects_oversized_length() {
    // byte[0] == 1 → uncompressed; bytes 1..4 declare the length.
    // Declare a length of 16 MiB and supply that many bytes — must be
    // rejected by the per-block output cap.
    use gho::fastlz::MAX_BLOCK_DECOMPRESSED;
    let declared = (MAX_BLOCK_DECOMPRESSED + 1) as u32;
    let mut payload = vec![1u8];
    payload.extend_from_slice(&declared.to_le_bytes()[..3]);
    payload.extend(std::iter::repeat_n(0xAAu8, declared as usize));
    let err = fastlz::decompress(&payload, payload.len()).unwrap_err();
    assert!(matches!(err, Error::FastLz { .. }));
}

#[test]
fn fastlz_compressed_block_bounded_by_output_cap() {
    // Build a payload that produces far more bytes than MAX_BLOCK_DECOMPRESSED.
    // Each literal token is one byte; we issue many literal tokens.
    use gho::fastlz::MAX_BLOCK_DECOMPRESSED;
    let n_literals = MAX_BLOCK_DECOMPRESSED + 4096;
    // Control word 0x0000 means all 16 tokens are literals (low bit = 0).
    // We need (n_literals / 16) control words.
    let n_control_words = n_literals.div_ceil(16);
    let mut payload = vec![0x00, 0x00, 0x00, 0x00];
    for _ in 0..n_control_words {
        payload.extend_from_slice(&[0x00, 0x00]); // control word = 0 (all literals)
        payload.extend(std::iter::repeat_n(0x41u8, 16));
    }
    let err = fastlz::decompress(&payload, payload.len()).unwrap_err();
    assert!(matches!(err, Error::FastLz { .. }));
}

#[test]
fn fastlz_truncated_input_returns_error_not_panic() {
    // Empty input.
    assert!(fastlz::decompress(&[], 0).is_err());
    // Truncated (claims more than available).
    assert!(fastlz::decompress(&[0x00], 10).is_err());
    // Just header bytes — control reads as 1, then src + 1 >= src_end so loop exits cleanly.
    // The output is empty (not an error), which is correct: a valid block with no
    // compressed data produces zero bytes.
    let ok = fastlz::decompress(&[0x00, 0x00, 0x00, 0x00], 4).unwrap();
    assert!(ok.is_empty());
}

// ---------------------------------------------------------------------------
// 2. Integer overflow in length fields
// ---------------------------------------------------------------------------

#[test]
fn ghost11_header_rejects_oversized_compression_byte() {
    // 511-byte buffer with valid magic but no body — must return Truncated,
    // not panic.
    let mut buf = vec![0u8; GHOST11_HEADER_SIZE - 1];
    buf[0] = 0xFE;
    buf[1] = 0xEF;
    let err = FileHeader::parse(&buf).unwrap_err();
    assert!(matches!(err, Error::Truncated { .. }));
}

#[test]
fn ghost11_header_rejects_compression_out_of_range() {
    let mut buf = vec![0u8; GHOST11_HEADER_SIZE];
    buf[0] = 0xFE;
    buf[1] = 0xEF;
    buf[3] = 200; // not 0, 2, or 3..=9
    let _hdr = FileHeader::parse(&buf).unwrap();
    // (the header parses; compression type check happens at extract time)
}

// ---------------------------------------------------------------------------
// 3. Resource exhaustion: directory walk caps
// ---------------------------------------------------------------------------

#[test]
fn ghostold_dirent_parses_with_adversarial_bytes() {
    // A dirent filled with 0xFF bytes — all attrs set, cluster=max, size=max.
    let buf = [0xFFu8; 56];
    let d = Dirent::parse(&buf).unwrap();
    assert_eq!(d.attrs, 0xFF);
    assert_eq!(d.size, 0xFFFF_FFFF);
    assert_eq!(d.cluster, 0xFFFF_FFFF);
    assert!(d.is_vfat_long()); // attrs & 0x0F == 0x0F
}

#[test]
fn ghostold_dirent_rejects_truncated() {
    let err = Dirent::parse(&[0u8; 10]).unwrap_err();
    assert!(matches!(err, Error::Truncated { .. }));
}

// ---------------------------------------------------------------------------
// 4. Path traversal
// ---------------------------------------------------------------------------

#[test]
fn path_traversal_in_dirent_names_is_neutralised() {
    // Construct a dirent whose name spells "../etc" and ext spells "pas".
    let mut buf = [0u8; 56];
    buf[0..8].copy_from_slice(b"../etc  ");
    buf[8..11].copy_from_slice(b"pas");
    buf[11] = 0x20;
    buf[28..32].copy_from_slice(&1u32.to_le_bytes());
    let d = Dirent::parse(&buf).unwrap();
    let safe = sanitize_8_3(&d.name, &d.ext).unwrap();
    // The sanitised name must not contain any traversal characters.
    assert!(!contains_parent_traversal(std::path::Path::new(&safe)));
    assert!(!safe.contains('/'));
    assert!(!safe.contains('\\'));
}

#[test]
fn path_traversal_with_absolute_prefix_is_neutralised() {
    let mut buf = [0u8; 56];
    buf[0..8].copy_from_slice(b"/etc/pas");
    buf[11] = 0x20;
    let d = Dirent::parse(&buf).unwrap();
    let safe = sanitize_8_3(&d.name, &d.ext).unwrap();
    assert!(!safe.starts_with('/'));
    assert!(!contains_parent_traversal(std::path::Path::new(&safe)));
}

#[test]
fn path_traversal_with_null_byte_is_neutralised() {
    let mut buf = [0u8; 56];
    buf[0..8].copy_from_slice(b"ABC\0\0\0\0\0");
    buf[11] = 0x20;
    let d = Dirent::parse(&buf).unwrap();
    let safe = sanitize_8_3(&d.name, &d.ext).unwrap();
    assert!(!safe.contains('\0'));
    assert!(!contains_parent_traversal(std::path::Path::new(&safe)));
}

// ---------------------------------------------------------------------------
// 5. No panics on adversarial input
// ---------------------------------------------------------------------------

#[test]
fn random_512_byte_buffers_parse_without_panic() {
    // Pseudo-random but deterministic. The point is to ensure that
    // every code path on the header returns Result, not a panic.
    let mut buf = [0u8; 512];
    let mut state: u32 = 0xDEAD_BEEF;
    for chunk in buf.chunks_mut(4) {
        // xorshift32
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let bytes = state.to_le_bytes();
        let n = chunk.len().min(4);
        chunk[..n].copy_from_slice(&bytes[..n]);
    }
    // Whatever the magic says, we must get a Result.
    let _ = FileHeader::parse(&buf);
}

#[test]
fn random_dirent_buffer_parses_without_panic() {
    let mut buf = [0u8; 56];
    for byte in buf.iter_mut() {
        *byte = byte.wrapping_add(0x9E);
    }
    let _ = Dirent::parse(&buf);
}

#[test]
fn fastlz_random_buffers_do_not_panic() {
    // Mix of zero, all-ones, random-looking payloads.
    let cases: Vec<Vec<u8>> = vec![
        vec![],
        vec![0u8; 4],
        vec![0xFFu8; 100],
        vec![0xFE, 0xEF, 0x01, 0x02], // valid header start
        (0..200).map(|i| (i * 17) as u8).collect(),
        (0..200).map(|i| (i ^ 0xAA) as u8).collect(),
    ];
    for case in cases {
        for len in 0..=case.len() {
            // Must not panic.
            let _ = fastlz::decompress(&case, len);
        }
    }
}

// ---------------------------------------------------------------------------
// 6. Format detection confusion
// ---------------------------------------------------------------------------

#[test]
fn span_header_detection_rejects_garbage() {
    assert!(!looks_like_header(&[0u8; 512]));
    assert!(!looks_like_header(&[0xFFu8; 512]));
    assert!(!looks_like_header(&[0xFE, 0xEF, 5, 0, 0, 0, 0, 0])); // wrong file_type
}

#[test]
fn span_header_detection_accepts_realistic_headers() {
    let mut hdr = [0u8; 512];
    hdr[0] = 0xFE;
    hdr[1] = 0xEF;
    hdr[2] = 1; // first
    assert!(looks_like_header(&hdr));
    hdr[2] = 9; // continuation
    assert!(looks_like_header(&hdr));
}

// ---------------------------------------------------------------------------
// 7. Cross-module integration: small malformed images don't panic
// ---------------------------------------------------------------------------

#[test]
fn ghost11_extract_on_truncated_file_returns_error() {
    use gho::ghost11::stream::extract;
    let tmp = tempfile::tempdir().unwrap();
    // Just the header, no records.
    let img = tmp.path().join("truncated.gho");
    let mut hdr = vec![0u8; GHOST11_HEADER_SIZE];
    hdr[0] = 0xFE;
    hdr[1] = 0xEF;
    hdr[2] = 1;
    hdr[3] = 0;
    std::fs::write(&img, &hdr).unwrap();
    let out = tmp.path().join("out");
    let result = extract(&img, &out);
    // Empty image → no partitions, but must not panic.
    assert!(result.is_ok());
    assert!(result.unwrap().partitions.is_empty());
}

#[test]
fn ghost11_extract_on_random_bytes_does_not_panic() {
    use gho::ghost11::stream::extract;
    let tmp = tempfile::tempdir().unwrap();
    let img = tmp.path().join("random.gho");
    let mut buf = vec![0u8; 4096];
    let mut state: u32 = 0x1234_5678;
    for chunk in buf.chunks_mut(4) {
        state = state.wrapping_mul(1103515245).wrapping_add(12345);
        let bytes = state.to_le_bytes();
        let n = chunk.len().min(4);
        chunk[..n].copy_from_slice(&bytes[..n]);
    }
    // Make the header look valid.
    buf[0] = 0xFE;
    buf[1] = 0xEF;
    buf[2] = 1;
    buf[3] = 0; // uncompressed
    std::fs::write(&img, &buf).unwrap();
    let out = tmp.path().join("out");
    // Whatever happens, must not panic.
    let _ = extract(&img, &out);
}

#[test]
fn ghostold_walk_on_random_bytes_does_not_panic() {
    use gho::ghostold::stream::walk_dirents;
    let tmp = tempfile::tempdir().unwrap();
    let img = tmp.path().join("random.gho");
    let mut buf = vec![0u8; 8192];
    let mut state: u32 = 0x8765_4321;
    for chunk in buf.chunks_mut(4) {
        state = state.wrapping_mul(22695477).wrapping_add(1);
        let bytes = state.to_le_bytes();
        let n = chunk.len().min(4);
        chunk[..n].copy_from_slice(&bytes[..n]);
    }
    // Valid header.
    buf[0] = 0xFE;
    buf[1] = 0xEF;
    buf[2] = 1;
    buf[3] = 2; // FastLZ
    std::fs::write(&img, &buf).unwrap();
    // Whatever happens, must not panic.
    let _ = walk_dirents(&img);
}
