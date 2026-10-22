use sag_core::store::Store;
use sag_protocol::types::UpdateSource;

#[test]
fn update_sources_survive_reopening_the_database() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.sqlite");
    let store = Store::open(&path).unwrap();
    let mut settings = store.settings().unwrap();
    settings.updates.github_url = "https://github.com/customer/wiki".into();
    settings.updates.website_url = "https://example.com/updates.json".into();
    settings.updates.source = UpdateSource::Website;
    store.save_settings(&settings).unwrap();
    drop(store);
    let reopened = Store::open(&path).unwrap();
    let saved = reopened.settings().unwrap();
    assert!(matches!(saved.updates.source, UpdateSource::Website));
    assert_eq!(saved.updates.github_url, settings.updates.github_url);
    assert_eq!(saved.updates.website_url, settings.updates.website_url);
    assert_eq!(saved.answer.model, settings.answer.model);
}
