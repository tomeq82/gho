#![no_main]

use libfuzzer_sys::fuzz_target;

// Fuzz harness for the pre-11.x directory walker.
fuzz_target!(|data: &[u8]| {
    use gho::ghostold::stream::walk_dirents;

    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmpdir = std::env::temp_dir().join(format!("gho-fuzz-old-{pid}-{nanos}"));
    let _ = std::fs::create_dir_all(&tmpdir);
    let img = tmpdir.join("img.gho");
    let _ = std::fs::write(&img, data);

    let result = walk_dirents(&img);
    if let Ok(entries) = result {
        // Each entry must have a sensible name (display_name never panics on
        // any byte pattern — exercised by the unit test suite).
        for entry in entries {
            let _ = entry.dirent.display_name();
        }
    }
    let _ = std::fs::remove_dir_all(&tmpdir);
});
