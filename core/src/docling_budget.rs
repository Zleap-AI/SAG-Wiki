use crate::process_supervisor::{Failure, FailureKind};
use std::{
    sync::{Condvar, LazyLock, Mutex},
    time::Duration,
};

const GIB: u64 = 1024 * 1024 * 1024;
static GLOBAL: LazyLock<Result<Budget, String>> =
    LazyLock::new(|| physical_memory().map(|physical| Budget::new(budget_for(physical))));
/// Reserve half of physical memory for the system and other applications.
fn budget_for(physical: u64) -> u64 {
    physical / 2
}
pub struct Budget {
    maximum: u64,
    job_bytes: u64,
    used: Mutex<u64>,
    changed: Condvar,
}
impl Budget {
    fn new(maximum: u64) -> Self {
        Self {
            maximum,
            // A quarter of physical memory, with the existing OCR minimum
            // bounded by the total budget on small machines.
            job_bytes: (maximum / 2).max(4 * GIB).min(maximum),
            used: Mutex::new(0),
            changed: Condvar::new(),
        }
    }
    fn acquire(&self, requested: u64, cancel: &dyn Fn() -> bool) -> Result<Permit<'_>, Failure> {
        self.acquire_with_limit(requested, self.maximum, cancel)
    }
    fn acquire_with_limit(&self, requested: u64, maximum: u64, cancel: &dyn Fn() -> bool) -> Result<Permit<'_>, Failure> {
        let bytes = requested.min(maximum);
        let mut used = self.used.lock().unwrap();
        loop {
            if cancel() {
                return Err(Failure::new(FailureKind::Cancelled, "已停止等待转换资源"));
            }
            if *used + bytes <= maximum {
                *used += bytes;
                return Ok(Permit {
                    budget: self,
                    bytes,
                });
            }
            used = self
                .changed
                .wait_timeout(used, Duration::from_millis(50))
                .unwrap()
                .0;
        }
    }
}
pub struct Permit<'a> {
    budget: &'a Budget,
    pub bytes: u64,
}
impl Drop for Permit<'_> {
    fn drop(&mut self) {
        *self.budget.used.lock().unwrap() -= self.bytes;
        self.budget.changed.notify_all();
    }
}
#[cfg(test)]
pub(crate) fn parallelism() -> usize {
    let budget = GLOBAL.as_ref().expect("读取物理内存失败");
    (budget.maximum / budget.job_bytes) as usize
}
pub fn acquire(cancel: &dyn Fn() -> bool) -> Result<Permit<'static>, Failure> {
    let budget = GLOBAL
        .as_ref()
        .map_err(|error| Failure::new(FailureKind::Monitoring, error.clone()))?;
    budget.acquire(budget.job_bytes, cancel)
}
fn configured_limits(physical: u64, runtime: &sag_protocol::runtime::RuntimeSettings) -> (u64, u64, usize) {
    let maximum = (physical / 100 * runtime.conversion_memory_percent + physical % 100 * runtime.conversion_memory_percent / 100).max(1);
    let requested = if runtime.conversion_job_mib == 0 {
        (maximum / 2).max(4 * GIB).min(maximum)
    } else { runtime.conversion_job_mib.saturating_mul(1024 * 1024).min(maximum) };
    (maximum, requested, (maximum / requested.max(1)) as usize)
}
pub fn resource_summary(runtime: &sag_protocol::runtime::RuntimeSettings) -> Result<(u64, u64, usize), String> {
    physical_memory().map(|physical| configured_limits(physical, runtime))
}
pub fn acquire_with_runtime(runtime: &sag_protocol::runtime::RuntimeSettings, cancel: &dyn Fn() -> bool) -> Result<Permit<'static>, Failure> {
    let budget = GLOBAL.as_ref().map_err(|error| Failure::new(FailureKind::Monitoring, error.clone()))?;
    let (maximum, requested, _) = configured_limits(budget.maximum * 2, runtime);
    budget.acquire_with_limit(requested, maximum, cancel)
}
fn physical_memory() -> Result<u64, String> {
    #[cfg(target_os = "macos")]
    unsafe {
        let mut bytes = 0u64;
        let mut size = std::mem::size_of::<u64>();
        if libc::sysctlbyname(
            c"hw.memsize".as_ptr(),
            (&mut bytes as *mut u64).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        ) == 0
        {
            return Ok(bytes);
        }
        return Err(format!(
            "无法读取物理内存：{}",
            std::io::Error::last_os_error()
        ));
    }
    #[cfg(target_os = "linux")]
    {
        let mem = std::fs::read_to_string("/proc/meminfo")
            .map_err(|error| format!("无法读取物理内存：{error}"))?;
        let kib = mem
            .lines()
            .find_map(|line| {
                line.strip_prefix("MemTotal:")
                    .and_then(|s| s.split_whitespace().next()?.parse::<u64>().ok())
            })
            .ok_or_else(|| "无法读取物理内存：缺少有效的 MemTotal".to_owned())?;
        return Ok(kib * 1024);
    }
    #[cfg(windows)]
    unsafe {
        #[repr(C)]
        struct Status {
            length: u32,
            load: u32,
            physical: u64,
            available: u64,
            page: u64,
            avail_page: u64,
            virtual_size: u64,
            avail_virtual: u64,
            extended: u64,
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GlobalMemoryStatusEx(status: *mut Status) -> i32;
        }
        let mut status: Status = std::mem::zeroed();
        status.length = std::mem::size_of::<Status>() as u32;
        if GlobalMemoryStatusEx(&mut status) != 0 {
            return Ok(status.physical);
        }
        return Err(format!(
            "无法读取物理内存：{}",
            std::io::Error::last_os_error()
        ));
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    Err("当前系统不支持读取物理内存".to_owned())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configured_memory_limit_clamps_jobs_and_reports_effective_parallelism() {
        let mut runtime = sag_protocol::runtime::RuntimeSettings::default();
        runtime.conversion_memory_percent = 25;
        runtime.conversion_job_mib = 1024;
        assert_eq!(configured_limits(8 * GIB, &runtime), (2 * GIB, GIB, 2));
        runtime.conversion_job_mib = 100000;
        assert_eq!(configured_limits(8 * GIB, &runtime), (2 * GIB, 2 * GIB, 1));
    }

    #[test]
    fn conversion_limits_scale_with_physical_memory_and_preserve_ocr_minimum() {
        for (physical, job, total) in [
            (4, 2, 2),
            (8, 4, 4),
            (16, 4, 8),
            (32, 8, 16),
            (64, 16, 32),
            (128, 32, 64),
        ] {
            let budget = Budget::new(budget_for(physical * GIB));
            assert_eq!(budget.maximum, total * GIB);
            let permit = budget.acquire(budget.job_bytes, &|| false).unwrap();
            assert_eq!(permit.bytes, job * GIB);
            assert!(permit.bytes <= budget.maximum);
        }
    }
    #[test]
    fn detected_host_memory_drives_the_live_budget() {
        let physical = physical_memory().unwrap();
        assert!(physical > 0);
        let budget = GLOBAL.as_ref().unwrap();
        assert_eq!(budget.maximum, physical / 2);
        let permit = acquire(&|| false).unwrap();
        assert_eq!(permit.bytes, (physical / 4).max(4 * GIB).min(physical / 2));
    }
    #[test]
    fn waiting_admission_is_cancelled_and_drop_releases_the_reservation() {
        let budget = Budget::new(10);
        let first = budget.acquire(10, &|| false).unwrap();
        let cancelled = budget.acquire(10, &|| true).err().unwrap();
        assert_eq!(cancelled.kind, FailureKind::Cancelled);
        assert_eq!(*budget.used.lock().unwrap(), 10);
        drop(first);
        assert_eq!(budget.acquire(10, &|| false).unwrap().bytes, 10);
    }
    #[test]
    fn simultaneous_jobs_never_overbook_the_shared_budget() {
        let budget = Budget::new(10);
        let active = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..4 {
                let budget = &budget;
                let active = &active;
                scope.spawn(move || {
                    let _permit = budget.acquire(10, &|| false).unwrap();
                    assert_eq!(active.fetch_add(1, std::sync::atomic::Ordering::SeqCst), 0);
                    std::thread::sleep(Duration::from_millis(20));
                    active.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
                });
            }
        });
        assert_eq!(*budget.used.lock().unwrap(), 0);
    }
}
