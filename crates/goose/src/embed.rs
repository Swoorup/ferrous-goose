//! Rust using Goose: the runtime token every generated wrapper takes, and
//! what `goose::include!` expands to.

use std::marker::PhantomData;
use std::sync::OnceLock;
use std::thread::ThreadId;

/// Proof that `goose_init` ran on this thread. Neither Send nor Sync, so
/// the Rust compiler keeps every Goose call on the thread that initialized
/// the program (spec §7.11).
pub struct Runtime {
    _not_send: PhantomData<*const ()>,
}

/// The `goose_init` of the Goose program built into this binary, which the
/// module `goose::include!` brings in registers.
pub struct GooseInit {
    pub program: &'static str,
    pub init: unsafe extern "C" fn(argc: libc::c_int, argv: *mut *mut libc::c_char),
}

inventory::collect!(GooseInit);

static INIT_THREAD: OnceLock<ThreadId> = OnceLock::new();

/// Why `Runtime::init` failed.
#[derive(Debug)]
pub enum InitError {
    /// No module from `goose::include!` is linked in.
    NoProgram,
    /// Two Goose programs in one binary: whole-program compilation allows one.
    TwoPrograms(&'static str, &'static str),
    /// The runtime was initialized on another thread.
    OtherThread,
}

impl std::fmt::Display for InitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InitError::NoProgram => write!(f, "no Goose program is linked in: use goose::include! with goose_build"),
            InitError::TwoPrograms(a, b) => write!(f, "two Goose programs are linked in ({a} and {b}); a binary holds one"),
            InitError::OtherThread => write!(f, "the Goose runtime was initialized on another thread"),
        }
    }
}

impl std::error::Error for InitError {}

impl Runtime {
    /// Runs `goose_init` once (the runtime, the globals and the program's
    /// own `fn main()`), on this thread, and hands back the token.
    pub fn init() -> Result<Runtime, InitError> {
        let mut progs = inventory::iter::<GooseInit>.into_iter();
        let p = progs.next().ok_or(InitError::NoProgram)?;
        if let Some(q) = progs.next() {
            return Err(InitError::TwoPrograms(p.program, q.program));
        }
        let me = std::thread::current().id();
        let first = *INIT_THREAD.get_or_init(|| me);
        if first != me {
            return Err(InitError::OtherThread);
        }
        // goose_init keeps argv; a program name is all it is given.
        static ARGV0: &[u8] = b"rust-host\0";
        let mut argv = [ARGV0.as_ptr() as *mut libc::c_char, core::ptr::null_mut()];
        unsafe { (p.init)(1, argv.as_mut_ptr()) };
        Ok(Runtime { _not_send: PhantomData })
    }
}

/// Brings in the module `goose_build` generated for a Goose program:
/// `goose::include!(kernels);` declares `mod kernels` from
/// `$OUT_DIR/kernels.rs`.
#[macro_export]
macro_rules! include {
    ($name:ident) => {
        #[allow(dead_code, non_snake_case, non_camel_case_types, clippy::all)]
        pub mod $name {
            ::core::include!(::core::concat!(::core::env!("OUT_DIR"), "/", ::core::stringify!($name), ".rs"));
        }
    };
}
