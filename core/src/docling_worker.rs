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

const START_TIMEOUT: Duration = Duration::from_secs(120);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
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
}

enum Event {
    Line(String),
    Error(String),
}

struct Worker {
    child: Child,
    stdin: ChildStdin,
    events: Receiver<Event>,
    stderr: Arc<Mutex<String>>,
    _runner: TempPath,
    permit: Permit<'static>,
    runtime_root: PathBuf,
    next_id: u64,
    alive: bool,
}

impl Worker {
    fn spawn(spec: SpawnSpec, cancel: &dyn Fn() -> bool) -> Result<Self> {
        let SpawnSpec {
            mut command,
            runner,
            runtime_root,
            permit,
        } = spec;
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().context("无法启动图片 OCR Worker")?;
        let stdin = child.stdin.take().context("图片 OCR Worker 缺少输入管道")?;
        let stdout = child
            .stdout
            .take()
            .context("图片 OCR Worker 缺少输出管道")?;
        let stderr_pipe = child
            .stderr
            .take()
            .context("图片 OCR Worker 缺少错误管道")?;
        let (tx, events) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                match reader.read_until(b'\n', &mut line) {
                    Ok(0) => break,
                    Ok(_) if line.len() > MAX_LINE => {
                        let _ = tx.send(Event::Error("图片 OCR Worker 输出行过长".into()));
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
            stdin,
            events,
            stderr,
            _runner: runner,
            permit,
            runtime_root,
            next_id: 1,
            alive: true,
        };
        worker.wait_ready(cancel)?;
        Ok(worker)
    }

    fn wait_ready(&mut self, cancel: &dyn Fn() -> bool) -> Result<()> {
        let started = Instant::now();
        let mut last_memory_check = started;
        loop {
            if cancel() {
                self.terminate();
                bail!(Failure::new(
                    FailureKind::Cancelled,
                    "已停止启动图片 OCR Worker"
                ));
            }
            if started.elapsed() >= START_TIMEOUT {
                self.terminate();
                bail!(Failure::new(
                    FailureKind::Timeout,
                    "图片 OCR Worker 启动超时"
                ));
            }
            self.check_memory_periodically(&mut last_memory_check)?;
            match self.events.recv_timeout(POLL) {
                Ok(Event::Line(line)) => {
                    let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                        continue;
                    };
                    if value.get("status").and_then(|value| value.as_str()) == Some("ready") {
                        return Ok(());
                    }
                    if value.get("status").and_then(|value| value.as_str()) == Some("error") {
                        self.terminate();
                        bail!(
                            "图片 OCR Worker 初始化失败：{}",
                            value
                                .get("message")
                                .and_then(|value| value.as_str())
                                .unwrap_or("未知错误")
                        );
                    }
                }
                Ok(Event::Error(error)) => {
                    self.terminate();
                    bail!("无法读取图片 OCR Worker：{error}");
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.terminate();
                    bail!("图片 OCR Worker 在就绪前退出{}", self.stderr_suffix());
                }
            }
            if let Some(status) = self.child.try_wait()? {
                self.alive = false;
                bail!(
                    "图片 OCR Worker 在就绪前退出（{status}）{}",
                    self.stderr_suffix()
                );
            }
        }
    }

    fn convert(
        &mut self,
        source: &Path,
        output: &Path,
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
            .context("无法向图片 OCR Worker 发送任务")?;
        let started = Instant::now();
        let mut last_memory_check = started;
        loop {
            if cancel() {
                self.terminate();
                bail!(Failure::new(
                    FailureKind::Cancelled,
                    "已停止图片 OCR Worker"
                ));
            }
            if started.elapsed() >= REQUEST_TIMEOUT {
                self.terminate();
                bail!(Failure::new(
                    FailureKind::Timeout,
                    "图片 OCR Worker 处理超时"
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
                        "ok" => Ok(ImageOutcome::Converted),
                        "fallback" => Ok(ImageOutcome::Fallback),
                        "error" => bail!(
                            "{}",
                            response
                                .message
                                .as_deref()
                                .unwrap_or("图片 OCR Worker 处理失败")
                        ),
                        status => bail!("图片 OCR Worker 返回未知状态：{status}"),
                    };
                }
                Ok(Event::Error(error)) => {
                    self.terminate();
                    bail!("无法读取图片 OCR Worker：{error}");
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.terminate();
                    bail!("图片 OCR Worker 意外退出{}", self.stderr_suffix());
                }
            }
            if let Some(status) = self.child.try_wait()? {
                self.alive = false;
                bail!(
                    "图片 OCR Worker 意外退出（{status}）{}",
                    self.stderr_suffix()
                );
            }
        }
    }

    fn check_memory_periodically(&self, last_check: &mut Instant) -> Result<()> {
        if last_check.elapsed() < MEMORY_POLL {
            return Ok(());
        }
        self.check_memory()?;
        *last_check = Instant::now();
        Ok(())
    }

    fn check_memory(&self) -> Result<()> {
        match process_supervisor::process_bytes(self.child.id()) {
            Ok(bytes) if bytes > self.permit.bytes => {
                bail!(Failure::new(
                    FailureKind::Memory,
                    format!(
                        "图片 OCR Worker 内存超过 {} MiB 限制",
                        self.permit.bytes / 1024 / 1024
                    )
                ))
            }
            Ok(_) => Ok(()),
            Err(error) if self.alive => Err(error.into()),
            Err(_) => Ok(()),
        }
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
        cancel: &dyn Fn() -> bool,
        mut spawn: impl FnMut() -> Result<SpawnSpec>,
    ) -> Result<ImageOutcome> {
        loop {
            if cancel() {
                bail!(Failure::new(
                    FailureKind::Cancelled,
                    "已停止等待图片 OCR Worker"
                ));
            }
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
        cancel: &dyn Fn() -> bool,
        spawn: &mut impl FnMut() -> Result<SpawnSpec>,
    ) -> Result<ImageOutcome> {
        if slot
            .worker
            .as_ref()
            .is_some_and(|worker| worker.runtime_root != runtime_root)
        {
            slot.worker = None;
        }
        if slot.worker.is_none() {
            slot.worker = Some(Worker::spawn(spawn()?, cancel)?);
        }
        let result = slot
            .worker
            .as_mut()
            .unwrap()
            .convert(source, output, cancel);
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
