//! macOS FSEvents history (level 1 of the proposal's catch-up).
//!
//! FSEvents keeps a per-volume record of changes that survives restarts and
//! is readable by ordinary users. While the app runs, the watcher saves the
//! current event id per root (see `watch.rs`); at the next start the history
//! since that id is replayed, so only paths that changed meanwhile are looked
//! at. The volume's event database UUID is saved with the id: if it changes
//! (database reset, different disk), the ids are meaningless and the root
//! falls back to level 2.

use std::collections::HashSet;
use std::ffi::{c_char, c_void, CStr, CString, OsStr};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use fsevent_sys as fse;
use fsevent_sys::core_foundation as cf;

#[link(name = "CoreServices", kind = "framework")]
extern "C" {
    fn FSEventsCopyUUIDForDevice(dev: i32) -> cf::CFRef;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFUUIDCreateString(alloc: cf::CFAllocatorRef, uuid: cf::CFRef) -> cf::CFStringRef;
    fn CFRunLoopRunInMode(mode: cf::CFStringRef, seconds: f64, return_after_source_handled: u8) -> i32;
}

/// Events that mean changes were not recorded individually below their path.
const LOST: u32 = fse::kFSEventStreamEventFlagMustScanSubDirs
    | fse::kFSEventStreamEventFlagUserDropped
    | fse::kFSEventStreamEventFlagKernelDropped
    | fse::kFSEventStreamEventFlagEventIdsWrapped
    | fse::kFSEventStreamEventFlagRootChanged
    | fse::kFSEventStreamEventFlagMount
    | fse::kFSEventStreamEventFlagUnmount;

/// The latest event id on this system.
pub fn current_event_id() -> u64 {
    unsafe { fse::FSEventsGetCurrentEventId() }
}

/// UUID of the event database of the volume holding `path`; `None` when the
/// volume keeps no persistent history (some external or network volumes).
pub fn journal_id(path: &Path) -> Option<String> {
    let dev = std::fs::metadata(path).ok()?.dev() as i32;
    unsafe {
        let uuid = FSEventsCopyUUIDForDevice(dev);
        if uuid.is_null() {
            return None;
        }
        let s = CFUUIDCreateString(cf::kCFAllocatorDefault, uuid);
        cf::CFRelease(uuid);
        if s.is_null() {
            return None;
        }
        let mut buf = [0 as c_char; 64];
        let ok = cf::CFStringGetCString(s, buf.as_mut_ptr(), buf.len() as cf::CFIndex, cf::kCFStringEncodingUTF8);
        cf::CFRelease(s);
        ok.then(|| CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned())
    }
}

/// Changes recorded since an event id.
#[derive(Debug, Default)]
pub struct Replay {
    /// Paths that changed (files or directories).
    pub paths: HashSet<PathBuf>,
    /// Changes below these paths were not recorded individually.
    pub lost: Vec<PathBuf>,
    /// The whole history was read. When false, nothing can be trusted.
    pub complete: bool,
}

struct Collector {
    replay: Replay,
    done: bool,
}

extern "C" fn callback(
    _stream: fse::FSEventStreamRef,
    info: *mut c_void,
    count: usize,
    paths: *mut c_void,
    flags: *const fse::FSEventStreamEventFlags,
    _ids: *const fse::FSEventStreamEventId,
) {
    // SAFETY: `info` is the `Collector` owned by `replay`, which outlives the
    // stream; the arrays hold `count` entries (no `UseCFTypes`, so paths are
    // C strings).
    unsafe {
        let c = &mut *(info as *mut Collector);
        let paths = paths as *const *const c_char;
        for i in 0..count {
            let flag = *flags.add(i);
            let path = PathBuf::from(OsStr::from_bytes(CStr::from_ptr(*paths.add(i)).to_bytes()));
            if flag & fse::kFSEventStreamEventFlagHistoryDone != 0 {
                c.done = true;
                cf::CFRunLoopStop(cf::CFRunLoopGetCurrent());
            } else if flag & LOST != 0 {
                c.replay.lost.push(path);
            } else {
                c.replay.paths.insert(path);
            }
        }
    }
}

/// Reads the history for `dirs` (recursively) after event `since`. Gives up
/// (incomplete) after `timeout` or more than `max_paths` changed paths, when
/// listing directories is cheaper anyway.
pub fn replay(dirs: &[PathBuf], since: u64, timeout: Duration, max_paths: usize) -> Replay {
    let mut collector = Box::new(Collector {
        replay: Replay::default(),
        done: false,
    });
    unsafe {
        let arr = cf::CFArrayCreateMutable(cf::kCFAllocatorDefault, 0, &cf::kCFTypeArrayCallBacks);
        for d in dirs {
            let Ok(c) = CString::new(d.as_os_str().as_bytes()) else {
                continue;
            };
            let s = cf::CFStringCreateWithCString(cf::kCFAllocatorDefault, c.as_ptr(), cf::kCFStringEncodingUTF8);
            cf::CFArrayAppendValue(arr, s);
            cf::CFRelease(s);
        }
        let context = fse::FSEventStreamContext {
            version: 0,
            info: &mut *collector as *mut Collector as *mut c_void,
            retain: None,
            release: None,
            copy_description: None,
        };
        let stream = fse::FSEventStreamCreate(
            cf::kCFAllocatorDefault,
            callback,
            &context,
            arr,
            since,
            0.0,
            fse::kFSEventStreamCreateFlagFileEvents | fse::kFSEventStreamCreateFlagNoDefer,
        );
        cf::CFRelease(arr);
        if stream.is_null() {
            return collector.replay;
        }
        let run_loop = cf::CFRunLoopGetCurrent();
        fse::FSEventStreamScheduleWithRunLoop(stream, run_loop, cf::kCFRunLoopDefaultMode);
        if fse::FSEventStreamStart(stream) != 0 {
            let deadline = Instant::now() + timeout;
            while !collector.done
                && collector.replay.paths.len() <= max_paths
                && Instant::now() < deadline
            {
                CFRunLoopRunInMode(cf::kCFRunLoopDefaultMode, 0.25, 1);
            }
            fse::FSEventStreamStop(stream);
        }
        fse::FSEventStreamInvalidate(stream);
        fse::FSEventStreamRelease(stream);
    }
    let mut replay = collector.replay;
    replay.complete = collector.done && replay.paths.len() <= max_paths;
    replay
}
