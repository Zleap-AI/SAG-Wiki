use sag_core::store::Store;

#[test]
fn index_details_visibility_survives_reopening_the_database() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("settings.sqlite");
    for enabled in [true, false] {
        let store = Store::open(&path).unwrap();
        let mut settings = store.settings().unwrap();
        settings.show_index_details = enabled;
        store.save_settings(&settings).unwrap();
        drop(store);
        let reopened = Store::open(&path).unwrap();
        assert_eq!(reopened.settings().unwrap().show_index_details, enabled);
    }
}
