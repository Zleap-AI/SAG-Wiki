// Resource staging is a build-time concern. The parallel release compiler must
// embed the original configuration rather than the temporary copy-only override.
pub fn resource_configuration(profile: &str, deferred: bool, original: Option<&str>, stripped: &str) -> (Option<String>, Option<String>) {
    if profile == "debug" {
        (Some(stripped.into()), Some(stripped.into()))
    } else if profile == "release" && deferred {
        (Some(stripped.into()), Some(original.unwrap_or("{}").into()))
    } else {
        (None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn release_defers_resource_copy_but_embeds_the_exact_original_config() {
        let original = r#"{"app":{"security":{"csp":"fixture"}},"bundle":{"resources":{"bin/sag.exe":"bin/sag.exe"}}}"#;
        let (copy, embedded) = resource_configuration("release", true, Some(original), "stripped");
        assert_eq!(copy.as_deref(), Some("stripped"));
        assert_eq!(embedded.as_deref(), Some(original));
    }
    #[test]
    fn absent_release_override_still_embeds_the_full_base_configuration() {
        assert_eq!(resource_configuration("release", true, None, "stripped"), (Some("stripped".into()), Some("{}".into())));
    }
    #[test]
    fn ordinary_release_keeps_the_existing_resource_check() {
        assert_eq!(resource_configuration("release", false, Some("original"), "stripped"), (None, None));
    }
    #[test]
    fn development_keeps_its_existing_separate_cli_configuration() {
        assert_eq!(resource_configuration("debug", false, Some("original"), "stripped"), (Some("stripped".into()), Some("stripped".into())));
    }
}
