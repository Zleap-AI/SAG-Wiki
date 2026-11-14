fn main() {
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let mut entries = String::new();
    println!("cargo:rerun-if-env-changed=SAG_WEB_DIST");
    if let Some(dist) = std::env::var_os("SAG_WEB_DIST") {
        let dist = std::path::PathBuf::from(dist).canonicalize().unwrap();
        assert!(dist.join("index.html").is_file(), "SAG_WEB_DIST must contain a built app");
        println!("cargo:rerun-if-changed={}", dist.display());
        for directory in ["", "assets", "textfold"] {
            for entry in std::fs::read_dir(dist.join(directory)).unwrap() {
                let path = entry.unwrap().path();
                if !path.is_file() { continue; }
                let name = path.file_name().unwrap().to_str().unwrap();
                if directory.is_empty() && !matches!(name, "index.html" | "favicon.png") {
                    continue;
                }
                let route = if directory.is_empty() {
                    format!("/{name}")
                } else {
                    format!("/{directory}/{name}")
                };
                entries.push_str(&format!("({route:?}, include_bytes!({path:?})),\n"));
            }
        }
    }
    std::fs::write(output.join("web-assets.rs"), format!("&[\n{entries}]")).unwrap();
    cc::Build::new()
        .file("native/sag_rank.c")
        .include("native")
        .define("SQLITE_CORE", None)
        .define("SAG_STATIC", None)
        .opt_level(3)
        .compile("sag_rank");
    println!("cargo:rerun-if-changed=native/sag_rank.c");
    println!("cargo:rerun-if-changed=native/sqlite3.h");
    println!("cargo:rerun-if-changed=native/sqlite3ext.h");
}
