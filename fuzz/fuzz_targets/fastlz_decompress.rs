#![no_main]

use libfuzzer_sys::fuzz_target;

// Fuzz harness for `fastlz::decompress`.
//
// The harness feeds the entire fuzzer input as the block payload and
// exercises both branches (uncompressed-escape and compressed). The
// decompressor must:
// - never panic,
// - never allocate more than `MAX_BLOCK_DECOMPRESSED` bytes,
// - return `Err` on malformed input.
fuzz_target!(|data: &[u8]| {
    use gho::fastlz::{decompress, MAX_BLOCK_DECOMPRESSED};

    for &comp_len_suffix in &[0usize, 1, 4, 8, 16, 64, 256, data.len()] {
        let comp_len = comp_len_suffix.min(data.len());
        if let Ok(out) = decompress(data, comp_len) {
            assert!(
                out.len() <= MAX_BLOCK_DECOMPRESSED,
                "decompressed output {} exceeds cap {}",
                out.len(),
                MAX_BLOCK_DECOMPRESSED
            );
        }
    }
});
