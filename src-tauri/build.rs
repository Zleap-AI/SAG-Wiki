fn main() {
    println!("cargo:rerun-if-changed=icons");
    if std::env::var("PROFILE").unwrap() == "debug" {
        let mut config: serde_json::Value =
            serde_json::from_str(&std::env::var("TAURI_CONFIG").unwrap_or_else(|_| "{}".into()))
                .unwrap();
        // Development reads the separate CLI directly; release resources are not required.
        config["bundle"]["resources"] = serde_json::json!([]);
        let config = config.to_string();
        std::env::set_var("TAURI_CONFIG", &config);
        println!("cargo:rustc-env=TAURI_CONFIG={config}");
    }
    tauri_build::build()
}
