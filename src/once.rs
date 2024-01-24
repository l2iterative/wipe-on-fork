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

pub struct WipeOnForkOnce {
    pid: Mutex<Option<u32>>,
    state: Mutex<State>,
}

impl UnwindSafe for WipeOnForkOnce {}
impl RefUnwindSafe for WipeOnForkOnce {}

pub const WIPE_ON_FORK_ONCE_INIT: WipeOnForkOnce = WipeOnForkOnce::new();

pub struct WipeOnForkOnceState {
    poisoned: bool,
    set_state_to: Cell<State>,
}

struct CompletionGuard<'a> {
    state: &'a Mutex<State>,
    set_state_on_drop_to: State,
}

impl<'a> Drop for CompletionGuard<'a> {
    fn drop(&mut self) {
        let mut lock = self.state.lock().unwrap();
        *lock = self.set_state_on_drop_to;
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
            unsafe {
                *self.state.lock().unwrap() = State::Incomplete;
            }
        }
    }

    #[cfg(not(unix))]
    #[inline]
    fn wipe_if_should_wipe(&self) {}

    #[inline]
    pub const fn new() -> WipeOnForkOnce {
        WipeOnForkOnce {
            pid: Mutex::new(None),
            state: Mutex::new(State::Incomplete)
        }
    }

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
        self.call(false, &mut |_| f.take().unwrap()());
    }

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
        self.call(true, &mut |p| f.take().unwrap()(p));
    }

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
    pub(crate) fn call(&self, ignore_poisoning: bool, f: &mut impl FnMut(&WipeOnForkOnceState)) {
        self.wipe_if_should_wipe();

        let mut lock = self.state.lock().unwrap();
        match *lock {
            State::Poisoned if !ignore_poisoning => {
                panic!("WipeOnForkOnce instance has previously been poisoned");
            }
            State::Incomplete | State::Poisoned => {
                *lock = State::Running;

                let mut guard = CompletionGuard {
                    state: &self.state,
                    set_state_on_drop_to: State::Poisoned,
                };
                let f_state = WipeOnForkOnceState {
                    poisoned: *lock == State::Poisoned,
                    set_state_to: Cell::new(State::Complete),
                };
                f(&f_state);
                guard.set_state_on_drop_to = f_state.set_state_to.get();
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
        f.debug_struct("WipeOnForkOnceState").field("poisoned", &self.is_poisoned()).finish()
    }
}