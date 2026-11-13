use crate::{
    docling_budget::Permit,
    process_supervisor::{self, Failure, FailureKind},
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use tempfile::TempPath;

const IDLE_TIMEOUT: Duration = Duration::from_secs(120);
const MEMORY_POLL: Duration = Duration::from_secs(1);
const POLL: Duration = Duration::from_millis(50);
const MAX_LINE: usize = 16 * 1024;
const STDERR_TAIL: usize = 8 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImageOutcome {
    Converted,
    Fallback,
}

pub(crate) struct SpawnSpec {
    pub command: Command,
    pub runner: TempPath,
    pub runtime_root: PathBuf,
    pub permit: Permit<'static>,
    pub runtime: sag_protocol::runtime::RuntimeSettings,
}

#[derive(Serialize)]
struct WorkerRequest<'a> {
    id: u64,
    source: &'a str,
    output: &'a str,
}

#[derive(Deserialize)]
struct WorkerResponse {
    id: u64,
    status: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    code: Option<u32>,
}

enum Event {
    Line(String),
    Error(String),
}

pub(crate) struct Worker {
    child: Child,
    #[cfg(windows)]
    job: process_supervisor::windows_job::Job,
    stdin: ChildStdin,
    events: Receiver<Event>,
    stderr: Arc<Mutex<String>>,
    _runner: TempPath,
    permit: Permit<'static>,
    runtime_root: PathBuf,
    runtime: sag_protocol::runtime::RuntimeSettings,
    next_id: u64,
    alive: bool,
}

