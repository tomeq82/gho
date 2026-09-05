//! Integration test against the real pre-11.x Norton Ghost image.
//!
//! Run with: `cargo test --test real_image -- --ignored`
//!
//! This test reads the user's real .gho + .ghs files from
//! `/mnt/storage/ghost_backups_old/` and exercises the span header detection
//! and concatenate helper. It is `#[ignore]` by default because it depends
//! on a local file path that won't exist on CI.

use std::path::PathBuf;

use gho::span::{concatenate_spans, looks_like_header, read_file_header};

fn real_image_paths() -> Option<Vec<PathBuf>> {
    let dir = PathBuf::from("/mnt/storage/ghost_backups_old");
    if !dir.exists() {
        return None;
    }
    let files = ["laptopas.gho", "lapto001.GHS", "lapto002.GHS"];
    let paths: Vec<PathBuf> = files
        .iter()
        .map(|n| dir.join(n))
        .filter(|p| p.exists())
        .collect();
    if paths.len() == files.len() {
        Some(paths)
    } else {
        None
    }
}

#[test]
#[ignore]
fn real_image_headers_are_recognised() {
    let paths = real_image_paths().expect("real image files not available");
    let mut image_ids = Vec::new();
    for p in &paths {
        let hdr = read_file_header(p).expect("read header");
        assert!(
            looks_like_header(&hdr),
            "{} does not look like a Ghost file header",
            p.display()
        );
        // file_type 1 (first) for the .gho, 9 (continuation) for .ghs
        assert!(hdr[2] == 1 || hdr[2] == 9, "unexpected file_type byte");
        let image_id = u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]);
        image_ids.push(image_id);
    }
    // Per history-recovery, the image IDs are 0x3bf50224, 0x3bf50225, 0x3bf50226.
    assert_eq!(image_ids[0], 0x3bf5_0224);
    assert_eq!(image_ids[1], 0x3bf5_0225);
    assert_eq!(image_ids[2], 0x3bf5_0226);
}

#[test]
#[ignore]
fn real_image_span_concat_writes_expected_size() {
    let paths = real_image_paths().expect("real image files not available");
    let tmp = tempfile::tempdir().expect("tmp dir");
    let out = tmp.path().join("combined.gho");
    let result = concatenate_spans(paths.iter(), &out).expect("concat");
    let meta = std::fs::metadata(&result).expect("stat");
    // Expected: sum of file sizes minus 512 per span (3 spans).
    let expected: u64 = paths.iter().map(|p| std::fs::metadata(p).unwrap().len()).sum::<u64>() - 3 * 512;
    assert_eq!(meta.len(), expected);
}
