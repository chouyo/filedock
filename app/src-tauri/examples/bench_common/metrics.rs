//! Timing and memory helpers.

use std::time::{Duration, Instant};

pub fn time<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let start = Instant::now();
    let out = f();
    (out, start.elapsed())
}

pub fn ms(d: Duration) -> f64 {
    (d.as_secs_f64() * 1000.0 * 100.0).round() / 100.0
}

pub fn median(mut values: Vec<f64>) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    values[values.len() / 2]
}

/// Peak resident set size of this process so far, in bytes.
#[cfg(unix)]
pub fn peak_rss_bytes() -> Option<u64> {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return None;
    }
    let max = usage.ru_maxrss as u64;
    // macOS reports bytes, Linux kilobytes.
    if cfg!(target_os = "macos") {
        Some(max)
    } else {
        Some(max * 1024)
    }
}

#[cfg(not(unix))]
pub fn peak_rss_bytes() -> Option<u64> {
    None
}

pub fn logical_cpus() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
}
