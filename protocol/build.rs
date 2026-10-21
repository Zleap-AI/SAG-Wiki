use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::Path,
};
fn sources(path: &Path, hash: &mut DefaultHasher) {
    if path.is_dir() {
        let mut files: Vec<_> = fs::read_dir(path)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        files.sort();
        for file in files {
            sources(&file, hash);
        }
    } else {
        path.file_name().unwrap().hash(hash);
        fs::read(path).unwrap().hash(hash);
    }
}
fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    // CLI packages and desktop bundles distribute the same native release.
    let manifest = fs::read_to_string(root.join("backend/Cargo.toml")).unwrap();
    let version = manifest
        .lines()
        .find_map(|line| {
            line.strip_prefix("version = \"")
                .and_then(|value| value.strip_suffix('"'))
        })
        .unwrap();
    println!("cargo:rustc-env=SAG_RELEASE_VERSION={version}");
    let mut hash = DefaultHasher::new();
    for name in [
        "protocol/src",
        "client/src",
        "core/src",
        "core/native",
        "core/build.rs",
        "backend/src",
        
        
        
        "protocol/Cargo.toml",
        "client/Cargo.toml",
        "core/Cargo.toml",
        "backend/Cargo.toml",
        "Cargo.lock",
        
    ] {
        let path = root.join(name);
        println!("cargo:rerun-if-changed={}", path.display());
        if path.exists() {
            sources(&path, &mut hash);
        }
    }
    std::env::var("TARGET").unwrap().hash(&mut hash);
    let channel = if std::env::var("PROFILE").unwrap() == "release" {
        "release"
    } else {
        "development"
    };
    println!("cargo:rustc-env=SAG_BUILD_CHANNEL={channel}");
    println!(
        "cargo:rustc-env=SAG_BUILD_ID={channel}-{:016x}",
        hash.finish()
    );
}
