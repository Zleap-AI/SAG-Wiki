use crate::process_supervisor::{Failure, FailureKind};
use std::{
    sync::{Condvar, LazyLock, Mutex},
    time::Duration,
};

const GIB: u64 = 1024 * 1024 * 1024;
static GLOBAL: LazyLock<Budget> =
    LazyLock::new(|| Budget::new((physical_memory() / 4).clamp(256 * 1024 * 1024, 12 * GIB)));
pub struct Budget {
    maximum: u64,
    used: Mutex<u64>,
    changed: Condvar,
}
impl Budget {
    fn new(maximum: u64) -> Self {
        Self {
            maximum,
            used: Mutex::new(0),
            changed: Condvar::new(),
        }
    }
    fn acquire(&self, requested: u64, cancel: &dyn Fn() -> bool) -> Result<Permit<'_>, Failure> {
        let bytes = requested.min(self.maximum);
        let mut used = self.used.lock().unwrap();
        loop {
            if cancel() {
                return Err(Failure::new(FailureKind::Cancelled, "已停止等待转换资源"));
            }
            if *used + bytes <= self.maximum {
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
    (GLOBAL.maximum / (4 * GIB).min(GLOBAL.maximum)) as usize
}
pub fn acquire(cancel: &dyn Fn() -> bool) -> Result<Permit<'static>, Failure> {
    GLOBAL.acquire(4 * GIB, cancel)
}
fn physical_memory() -> u64 {
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
            return bytes;
        }
    }
    #[cfg(target_os = "linux")]
    if let Ok(mem) = std::fs::read_to_string("/proc/meminfo") {
        if let Some(kib) = mem.lines().find_map(|line| {
            line.strip_prefix("MemTotal:")
                .and_then(|s| s.split_whitespace().next()?.parse::<u64>().ok())
        }) {
            return kib * 1024;
        }
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
            return status.physical;
        }
    }
    // Unknown machines get one conservative 1 GiB reservation and limit.
    4 * GIB
}
#[cfg(test)]
mod tests {
    use super::*;
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
