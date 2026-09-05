// AFL++ persistent-mode harness for `fastlz::decompress`.
//
// AFL calls `afl_persistent` once per mutated input. We feed the entire
// input as a single compressed block and assert invariants on the
// output. The decompressor must never panic, never allocate more than
// MAX_BLOCK_DECOMPRESSED bytes, and must return Err on malformed input.

#[unsafe(no_mangle)]
pub extern "C" fn afl_persistent(data: *const u8, size: usize) -> i32 {
    let input = unsafe { std::slice::from_raw_parts(data, size) };
    use gho::fastlz::{decompress, MAX_BLOCK_DECOMPRESSED};

    // Try a few different comp_len values to maximise coverage of the
    // boundary checks in `decompress`.
    for &comp_len in &[0usize, 1, 4, 8, 16, 64, 256, input.len()] {
        if let Ok(out) = decompress(input, comp_len) {
            assert!(
                out.len() <= MAX_BLOCK_DECOMPRESSED,
                "decompressed output {} exceeds cap {}",
                out.len(),
                MAX_BLOCK_DECOMPRESSED
            );
        }
    }
    0
}

fn main() {
    // AFL persistent-mode entry: read input from stdin (AFL pipes the
    // mutated test case here), pass it to the harness, and exit.
    use std::io::Read;
    let mut data = Vec::new();
    std::io::stdin().read_to_end(&mut data).unwrap();
    afl_persistent(data.as_ptr(), data.len());
}
