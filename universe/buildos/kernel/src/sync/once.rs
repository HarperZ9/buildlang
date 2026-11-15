//! BuildOS Once Implementation
//!
//! One-time initialization primitives.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU8, Ordering};
use super::WaitQueue;

/// State of Once
const INCOMPLETE: u8 = 0;
const RUNNING: u8 = 1;
const COMPLETE: u8 = 2;
const POISONED: u8 = 3;

/// A synchronization primitive which can be used to run a one-time
/// global initialization.
pub struct Once {
    state: AtomicU8,
    waiters: WaitQueue,
}

impl Once {
    /// Create a new `Once` value
    pub const fn new() -> Self {
        Self {
            state: AtomicU8::new(INCOMPLETE),
            waiters: WaitQueue::new(),
        }
    }

    /// Performs an initialization routine once and only once
    pub fn call_once<F>(&self, f: F)
    where
        F: FnOnce(),
    {
        // Fast path: already complete
        if self.is_completed() {
            return;
        }

        self.call_once_slow(f);
    }

    /// Slow path for call_once
    #[cold]
    fn call_once_slow<F>(&self, f: F)
    where
        F: FnOnce(),
    {
        loop {
            match self.state.compare_exchange(
                INCOMPLETE,
                RUNNING,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // We won the race, run the initializer
                    // Use a guard to handle panics
                    struct Guard<'a>(&'a AtomicU8, &'a WaitQueue);

                    impl Drop for Guard<'_> {
                        fn drop(&mut self) {
                            // If we're dropping due to panic, poison the Once
                            self.0.store(POISONED, Ordering::Release);
                            self.1.wake_all();
                        }
                    }

                    let guard = Guard(&self.state, &self.waiters);
                    f();
                    core::mem::forget(guard);

                    self.state.store(COMPLETE, Ordering::Release);
                    self.waiters.wake_all();
                    return;
                }
                Err(RUNNING) => {
                    // Someone else is running, wait
                    while self.state.load(Ordering::Acquire) == RUNNING {
                        self.waiters.wait();
                    }

                    // Check if it completed or was poisoned
                    match self.state.load(Ordering::Acquire) {
                        COMPLETE => return,
                        POISONED => panic!("Once instance has been poisoned"),
                        _ => continue,
                    }
                }
                Err(COMPLETE) => return,
                Err(POISONED) => panic!("Once instance has been poisoned"),
                Err(_) => unreachable!(),
            }
        }
    }

    /// Performs the same function as `call_once` except it may be called
    /// multiple times if the initialization fails.
    pub fn call_once_force<F>(&self, f: F)
    where
        F: FnOnce(&OnceState),
    {
        if self.is_completed() {
            return;
        }

        self.call_once_force_slow(f);
    }

    #[cold]
    fn call_once_force_slow<F>(&self, f: F)
    where
        F: FnOnce(&OnceState),
    {
        loop {
            let current = self.state.load(Ordering::Relaxed);
            let next = if current == POISONED { INCOMPLETE } else { current };

            match self.state.compare_exchange(
                next,
                RUNNING,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(prev) => {
                    let state = OnceState {
                        poisoned: prev == POISONED,
                    };

                    struct Guard<'a>(&'a AtomicU8, &'a WaitQueue);

                    impl Drop for Guard<'_> {
                        fn drop(&mut self) {
                            self.0.store(POISONED, Ordering::Release);
                            self.1.wake_all();
                        }
                    }

                    let guard = Guard(&self.state, &self.waiters);
                    f(&state);
                    core::mem::forget(guard);

                    self.state.store(COMPLETE, Ordering::Release);
                    self.waiters.wake_all();
                    return;
                }
                Err(RUNNING) => {
                    while self.state.load(Ordering::Acquire) == RUNNING {
                        self.waiters.wait();
                    }

                    if self.state.load(Ordering::Acquire) == COMPLETE {
                        return;
                    }
                    // If poisoned, loop and try again
                }
                Err(COMPLETE) => return,
                Err(_) => continue,
            }
        }
    }

    /// Returns true if some `call_once` call has completed successfully
    #[inline]
    pub fn is_completed(&self) -> bool {
        self.state.load(Ordering::Acquire) == COMPLETE
    }
}

