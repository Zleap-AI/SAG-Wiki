use anyhow::{bail, Result};
use sag_protocol::{IndexConversion, IndexFileLog, IndexStatus, IndexTask};
use std::{collections::{BTreeMap,BTreeSet}, path::Path, sync::{Arc, Condvar, Mutex}};

#[derive(Default)]
struct State {
    paused: bool,
    stopped: bool,
    cancelled: BTreeSet<String>,
    tasks: BTreeMap<String, IndexTask>,
    logs: BTreeMap<String, Vec<IndexFileLog>>,
    indexed: BTreeMap<String, (usize, usize)>,
    retrying: BTreeSet<(String, String)>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum IndexLane {
    #[default]
    Primary,
    Enhancement,
}
impl IndexLane {
    fn kind(self) -> &'static str {
        match self { Self::Primary => "primary", Self::Enhancement => "enhancement" }
    }
    fn key(self, root: &str) -> String { format!("{}:{root}",self.kind()) }
}
#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    resumed: Condvar,
}
/// Lane-local tasks share global pause/stop and root cancellation signals.
#[derive(Clone, Default)]
pub struct IndexControl {
    shared: Arc<Shared>,
    lane: IndexLane,
}
impl IndexControl {
    pub fn for_lane(&self, lane: IndexLane) -> Self { Self { shared: self.shared.clone(), lane } }
    fn key(&self, root: &str) -> String { self.lane.key(root) }
    pub fn is_active(&self, root: &str, lane: IndexLane) -> bool {
        self.shared.state.lock().unwrap().tasks.contains_key(&lane.key(root))
    }
    pub fn has_active_source(&self, root: &str, source: &Path, lane: IndexLane) -> bool {
        let source = source.to_string_lossy();
        self.shared.state.lock().unwrap().tasks.get(&lane.key(root))
            .is_some_and(|task| task.files.iter().any(|path|path == source.as_ref()))
    }
    pub fn cancel_lane(&self, root: &str, lane: IndexLane) {
        let mut state = self.shared.state.lock().unwrap();
        let key = lane.key(root);
        if state.tasks.contains_key(&key) { state.cancelled.insert(key); }
        self.shared.resumed.notify_all();
    }

