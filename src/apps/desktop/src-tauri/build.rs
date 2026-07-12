use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

fn collect_files(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, files);
        } else if path.is_file() && path.strip_prefix(root).is_ok() {
            files.push(path);
        }
    }
}

fn ui_build_id() -> String {
    let root = Path::new("../out");
    let mut files = Vec::new();
    collect_files(root, root, &mut files);
    files.sort();
    let mut hasher = DefaultHasher::new();
    for path in files {
        path.strip_prefix(root).unwrap_or(&path).hash(&mut hasher);
        if let Ok(bytes) = fs::read(&path) { bytes.hash(&mut hasher); }
    }
    format!("{:016x}", hasher.finish())
}

fn main() {
    println!("cargo:rerun-if-changed=../out");
    println!("cargo:rustc-env=APIARRAY_UI_BUILD_ID={}", ui_build_id());
    tauri_build::build()
}