impl Default for Once {
    fn default() -> Self {
        Self::new()
    }
}

/// State argument to `call_once_force`
pub struct OnceState {
    poisoned: bool,
}

impl OnceState {
    /// Returns true if the `Once` was previously poisoned
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }
}

/// A cell that can be written to only once
pub struct OnceCell<T> {
    once: Once,
    value: UnsafeCell<Option<T>>,
}

unsafe impl<T: Send + Sync> Sync for OnceCell<T> {}
unsafe impl<T: Send> Send for OnceCell<T> {}

impl<T> OnceCell<T> {
    /// Create a new empty `OnceCell`
    pub const fn new() -> Self {
        Self {
            once: Once::new(),
            value: UnsafeCell::new(None),
        }
    }

    /// Get the value, or initialize it
    pub fn get_or_init<F>(&self, f: F) -> &T
    where
        F: FnOnce() -> T,
    {
        self.once.call_once(|| {
            unsafe {
                *self.value.get() = Some(f());
            }
        });

        self.get().unwrap()
    }

    /// Get the value, or try to initialize it
    pub fn get_or_try_init<F, E>(&self, f: F) -> Result<&T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        if let Some(value) = self.get() {
            return Ok(value);
        }

        let mut result: Option<Result<(), E>> = None;
        self.once.call_once_force(|_| {
            match f() {
                Ok(v) => unsafe {
                    *self.value.get() = Some(v);
                }
                Err(e) => result = Some(Err(e)),
            }
        });

        match result {
            Some(Err(e)) => Err(e),
            _ => Ok(self.get().unwrap()),
        }
    }

    /// Get the value if initialized
    pub fn get(&self) -> Option<&T> {
        if self.once.is_completed() {
            unsafe { (*self.value.get()).as_ref() }
        } else {
            None
        }
    }

    /// Get mutable value if initialized
    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.value.get_mut().as_mut()
    }

    /// Set the value (fails if already set)
    pub fn set(&self, value: T) -> Result<(), T> {
        let mut val = Some(value);
        self.once.call_once(|| {
            unsafe {
                *self.value.get() = val.take();
            }
        });

        match val {
            Some(v) => Err(v),
            None => Ok(()),
        }
    }

    /// Take the value, leaving the cell empty
    pub fn take(&mut self) -> Option<T> {
        // Can only take if we have exclusive access
        self.value.get_mut().take()
    }

    /// Check if initialized
    pub fn is_initialized(&self) -> bool {
        self.once.is_completed()
    }
}

impl<T> Default for OnceCell<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> From<T> for OnceCell<T> {
    fn from(value: T) -> Self {
        let cell = Self::new();
        let _ = cell.set(value);
        cell
    }
}

/// A lazily initialized value
pub struct Lazy<T, F = fn() -> T> {
    cell: OnceCell<T>,
    init: UnsafeCell<Option<F>>,
}

unsafe impl<T: Send + Sync, F: Send> Sync for Lazy<T, F> {}

impl<T, F> Lazy<T, F> {
    /// Create a new lazy value with the given initializer
    pub const fn new(f: F) -> Self {
        Self {
            cell: OnceCell::new(),
            init: UnsafeCell::new(Some(f)),
        }
    }
}

impl<T, F: FnOnce() -> T> Lazy<T, F> {
    /// Force initialization and get the value
    pub fn force(this: &Self) -> &T {
        this.cell.get_or_init(|| {
            let init = unsafe { (*this.init.get()).take() };
            match init {
                Some(f) => f(),
                None => panic!("Lazy instance has been poisoned"),
            }
        })
    }

    /// Get the value if initialized
    pub fn get(this: &Self) -> Option<&T> {
        this.cell.get()
    }
}

impl<T, F: FnOnce() -> T> core::ops::Deref for Lazy<T, F> {
    type Target = T;

    fn deref(&self) -> &T {
        Lazy::force(self)
    }
}

impl<T: Default> Default for Lazy<T> {
    fn default() -> Self {
        Self::new(T::default)
    }
}

/// Static initializer (like lazy_static!)
#[macro_export]
macro_rules! static_init {
    ($name:ident: $ty:ty = $init:expr) => {
        static $name: $crate::sync::Lazy<$ty> = $crate::sync::Lazy::new(|| $init);
    };
}
