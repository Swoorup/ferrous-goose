//! What every generated shim runs inside: the panic catcher, the errors the
//! boundary checks raise, and the abort that ends a program the way a Goose
//! abort does.

use std::cell::{Cell, RefCell};
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Once;

/// Why a call could not go ahead.
#[derive(Debug, Clone)]
pub enum BridgeError {
    /// Bad input a fallible function reports as a failure (invalid UTF-8,
    /// a `char` out of range), and anything else aborts on.
    Fail(String),
    /// A broken rule of either language (a stale handle, overlapping
    /// arguments): always the end of the program.
    Abort(String),
}

impl BridgeError {
    pub fn message(&self) -> &str {
        match self {
            BridgeError::Fail(m) | BridgeError::Abort(m) => m,
        }
    }
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

/// An exported function, as its shim names it in messages.
pub struct Site {
    /// `ns::name`, as Goose calls it.
    pub name: &'static str,
    /// Returns `Result` or `Option`, so failures are values.
    pub fallible: bool,
    /// `#[goose::export(main_thread)]`.
    pub main_thread: bool,
}

thread_local! {
    static IN_SHIM: Cell<u32> = const { Cell::new(0) };
    static PANIC_AT: RefCell<Option<String>> = const { RefCell::new(None) };
}

static HOOK: Once = Once::new();

/// A panic inside a shim is reported by the shim, as a failure or an abort,
/// so the default hook's message would say it twice: record where it
/// happened instead. Panics anywhere else print as they always do.
fn install_hook() {
    HOOK.call_once(|| {
        let prev = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if IN_SHIM.with(|d| d.get()) > 0 {
                let at = info.location().map(|l| format!("{}:{}", l.file(), l.line()));
                PANIC_AT.with(|p| *p.borrow_mut() = at);
            } else {
                prev(info);
            }
        }));
    });
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    let msg = if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "a panic with a non-text payload".to_string()
    };
    match PANIC_AT.with(|p| p.borrow_mut().take()) {
        Some(at) => format!("{msg} ({at})"),
        None => msg,
    }
}

/// Runs a shim's body. A failure (an `Err(Fail)` from a boundary check, or a
/// panic) goes to `fail` when the function is fallible, which records it and
/// gives the failure result; otherwise, and for every `Err(Abort)`, the
/// program ends with the message.
pub fn call<R>(
    site: &Site,
    body: impl FnOnce() -> Result<R, BridgeError>,
    fail: impl FnOnce(&str) -> R,
) -> R {
    install_hook();
    if site.main_thread && !on_main_thread() {
        abort(site, "called from a thread other than the main thread, which this function is restricted to");
    }
    IN_SHIM.with(|d| d.set(d.get() + 1));
    let r = panic::catch_unwind(AssertUnwindSafe(body));
    IN_SHIM.with(|d| d.set(d.get() - 1));
    let m = match r {
        Ok(Ok(v)) => return v,
        Ok(Err(BridgeError::Fail(m))) if site.fallible => return fail(&m),
        Ok(Err(e)) => match e {
            BridgeError::Fail(m) | BridgeError::Abort(m) => m,
        },
        Err(payload) => {
            let m = format!("panic: {}", panic_message(&*payload));
            drop(payload);
            if site.fallible {
                return fail(&m);
            }
            m
        }
    };
    // An abort may jump out of this frame (a contained worker's): the
    // message goes as a leaked string, leaving nothing here to drop.
    let m: &'static str = Box::leak(m.into_boxed_str());
    abort(site, m)
}

// ---- ending the program ----

/// The Goose runtime's own abort, `gs_abort_msg(msg, len, file, line)`,
/// where the runtime hands it over (goose-patches/0004: a contained worker
/// then ends alone, as with any Goose abort).
type GsAbortMsg = unsafe extern "C" fn(*const u8, i64, *const libc::c_char, libc::c_int) -> !;

