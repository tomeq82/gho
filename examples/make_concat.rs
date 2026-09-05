use std::path::Path;
fn main() {
    let paths = [
        "/mnt/storage/ghost_backups_old/laptopas.gho",
        "/mnt/storage/ghost_backups_old/lapto001.GHS",
        "/mnt/storage/ghost_backups_old/lapto002.GHS",
    ];
    let p: Vec<&Path> = paths.iter().map(|s| Path::new(s)).collect();
    gho::span::concatenate_spans(p, Path::new("/tmp/concat.gho")).unwrap();
    let meta = std::fs::metadata("/tmp/concat.gho").unwrap();
    println!("concat size: {}", meta.len());
}