impl Worker {
    pub(crate) fn spawn(spec: SpawnSpec, deadline: Instant, cancel: &dyn Fn() -> bool) -> Result<Self> {
        let SpawnSpec {
            mut command,
            runner,
            runtime_root,
            permit,
            runtime,
        } = spec;
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000004);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().context("无法启动文档解析 Worker")?;
        #[cfg(windows)]
        let job = match process_supervisor::windows_job::Job::new(&child, Some(permit.bytes)) {
            Ok(job) => job,
            Err(error) => { let _ = child.kill(); let _ = child.wait(); return Err(error.into()); }
        };
        let stdin = child.stdin.take().context("文档解析 Worker 缺少输入管道")?;
        let stdout = child
            .stdout
            .take()
            .context("文档解析 Worker 缺少输出管道")?;
        let stderr_pipe = child
            .stderr
            .take()
            .context("文档解析 Worker 缺少错误管道")?;
        let (tx, events) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                match reader.read_until(b'\n', &mut line) {
                    Ok(0) => break,
                    Ok(_) if line.len() > MAX_LINE => {
                        let _ = tx.send(Event::Error("文档解析 Worker 输出行过长".into()));
                        break;
                    }
                    Ok(_) => {
                        if line.last() == Some(&b'\n') {
                            line.pop();
                        }
                        if line.last() == Some(&b'\r') {
                            line.pop();
                        }
                        if tx
                            .send(Event::Line(String::from_utf8_lossy(&line).into_owned()))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = tx.send(Event::Error(error.to_string()));
                        break;
                    }
                }
            }
        });
        let stderr = Arc::new(Mutex::new(String::new()));
        let stderr_tail = stderr.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(stderr_pipe);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let mut tail = stderr_tail.lock().unwrap();
                        tail.push_str(&line);
                        if tail.len() > STDERR_TAIL {
                            let keep = tail.len() - STDERR_TAIL;
                            let boundary = tail
                                .char_indices()
                                .find_map(|(index, _)| (index >= keep).then_some(index))
                                .unwrap_or(tail.len());
                            tail.drain(..boundary);
                        }
                    }
                }
            }
        });
        let mut worker = Self {
            child,
            #[cfg(windows)]
            job,
            stdin,
            events,
            stderr,
            _runner: runner,
            permit,
            runtime_root,
            runtime,
            next_id: 1,
            alive: true,
        };
        worker.wait_ready(deadline, cancel)?;
        Ok(worker)
    }

    fn wait_ready(&mut self, deadline: Instant, cancel: &dyn Fn() -> bool) -> Result<()> {
        let started = Instant::now();
        let mut last_memory_check = started;
        loop {
            if cancel() {
                self.terminate();
                bail!(Failure::new(
                    FailureKind::Cancelled,
                    "已停止启动文档解析 Worker"
                ));
            }
            if Instant::now() >= deadline {
                self.terminate();
                bail!(Failure::new(
                    FailureKind::Timeout,
                    "文档解析 Worker 启动超时"
                ));
            }
            self.check_memory_periodically(&mut last_memory_check)?;
            match self.events.recv_timeout(POLL) {
                Ok(Event::Line(line)) => {
                    let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                        continue;
                    };
                    if value.get("status").and_then(|value| value.as_str()) == Some("ready") {
                        self.check_memory()?;
                        return Ok(());
                    }
                    if value.get("status").and_then(|value| value.as_str()) == Some("error") {
                        self.terminate();
                        bail!(
                            "文档解析 Worker 初始化失败：{}",
                            value
                                .get("message")
                                .and_then(|value| value.as_str())
                                .unwrap_or("未知错误")
                        );
                    }
                }
                Ok(Event::Error(error)) => {
                    self.terminate();
                    bail!("无法读取文档解析 Worker：{error}");
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.check_memory()?;
                    self.terminate();
                    bail!("文档解析 Worker 在就绪前退出{}", self.stderr_suffix());
                }
            }
            if let Some(status) = self.child.try_wait()? {
                self.check_memory()?;
                bail!(
                    "文档解析 Worker 在就绪前退出（{status}）{}",
                    self.stderr_suffix()
                );
            }
        }
    }

    pub(crate) fn convert(
        &mut self,
        source: &Path,
        output: &Path,
        deadline: Instant,
        cancel: &dyn Fn() -> bool,
    ) -> Result<ImageOutcome> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let source = source.to_str().context("图片路径不是有效 UTF-8")?;
        let output = output.to_str().context("转换路径不是有效 UTF-8")?;
        let mut request = serde_json::to_vec(&WorkerRequest { id, source, output })?;
        request.push(b'\n');
        self.stdin
            .write_all(&request)
            .and_then(|_| self.stdin.flush())
            .context("无法向文档解析 Worker 发送任务")?;
        let started = Instant::now();
        let mut last_memory_check = started;
        let mut exited_at = None;
        loop {
            if cancel() {
                self.terminate();
                bail!(Failure::new(
                    FailureKind::Cancelled,
                    "已停止文档解析 Worker"
                ));
            }
            if Instant::now() >= deadline {
                self.terminate();
                bail!(Failure::new(
                    FailureKind::Timeout,
                    "文档解析 Worker 处理超时"
                ));
            }
            self.check_memory_periodically(&mut last_memory_check)?;
            match self.events.recv_timeout(POLL) {
                Ok(Event::Line(line)) => {
                    let Ok(response) = serde_json::from_str::<WorkerResponse>(&line) else {
                        continue;
                    };
                    if response.id != id {
                        continue;
                    }
                    return match response.status.as_str() {
                        "ok" => { self.check_memory()?; Ok(ImageOutcome::Converted) },
                        "fallback" => { self.check_memory()?; Ok(ImageOutcome::Fallback) },
                        "error" => {
                            let message = format!("{}{}", response.message.as_deref().unwrap_or("解析 Worker 处理失败"), self.stderr_suffix());
                            match response.code {
                                Some(75) => Err(Failure::new(FailureKind::Memory, message).into()),
                                Some(76) => Err(Failure::new(FailureKind::Compatibility, message).into()),
                                _ => Err(anyhow::anyhow!(message)),
                            }
                        },
                        status => bail!("文档解析 Worker 返回未知状态：{status}"),
                    };
                }
                Ok(Event::Error(error)) => {
                    self.terminate();
                    bail!("无法读取文档解析 Worker：{error}");
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.check_memory()?;
                    self.terminate();
                    bail!("文档解析 Worker 意外退出{}", self.stderr_suffix());
                }
            }
            if let Some(status) = self.child.try_wait()? {
                // The pipe reader may enqueue the final reply after waitpid.
                // Keep ownership of the process group until Drop/terminate.
                let exited = *exited_at.get_or_insert_with(Instant::now);
                if exited.elapsed() >= Duration::from_secs(1) {
                    bail!("文档解析 Worker 意外退出（{status}）{}", self.stderr_suffix());
                }
            }
        }
    }

    fn check_memory_periodically(&mut self, last_check: &mut Instant) -> Result<()> {
        if last_check.elapsed() < MEMORY_POLL {
            return Ok(());
        }
        self.check_memory()?;
        *last_check = Instant::now();
        Ok(())
    }

    fn check_memory(&mut self) -> Result<()> {
        #[cfg(windows)]
        if self.job.peak_memory_bytes()? > self.permit.bytes {
            bail!(Failure::new(FailureKind::Memory,
                format!("文档解析 Worker 内存超过 {} MiB 限制", self.permit.bytes / 1024 / 1024)));
        }
        match process_supervisor::process_bytes(self.child.id()) {
            Ok(bytes) if bytes > self.permit.bytes => {
                bail!(Failure::new(
                    FailureKind::Memory,
                    format!(
                        "文档解析 Worker 内存超过 {} MiB 限制",
                        self.permit.bytes / 1024 / 1024
                    )
                ))
            }
            Ok(_) => Ok(()),
            Err(error) if self.alive => {
                if self.child.try_wait()?.is_some() { Ok(()) } else { Err(error.into()) }
            },
            Err(_) => Ok(()),
        }
    }

    pub(crate) fn is_running(&mut self) -> Result<bool> {
        Ok(self.alive && self.child.try_wait()?.is_none())
    }

    fn stderr_suffix(&self) -> String {
        let tail = self.stderr.lock().unwrap().trim().to_owned();
        if tail.is_empty() {
            String::new()
        } else {
            format!("：{tail}")
        }
    }

    fn terminate(&mut self) {
        if !self.alive {
            return;
        }
        #[cfg(unix)]
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        #[cfg(windows)]
        self.job.terminate();
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.alive = false;
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.terminate();
    }
}

