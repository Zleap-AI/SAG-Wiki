use sag_core::store::Store;
use std::{fs, sync::{mpsc, Arc}, time::Duration};

#[test]
fn paused_index_keeps_search_available_and_resumes_remaining_files() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("notes");
    fs::create_dir(&folder).unwrap();
    for i in 0..300 { fs::write(folder.join(format!("{i:03}.md")), "searchable evidence").unwrap(); }
    let store = Arc::new(Store::open(&temp.path().join("data/index.sqlite")).unwrap());
    let root = store.add_root(&folder).unwrap();
    let (paused_tx, paused_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let worker = store.clone();
    let worker_root = root.clone();
    let thread = std::thread::spawn(move || {
        let result = worker.scan(&worker_root, |progress| {
            if progress.count == 20 {
                worker.indexing.set_paused(true);
                paused_tx.send(()).unwrap();
            }
        });
        done_tx.send(result).unwrap();
    });
    paused_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(done_rx.recv_timeout(Duration::from_millis(100)).is_err());
    let status = store.indexing.status();
    assert!(status.paused);
    assert_eq!(status.tasks[0].path, root);
    assert_eq!(status.tasks[0].total, 300);
    assert!(!store.search_files("searchable", &[], 10).unwrap().is_empty());
    store.indexing.set_paused(false);
    assert_eq!(done_rx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap(), 300);
    thread.join().unwrap();
    assert!(store.indexing.status().tasks.is_empty());
    assert_eq!(store.files().unwrap().len(), 300);
}

#[test]
fn stopping_releases_a_paused_index_worker() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("notes");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("one.md"), "one").unwrap();
    let store = Arc::new(Store::open(&temp.path().join("data/index.sqlite")).unwrap());
    let root = store.add_root(&folder).unwrap();
    store.indexing.set_paused(true);
    let worker = store.clone();
    let (tx, rx) = mpsc::channel();
    let thread = std::thread::spawn(move || tx.send(worker.scan(&root, |_| {})).unwrap());
    store.indexing.stop();
    assert!(rx.recv_timeout(Duration::from_secs(2)).unwrap().is_err());
    thread.join().unwrap();
    assert!(store.indexing.status().tasks.is_empty());
    assert_eq!(store.index_history().unwrap()[0].status, "stopped");
}

#[test]
fn details_report_the_file_until_its_current_operation_finishes() {
    let control = Arc::new(sag_core::index_status::IndexControl::default());
    control.begin("/notes");
    control.phase("/notes", "indexing", 2);
    let (started_tx, started_rx) = mpsc::channel();
    let (finish_tx, finish_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let worker = control.clone();
    let thread = std::thread::spawn(move || {
        worker.file("/notes", std::path::Path::new("/notes/meeting.md"), || {
            started_tx.send(()).unwrap();
            finish_rx.recv().unwrap();
            Ok(())
        }).unwrap();
        worker.file("/notes", std::path::Path::new("/notes/next.md"), || Ok(())).unwrap();
        done_tx.send(()).unwrap();
    });
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(control.status().tasks[0].files, ["/notes/meeting.md"]);
    control.set_paused(true);
    finish_tx.send(()).unwrap();
    assert!(done_rx.recv_timeout(Duration::from_millis(100)).is_err());
    assert!(control.status().tasks[0].files.is_empty());
    control.set_paused(false);
    done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    thread.join().unwrap();
    let files = control.log("/notes");
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].path, "/notes/meeting.md");
    control.file::<()>("/notes", std::path::Path::new("/notes/failure.pdf"), || anyhow::bail!("conversion failed")).unwrap_err();
    let files = control.log("/notes");
    assert_eq!(files[2].status, "failed");
    assert_eq!(files[2].error.as_deref(), Some("conversion failed"));
}

#[test]
fn indexing_history_survives_reopening_and_includes_updates_and_failures() {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("data/index.sqlite");
    let folder = temp.path().join("notes");
    fs::create_dir(&folder).unwrap();
    let file = folder.join("one.md");
    fs::write(&file, "first indexed document").unwrap();
    let store = Store::open(&database).unwrap();
    let root = store.add_root(&folder).unwrap();
    store.scan(&root, |_| {}).unwrap();
    fs::write(&file, "updated document").unwrap();
    store.update_paths(&root, &[file.canonicalize().unwrap()], |_| {}).unwrap();
    fs::remove_dir_all(&folder).unwrap();
    assert!(store.scan(&root, |_| {}).is_err());
    drop(store);

    let store = Store::open(&database).unwrap();
    let history = store.index_history().unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(history[0].status, "error");
    assert!(history[0].error.as_ref().unwrap().contains("文件夹不可用"));
    for record in &history[1..] {
        assert_eq!(record.path, root);
        assert_eq!(record.name, "notes");
        assert_eq!(record.status, "ready");
        assert_eq!((record.completed, record.total), (1, 1));
        assert!(record.started_at <= record.finished_at);
        assert!(record.error.is_none());
    }
    assert!(history.windows(2).all(|records| records[0].id > records[1].id));
    store.clear_all_data().unwrap();
    assert!(store.index_history().unwrap().is_empty());
}