    pub fn status(&self) -> IndexStatus {
        let state = self.shared.state.lock().unwrap();
        IndexStatus { failed_files: 0, paused: state.paused, tasks: state.tasks.values().cloned().map(|mut task| {
            if (task.stage == "converting" || task.stage == "enhancing") && task.files.iter().any(|file| state.retrying.contains(&(task.id.clone(), file.clone()))) {
                task.stage = if task.stage == "enhancing" { "retrying_enhancement" } else { "retrying_conversion" }.into();
            }
            task
        }).collect(), conversion_states: vec![] }
    }
    pub fn set_paused(&self, paused: bool) -> IndexStatus {
        self.shared.state.lock().unwrap().paused = paused;
        if !paused { self.shared.resumed.notify_all(); }
        self.status()
    }
    pub fn stop(&self) {
        self.shared.state.lock().unwrap().stopped = true;
        self.shared.resumed.notify_all();
    }
    pub fn stopped(&self) -> bool { self.shared.state.lock().unwrap().stopped }
    pub fn cancel_root(&self,root:&str) {
        let mut state=self.shared.state.lock().unwrap();
        let keys: Vec<_> = state.tasks.iter().filter(|(_,task)|task.path==root).map(|(key,_)|key.clone()).collect();
        state.cancelled.extend(keys);
        self.shared.resumed.notify_all();
    }
    pub fn cancelled(&self,root:&str)->bool {let state=self.shared.state.lock().unwrap();state.stopped||state.cancelled.contains(&self.key(root))}
    pub fn begin(&self, root: &str) {
        let mut state = self.shared.state.lock().unwrap();
        self.begin_locked(&mut state, root);
    }
    /// Atomically claim background work only while its root has no primary task.
    /// A primary task may begin afterwards; Store's source lease then protects
    /// the background input while foreground ingestion handles other files.
    pub fn try_begin(&self, root: &str) -> bool {
        let mut state = self.shared.state.lock().unwrap();
        if state.stopped || self.lane == IndexLane::Enhancement
            && (state.tasks.contains_key(&IndexLane::Primary.key(root))
                || state.tasks.contains_key(&self.key(root))) {
            return false;
        }
        self.begin_locked(&mut state, root);
        true
    }
    fn begin_locked(&self, state: &mut State, root: &str) {
        state.cancelled.remove(&self.key(root));
        state.logs.insert(self.key(root), vec![]);
        state.indexed.insert(self.key(root), (0, 0));
        state.tasks.insert(self.key(root), IndexTask {
            id: self.key(root), kind: self.lane.kind().into(),
            path: root.into(), stage: "scanning".into(), completed: 0, total: 0, files: vec![], conversions: vec![],
        });
    }
    pub fn task(&self, root: &str) -> IndexTask { self.shared.state.lock().unwrap().tasks[&self.key(root)].clone() }
    pub fn log(&self, root: &str) -> Vec<IndexFileLog> { self.shared.state.lock().unwrap().logs[&self.key(root)].clone() }
    pub fn indexed(&self, root: &str) -> (usize, usize) { self.shared.state.lock().unwrap().indexed[&self.key(root)] }
    pub(crate) fn record_indexed_progress(&self, root: &str, completed: usize, total: usize) {
        *self.shared.state.lock().unwrap().indexed.get_mut(&self.key(root)).unwrap() = (completed, total);
    }
    pub fn finish(&self, root: &str) {
        let mut state = self.shared.state.lock().unwrap();
        state.cancelled.remove(&self.key(root));
        state.tasks.remove(&self.key(root));
        state.logs.remove(&self.key(root));
        state.indexed.remove(&self.key(root));
        state.retrying.retain(|(key,_)|key != &self.key(root));
    }
    pub fn phase(&self, root: &str, stage: &str, total: usize) {
        let mut state = self.shared.state.lock().unwrap();
        let task = state.tasks.get_mut(&self.key(root)).unwrap();
        task.stage = stage.into(); task.total = total; task.completed = 0;
        task.conversions.clear();
        if stage == "indexing_converted" {
            let count = state.indexed.get_mut(&self.key(root)).unwrap();
            count.1 = count.0 + total;
        } else if stage == "indexing" || stage == "indexing_native" || stage == "enhancing" {
            state.indexed.insert(self.key(root), (0, total));
        }
    }
    pub fn progress(&self, root: &str, completed: usize, total: usize) {
        let mut state = self.shared.state.lock().unwrap();
        let task = state.tasks.get_mut(&self.key(root)).unwrap();
        task.completed = completed; task.total = total;
        let stage = task.stage.clone();
        let count = state.indexed.get_mut(&self.key(root)).unwrap();
        if stage == "indexing_converted" {
            count.0 = count.1 - total + completed;
        } else if stage == "indexing" || stage == "indexing_native" || stage == "enhancing" {
            *count = (completed, total);
        }
    }
    pub fn checkpoint_root(&self,root:&str)->Result<()> {
        let state=self.shared.state.lock().unwrap();
        let state=self.shared.resumed.wait_while(state,|s|s.paused&&!s.stopped&&!s.cancelled.contains(&self.key(root))).unwrap();
        if state.stopped||state.cancelled.contains(&self.key(root)){bail!("文件夹索引已停止");}Ok(())
    }
    pub fn checkpoint(&self) -> Result<()> {
        let state = self.shared.state.lock().unwrap();
        let state = self.shared.resumed.wait_while(state, |state| state.paused && !state.stopped).unwrap();
        if state.stopped { bail!("索引已停止") }
        Ok(())
    }
    pub(crate) fn conversion_attempt(&self,root:&str,path:&Path,attempt:crate::conversion::Attempt) {
        let mut state=self.shared.state.lock().unwrap();
        let path=path.to_string_lossy().into_owned();
        // Live parser state is independent of the bounded history and survives
        // cloud phase updates without being inferred from old log rows.
        if let Some(task) = state.tasks.get_mut(&self.key(root)) {
            if task.files.contains(&path) && (task.stage == "converting" || task.stage == "enhancing") {
                let segment = attempt.segment.clone().or_else(|| task.conversions.iter()
                    .find(|item| item.path == path).and_then(|item| item.segment.clone()));
                task.conversions.retain(|item| item.path != path);
                if attempt.phase.is_some() || segment.is_some() {
                    task.conversions.push(IndexConversion { path: path.clone(), engine: if attempt.phase.is_none() { String::new() } else { attempt.engine.clone() }, segment });
                    task.conversions.sort_by(|a,b| a.path.cmp(&b.path));
                }
            }
        }
        // Starting only drives live status; retain the existing history format.
        if matches!(attempt.phase.as_deref(), Some("starting" | "segment")) { return; }
        if let Some(log)=state.logs.get_mut(&self.key(root)) {
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
        self.file_attempt(root, path, false, work)
    }
    pub fn file_attempt<T>(&self, root: &str, path: &Path, retry: bool, work: impl FnOnce() -> Result<T>) -> Result<T> {
        let state = self.shared.state.lock().unwrap();
        let mut state = self.shared.resumed.wait_while(state, |state| state.paused && !state.stopped && !state.cancelled.contains(&self.key(root))).unwrap();
        if state.stopped || state.cancelled.contains(&self.key(root)) { bail!("索引已停止") }
        let path = path.to_string_lossy().into_owned();
        let current_stage = &state.tasks[&self.key(root)].stage;
        let stage = if retry { if current_stage == "enhancing" { "retrying_enhancement" } else { "retrying_conversion" }.into() } else { current_stage.clone() };
        if retry { state.retrying.insert((self.key(root), path.clone())); }
        state.tasks.get_mut(&self.key(root)).unwrap().files.push(path.clone());
        drop(state);
        let mut result = work();
        let mut state = self.shared.state.lock().unwrap();
        state.retrying.remove(&(self.key(root), path.clone()));
        if state.stopped || state.cancelled.contains(&self.key(root)) {result=Err(anyhow::anyhow!("文件夹已移除，已停止索引"));}
        state.tasks.get_mut(&self.key(root)).unwrap().files.retain(|file| file != &path);
        state.tasks.get_mut(&self.key(root)).unwrap().conversions.retain(|item| item.path != path);
        let log = state.logs.get_mut(&self.key(root)).unwrap();
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
    fn retry_label_only_applies_while_failed_file_is_running() {
        let control = IndexControl::default();
        control.begin("root"); control.phase("root", "converting", 2);
        control.file_attempt("root", Path::new("failed.pdf"), true, || {
            assert_eq!(control.status().tasks[0].stage, "retrying_conversion");
            assert_eq!(control.status().tasks[0].total, 2);
            Ok(())
        }).unwrap();
        control.file("root", Path::new("new.pdf"), || {
            assert_eq!(control.status().tasks[0].stage, "converting");
            Ok(())
        }).unwrap();
        assert_eq!(control.log("root")[0].stage, "retrying_conversion");
    }
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

#[cfg(test)]
mod parser_status_tests {
    use super::*;
    fn report(control: &IndexControl, path: &str, engine: &str, phase: Option<&str>) {
        control.conversion_attempt("root", Path::new(path), crate::conversion::Attempt {
            segment: None, engine: engine.into(), phase: phase.map(Into::into), elapsed_ms: 0, error: None,
        });
    }
    #[test]
    fn concurrent_parsers_follow_fallback_and_clear_at_file_boundaries() {
        let control = IndexControl::default();
        control.begin("root"); control.phase("root", "converting", 2);
        control.file("root", Path::new("a.pdf"), || {
            report(&control, "a.pdf", "MinerU", Some("starting"));
            control.file("root", Path::new("b.pdf"), || {
                report(&control, "b.pdf", "anydoc", Some("starting"));
                assert_eq!(control.task("root").conversions.len(), 2);
                report(&control, "a.pdf", "MinerU", None);
                report(&control, "a.pdf", "Docling", Some("starting"));
                let current = control.task("root").conversions;
                assert_eq!(current.iter().find(|item|item.path=="a.pdf").unwrap().engine,"Docling");
                control.set_paused(true);
                assert_eq!(control.status().tasks[0].conversions.len(), 2);
                control.set_paused(false);
                Ok(())
            })?;
            assert_eq!(control.task("root").conversions.len(), 1);
            control.cancel_root("root");
            Ok(())
        }).unwrap_err();
        assert!(control.task("root").conversions.is_empty());
        control.phase("root", "indexing_converted", 2);
        assert!(control.status().tasks[0].conversions.is_empty());
        control.finish("root");
        assert!(control.status().tasks.is_empty());
    }
    #[test]
    fn segment_status_survives_engine_boundaries_and_clears_with_the_file() {
        let control=IndexControl::default();control.begin("root");control.phase("root","converting",1);
        control.file("root",Path::new("long.pdf"),|| {
            control.conversion_attempt("root",Path::new("long.pdf"),crate::conversion::Attempt {
                segment:Some(sag_protocol::ConversionSegment {current:2,total:20,start_page:51,end_page:100}),
                phase:Some("segment".into()),engine:String::new(),elapsed_ms:0,error:None,
            });
            report(&control,"long.pdf","MinerU",Some("starting"));
            report(&control,"long.pdf","MinerU",None);
            let item=control.task("root").conversions.remove(0);
            assert!(item.engine.is_empty());assert_eq!(item.segment.unwrap().current,2);
            assert_eq!(control.status().tasks.len(),1);
            report(&control,"long.pdf","Docling",Some("starting"));
            let item=control.task("root").conversions.remove(0);
            assert_eq!(item.engine,"Docling");assert_eq!(item.segment.unwrap().start_page,51);
            Ok(())
        }).unwrap();
        assert!(control.task("root").conversions.is_empty());
        assert_eq!(control.status().tasks.len(),1);
    }
    #[test]
    fn active_parser_does_not_depend_on_bounded_history() {
        let control = IndexControl::default(); control.begin("root"); control.phase("root", "converting", 1);
        control.shared.state.lock().unwrap().logs.insert(control.key("root"), (0..100).map(|_| IndexFileLog {
            time:0,path:"old.pdf".into(),stage:"converting".into(),phase:None,engine:None,elapsed_ms:None,status:"completed".into(),error:None,
        }).collect());
        control.file("root", Path::new("a.pdf"), || {
            report(&control,"a.pdf","anydoc",Some("starting"));
            assert_eq!(control.task("root").conversions[0].engine,"anydoc");
            report(&control,"a.pdf","anydoc",None);
            assert!(control.task("root").conversions.is_empty());
            Ok(())
        }).unwrap();
    }
}

#[cfg(test)]
mod lane_tests {
    use super::*;
    #[test]
    fn same_root_lanes_keep_progress_logs_and_finish_independent() {
        let primary = IndexControl::default();
        let background = primary.for_lane(IndexLane::Enhancement);
        primary.begin("root"); background.begin("root");
        primary.phase("root", "indexing_native", 3);
        background.phase("root", "enhancing", 8);
        primary.progress("root", 2, 3); background.progress("root", 1, 8);
        primary.file("root",Path::new("new.txt"),|| Ok(())).unwrap();
        background.file("root",Path::new("old.pdf"),|| Ok(())).unwrap();
        assert_eq!(primary.indexed("root"),(2,3));
        assert_eq!(background.indexed("root"),(1,8));
        assert_eq!(primary.log("root")[0].path,"new.txt");
        assert_eq!(background.log("root")[0].path,"old.pdf");
        let tasks = primary.status().tasks;
        assert_eq!(tasks.len(),2);
        assert!(tasks.iter().all(|task|task.path=="root"));
        assert_ne!(tasks[0].id,tasks[1].id);
        assert_ne!(tasks[0].kind,tasks[1].kind);
        primary.finish("root");
        assert_eq!(background.task("root").completed,1);
        assert_eq!(primary.status().tasks.len(),1);
    }
    #[test]
    fn enhancement_claim_is_atomic_and_defers_to_active_primary() {
        let primary = IndexControl::default();
        let background = primary.for_lane(IndexLane::Enhancement);
        primary.begin("root");
        assert!(!background.try_begin("root"));
        assert_eq!(primary.status().tasks.len(),1);
        primary.finish("root");
        assert!(background.try_begin("root"));
        assert!(!background.try_begin("root"));
        assert!(primary.try_begin("root"));
        assert_eq!(primary.status().tasks.len(),2);
        primary.stop(); background.finish("root");
        assert!(!background.try_begin("other"));
    }
    #[test]
    fn retry_parser_and_lane_cancellation_do_not_leak_to_primary() {
        let primary = IndexControl::default();
        let background = primary.for_lane(IndexLane::Enhancement);
        primary.begin("root"); background.begin("root");
        primary.phase("root","converting",1); background.phase("root","enhancing",1);
        background.file_attempt("root",Path::new("same.pdf"),true,|| {
            primary.file("root",Path::new("same.pdf"),|| {
                let tasks = primary.status().tasks;
                assert_eq!(tasks.iter().find(|task|task.kind=="primary").unwrap().stage,"converting");
                assert_eq!(tasks.iter().find(|task|task.kind=="enhancement").unwrap().stage,"retrying_enhancement");
                assert!(primary.has_active_source("root",Path::new("same.pdf"),IndexLane::Enhancement));
                assert!(background.is_active("root",IndexLane::Primary));
                primary.cancel_lane("root",IndexLane::Enhancement);
                assert!(!primary.cancelled("root"));
                assert!(background.cancelled("root"));
                Ok(())
            })?;
            Ok(())
        }).unwrap_err();
        assert_eq!(background.log("root")[0].stage,"retrying_enhancement");
        assert!(!primary.has_active_source("root",Path::new("same.pdf"),IndexLane::Enhancement));
        assert_eq!(primary.log("root")[0].status,"completed");
    }
    #[test]
    fn readding_primary_does_not_revive_cancelled_background() {
        let primary = IndexControl::default();
        let background = primary.for_lane(IndexLane::Enhancement);
        primary.begin("root"); background.begin("root");
        background.file("root",Path::new("old.pdf"),|| {
            primary.cancel_root("root");
            primary.finish("root"); primary.begin("root");
            assert!(!primary.cancelled("root"));
            assert!(background.cancelled("root"));
            assert!(background.checkpoint_root("root").is_err());
            Ok(())
        }).unwrap_err();
        assert!(background.log("root")[0].error.is_some());
        background.finish("root");
        assert!(!primary.cancelled("root"));
        assert_eq!(primary.status().tasks.len(),1);
    }
    #[test]
    fn pause_and_stop_are_shared_but_root_cancellation_is_local() {
        let primary = IndexControl::default();
        let background = primary.for_lane(IndexLane::Enhancement);
        primary.begin("root"); background.begin("root"); background.begin("other");
        primary.set_paused(true);
        assert!(background.status().paused);
        let (tx,rx) = std::sync::mpsc::channel();
        let waiting = background.clone();
        let worker = std::thread::spawn(move||tx.send(waiting.checkpoint_root("root").is_err()).unwrap());
        primary.cancel_root("root");
        assert!(rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap());
        worker.join().unwrap();
        assert!(!background.cancelled("other"));
        background.set_paused(false);
        assert!(!primary.status().paused);
        assert!(background.checkpoint_root("other").is_ok());
        background.stop();
        assert!(primary.stopped());
        assert!(primary.checkpoint().is_err());
        assert!(background.checkpoint_root("other").is_err());
    }
}
