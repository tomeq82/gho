// AFL++ persistent-mode harness for the Ghost 11.x / 12.x extractor.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[unsafe(no_mangle)]
pub extern "C" fn afl_persistent(data: *const u8, size: usize) -> i32 {
    let input = unsafe { std::slice::from_raw_parts(data, size) };

    // Unique tempdir per call so concurrent AFL workers don't trip over
    // each other. AFL forks a new process for each input so this is safe.
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmpdir = std::env::temp_dir().join(format!("gho-afl-{pid}-{nanos}"));
    let _ = std::fs::create_dir_all(&tmpdir);

    let img = tmpdir.join("img.gho");
    let out = tmpdir.join("out");
    let _ = std::fs::write(&img, input);

    // Either Ok or Err is fine — AFL cares about no panic, no hang, no
    // excessive allocation. The unit/integration tests cover correctness.
    let _ = gho::ghost11::stream::extract(&img, &out);

    let _ = std::fs::remove_dir_all(&tmpdir);
    0
}

fn main() {
    use std::io::Read;
    let mut data = Vec::new();
    std::io::stdin().read_to_end(&mut data).unwrap();
    afl_persistent(data.as_ptr(), data.len());
}

#[allow(dead_code)]
fn _ensure_link() {
    // Force the linker to keep `PathBuf` in case AFL's GC strips it.
    let _ = PathBuf::from("/tmp");
}
