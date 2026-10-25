//! Handles: Rust objects in a slot table, named in Goose by an 8-byte slot
//! and generation. Every use checks the generation, so a released handle is
//! a clean abort with a message, never freed memory.

use crate::abi::Handle;
use crate::shim::BridgeError;
use parking_lot::lock_api::{ArcRwLockReadGuard, ArcRwLockWriteGuard};
use parking_lot::{Mutex, RawRwLock, RwLock};
use std::cell::RefCell;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::thread::ThreadId;

thread_local! {
    /// The objects this thread's running calls have borrowed: (table, slot,
    /// exclusively). A second borrow that would wait on one of them, which
    /// a callback in progress can ask for, aborts instead of deadlocking.
    static BORROWS: RefCell<Vec<(usize, u32, bool)>> = const { RefCell::new(Vec::new()) };
}

fn unregister(key: (usize, u32, bool)) {
    BORROWS.with(|b| {
        let mut b = b.borrow_mut();
        if let Some(i) = b.iter().rposition(|k| *k == key) {
            b.swap_remove(i);
        }
    });
}

/// A handle's object, shared for a call.
pub struct HandleRef<T: 'static> {
    guard: ArcRwLockReadGuard<RawRwLock, T>,
    key: (usize, u32, bool),
}

impl<T: 'static> Deref for HandleRef<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.guard
    }
}

impl<T: 'static> Drop for HandleRef<T> {
    fn drop(&mut self) {
        unregister(self.key);
    }
}

/// A handle's object, exclusively for a call.
pub struct HandleMut<T: 'static> {
    guard: ArcRwLockWriteGuard<RawRwLock, T>,
    key: (usize, u32, bool),
}

impl<T: 'static> Deref for HandleMut<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.guard
    }
}

impl<T: 'static> DerefMut for HandleMut<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.guard
    }
}

impl<T: 'static> Drop for HandleMut<T> {
    fn drop(&mut self) {
        unregister(self.key);
    }
}

/// The bound a `#[goose::handle]` type meets unless it is pinned.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not Send + Sync, so Goose threads cannot share it",
    label = "a #[goose::handle] type must be Send + Sync",
    note = "mark it #[goose::handle(pinned)] to keep each object on the thread that made it"
)]
pub trait SendSyncHandle {}
impl<T: Send + Sync> SendSyncHandle for T {}

struct Slot<T> {
    generation: u32,
    obj: Option<Arc<RwLock<T>>>,
    owner: Option<ThreadId>,
}

struct Slots<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
    live: usize,
}

/// The slot table behind one `#[goose::handle]` type.
pub struct HandleTable<T: 'static> {
    /// `ns::Name`, for messages.
    name: &'static str,
    /// Not Send + Sync: every use must be on the thread that made the object.
    pinned: bool,
    inner: Mutex<Slots<T>>,
}

// A table hands an object only to the thread that made it when it is pinned
// (checked on every use, release included), and is otherwise built only for
// Send + Sync types (`new`'s bounds). So sharing the table is sound.
unsafe impl<T: 'static> Sync for HandleTable<T> {}
unsafe impl<T: 'static> Send for HandleTable<T> {}

/// What the leak report and `live_count` read, whatever the table's type.
pub trait LiveTable: Sync {
    fn name(&self) -> &'static str;
    fn live(&self) -> usize;
    fn live_handles(&self) -> Vec<Handle>;
}

static TABLES: Mutex<Vec<&'static dyn LiveTable>> = Mutex::new(Vec::new());

