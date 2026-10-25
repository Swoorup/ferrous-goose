//! Calling the Goose program back from Rust while it waits on a call
//! (goose-patches/0008).
//!
//! An exported function that takes `&goose::Reentry` is declared `reentrant`
//! in the generated module: the Goose code calling it publishes every cached
//! stack top first, so a call back starts above all live data. The token is
//! made only on the program's main thread and only for a function without
//! builders (which would be live meanwhile), and `goose::callbacks!` turns a
//! declared signature into a function that finds the Goose export fn by name
//! and signature through the runtime and calls it.

use crate::meta::Ty;
use crate::shim::BridgeError;
use crate::value::Value;
use std::ffi::{CStr, CString};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicPtr, Ordering};

type Lookup = unsafe extern "C" fn(*const libc::c_char, *const libc::c_char, *mut *const libc::c_char) -> *mut libc::c_void;
type OnThread = unsafe extern "C" fn() -> libc::c_int;

static LOOKUP: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());
static ON_THREAD: AtomicPtr<()> = AtomicPtr::new(std::ptr::null_mut());

pub(crate) fn set_hooks(lookup: Option<Lookup>, on_thread: Option<OnThread>) {
    if let Some(l) = lookup {
        LOOKUP.store(l as *mut (), Ordering::Release);
    }
    if let Some(t) = on_thread {
        ON_THREAD.store(t as *mut (), Ordering::Release);
    }
}

/// Permission to call the Goose program back, for the length of one call
/// into Rust. Neither Send nor Sync: calls back happen on the thread Goose
/// called from.
pub struct Reentry<'call> {
    _call: PhantomData<&'call ()>,
    _not_send: PhantomData<*const ()>,
}

impl Reentry<'_> {
    /// The token for the call a shim is running. Fails where the runtime
    /// handed over no way back (a compiler without goose-patches/0008) or
    /// the call came from a worker, whose stacks the export wrappers do not
    /// use.
    #[doc(hidden)]
    pub fn enter() -> Result<Reentry<'static>, BridgeError> {
        let l = LOOKUP.load(Ordering::Acquire);
        if l.is_null() {
            return Err(BridgeError::Abort(
                "this function calls the Goose program back, which needs a runtime with reentrant (goose-patches/0008)".into(),
            ));
        }
        let t = ON_THREAD.load(Ordering::Acquire);
        if !t.is_null() {
            let t: OnThread = unsafe { std::mem::transmute(t) };
            if unsafe { t() } == 0 {
                return Err(BridgeError::Abort(
                    "this function calls the Goose program back, which only the main program's thread can".into(),
                ));
            }
        }
        Ok(Reentry { _call: PhantomData, _not_send: PhantomData })
    }
}

/// Why a call back could not be made.
#[derive(Debug, Clone)]
pub struct ReentryError(pub String);

impl std::fmt::Display for ReentryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ReentryError {}

/// Where a declared call back keeps the export fn's wrapper once found.
pub struct ReentrySlot {
    name: &'static str,
    found: AtomicPtr<()>,
}

impl ReentrySlot {
    pub const fn new(name: &'static str) -> ReentrySlot {
        ReentrySlot { name, found: AtomicPtr::new(std::ptr::null_mut()) }
    }

    /// The wrapper of the export fn `name` whose Goose signature is what
    /// `sig` makes, looked up the first time.
    pub fn get(&self, _token: &Reentry<'_>, sig: impl FnOnce() -> String) -> Result<*const (), ReentryError> {
        let f = self.found.load(Ordering::Acquire);
        if !f.is_null() {
            return Ok(f);
        }
        let l = LOOKUP.load(Ordering::Acquire);
        if l.is_null() {
            return Err(ReentryError("no runtime to call back into".into()));
        }
        let l: Lookup = unsafe { std::mem::transmute(l) };
        let name = CString::new(self.name).unwrap_or_default();
        let sig = CString::new(sig()).unwrap_or_default();
        let mut why: *const libc::c_char = std::ptr::null();
        let p = unsafe { l(name.as_ptr(), sig.as_ptr(), &mut why) };
        if p.is_null() {
            let msg = if why.is_null() {
                format!("no export fn {}", self.name)
            } else {
                unsafe { CStr::from_ptr(why) }.to_string_lossy().into_owned()
            };
            return Err(ReentryError(msg));
        }
        self.found.store(p as *mut (), Ordering::Release);
        Ok(p as *const ())
    }
}

/// A type as Goose writes it in a signature, its structs under the
/// namespace `ns` the bindings declared them in.
pub fn ty_text(t: &Ty, ns: &str) -> String {
    match t {
        Ty::Prim(p) => p.to_string(),
        Ty::Named(n) => format!("{ns}::{n}"),
        Ty::Array(e, n) => format!("{}[{n}]", ty_text(e, ns)),
    }
}

/// `const T&`, `T&`, `const T[:]`, `T[:]` for a value type.
pub fn ref_text<T: Value>(ns: &str, mutable: bool, slice: bool) -> String {
    let t = ty_text(&T::goose_ty(), ns);
    let c = if mutable { "" } else { "const " };
    if slice {
        format!("{c}{t}[:]")
    } else {
        format!("{c}{t}&")
    }
}
