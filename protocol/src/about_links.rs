use std::sync::OnceLock;

/// Resolve the same bundled about-page destinations used by the browser UI.
pub fn url(target: &str) -> Option<&'static str> {
    static LINKS: OnceLock<serde_json::Value> = OnceLock::new();
    LINKS
        .get_or_init(|| {
            serde_json::from_str(include_str!("about-links.json"))
                .expect("bundled about links must be valid JSON")
        })
        .get(target)?
        .as_str()
}

#[cfg(test)]
mod tests {
    #[test]
    fn resolves_public_destinations_and_rejects_unknown_targets() {
        for target in ["project", "company", "docling", "zleap-console", "302-api-keys"] {
            assert!(super::url(target).unwrap().starts_with("https://"));
        }
        for target in ["", "unknown", "__proto__", "project/../company"] {
            assert!(super::url(target).is_none());
        }
    }
}
