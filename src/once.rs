use core::cell::Cell;
use std::panic::{RefUnwindSafe, UnwindSafe};
use std::sync::Mutex;

pub enum ExclusiveState {
    Incomplete,
    Poisoned,
    Complete,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum State {
    Incomplete,
    Poisoned,
    Running,
    Complete,
}

/// ```
/// use wipe_on_fork::WipeOnForkOnce;
///
/// static START: WipeOnForkOnce = WipeOnForkOnce::new();
///
/// START.call_once(|| {
///     // run initialization here
/// });
/// ```
pub struct WipeOnForkOnce {
    pid: Mutex<Option<u32>>,
    state: Mutex<State>,
}

impl UnwindSafe for WipeOnForkOnce {}
impl RefUnwindSafe for WipeOnForkOnce {}

/// # Examples
///
/// ```
/// use wipe_on_fork::{WipeOnForkOnce, WIPE_ON_FORK_ONCE_INIT};
///
/// static START: WipeOnForkOnce = WIPE_ON_FORK_ONCE_INIT;
/// ```
pub const WIPE_ON_FORK_ONCE_INIT: WipeOnForkOnce = WipeOnForkOnce::new();

pub struct WipeOnForkOnceState {
    poisoned: bool,
    set_state_to: Cell<State>,
}

struct CompletionGuard<'a> {
    pid: &'a Mutex<Option<u32>>,
    state: &'a Mutex<State>,
    set_state_on_drop_to: State,
    set_pid_on_drop_to: Option<u32>,
}

impl<'a> Drop for CompletionGuard<'a> {
    fn drop(&mut self) {
        let mut lock = self.state.lock().unwrap();
        *lock = self.set_state_on_drop_to;

        let mut lock = self.pid.lock().unwrap();
        *lock = self.set_pid_on_drop_to;
    }
}

unsafe impl Sync for WipeOnForkOnce {}

impl WipeOnForkOnce {
    #[cfg(unix)]
    #[inline]
    fn wipe_if_should_wipe(&self) {
        let mut lock = self.pid.lock().unwrap();

        let res = match *lock {
            None => false,
            Some(pid) => pid != std::process::id(),
        };

        if res {
            *lock = None;
            *self.state.lock().unwrap() = State::Incomplete;
        }
    }

    #[cfg(not(unix))]
    #[inline]
    fn wipe_if_should_wipe(&self) {}

    #[inline]
    pub const fn new() -> WipeOnForkOnce {
        WipeOnForkOnce {
            pid: Mutex::new(None),
            state: Mutex::new(State::Incomplete),
        }
    }

    /// ```
    /// use wipe_on_fork::WipeOnForkOnce;
    ///
    /// static mut VAL: usize = 0;
    /// static INIT: WipeOnForkOnce = WipeOnForkOnce::new();
    ///
    /// fn get_cached_val() -> usize {
    ///     unsafe {
    ///         INIT.call_once(|| {
    ///             VAL = expensive_computation();
    ///         });
    ///         VAL
    ///     }
    /// }
    ///
    /// fn expensive_computation() -> usize {
    ///     // ...
    /// # 2
    /// }
    /// ```
    #[inline]
    pub fn call_once<F>(&self, f: F)
    where
        F: FnOnce(),
    {
        self.wipe_if_should_wipe();
        if self.is_completed() {
            return;
        }

        let mut f = Some(f);
        self._call(false, &mut |_| f.take().unwrap()());
    }

    /// ```
    /// use wipe_on_fork::WipeOnForkOnce;
    /// use std::thread;
    ///
    /// static INIT: WipeOnForkOnce = WipeOnForkOnce::new();
    ///
    /// // poison the once
    /// let handle = thread::spawn(|| {
    ///     INIT.call_once(|| panic!());
    /// });
    /// assert!(handle.join().is_err());
    ///
    /// // poisoning propagates
    /// let handle = thread::spawn(|| {
    ///     INIT.call_once(|| {});
    /// });
    /// assert!(handle.join().is_err());
    ///
    /// // call_once_force will still run and reset the poisoned state
    /// INIT.call_once_force(|state| {
    ///     assert!(state.is_poisoned());
    /// });
    ///
    /// // once any success happens, we stop propagating the poison
    /// INIT.call_once(|| {});
    /// ```
    #[inline]
    pub fn call_once_force<F>(&self, f: F)
    where
        F: FnOnce(&WipeOnForkOnceState),
    {
        self.wipe_if_should_wipe();
        if self.is_completed() {
            return;
        }

        let mut f = Some(f);
        self._call(true, &mut |p| f.take().unwrap()(p));
    }