struct Slot {
    worker: Option<Worker>,
    last_used: Instant,
}

pub(crate) struct WorkerPool {
    slots: Arc<Vec<Mutex<Slot>>>,
    next: AtomicUsize,
    reaper_started: AtomicBool,
}

impl WorkerPool {
    pub fn clear(&self) {
        for slot in self.slots.iter() {
            slot.lock().unwrap().worker = None;
        }
    }

    pub fn release_outdated(&self, runtime: &sag_protocol::runtime::RuntimeSettings) {
        for slot in self.slots.iter() {
            if let Ok(mut slot) = slot.try_lock() {
                if slot
                    .worker
                    .as_ref()
                    .is_some_and(|worker| worker.runtime != *runtime)
                {
                    slot.worker = None;
                }
            }
        }
    }

    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            slots: Arc::new(
                (0..capacity)
                    .map(|_| {
                        Mutex::new(Slot {
                            worker: None,
                            last_used: Instant::now(),
                        })
                    })
                    .collect(),
            ),
            next: AtomicUsize::new(0),
            reaper_started: AtomicBool::new(false),
        }
    }

    pub fn convert(
        &self,
        runtime_root: &Path,
        source: &Path,
        output: &Path,
        deadline: Instant,
        runtime: &sag_protocol::runtime::RuntimeSettings,
        cancel: &dyn Fn() -> bool,
        mut spawn: impl FnMut() -> Result<SpawnSpec>,
    ) -> Result<ImageOutcome> {
        loop {
            if cancel() {
                bail!(Failure::new(
                    FailureKind::Cancelled,
                    "已停止等待文档解析 Worker"
                ));
            }
            if Instant::now() >= deadline {
                bail!(Failure::new(
                    FailureKind::Timeout,
                    "等待文档解析 Worker 超时"
                ));
            }
            // Saved settings apply to the next task. Release idle workers with
            // old reservations before admission under the new memory budget.
            self.release_outdated(runtime);
            let start = self.next.fetch_add(1, Ordering::Relaxed) % self.slots.len();
            // Prefer an already initialized worker before paying another model load.
            for offset in 0..self.slots.len() {
                let index = (start + offset) % self.slots.len();
                let Ok(mut slot) = self.slots[index].try_lock() else {
                    continue;
                };
                if slot.worker.is_none() {
                    continue;
                }
                return self.run_in_slot(
                    &mut slot,
                    runtime_root,
                    source,
                    output,
                    deadline,
                    runtime,
                    cancel,
                    &mut spawn,
                );
            }
            for offset in 0..self.slots.len() {
                let index = (start + offset) % self.slots.len();
                let Ok(mut slot) = self.slots[index].try_lock() else {
                    continue;
                };
                if slot.worker.is_some() {
                    continue;
                }
                self.start_reaper();
                return self.run_in_slot(
                    &mut slot,
                    runtime_root,
                    source,
                    output,
                    deadline,
                    runtime,
                    cancel,
                    &mut spawn,
                );
            }
            thread::sleep(POLL);
        }
    }

    fn run_in_slot(
        &self,
        slot: &mut Slot,
        runtime_root: &Path,
        source: &Path,
        output: &Path,
        deadline: Instant,
        runtime: &sag_protocol::runtime::RuntimeSettings,
        cancel: &dyn Fn() -> bool,
        spawn: &mut impl FnMut() -> Result<SpawnSpec>,
    ) -> Result<ImageOutcome> {
        if slot
            .worker
            .as_ref()
            .is_some_and(|worker| worker.runtime_root != runtime_root || worker.runtime != *runtime)
        {
            slot.worker = None;
        }
        if slot.worker.is_none() {
            slot.worker = Some(Worker::spawn(spawn()?, deadline, cancel)?);
        }
        let result = slot
            .worker
            .as_mut()
            .unwrap()
            .convert(source, output, deadline, cancel);
        match result {
            Ok(outcome) => {
                // The full document pipeline may need this reservation as well.
                // Release the lightweight worker before handing the image back.
                if outcome == ImageOutcome::Fallback {
                    slot.worker = None;
                }
                slot.last_used = Instant::now();
                Ok(outcome)
            }
            Err(error) => {
                slot.worker = None;
                Err(error)
            }
        }
    }

    fn start_reaper(&self) {
        if self.reaper_started.swap(true, Ordering::Relaxed) {
            return;
        }
        let slots = Arc::downgrade(&self.slots);
        thread::spawn(move || loop {
            thread::sleep(Duration::from_secs(15));
            let Some(slots) = slots.upgrade() else {
                break;
            };
            for slot in slots.iter() {
                let Ok(mut slot) = slot.try_lock() else {
                    continue;
                };
                if slot.worker.is_some() && slot.last_used.elapsed() >= IDLE_TIMEOUT {
                    slot.worker = None;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(mode: &str, memory_mib: u64) -> SpawnSpec {
        let runner = tempfile::NamedTempFile::new().unwrap().into_temp_path();
        let mut command = Command::new(if cfg!(windows) { "python" } else { "python3" });
        command.args([
            "-u",
            "-c",
            r#"
import json, pathlib, sys, time
mode = sys.argv[1]
if mode == 'startup': time.sleep(10)
print(json.dumps({'status': 'ready'}), flush=True)
for line in sys.stdin:
    request = json.loads(line)
    if mode == 'exit': sys.exit(1)
    if mode == 'request': time.sleep(10)
    pathlib.Path(request['output']).write_text('fixture OCR', encoding='utf-8')
    print(json.dumps({'id': request['id'], 'status': 'ok'}), flush=True)
"#,
            mode,
        ]);
        let mut runtime = sag_protocol::runtime::RuntimeSettings::default();
        runtime.conversion_job_mib = memory_mib;
        SpawnSpec {
            command,
            runtime_root: PathBuf::from("fixture"),
            runner,
            permit: crate::docling_budget::acquire_bytes_with_runtime(
                crate::docling_budget::IMAGE_WORKER_BYTES,
                &runtime,
                &|| false,
                Instant::now() + Duration::from_secs(5),
            )
            .unwrap(),
            runtime,
        }
    }

    #[test]
    fn final_error_reply_is_drained_after_process_exit() {
        for (code, expected) in [(75, FailureKind::Memory), (76, FailureKind::Compatibility)] {
            let mut worker = Worker::spawn(fixture("exit", 128), Instant::now() + Duration::from_secs(5), &|| false).unwrap();
            // Model a reader thread delivering the flushed final reply after waitpid.
            let (tx, rx) = mpsc::channel();
            worker.events = rx;
            let reader = thread::spawn(move || {
                thread::sleep(Duration::from_millis(150));
                let _ = tx.send(Event::Line(format!("{{\"id\":1,\"status\":\"error\",\"code\":{code},\"message\":\"parser failed\"}}")));
            });
            let output = tempfile::tempdir().unwrap();
            let error = worker.convert(Path::new("part.pdf"), &output.path().join("part.md"), Instant::now() + Duration::from_secs(3), &|| false).unwrap_err();
            reader.join().unwrap();
            assert_eq!(error.downcast_ref::<Failure>().map(|e| e.kind), Some(expected), "{error:#}");
        }
    }

    #[test]
    fn image_worker_startup_obeys_the_task_timeout() {
        let started = Instant::now();
        let error = Worker::spawn(
            fixture("startup", 128),
            started + Duration::from_millis(250),
            &|| started.elapsed() > Duration::from_secs(2),
        )
        .err()
        .expect("startup must stop");
        assert_eq!(
            error.downcast_ref::<Failure>().unwrap().kind,
            FailureKind::Timeout
        );
    }

    #[test]
    fn image_worker_conversion_obeys_the_task_timeout() {
        let mut worker = Worker::spawn(
            fixture("request", 128),
            Instant::now() + Duration::from_secs(5),
            &|| false,
        )
        .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let started = Instant::now();
        let error = worker
            .convert(
                Path::new("image.png"),
                &temp.path().join("image.md"),
                started + Duration::from_millis(250),
                &|| started.elapsed() > Duration::from_secs(2),
            )
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<Failure>().unwrap().kind,
            FailureKind::Timeout
        );
        assert!(!temp.path().join("image.md").exists());
    }

    #[test]
    fn image_worker_enforces_memory_and_releases_failed_workers() {
        let error = Worker::spawn(
            fixture("request", 1),
            Instant::now() + Duration::from_secs(5),
            &|| false,
        )
        .err()
        .expect("Python exceeds a 1 MiB budget");
        assert_eq!(
            error.downcast_ref::<Failure>().map(|failure| failure.kind),
            Some(FailureKind::Memory),
            "{error:#}"
        );
    }

    #[test]
    fn image_worker_queue_obeys_the_task_timeout() {
        let pool = WorkerPool::new(1);
        let _busy = pool.slots[0].lock().unwrap();
        let error = pool
            .convert(
                Path::new("fixture"),
                Path::new("image.png"),
                Path::new("image.md"),
                Instant::now() + Duration::from_millis(100),
                &Default::default(),
                &|| false,
                || panic!("a busy slot must not spawn a worker"),
            )
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<Failure>().unwrap().kind,
            FailureKind::Timeout
        );
    }

    #[test]
    fn image_worker_reuses_models_until_saved_settings_change() {
        let pool = WorkerPool::new(1);
        let temp = tempfile::tempdir().unwrap();
        let mut runtime = sag_protocol::runtime::RuntimeSettings::default();
        runtime.conversion_job_mib = 128;
        let spawns = AtomicUsize::new(0);
        for memory in [128, 128, 64] {
            runtime.conversion_job_mib = memory;
            let output = temp.path().join("image.md");
            assert_eq!(
                pool.convert(
                    Path::new("fixture"),
                    Path::new("image.png"),
                    &output,
                    Instant::now() + Duration::from_secs(5),
                    &runtime,
                    &|| false,
                    || {
                        spawns.fetch_add(1, Ordering::Relaxed);
                        Ok(fixture("ok", memory))
                    }
                )
                .unwrap(),
                ImageOutcome::Converted
            );
            assert_eq!(std::fs::read_to_string(output).unwrap(), "fixture OCR");
            assert_eq!(
                pool.slots[0]
                    .lock()
                    .unwrap()
                    .worker
                    .as_ref()
                    .unwrap()
                    .permit
                    .bytes,
                memory * 1024 * 1024
            );
        }
        assert_eq!(spawns.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn image_worker_old_reservations_are_released_as_soon_as_the_slot_becomes_idle() {
        let pool = WorkerPool::new(1);
        let old = fixture("ok", 128);
        let runtime = old.runtime.clone();
        let worker =
            Worker::spawn(old, Instant::now() + Duration::from_secs(5), &|| false).unwrap();
        let pid = worker.child.id();
        let mut busy = pool.slots[0].lock().unwrap();
        busy.worker = Some(worker);
        let mut updated = runtime;
        updated.conversion_job_mib = 64;
        pool.release_outdated(&updated);
        assert!(
            busy.worker.is_some(),
            "a running task retains its parameters"
        );
        drop(busy);
        pool.release_outdated(&updated);
        assert!(pool.slots[0].lock().unwrap().worker.is_none());
        assert!(process_supervisor::process_bytes(pid).is_err());
    }

    #[test]
    fn image_worker_cancellation_terminates_without_publishing() {
        let mut worker = Worker::spawn(
            fixture("request", 128),
            Instant::now() + Duration::from_secs(5),
            &|| false,
        )
        .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let error = worker
            .convert(
                Path::new("image.png"),
                &temp.path().join("image.md"),
                Instant::now() + Duration::from_secs(5),
                &|| true,
            )
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<Failure>().unwrap().kind,
            FailureKind::Cancelled
        );
        assert!(!worker.alive);
        assert!(!temp.path().join("image.md").exists());
    }
}
