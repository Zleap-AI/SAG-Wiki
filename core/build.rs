fn main() {
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
