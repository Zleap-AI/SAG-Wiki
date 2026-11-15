#[path = "build-resource-config.rs"]
mod build_resource_config;

fn main() {
    println!("cargo:rerun-if-changed=icons");
    println!("cargo:rerun-if-changed=build-resource-config.rs");
    println!("cargo:rerun-if-env-changed=SAG_DEFER_RELEASE_RESOURCES");
    let profile = std::env::var("PROFILE").unwrap();
    let deferred = std::env::var("SAG_DEFER_RELEASE_RESOURCES").as_deref() == Ok("1");
    let original = std::env::var("TAURI_CONFIG").ok();
    let mut config: serde_json::Value = serde_json::from_str(original.as_deref().unwrap_or("{}")).unwrap();
    config["bundle"]["resources"] = serde_json::json!([]);
    let (copy_override, embedded_override) = build_resource_config::resource_configuration(
        &profile, deferred, original.as_deref(), &config.to_string());
    if let Some(value) = copy_override {
        std::env::set_var("TAURI_CONFIG", value);
    }
    tauri_build::build();
    if deferred && profile == "release" {
        if let Some(value) = &original {
            std::env::set_var("TAURI_CONFIG", value);
        } else {
            std::env::remove_var("TAURI_CONFIG");
        }
    }
    if let Some(value) = embedded_override {
        // Debug retains its existing separate CLI. A deferred release restores
        // every original runtime/security/updater/resource setting for codegen.
        println!("cargo:rustc-env=TAURI_CONFIG={value}");
    }
}