static GS_ABORT_MSG: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// The runtime functions a Goose runtime hands a library it loads, as
/// `goose_runtime_hooks` (goose-patches/0004): `gs_rt_hooks` in `goose.h`.
#[repr(C)]
pub struct RuntimeHooks {
    pub version: u32,
    pub abort_msg: Option<GsAbortMsg>,
    pub at_end: Option<unsafe extern "C" fn(extern "C" fn())>,
    /// Version 2 (goose-patches/0008): an export fn to call back, by name
    /// and signature; and whether this thread runs the main program.
    pub reentry_lookup: Option<
        unsafe extern "C" fn(*const libc::c_char, *const libc::c_char, *mut *const libc::c_char) -> *mut libc::c_void,
    >,
    pub on_program_thread: Option<unsafe extern "C" fn() -> libc::c_int>,
}

/// Called by a patched Goose runtime when it opens this library, or by a
/// statically linked program at start.
///
/// # Safety
/// `hooks` must point at a valid `RuntimeHooks` for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn goose_runtime_hooks(hooks: *const RuntimeHooks) {
    if hooks.is_null() {
        return;
    }
    let h = &*hooks;
    if h.version >= 1 {
        if let Some(f) = h.abort_msg {
            GS_ABORT_MSG.store(f as *mut (), Ordering::Release);
        }
        if let Some(at_end) = h.at_end {
            at_end(crate::goose_at_end);
        }
    }
    if h.version >= 2 {
        crate::reentry::set_hooks(h.reentry_lookup, h.on_program_thread);
    }
}

/// Ends the program as a Goose abort does: `goose runtime error: <msg>
/// (<where>)` on stderr, exit code 1, or only the contained worker where
/// the runtime gave us its abort.
pub fn abort(site: &Site, msg: &str) -> ! {
    abort_at(site.name, msg)
}

pub fn abort_at(name: &str, msg: &str) -> ! {
    let f = GS_ABORT_MSG.load(Ordering::Acquire);
    if !f.is_null() {
        // The runtime's abort may jump out of a contained worker, past
        // this frame and the shim's: nothing there may need dropping, so
        // what it is given is leaked (it ends a thread or the program).
        let file: &'static std::ffi::CStr =
            Box::leak(std::ffi::CString::new(format!("rust {name}")).unwrap_or_default().into_boxed_c_str());
        let msg: &'static str = Box::leak(msg.to_string().into_boxed_str());
        let f: GsAbortMsg = unsafe { core::mem::transmute(f) };
        unsafe { f(msg.as_ptr(), msg.len() as i64, file.as_ptr(), 0) }
    }
    use std::io::Write;
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "goose runtime error: {msg} (rust {name})");
    let _ = err.flush();
    std::process::exit(1)
}

#[cfg(any(test, not(any(target_os = "macos", target_os = "linux"))))]
struct ProgramThread(std::sync::OnceLock<std::thread::ThreadId>);

#[cfg(any(test, not(any(target_os = "macos", target_os = "linux"))))]
impl ProgramThread {
    const fn new() -> Self { Self(std::sync::OnceLock::new()) }
    fn register(&self) { let _ = self.0.set(std::thread::current().id()); }
    fn is_current(&self) -> bool {
        self.0.get().is_some_and(|id| *id == std::thread::current().id())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
static PROGRAM_THREAD: ProgramThread = ProgramThread::new();

// Generated modules call the ABI entry during initialization, before workers
// can query main_thread. A query must never register the querying worker.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn register_program_thread() { PROGRAM_THREAD.register(); }

/// Whether this is the main thread (recorded at module startup on Windows).
pub fn on_main_thread() -> bool {
    #[cfg(target_os = "macos")]
    {
        extern "C" {
            fn pthread_main_np() -> libc::c_int;
        }
        unsafe { pthread_main_np() == 1 }
    }
    #[cfg(target_os = "linux")]
    {
        unsafe { libc::syscall(libc::SYS_gettid) as libc::pid_t == libc::getpid() }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        PROGRAM_THREAD.is_current()
    }
}

#[cfg(test)]
mod tests {
    use super::ProgramThread;
    use std::sync::Arc;

    #[test]
    fn workers_cannot_claim_the_program_thread() {
        let tracker = Arc::new(ProgramThread::new());
        let worker = tracker.clone();
        assert!(!std::thread::spawn(move || worker.is_current()).join().unwrap());
        tracker.register();
        assert!(tracker.is_current());
        let worker = tracker.clone();
        assert!(!std::thread::spawn(move || {
            worker.register();
            worker.is_current()
        }).join().unwrap());
        assert!(tracker.is_current());
    }
}
