use core::cell::Cell;
use core::cell::UnsafeCell;
use std::marker::PhantomData;

#[cfg(not(unix))]
compile_error!("This crate should only be compiled with a Unix target.");

#[cfg(test)]
mod tests;

pub struct WipeOnForkOnceCell<T> {
    pid: Cell<Option<u32>>,
    inner: UnsafeCell<Option<T>>,
    _not_send_sync: core::marker::PhantomData<*const ()>,
}

impl<T> WipeOnForkOnceCell<T> {
    #[inline]
    fn check_if_should_wipe(&self) -> bool {
        return match self.pid.get() {
            None => false,
            Some(pid) => pid != std::process::id(),
        };
    }

    #[inline]
    fn wipe_if_should_wipe(&self) {
        if self.check_if_should_wipe() {
            self.pid.set(None);
            unsafe {
                *self.inner.get() = None;
            }
        }
    }

    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        WipeOnForkOnceCell {
            pid: Cell::new(None),
            inner: UnsafeCell::new(None),
            _not_send_sync: PhantomData,
        }
    }

    #[inline]
    pub fn get(&self) -> Option<&T> {
        self.wipe_if_should_wipe();
        unsafe { &*self.inner.get() }.as_ref()
    }

    #[inline]
    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.wipe_if_should_wipe();
        self.inner.get_mut().as_mut()
    }

    #[inline]
    pub fn set(&self, value: T) -> Result<(), T> {
        self.wipe_if_should_wipe();

        match self.try_insert(value) {
            Ok(_) => Ok(()),
            Err((_, value)) => Err(value),
        }
    }

    #[inline]
    pub fn try_insert(&self, value: T) -> Result<&T, (&T, T)> {
        self.wipe_if_should_wipe();

        if let Some(old) = self.get() {
            return Err((old, value));
        }

        self.pid.set(Some(std::process::id()));

        let slot = unsafe { &mut *self.inner.get() };
        Ok(slot.insert(value))
    }

    #[inline]
    pub fn get_or_init<F>(&self, f: F) -> &T
    where
        F: FnOnce() -> T,
    {
        match self.get_or_try_init(|| Ok::<T, ()>(f())) {
            Ok(val) => val,
            _ => unreachable!(),
        }
    }

    #[inline]
    pub fn get_or_try_init<F, E>(&self, f: F) -> Result<&T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        if let Some(val) = self.get() {
            return Ok(val);
        }

        #[cold]
        fn outlined_call<F, T, E>(f: F) -> Result<T, E>
        where
            F: FnOnce() -> Result<T, E>,
        {
            f()
        }
        let val = outlined_call(f)?;

        if let Ok(val) = self.try_insert(val) {
            Ok(val)
        } else {
            panic!("reentrant init")
        }
    }

    #[inline]
    pub fn into_inner(self) -> Option<T> {
        self.inner.into_inner()
    }

    #[inline]
    pub fn take(&mut self) -> Option<T> {
        core::mem::take(self).into_inner()
    }
}

impl<T> Default for WipeOnForkOnceCell<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for WipeOnForkOnceCell<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut d = f.debug_tuple("OnceCell");
        match self.get() {
            Some(v) => d.field(v),
            None => d.field(&format_args!("<uninit>")),
        };
        d.finish()
    }
}

impl<T: Clone> Clone for WipeOnForkOnceCell<T> {
    #[inline]
    fn clone(&self) -> WipeOnForkOnceCell<T> {
        let res = WipeOnForkOnceCell::new();
        if let Some(value) = self.get() {
            match res.set(value.clone()) {
                Ok(()) => (),
                Err(_) => unreachable!(),
            }
        }
        res
    }
}

impl<T: PartialEq> PartialEq for WipeOnForkOnceCell<T> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}

impl<T: Eq> Eq for WipeOnForkOnceCell<T> {}

impl<T> From<T> for WipeOnForkOnceCell<T> {
    /// Creates a new `OnceCell<T>` which already contains the given `value`.
    #[inline]
    fn from(value: T) -> Self {
        WipeOnForkOnceCell {
            pid: Cell::new(Some(std::process::id())),
            inner: UnsafeCell::new(Some(value)),
            _not_send_sync: PhantomData,
        }
    }
}