    /// ```
    /// use wipe_on_fork::WipeOnForkOnce;
    ///
    /// static INIT: WipeOnForkOnce = WipeOnForkOnce::new();
    ///
    /// assert_eq!(INIT.is_completed(), false);
    /// INIT.call_once(|| {
    ///     assert_eq!(INIT.is_completed(), false);
    /// });
    /// assert_eq!(INIT.is_completed(), true);
    /// ```
    ///
    /// ```
    /// use wipe_on_fork::WipeOnForkOnce;
    /// use std::thread;
    ///
    /// static INIT: WipeOnForkOnce = WipeOnForkOnce::new();
    ///
    /// assert_eq!(INIT.is_completed(), false);
    /// let handle = thread::spawn(|| {
    ///     INIT.call_once(|| panic!());
    /// });
    /// assert!(handle.join().is_err());
    /// assert_eq!(INIT.is_completed(), false);
    /// ```
    #[inline]
    pub fn is_completed(&self) -> bool {
        self.wipe_if_should_wipe();
        let lock = self.state.lock().unwrap();
        *lock == State::Complete
    }

    #[inline]
    pub fn state(&mut self) -> ExclusiveState {
        self.wipe_if_should_wipe();
        let lock = self.state.lock().unwrap();
        match *lock {
            State::Incomplete => ExclusiveState::Incomplete,
            State::Poisoned => ExclusiveState::Poisoned,
            State::Complete => ExclusiveState::Complete,
            _ => unreachable!("invalid Once state"),
        }
    }

    #[cold]
    pub(crate) fn _call(&self, ignore_poisoning: bool, f: &mut impl FnMut(&WipeOnForkOnceState)) {
        self.wipe_if_should_wipe();

        let cur_state: State = {
            let lock = self.state.lock().unwrap();
            lock.clone()
        };
        match cur_state {
            State::Poisoned if !ignore_poisoning => {
                panic!("WipeOnForkOnce instance has previously been poisoned");
            }
            State::Incomplete | State::Poisoned => {
                *self.state.lock().unwrap() = State::Running;

                let mut guard = CompletionGuard {
                    pid: &self.pid,
                    state: &self.state,
                    set_state_on_drop_to: State::Poisoned,
                    set_pid_on_drop_to: None,
                };
                let f_state = WipeOnForkOnceState {
                    poisoned: cur_state == State::Poisoned,
                    set_state_to: Cell::new(State::Complete),
                };
                f(&f_state);
                guard.set_state_on_drop_to = f_state.set_state_to.get();
                guard.set_pid_on_drop_to = Some(std::process::id());
            }
            State::Running => {
                panic!("one-time initialization may not be performed recursively");
            }
            State::Complete => {}
        }
    }
}

impl core::fmt::Debug for WipeOnForkOnce {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WipeOnForkOnce").finish_non_exhaustive()
    }
}

impl WipeOnForkOnceState {
    /// # Examples
    ///
    /// A poisoned [`Once`]:
    ///
    /// ```
    /// use wipe_on_fork::WipeOnForkOnce;
    /// use std::thread;
    ///
    /// static INIT: WipeOnForkOnce = WipeOnForkOnce::new();
    ///
    /// // poison the once
    /// let handle = thread::spawn(|| {
    ///     INIT.call_once(|| panic!());
    /// });
    /// assert!(handle.join().is_err());
    ///
    /// INIT.call_once_force(|state| {
    ///     assert!(state.is_poisoned());
    /// });
    /// ```
    ///
    /// An unpoisoned [`Once`]:
    ///
    /// ```
    /// use wipe_on_fork::WipeOnForkOnce;
    ///
    /// static INIT: WipeOnForkOnce = WipeOnForkOnce::new();
    ///
    /// INIT.call_once_force(|state| {
    ///     assert!(!state.is_poisoned());
    /// });
    #[inline]
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    #[inline]
    pub fn poison(&self) {
        self.set_state_to.set(State::Poisoned)
    }
}

impl core::fmt::Debug for WipeOnForkOnceState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("WipeOnForkOnceState")
            .field("poisoned", &self.is_poisoned())
            .finish()
    }
}
