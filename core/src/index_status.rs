use anyhow::{bail, Result};
use sag_protocol::{IndexFileLog, IndexStatus, IndexTask};
use std::{collections::{BTreeMap,BTreeSet}, path::Path, sync::{Condvar, Mutex}};

#[derive(Default)]
struct State {
    paused: bool,
    stopped: bool,
    cancelled: BTreeSet<String>,
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
    pub fn cancel_root(&self,root:&str) {
        let mut state=self.state.lock().unwrap();
        if state.tasks.contains_key(root) {state.cancelled.insert(root.into());}
        self.resumed.notify_all();
    }
    pub fn cancelled(&self,root:&str)->bool {let state=self.state.lock().unwrap();state.stopped||state.cancelled.contains(root)}
    pub fn begin(&self, root: &str) {
        let mut state = self.state.lock().unwrap();
        state.cancelled.remove(root);
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
        state.cancelled.remove(root);
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
    pub fn checkpoint_root(&self,root:&str)->Result<()> {
        let state=self.state.lock().unwrap();
        let state=self.resumed.wait_while(state,|s|s.paused&&!s.stopped&&!s.cancelled.contains(root)).unwrap();
        if state.stopped||state.cancelled.contains(root){bail!("文件夹索引已停止");}Ok(())
    }
    pub fn checkpoint(&self) -> Result<()> {
        let state = self.state.lock().unwrap();
        let state = self.resumed.wait_while(state, |state| state.paused && !state.stopped).unwrap();
        if state.stopped { bail!("索引已停止") }
        Ok(())
    }
    pub(crate) fn conversion_attempt(&self,root:&str,path:&Path,attempt:crate::conversion::Attempt) {
        let mut state=self.state.lock().unwrap();
        let path=path.to_string_lossy().into_owned();
        if let Some(log)=state.logs.get_mut(root) {
            if attempt.phase.is_some() && log.iter().rev().find(|row|row.path==path&&row.engine.as_deref()==Some(&attempt.engine))
                .is_some_and(|row|row.phase==attempt.phase) {return;}
            // Preserve final engine outcomes when long cloud queues fill the bounded log.
            if log.len()>=100 && attempt.phase.is_none() {
                if let Some(index)=log.iter().position(|row|row.path==path&&row.phase.is_some()){log.remove(index);}
            }
            if log.len()<100 {log.push(IndexFileLog {time:crate::store::now(),path,stage:"converting".into(),phase:attempt.phase.clone(),engine:Some(attempt.engine),elapsed_ms:Some(attempt.elapsed_ms),status:if attempt.phase.is_some(){"running"}else if attempt.error.is_some(){"fallback"}else{"completed"}.into(),error:attempt.error});}
        }
    }
    pub fn file<T>(&self, root: &str, path: &Path, work: impl FnOnce() -> Result<T>) -> Result<T> {
        let state = self.state.lock().unwrap();
        let mut state = self.resumed.wait_while(state, |state| state.paused && !state.stopped && !state.cancelled.contains(root)).unwrap();
        if state.stopped || state.cancelled.contains(root) { bail!("索引已停止") }
        let path = path.to_string_lossy().into_owned();
        let stage = state.tasks[root].stage.clone();
        state.tasks.get_mut(root).unwrap().files.push(path.clone());
        drop(state);
        let mut result = work();
        let mut state = self.state.lock().unwrap();
        if state.cancelled.contains(root) {result=Err(anyhow::anyhow!("文件夹已移除，已停止索引"));}
        state.tasks.get_mut(root).unwrap().files.retain(|file| file != &path);
        let log = state.logs.get_mut(root).unwrap();
        if log.len() < 100 {
            log.push(IndexFileLog {
                time: crate::store::now(), path, stage, phase: None, engine: None, elapsed_ms: None,
                status: if result.is_ok() { "completed" } else { "failed" }.into(),
                error: result.as_ref().err().map(ToString::to_string),
            });
        }
        result
    }
}

#[cfg(test)]
mod cancellation_tests {
    use super::*;
    #[test]
    fn removing_one_root_cancels_its_work_without_stopping_other_roots() {
        let control=IndexControl::default();control.begin("old");control.begin("new");
        let result=control.file("old",Path::new("old/a.pdf"),||{control.cancel_root("old");Ok(())});
        assert!(result.is_err());assert!(control.cancelled("old"));assert!(!control.cancelled("new"));
        assert!(control.file("new",Path::new("new/a.txt"),||Ok(())).is_ok());
        control.finish("old");control.begin("old");assert!(!control.cancelled("old"));
    }
    #[test]
    fn removing_a_paused_root_wakes_its_checkpoint() {
        let control=std::sync::Arc::new(IndexControl::default());control.begin("old");control.set_paused(true);
        let (tx,rx)=std::sync::mpsc::channel();let worker=control.clone();
        let thread=std::thread::spawn(move||tx.send(worker.checkpoint_root("old").is_err()).unwrap());
        control.cancel_root("old");assert!(rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap());thread.join().unwrap();assert!(control.status().paused);
    }
}
