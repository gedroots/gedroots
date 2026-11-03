//! Embeds the Atlas (`atlas/*.yml`, see `src/atlas.rs`) in the library.

use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("atlas");
    println!("cargo:rerun-if-changed={}", dir.display());
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "yml"))
        .collect();
    files.sort();
    let mut out = String::from("&[\n");
    for f in &files {
        println!("cargo:rerun-if-changed={}", f.display());
        let name = f.file_stem().unwrap().to_string_lossy();
        out.push_str(&format!("    ({name:?}, include_str!({:?})),\n", f.display().to_string()));
    }
    out.push(']');
    let target = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("atlas.rs");
    std::fs::write(target, out).unwrap();
}