impl<T: 'static> HandleTable<T> {
    /// A table for a type any thread may use.
    pub const fn new(name: &'static str) -> Self
    where
        T: Send + Sync,
    {
        HandleTable { name, pinned: false, inner: Mutex::new(Slots { slots: Vec::new(), free: Vec::new(), live: 0 }) }
    }

    /// A table for a type that stays on the thread that made each object.
    ///
    /// # Safety
    /// Only the bridge's own generated code may build one: it relies on the
    /// table's thread checks to keep a `!Send` type on its thread.
    pub const unsafe fn new_pinned(name: &'static str) -> Self {
        HandleTable { name, pinned: true, inner: Mutex::new(Slots { slots: Vec::new(), free: Vec::new(), live: 0 }) }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    fn register(&'static self) {
        let mut t = TABLES.lock();
        let me = self as &dyn LiveTable as *const dyn LiveTable as *const ();
        if !t.iter().any(|x| *x as *const dyn LiveTable as *const () == me) {
            t.push(self);
        }
    }

    /// Puts an object in the table: the handle Goose will hold.
    pub fn insert(&'static self, v: T) -> Handle {
        let owner = if self.pinned { Some(std::thread::current().id()) } else { None };
        let obj = Some(Arc::new(RwLock::new(v)));
        let mut s = self.inner.lock();
        let first = s.slots.is_empty() && s.free.is_empty();
        s.live += 1;
        let h = if let Some(i) = s.free.pop() {
            let slot = &mut s.slots[i as usize];
            slot.obj = obj;
            slot.owner = owner;
            Handle { slot: i, generation: slot.generation }
        } else {
            let i = s.slots.len() as u32;
            s.slots.push(Slot { generation: 1, obj, owner });
            Handle { slot: i, generation: 1 }
        };
        drop(s);
        if first {
            self.register();
        }
        h
    }

    fn stale(&self, h: Handle) -> BridgeError {
        BridgeError::Abort(format!("stale {} handle ({h:?})", self.name))
    }

    fn check_thread(&self, owner: Option<ThreadId>) -> Result<(), BridgeError> {
        match owner {
            Some(o) if o != std::thread::current().id() => Err(BridgeError::Abort(format!(
                "{} handle used on a thread other than the one that made it: the type is not Send + Sync",
                self.name
            ))),
            _ => Ok(()),
        }
    }

    fn get(&self, h: Handle) -> Result<Arc<RwLock<T>>, BridgeError> {
        let s = self.inner.lock();
        let slot = s.slots.get(h.slot as usize).ok_or_else(|| self.stale(h))?;
        if slot.generation != h.generation {
            return Err(self.stale(h));
        }
        self.check_thread(slot.owner)?;
        slot.obj.clone().ok_or_else(|| self.stale(h))
    }

    fn busy(&self, h: Handle) -> BridgeError {
        BridgeError::Abort(format!(
            "{} handle ({h:?}) is already borrowed by a call still running on this thread (a callback in progress?)",
            self.name
        ))
    }

    /// The object, shared for the call.
    pub fn shared(&'static self, h: Handle) -> Result<HandleRef<T>, BridgeError> {
        let obj = self.get(h)?;
        let me = self.space();
        if BORROWS.with(|b| b.borrow().iter().any(|&(t, s, x)| t == me && s == h.slot && x)) {
            return Err(self.busy(h));
        }
        let key = (me, h.slot, false);
        BORROWS.with(|b| b.borrow_mut().push(key));
        Ok(HandleRef { guard: RwLock::read_arc_recursive(&obj), key })
    }

    /// The object, exclusively for the call.
    pub fn exclusive(&'static self, h: Handle) -> Result<HandleMut<T>, BridgeError> {
        let obj = self.get(h)?;
        let me = self.space();
        if BORROWS.with(|b| b.borrow().iter().any(|&(t, s, _)| t == me && s == h.slot)) {
            return Err(self.busy(h));
        }
        let key = (me, h.slot, true);
        BORROWS.with(|b| b.borrow_mut().push(key));
        Ok(HandleMut { guard: RwLock::write_arc(&obj), key })
    }

    /// Takes the object out: the handle is released and the value is the
    /// caller's. Waits for calls on other threads still using it.
    pub fn take(&'static self, h: Handle) -> Result<T, BridgeError> {
        let me = self.space();
        if BORROWS.with(|b| b.borrow().iter().any(|&(t, s, _)| t == me && s == h.slot)) {
            return Err(self.busy(h));
        }
        let mut obj = self.remove(h)?;
        loop {
            match Arc::try_unwrap(obj) {
                Ok(lock) => return Ok(lock.into_inner()),
                Err(still) => {
                    obj = still;
                    std::thread::yield_now();
                }
            }
        }
    }

    fn remove(&self, h: Handle) -> Result<Arc<RwLock<T>>, BridgeError> {
        let mut s = self.inner.lock();
        let slot = s.slots.get_mut(h.slot as usize).ok_or_else(|| self.stale(h))?;
        if slot.generation != h.generation || slot.obj.is_none() {
            return Err(self.stale(h));
        }
        self.check_thread(slot.owner)?;
        let obj = slot.obj.take().unwrap();
        slot.generation = slot.generation.wrapping_add(1).max(1);
        slot.owner = None;
        s.free.push(h.slot);
        s.live -= 1;
        Ok(obj)
    }

    /// Releases a handle: the object is dropped once no call is using it.
    pub fn release(&'static self, h: Handle) -> Result<(), BridgeError> {
        let me = self.space();
        if BORROWS.with(|b| b.borrow().iter().any(|&(t, s, _)| t == me && s == h.slot)) {
            return Err(self.busy(h));
        }
        let obj = self.remove(h)?;
        drop(obj);
        Ok(())
    }

    pub fn live(&self) -> usize {
        self.inner.lock().live
    }

    pub fn live_handles(&self) -> Vec<Handle> {
        let s = self.inner.lock();
        s.slots
            .iter()
            .enumerate()
            .filter(|(_, sl)| sl.obj.is_some())
            .map(|(i, sl)| Handle { slot: i as u32, generation: sl.generation })
            .collect()
    }

    /// The value naming this table in the aliasing check.
    pub fn space(&'static self) -> usize {
        self as *const Self as usize
    }
}

impl<T: 'static> LiveTable for HandleTable<T> {
    fn name(&self) -> &'static str {
        self.name
    }
    fn live(&self) -> usize {
        HandleTable::live(self)
    }
    fn live_handles(&self) -> Vec<Handle> {
        HandleTable::live_handles(self)
    }
}

/// A `#[goose::handle]` type: its table.
pub trait HandleType: Sized + 'static {
    fn table() -> &'static HandleTable<Self>;
}

/// Live handles of every type this library has made.
pub fn live_count() -> usize {
    TABLES.lock().iter().map(|t| t.live()).sum()
}

/// One line per handle type with live handles, for the leak report.
pub fn leak_report() -> Vec<String> {
    let mut out = Vec::new();
    for t in TABLES.lock().iter() {
        let hs = t.live_handles();
        if hs.is_empty() {
            continue;
        }
        let slots: Vec<String> = hs.iter().take(8).map(|h| { let s = h.slot; s.to_string() }).collect();
        let more = if hs.len() > 8 { ", ..." } else { "" };
        out.push(format!(
            "{} live {} handle{} (slot{} {}{more})",
            hs.len(),
            t.name(),
            if hs.len() == 1 { "" } else { "s" },
            if hs.len() == 1 { "" } else { "s" },
            slots.join(", ")
        ));
    }
    out
}

/// Prints the leak report on stderr; the number of live handles.
pub fn print_leak_report(library: &str) -> usize {
    let lines = leak_report();
    for l in &lines {
        eprintln!("goose-rust: {library}: {l} at exit");
    }
    live_count()
}
