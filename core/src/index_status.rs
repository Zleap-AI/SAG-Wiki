use anyhow::{bail, Result};
use sag_protocol::{IndexFileLog, IndexStatus, IndexTask};
use std::{collections::BTreeMap, path::Path, sync::{Condvar, Mutex}};

#[derive(Default)]
struct State {
    paused: bool,
    stopped: bool,
    tasks: BTreeMap<String, IndexTask>,
    logs: BTreeMap<String, Vec<IndexFileLog>>,
    indexed: BTreeMap<String, (usize, usize)>,
}

/// Pause only at file boundaries, without holding the database lock.
#[derive(Default)]
pub struct IndexControl {
    state: Mutex<State>,
    resumed: Condvar,
}
impl IndexControl {
    pub fn status(&self) -> IndexStatus {
        let state = self.state.lock().unwrap();
        IndexStatus { paused: state.paused, tasks: state.tasks.values().cloned().collect() }
    }
    pub fn set_paused(&self, paused: bool) -> IndexStatus {
        self.state.lock().unwrap().paused = paused;
        if !paused { self.resumed.notify_all(); }
        self.status()
    }
    pub fn stop(&self) {
        self.state.lock().unwrap().stopped = true;
        self.resumed.notify_all();
    }
    pub fn stopped(&self) -> bool { self.state.lock().unwrap().stopped }
    pub fn begin(&self, root: &str) {
        let mut state = self.state.lock().unwrap();
        state.logs.insert(root.into(), vec![]);
        state.indexed.insert(root.into(), (0, 0));
        state.tasks.insert(root.into(), IndexTask {
            path: root.into(), stage: "scanning".into(), completed: 0, total: 0, files: vec![],
        });
    }
    pub fn task(&self, root: &str) -> IndexTask { self.state.lock().unwrap().tasks[root].clone() }
    pub fn log(&self, root: &str) -> Vec<IndexFileLog> { self.state.lock().unwrap().logs[root].clone() }
    pub fn indexed(&self, root: &str) -> (usize, usize) { self.state.lock().unwrap().indexed[root] }
    pub fn finish(&self, root: &str) {
        let mut state = self.state.lock().unwrap();
        state.tasks.remove(root);
        state.logs.remove(root);
        state.indexed.remove(root);
    }
    pub fn phase(&self, root: &str, stage: &str, total: usize) {
        let mut state = self.state.lock().unwrap();
        let task = state.tasks.get_mut(root).unwrap();
        task.stage = stage.into(); task.total = total; task.completed = 0;
        if stage == "indexing_converted" {
            let count = state.indexed.get_mut(root).unwrap();
            count.1 = count.0 + total;
        } else if stage == "indexing" || stage == "indexing_native" {
            state.indexed.insert(root.into(), (0, total));
        }
    }
    pub fn progress(&self, root: &str, completed: usize, total: usize) {
        let mut state = self.state.lock().unwrap();
        let task = state.tasks.get_mut(root).unwrap();
        task.completed = completed; task.total = total;
        let stage = task.stage.clone();
        let count = state.indexed.get_mut(root).unwrap();
        if stage == "indexing_converted" {
            count.0 = count.1 - total + completed;
        } else if stage == "indexing" || stage == "indexing_native" {
            *count = (completed, total);
        }
    }
    pub fn checkpoint(&self) -> Result<()> {
        let state = self.state.lock().unwrap();
        let state = self.resumed.wait_while(state, |state| state.paused && !state.stopped).unwrap();
        if state.stopped { bail!("索引已停止") }
        Ok(())
    }
    pub fn file<T>(&self, root: &str, path: &Path, work: impl FnOnce() -> Result<T>) -> Result<T> {
        let state = self.state.lock().unwrap();
        let mut state = self.resumed.wait_while(state, |state| state.paused && !state.stopped).unwrap();
        if state.stopped { bail!("索引已停止") }
        let path = path.to_string_lossy().into_owned();
        let stage = state.tasks[root].stage.clone();
        state.tasks.get_mut(root).unwrap().files.push(path.clone());
        drop(state);
        let result = work();
        let mut state = self.state.lock().unwrap();
        state.tasks.get_mut(root).unwrap().files.retain(|file| file != &path);
        let log = state.logs.get_mut(root).unwrap();
        if log.len() < 100 {
            log.push(IndexFileLog {
                time: crate::store::now(), path, stage,
                status: if result.is_ok() { "completed" } else { "failed" }.into(),
                error: result.as_ref().err().map(ToString::to_string),
            });
        }
        result
    }
}