#[test]
fn clearing_history_preserves_indexed_files_active_tasks_and_new_history() {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("data/index.sqlite");
    let folder = temp.path().join("notes");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("note.md"), "searchable evidence").unwrap();
    let store = Store::open(&database).unwrap();
    let root = store.add_root(&folder).unwrap();
    store.scan(&root, |_| {}).unwrap();
    let first_id = store.index_history().unwrap()[0].id;
    store.clear_index_history().unwrap();
    assert!(store.index_history().unwrap().is_empty());
    drop(store);
    let store = Arc::new(Store::open(&database).unwrap());
    assert!(store.index_history().unwrap().is_empty());
    assert_eq!(store.files().unwrap().len(), 1);
    store.scan(&root, |_| {}).unwrap();
    let (paused_tx, paused_rx) = mpsc::channel();
    let worker = store.clone();
    let worker_root = root.clone();
    let thread = std::thread::spawn(move || worker.scan(&worker_root, |progress| {
        if progress.count == 0 {
            worker.indexing.set_paused(true);
            paused_tx.send(()).unwrap();
        }
    }));
    paused_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let status = store.indexing.status();
    store.clear_index_history().unwrap();
    assert!(store.index_history().unwrap().is_empty());
    assert!(store.indexing.status().paused);
    assert_eq!(store.indexing.status().tasks[0].path, status.tasks[0].path);
    assert_eq!(store.search_files("searchable", &[], 10).unwrap().len(), 1);
    assert_eq!(fs::read_to_string(folder.join("note.md")).unwrap(), "searchable evidence");
    store.indexing.set_paused(false);
    assert_eq!(thread.join().unwrap().unwrap(), 1);
    let records = store.index_history().unwrap();
    assert_eq!(records.len(), 1);
    assert!(records[0].id > first_id);
    assert_eq!(records[0].status, "ready");
}

#[test]
fn file_history_is_bounded_persistent_and_authorizes_only_recorded_originals() {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("data/index.sqlite");
    let folder = temp.path().join("notes");
    fs::create_dir(&folder).unwrap();
    for i in 0..130 { fs::write(folder.join(format!("{i:03}.md")), "indexed evidence").unwrap(); }
    let store = Store::open(&database).unwrap();
    let root = store.add_root(&folder).unwrap();
    store.scan(&root, |_| {}).unwrap();
    let id = store.index_history().unwrap()[0].id;
    let raw = store.index_history_log(id).unwrap().raw.unwrap();
    let files = raw.lines().map(|line| serde_json::from_str::<sag_protocol::IndexFileLog>(line).unwrap()).collect::<Vec<_>>();
    assert_eq!(files.len(), 100);
    assert_eq!(store.files().unwrap().len(), 130);
    assert!(files.iter().all(|file| file.status == "completed" && file.error.is_none()));
    assert_eq!(store.authorize_index_history_file(id, &files[0].path).unwrap().to_str().unwrap(), files[0].path);
    let later = folder.join("later.md");
    fs::write(&later, "new file").unwrap();
    assert!(store.authorize_index_history_file(id, later.to_str().unwrap()).is_err());
    fs::remove_file(&files[0].path).unwrap();
    assert!(store.authorize_index_history_file(id, &files[0].path).is_err());
    drop(store);
    let store = Store::open(&database).unwrap();
    assert_eq!(store.index_history_log(id).unwrap().raw.unwrap(), raw);
    store.clear_index_history().unwrap();
    assert!(store.index_history_log(id).is_err());
    assert_eq!(store.files().unwrap().len(), 130);
}

#[test]
fn legacy_history_has_no_fabricated_file_log_and_empty_new_jobs_are_distinct() {
    let temp = tempfile::tempdir().unwrap();
    let database = temp.path().join("index.sqlite");
    let db = rusqlite::Connection::open(&database).unwrap();
    db.execute_batch("CREATE TABLE index_history(id INTEGER PRIMARY KEY AUTOINCREMENT,path TEXT NOT NULL,name TEXT NOT NULL,started_at INTEGER NOT NULL,finished_at INTEGER NOT NULL,completed INTEGER NOT NULL,total INTEGER NOT NULL,status TEXT NOT NULL,error TEXT);
        INSERT INTO index_history VALUES(1,'/old','old',0,1,2,2,'ready',NULL);").unwrap();
    drop(db);
    let store = Store::open(&database).unwrap();
    assert!(store.index_history_log(1).unwrap().raw.is_none());
    let folder = temp.path().join("empty");
    fs::create_dir(&folder).unwrap();
    let root = store.add_root(&folder).unwrap();
    store.scan(&root, |_| {}).unwrap();
    assert_eq!(store.index_history_log(store.index_history().unwrap()[0].id).unwrap().raw.as_deref(), Some(""));
}
