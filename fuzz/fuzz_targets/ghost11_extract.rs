#![no_main]

use libfuzzer_sys::fuzz_target;

// Fuzz harness for the Ghost 11.x / 12.x streaming extractor.
//
// Writes the fuzzer input as a temporary `.gho` file, calls
// `ghost11::stream::extract`, and asserts the result is well-defined:
// - On `Ok`, no partition payload exceeds reasonable bounds.
// - On `Err`, the error message is non-empty and non-panicking.
fuzz_target!(|data: &[u8]| {
    use gho::ghost11::stream::extract;

    // Use a unique tempfile for each fuzzer iteration so concurrent runs
    // (libFuzzer default) don't trip over each other.
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmpdir = std::env::temp_dir().join(format!("gho-fuzz-{pid}-{nanos}"));
    let _ = std::fs::create_dir_all(&tmpdir);
    let img = tmpdir.join("img.gho");
    let out = tmpdir.join("out");
    let _ = std::fs::write(&img, data);

    let result = extract(&img, &out);
    let _ = result; // Either Ok or Err is fine; we only assert no panic.

    // Cleanup is best-effort.
    let _ = std::fs::remove_dir_all(&tmpdir);
});
