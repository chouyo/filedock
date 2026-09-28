//! Minimal binding to Spotlight's `MDQuery` (CoreServices), synchronous only.

use std::ffi::c_void;
use std::path::Path;

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{Boolean, CFAllocatorRef, CFIndex, CFOptionFlags, CFRelease, CFTypeRef, TCFType};
use core_foundation::date::{CFDate, CFDateRef};
use core_foundation::number::{CFNumber, CFNumberRef};
use core_foundation::string::{CFString, CFStringRef};

use crate::bench_common::BResult;

type MDQueryRef = *mut c_void;
type MDItemRef = *const c_void;

const K_MD_QUERY_SYNCHRONOUS: CFOptionFlags = 1;
/// Seconds between the Unix epoch and the Core Foundation epoch (2001-01-01).
const CF_EPOCH_OFFSET: f64 = 978_307_200.0;

#[link(name = "CoreServices", kind = "framework")]
extern "C" {
    fn MDQueryCreate(
        allocator: CFAllocatorRef,
        query: CFStringRef,
        value_list_attrs: CFArrayRef,
        sorting_attrs: CFArrayRef,
    ) -> MDQueryRef;
    fn MDQuerySetSearchScope(query: MDQueryRef, scope_directories: CFArrayRef, scope_options: u32);
    fn MDQueryExecute(query: MDQueryRef, option_flags: CFOptionFlags) -> Boolean;
    fn MDQueryStop(query: MDQueryRef);
    fn MDQueryGetResultCount(query: MDQueryRef) -> CFIndex;
    fn MDQueryGetResultAtIndex(query: MDQueryRef, idx: CFIndex) -> *const c_void;
    fn MDQueryGetAttributeValueOfResultAtIndex(query: MDQueryRef, name: CFStringRef, idx: CFIndex) -> *mut c_void;
    fn MDItemCopyAttribute(item: MDItemRef, name: CFStringRef) -> CFTypeRef;
}

pub struct Hit {
    pub path: String,
    pub size: u64,
    pub modified_ms: i64,
}

/// Owned query, stopped and released on drop.
struct Query(MDQueryRef);

impl Drop for Query {
    fn drop(&mut self) {
        unsafe {
            MDQueryStop(self.0);
            CFRelease(self.0 as CFTypeRef);
        }
    }
}

pub struct Attrs {
    path: CFString,
    size: CFString,
    modified: CFString,
}

impl Attrs {
    pub fn new() -> Self {
        Attrs {
            path: CFString::from_static_string("kMDItemPath"),
            size: CFString::from_static_string("kMDItemFSSize"),
            modified: CFString::from_static_string("kMDItemFSContentChangeDate"),
        }
    }
}

/// Runs `query` limited to `scopes` and returns the result count, plus the
/// hits when `fetch` is set.
pub fn run(query: &str, scopes: &[&Path], fetch: bool, attrs: &Attrs) -> BResult<(usize, Vec<Hit>)> {
    let q = CFString::new(query);
    let value_attrs = CFArray::from_CFTypes(&[attrs.path.clone(), attrs.size.clone(), attrs.modified.clone()]);
    let scope_strings: Vec<CFString> = scopes.iter().map(|p| CFString::new(&p.to_string_lossy())).collect();
    let scope = CFArray::from_CFTypes(&scope_strings);

    let raw = unsafe {
        MDQueryCreate(
            std::ptr::null(),
            q.as_concrete_TypeRef(),
            value_attrs.as_concrete_TypeRef(),
            std::ptr::null(),
        )
    };
    if raw.is_null() {
        return Err(format!("invalid Spotlight query: {query}").into());
    }
    let query_ref = Query(raw);
    unsafe {
        MDQuerySetSearchScope(query_ref.0, scope.as_concrete_TypeRef(), 0);
        if MDQueryExecute(query_ref.0, K_MD_QUERY_SYNCHRONOUS) == 0 {
            return Err("MDQueryExecute failed".into());
        }
    }
    let count = unsafe { MDQueryGetResultCount(query_ref.0) } as usize;
    if !fetch {
        return Ok((count, Vec::new()));
    }

    let mut hits = Vec::with_capacity(count);
    for i in 0..count as CFIndex {
        let get = |name: &CFString| -> (*const c_void, bool) {
            // Fast path: values gathered with the query. Fallback: ask the item
            // (returns an owned value the caller must release).
            let v = unsafe { MDQueryGetAttributeValueOfResultAtIndex(query_ref.0, name.as_concrete_TypeRef(), i) };
            if !v.is_null() {
                return (v as *const c_void, false);
            }
            let item = unsafe { MDQueryGetResultAtIndex(query_ref.0, i) };
            if item.is_null() {
                return (std::ptr::null(), false);
            }
            (unsafe { MDItemCopyAttribute(item, name.as_concrete_TypeRef()) }, true)
        };
        let (p, owned_p) = get(&attrs.path);
        if p.is_null() {
            continue;
        }
        let path = unsafe { CFString::wrap_under_get_rule(p as CFStringRef) }.to_string();
        let (s, owned_s) = get(&attrs.size);
        let size = if s.is_null() {
            0
        } else {
            unsafe { CFNumber::wrap_under_get_rule(s as CFNumberRef) }.to_i64().unwrap_or(0) as u64
        };
        let (m, owned_m) = get(&attrs.modified);
        let modified_ms = if m.is_null() {
            0
        } else {
            let secs = unsafe { CFDate::wrap_under_get_rule(m as CFDateRef) }.abs_time();
            ((secs + CF_EPOCH_OFFSET) * 1000.0) as i64
        };
        for (v, owned) in [(p, owned_p), (s, owned_s), (m, owned_m)] {
            if owned && !v.is_null() {
                unsafe { CFRelease(v) };
            }
        }
        hits.push(Hit { path, size, modified_ms });
    }
    Ok((count, hits))
}
