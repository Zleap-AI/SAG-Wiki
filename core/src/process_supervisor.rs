//! Bounded output and sampled process-tree resource supervision.
//! The watchdog is a sampling limit, not an OS allocation limit. macOS samples
//! physical footprint as well as RSS; Windows owns a kill-on-close job object.
use std::{
    collections::{BTreeMap, VecDeque},
    fmt,
    io::Read,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    Cancelled,
    Timeout,
    Memory,
    Monitoring,
    Spawn,
    Io,
    Signal,
    Exit,
    Compatibility,
}
#[derive(Debug)]
pub struct Failure {
    pub kind: FailureKind,
    pub message: String,
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Failure {}
impl Failure {
    pub(crate) fn new(kind: FailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

pub struct Limits {
    pub timeout: Duration,
    pub memory_bytes: Option<u64>,
}
const POLL: Duration = Duration::from_millis(50);
const MAX_LINE: usize = 8192;
const MAX_PENDING: usize = 32;
#[derive(Default)]
struct Output {
    tail: VecDeque<Vec<u8>>,
    pending: VecDeque<Vec<u8>>,
    error: Option<String>,
}
impl Output {
    fn push(&mut self, line: Vec<u8>) {
        if line.is_empty() {
            return;
        }
        self.tail.push_back(line.clone());
        if self.tail.len() > 8 {
            self.tail.pop_front();
        }
        self.pending.push_back(line);
        if self.pending.len() > MAX_PENDING {
            self.pending.pop_front();
        }
    }
    fn text(&self) -> String {
        self.tail
            .iter()
            .map(|line| String::from_utf8_lossy(line))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
#[cfg(unix)]
trait Pipe: Read + std::os::fd::AsRawFd {}
#[cfg(unix)]
impl<T: Read + std::os::fd::AsRawFd> Pipe for T {}
#[cfg(windows)]
trait Pipe: Read + std::os::windows::io::AsRawHandle {}
#[cfg(windows)]
impl<T: Read + std::os::windows::io::AsRawHandle> Pipe for T {}
fn pipe_readable(reader: &impl Pipe) -> std::io::Result<bool> {
    #[cfg(unix)]
    {
        let mut descriptor = libc::pollfd {
            fd: reader.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let result = unsafe { libc::poll(&mut descriptor, 1, 20) };
        if result < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                return Ok(false);
            }
            return Err(error);
        }
        Ok(result > 0)
    }
    #[cfg(windows)]
    unsafe {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn PeekNamedPipe(
                pipe: *mut std::ffi::c_void,
                buffer: *mut std::ffi::c_void,
                size: u32,
                read: *mut u32,
                available: *mut u32,
                left: *mut u32,
            ) -> i32;
        }
        let mut available = 0u32;
        if PeekNamedPipe(
            reader.as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut available,
            std::ptr::null_mut(),
        ) == 0
        {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(109) {
                return Ok(true);
            }
            return Err(error);
        }
        if available == 0 {
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(available > 0)
    }
}
fn read_output(mut reader: impl Pipe, output: Arc<Mutex<Output>>, stop: Arc<AtomicBool>) {
    let mut line = Vec::new();
    let mut buffer = [0; 4096];
    let mut drained = 0;
    loop {
        let stopping = stop.load(Ordering::Acquire);
        if stopping && drained >= 64 * 1024 {
            output.lock().unwrap().push(line);
            return;
        }
        match pipe_readable(&reader) {
            Ok(false) => {
                if stopping {
                    output.lock().unwrap().push(line);
                    return;
                }
                continue;
            }
            Err(error) => {
                output.lock().unwrap().error = Some(error.to_string());
                return;
            }
            Ok(true) => {}
        }
        match reader.read(&mut buffer) {
            Ok(0) => {
                output.lock().unwrap().push(line);
                return;
            }
            Ok(n) => {
                if stopping {
                    drained += n;
                }
                for byte in &buffer[..n] {
                    if matches!(byte, b'\r' | b'\n') {
                        output.lock().unwrap().push(std::mem::take(&mut line));
                    } else {
                        line.push(*byte);
                        if line.len() == MAX_LINE {
                            output.lock().unwrap().push(std::mem::take(&mut line));
                        }
                    }
                }
            }
            Err(error) => {
                output.lock().unwrap().error = Some(error.to_string());
                return;
            }
        }
    }
}

#[derive(Clone)]
struct Process {
    parent: u32,
    group: u32,
    identity: String,
    bytes: u64,
}
#[cfg(all(unix, not(target_os = "macos")))]
fn snapshot() -> Result<BTreeMap<u32, Process>, Failure> {
    let result = Command::new("/bin/ps")
        .args(["-axo", "pid=,ppid=,pgid=,rss=,lstart="])
        .output()
        .map_err(|e| {
            Failure::new(
                FailureKind::Monitoring,
                format!("无法读取转换进程内存：{e}"),
            )
        })?;
    if !result.status.success() {
        return Err(Failure::new(
            FailureKind::Monitoring,
            "无法读取转换进程内存",
        ));
    }
    let mut processes = BTreeMap::new();
    for line in String::from_utf8_lossy(&result.stdout).lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 9 {
            continue;
        }
        let (Ok(pid), Ok(parent), Ok(group), Ok(kib)) = (
            fields[0].parse::<u32>(),
            fields[1].parse(),
            fields[2].parse(),
            fields[3].parse::<u64>(),
        ) else {
            continue;
        };
        #[cfg(not(target_os = "linux"))]
        let identity = fields[4..].join(" ");
        let bytes = kib.saturating_mul(1024);
        #[cfg(target_os = "linux")]
        {
            // Kernel start ticks distinguish a recycled PID, including within a second.
            let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
                continue;
            };
            let Some((_, rest)) = stat.rsplit_once(')') else {
                continue;
            };
            let Some(start) = rest.split_whitespace().nth(19) else {
                continue;
            };
            let identity = start.to_owned();
            processes.insert(
                pid,
                Process {
                    parent,
                    group,
                    identity,
                    bytes,
                },
            );
            continue;
        }
        #[cfg(not(target_os = "linux"))]
        processes.insert(
            pid,
            Process {
                parent,
                group,
                identity,
                bytes,
            },
        );
    }
    Ok(processes)
}
#[cfg(target_os = "macos")]
fn snapshot() -> Result<BTreeMap<u32, Process>, Failure> {
    // /bin/ps is setuid on macOS and cannot exec inside sandbox-exec, even
    // with allow-default. libproc avoids spawning a privileged executable.
    const ALL_PIDS: u32 = 1;
    const SHORT_BSD_INFO: i32 = 13;
    const MAX_PIDS: usize = 1024 * 1024;
    #[repr(C)]
    struct ShortBsdInfo {
        pid: u32,
        parent: u32,
        group: u32,
        status: u32,
        command: [u8; 16],
        // flags, uid, gid, ruid, rgid, svuid, svgid, reserved.
        credentials: [u32; 8],
    }
    const _: () = assert!(std::mem::size_of::<ShortBsdInfo>() == 64);
    unsafe extern "C" {
        fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut std::ffi::c_void) -> i32;
    }
    let failure = || {
        Failure::new(
            FailureKind::Monitoring,
            format!("无法枚举转换进程：{}", std::io::Error::last_os_error()),
        )
    };
    let estimate = unsafe { libc::proc_listpids(ALL_PIDS, 0, std::ptr::null_mut(), 0) };
    if estimate <= 0 {
        return Err(failure());
    }
    let mut capacity = (estimate as usize / std::mem::size_of::<libc::pid_t>()) + 128;
    let pids = loop {
        if capacity > MAX_PIDS {
            return Err(Failure::new(
                FailureKind::Monitoring,
                "进程枚举超过安全容量，无法完整监督转换进程",
            ));
        }
        let mut pids = vec![0 as libc::pid_t; capacity];
        let size = std::mem::size_of_val(pids.as_slice());
        let count =
            unsafe { libc::proc_listpids(ALL_PIDS, 0, pids.as_mut_ptr().cast(), size as i32) };
        if count <= 0 {
            return Err(failure());
        }
        // A full buffer may have omitted processes created since the size
        // query. Retry with slack rather than silently ignoring descendants.
        if count as usize >= size {
            capacity *= 2;
            continue;
        }
        pids.truncate(count as usize / std::mem::size_of::<libc::pid_t>());
        break pids;
    };
    let mut processes = BTreeMap::new();
    for pid in pids.into_iter().filter(|pid| *pid > 0) {
        let mut info: ShortBsdInfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<ShortBsdInfo>() as i32;
        if unsafe {
            libc::proc_pidinfo(
                pid,
                SHORT_BSD_INFO,
                0,
                (&mut info as *mut ShortBsdInfo).cast(),
                size,
            )
        } != size
            || info.pid != pid as u32
        {
            // Processes can disappear during enumeration or belong to another
            // user. Tree::sample fails closed if its own child is unavailable.
            continue;
        }
        // rusage_info_v2 begins with UUID then u64 fields. Indices 8/9/10
        // are RSS, physical footprint and process start mach absolute time.
        let mut usage = [0u64; 32];
        if unsafe { proc_pid_rusage(pid, 2, usage.as_mut_ptr().cast()) } != 0 {
            continue;
        }
        processes.insert(
            pid as u32,
            Process {
                parent: info.parent,
                group: info.group,
                identity: usage[10].to_string(),
                bytes: usage[8].max(usage[9]),
            },
        );
    }
    Ok(processes)
}
#[cfg(windows)]
fn snapshot() -> Result<BTreeMap<u32, Process>, Failure> {
    windows_job::snapshot()
}

struct Tree {
    root: u32,
    known: BTreeMap<u32, String>,
}
impl Tree {
    fn sample(&mut self) -> Result<u64, Failure> {
        let all = snapshot()?;
        if self.known.is_empty() {
            // Called before the first try_wait: the owned Child still reserves
            // its PID, so current members of its dedicated group cannot belong
            // to a later process group that reused that ID.
            for (pid, p) in &all {
                if *pid == self.root || p.group == self.root {
                    self.known.insert(*pid, p.identity.clone());
                }
            }
            if self.known.is_empty() {
                return Err(Failure::new(
                    FailureKind::Monitoring,
                    "转换进程已结束或无法读取其资源信息",
                ));
            }
        }
        self.known
            .retain(|pid, identity| all.get(pid).is_some_and(|p| &p.identity == identity));
        loop {
            let children: Vec<_> = all
                .iter()
                .filter(|(pid, p)| {
                    !self.known.contains_key(pid) && self.known.contains_key(&p.parent)
                })
                .map(|(pid, p)| (*pid, p.identity.clone()))
                .collect();
            if children.is_empty() {
                break;
            }
            self.known.extend(children);
        }
        Ok(self
            .known
            .keys()
            .filter_map(|pid| all.get(pid))
            .map(|p| p.bytes)
            .sum())
    }
    fn kill(&self) {
        #[cfg(unix)]
        if let Ok(all) = snapshot() {
            let owned: Vec<_> = all
                .iter()
                .filter(|(pid, p)| {
                    self.known
                        .get(pid)
                        .is_some_and(|identity| identity == &p.identity)
                })
                .collect();
            // Only signal a group still anchored by an observed birth identity.
            if owned.iter().any(|(_, p)| p.group == self.root) {
                unsafe {
                    libc::kill(-(self.root as i32), libc::SIGKILL);
                }
            }
            for (pid, _) in owned {
                unsafe {
                    libc::kill(*pid as i32, libc::SIGKILL);
                }
            }
        }
    }
}

struct Guard {
    child: Child,
    tree: Tree,
    armed: bool,
    reaped: bool,
    #[cfg(windows)]
    _job: windows_job::Job,
}
impl Guard {
    fn kill_unreaped_group(&self) {
        // An unreaped owned child reserves its PID. Its dedicated process group
        // cannot have been recycled, including after the direct child exited.
        #[cfg(unix)]
        if !self.reaped {
            unsafe {
                libc::kill(-(self.tree.root as i32), libc::SIGKILL);
            }
        }
    }
    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        #[cfg(unix)]
        unsafe {
            let mut info: libc::siginfo_t = std::mem::zeroed();
            if libc::waitid(
                libc::P_PID,
                self.child.id() as libc::id_t,
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            ) != 0
            {
                return Err(std::io::Error::last_os_error());
            }
            if info.si_pid() == 0 {
                return Ok(None);
            }
            // Kill same-group descendants *before* releasing the PID/group
            // anchor. This catches children born between monitoring samples.
            self.kill_unreaped_group();
        }
        let status = self.child.try_wait()?;
        if status.is_some() {
            self.reaped = true;
        }
        Ok(status)
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        if self.armed {
            #[cfg(windows)]
            self._job.terminate();
            self.kill_unreaped_group();
            self.tree.kill();
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

pub fn run(
    command: &mut Command,
    limits: Limits,
    cancel: &dyn Fn() -> bool,
    mut progress: impl FnMut(&[u8]),
) -> Result<(), Failure> {
    if cancel() {
        return Err(Failure::new(FailureKind::Cancelled, "已取消下载或停止转换"));
    }
    if limits.timeout.is_zero() {
        return Err(Failure::new(
            FailureKind::Timeout,
            "Docling 超过任务时间限制",
        ));
    }
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
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    let child = command
        .spawn()
        .map_err(|e| Failure::new(FailureKind::Spawn, format!("无法启动 Docling 组件：{e}")))?;
    #[cfg(windows)]
    let job = match windows_job::Job::new(&child, limits.memory_bytes) {
        Ok(job) => job,
        Err(error) => {
            let mut child = child;
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    let pid = child.id();
    let mut guard = Guard {
        child,
        tree: Tree {
            root: pid,
            known: BTreeMap::new(),
        },
        armed: true,
        reaped: false,
        #[cfg(windows)]
        _job: job,
    };
    let _ = guard.tree.sample();
    let output = Arc::new(Mutex::new(Output::default()));
    let stdout = guard.child.stdout.take().unwrap();
    let stderr = guard.child.stderr.take().unwrap();
    let stop_readers = Arc::new(AtomicBool::new(false));
    let a = output.clone();
    let b = output.clone();
    let a_stop = stop_readers.clone();
    let b_stop = stop_readers.clone();
    let reader_a = std::thread::spawn(move || read_output(stdout, a, a_stop));
    let reader_b = std::thread::spawn(move || read_output(stderr, b, b_stop));
    let start = Instant::now();
    let result = loop {
        let pending: Vec<_> = output.lock().unwrap().pending.drain(..).collect();
        for line in pending {
            progress(&line);
        }
        if cancel() {
            break Err(Failure::new(FailureKind::Cancelled, "已取消下载或停止转换"));
        }
        if start.elapsed() >= limits.timeout {
            break Err(Failure::new(
                FailureKind::Timeout,
                "Docling 超过任务时间限制，已终止进程树",
            ));
        }
        if let Some(error) = output.lock().unwrap().error.take() {
            break Err(Failure::new(
                FailureKind::Io,
                format!("无法读取 Docling 输出：{error}"),
            ));
        }
        match guard.try_wait() {
            Ok(Some(status)) => {
                let kind = match status.code() {
                    None => FailureKind::Signal,
                    Some(code)
                        if code == 75
                            || matches!(code as u32, 0xc0000017 | 0xc000009a | 0xc000012d) =>
                    {
                        FailureKind::Memory
                    }
                    Some(76) => FailureKind::Compatibility,
                    _ => FailureKind::Exit,
                };
                break if status.success() {
                    Ok(())
                } else {
                    Err(Failure::new(kind, format!("Docling 执行失败（{status}）")))
                };
            }
            Err(error) => break Err(Failure::new(FailureKind::Io, error.to_string())),
            Ok(None) => {}
        }
        if let Some(limit) = limits.memory_bytes {
            match guard.tree.sample() {
                Ok(bytes) if bytes > limit => {
                    break Err(Failure::new(
                        FailureKind::Memory,
                        format!(
                            "Docling 进程树内存超过 {} MiB 限制，已终止",
                            limit / 1024 / 1024
                        ),
                    ))
                }
                Ok(_) => {}
                // A short-lived child may exit between try_wait and sampling.
                Err(error) => match guard.try_wait() {
                    Ok(Some(status)) if status.success() => break Ok(()),
                    _ => break Err(error),
                },
            }
        }
        std::thread::sleep(POLL);
    };
    // Stop inherited writers too, even when the direct child exited first.
    #[cfg(windows)]
    guard._job.terminate();
    guard.kill_unreaped_group();
    guard.tree.kill();
    let _ = guard.child.kill();
    let _ = guard.child.wait();
    guard.armed = false;
    stop_readers.store(true, Ordering::Release);
    let _ = reader_a.join();
    let _ = reader_b.join();
    let mut output = output.lock().unwrap();
    for line in output.pending.drain(..) {
        progress(&line);
    }
    result.map_err(|mut error| {
        let tail = output.text();
        if !tail.is_empty() {
            error.message.push_str(&format!("：{tail}"));
        }
        error
    })
}

#[cfg(windows)]
mod windows_job {
    use super::*;
    use std::os::windows::io::AsRawHandle;
    type Handle = *mut std::ffi::c_void;
    #[repr(C)]
    struct Basic {
        per_process: i64,
        per_job: i64,
        flags: u32,
        minimum: usize,
        maximum: usize,
        active: u32,
        affinity: usize,
        priority: u32,
        scheduling: u32,
    }
    #[repr(C)]
    struct Io {
        read_ops: u64,
        write_ops: u64,
        other_ops: u64,
        read_bytes: u64,
        write_bytes: u64,
        other_bytes: u64,
    }
    #[repr(C)]
    struct Extended {
        basic: Basic,
        io: Io,
        process_memory: usize,
        job_memory: usize,
        peak_process: usize,
        peak_job: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(attributes: Handle, name: *const u16) -> Handle;
        fn SetInformationJobObject(
            job: Handle,
            class: i32,
            information: Handle,
            length: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
        fn TerminateJobObject(job: Handle, exit: u32) -> i32;
        fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> Handle;
        fn Thread32First(snapshot: Handle, entry: *mut ThreadEntry) -> i32;
        fn Thread32Next(snapshot: Handle, entry: *mut ThreadEntry) -> i32;
        fn OpenThread(access: u32, inherit: i32, id: u32) -> Handle;
        fn ResumeThread(thread: Handle) -> u32;
        fn Process32FirstW(snapshot: Handle, entry: *mut ProcessEntry) -> i32;
        fn Process32NextW(snapshot: Handle, entry: *mut ProcessEntry) -> i32;
        fn OpenProcess(access: u32, inherit: i32, id: u32) -> Handle;
        fn GetProcessTimes(
            process: Handle,
            creation: *mut u64,
            exit: *mut u64,
            kernel: *mut u64,
            user: *mut u64,
        ) -> i32;
    }
    #[repr(C)]
    struct ThreadEntry {
        size: u32,
        usage: u32,
        id: u32,
        owner: u32,
        priority: i32,
        delta: i32,
        flags: u32,
    }
    #[repr(C)]
    struct ProcessEntry {
        size: u32,
        usage: u32,
        id: u32,
        heap: usize,
        module: u32,
        threads: u32,
        parent: u32,
        priority: i32,
        flags: u32,
        exe: [u16; 260],
    }
    #[repr(C)]
    struct Memory {
        size: u32,
        faults: u32,
        peak_working: usize,
        working: usize,
        peak_paged: usize,
        paged: usize,
        peak_nonpaged: usize,
        nonpaged: usize,
        pagefile: usize,
        peak_pagefile: usize,
        private: usize,
    }
    #[link(name = "psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(process: Handle, info: *mut Memory, size: u32) -> i32;
    }
    pub fn snapshot() -> Result<BTreeMap<u32, Process>, Failure> {
        unsafe {
            let handle = CreateToolhelp32Snapshot(2, 0);
            if handle as isize == -1 {
                return Err(Failure::new(
                    FailureKind::Monitoring,
                    "无法读取转换进程资源",
                ));
            }
            let mut result = BTreeMap::new();
            let mut entry: ProcessEntry = std::mem::zeroed();
            entry.size = std::mem::size_of::<ProcessEntry>() as u32;
            let mut next = Process32FirstW(handle, &mut entry);
            while next != 0 {
                let process = OpenProcess(0x410, 0, entry.id);
                if !process.is_null() {
                    let (mut birth, mut exit, mut kernel, mut user) = (0u64, 0u64, 0u64, 0u64);
                    let mut memory: Memory = std::mem::zeroed();
                    memory.size = std::mem::size_of::<Memory>() as u32;
                    if GetProcessTimes(process, &mut birth, &mut exit, &mut kernel, &mut user) != 0
                        && GetProcessMemoryInfo(
                            process,
                            &mut memory,
                            std::mem::size_of::<Memory>() as u32,
                        ) != 0
                    {
                        result.insert(
                            entry.id,
                            Process {
                                parent: entry.parent,
                                group: 0,
                                identity: birth.to_string(),
                                bytes: memory.working.max(memory.private) as u64,
                            },
                        );
                    }
                    CloseHandle(process);
                }
                next = Process32NextW(handle, &mut entry);
            }
            CloseHandle(handle);
            Ok(result)
        }
    }
    pub struct Job(Handle);
    impl Job {
        pub fn new(child: &Child, memory: Option<u64>) -> Result<Self, Failure> {
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
                if handle.is_null() {
                    return Err(Failure::new(
                        FailureKind::Spawn,
                        "无法创建转换进程 Job Object",
                    ));
                }
                let job = Self(handle);
                let mut info: Extended = std::mem::zeroed();
                info.basic.flags = 0x2000;
                if let Some(bytes) = memory {
                    info.basic.flags |= 0x200;
                    info.job_memory = bytes.min(usize::MAX as u64) as usize;
                }
                if SetInformationJobObject(
                    handle,
                    9,
                    (&mut info as *mut Extended).cast(),
                    std::mem::size_of::<Extended>() as u32,
                ) == 0
                    || AssignProcessToJobObject(handle, child.as_raw_handle()) == 0
                {
                    return Err(Failure::new(FailureKind::Spawn, "无法监督转换进程树"));
                }
                let snapshot = CreateToolhelp32Snapshot(4, 0);
                let mut entry: ThreadEntry = std::mem::zeroed();
                entry.size = std::mem::size_of::<ThreadEntry>() as u32;
                let mut found = false;
                if snapshot as isize != -1 {
                    let mut next = Thread32First(snapshot, &mut entry);
                    while next != 0 {
                        if entry.owner == child.id() {
                            let thread = OpenThread(2, 0, entry.id);
                            if !thread.is_null() {
                                found = ResumeThread(thread) != u32::MAX;
                                CloseHandle(thread);
                            }
                            break;
                        }
                        next = Thread32Next(snapshot, &mut entry);
                    }
                    CloseHandle(snapshot);
                }
                if !found {
                    return Err(Failure::new(FailureKind::Spawn, "无法恢复受监督的转换进程"));
                }
                Ok(job)
            }
        }
        pub fn terminate(&self) {
            unsafe {
                TerminateJobObject(self.0, 1);
            }
        }
    }
    impl Drop for Job {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

#[cfg(test)]
#[path = "process_supervisor_tests.rs"]
mod tests;
