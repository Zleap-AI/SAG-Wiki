use serde::Deserialize;
use std::{collections::BTreeMap, sync::LazyLock};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocaleRegistry {
    default_language: String,
    languages: BTreeMap<String, InterfaceLocale>,
}

#[derive(Deserialize)]
pub struct InterfaceLocale {
    pub tray: TrayLabels,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayLabels {
    pub open: String,
    pub quit: String,
    pub quick_search: String,
}

static LOCALES: LazyLock<LocaleRegistry> = LazyLock::new(|| {
    serde_json::from_str(include_str!("interface-locales.json"))
        .expect("the bundled interface locale registry must be valid")
});

pub fn default_interface_language() -> &'static str {
    &LOCALES.default_language
}

pub fn supports_interface_language(language: &str) -> bool {
    LOCALES.languages.contains_key(language)
}

pub fn interface_locale(language: &str) -> &'static InterfaceLocale {
    LOCALES
        .languages
        .get(language)
        .unwrap_or_else(|| &LOCALES.languages[default_interface_language()])
}
