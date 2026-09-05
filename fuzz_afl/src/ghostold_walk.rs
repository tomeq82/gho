// AFL++ persistent-mode harness for the pre-11.x directory walker.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[unsafe(no_mangle)]
pub extern "C" fn afl_persistent(data: *const u8, size: usize) -> i32 {
    let input = unsafe { std::slice::from_raw_parts(data, size) };

    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmpdir = std::env::temp_dir().join(format!("gho-afl-old-{pid}-{nanos}"));
    let _ = std::fs::create_dir_all(&tmpdir);

    let img = tmpdir.join("img.gho");
    let _ = std::fs::write(&img, input);

    if let Ok(entries) = gho::ghostold::stream::walk_dirents(&img) {
        for entry in &entries {
            let _ = entry.dirent.display_name();
        }
    }

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
    let _ = PathBuf::from("/tmp");
}
