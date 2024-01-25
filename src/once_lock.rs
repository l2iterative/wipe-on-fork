use crate::WipeOnForkOnce;
use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::panic::{RefUnwindSafe, UnwindSafe};

/// ```
/// use wipe_on_fork::WipeOnForkOnceLock;
///
/// static CELL: WipeOnForkOnceLock<String> = WipeOnForkOnceLock::new();
/// assert!(CELL.get().is_none());
///
/// std::thread::spawn(|| {
///     let value: &String = CELL.get_or_init(|| {
///         "Hello, World!".to_string()
///     });
///     assert_eq!(value, "Hello, World!");
/// }).join().unwrap();
///
/// let value: Option<&String> = CELL.get();
/// assert!(value.is_some());
/// assert_eq!(value.unwrap().as_str(), "Hello, World!");
/// ```
pub struct WipeOnForkOnceLock<T> {
    once: WipeOnForkOnce,
    value: UnsafeCell<Option<T>>,
    _marker: PhantomData<T>,
}

impl<T> WipeOnForkOnceLock<T> {
    #[inline]
    #[must_use]
    pub const fn new() -> WipeOnForkOnceLock<T> {
        WipeOnForkOnceLock {
            once: WipeOnForkOnce::new(),
            value: UnsafeCell::new(None),
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn get(&self) -> Option<&T> {
        if self.is_initialized() {
            Some(self.get_unchecked())
        } else {
            None
        }
    }

    #[inline]
    pub fn get_mut(&mut self) -> Option<&mut T> {
        if self.is_initialized() {
            Some(self.get_unchecked_mut())
        } else {
            None
        }
    }

    /// ```
    /// use wipe_on_fork::WipeOnForkOnceLock;
    ///
    /// static CELL: WipeOnForkOnceLock<i32> = WipeOnForkOnceLock::new();
    ///
    /// fn main() {
    ///     assert!(CELL.get().is_none());
    ///
    ///     std::thread::spawn(|| {
    ///         assert_eq!(CELL.set(92), Ok(()));
    ///     }).join().unwrap();
    ///
    ///     assert_eq!(CELL.set(62), Err(62));
    ///     assert_eq!(CELL.get(), Some(&92));
    /// }
    /// ```
    #[inline]
    pub fn set(&self, value: T) -> Result<(), T> {
        match self.try_insert(value) {
            Ok(_) => Ok(()),
            Err((_, value)) => Err(value),
        }
    }

    /// ```
    /// use wipe_on_fork::WipeOnForkOnceLock;
    ///
    /// static CELL: WipeOnForkOnceLock<i32> = WipeOnForkOnceLock::new();
    ///
    /// fn main() {
    ///     assert!(CELL.get().is_none());
    ///
    ///     std::thread::spawn(|| {
    ///         assert_eq!(CELL.try_insert(92), Ok(&92));
    ///     }).join().unwrap();
    ///
    ///     assert_eq!(CELL.try_insert(62), Err((&92, 62)));
    ///     assert_eq!(CELL.get(), Some(&92));
    /// }
    /// ```
    #[inline]
    pub fn try_insert(&self, value: T) -> Result<&T, (&T, T)> {
        let mut value = Some(value);
        let res = self.get_or_init(|| value.take().unwrap());
        match value {
            None => Ok(res),
            Some(value) => Err((res, value)),
        }
    }

    /// ```
    /// use wipe_on_fork::WipeOnForkOnceLock;
    ///
    /// let cell = WipeOnForkOnceLock::new();
    /// let value = cell.get_or_init(|| 92);
    /// assert_eq!(value, &92);
    /// let value = cell.get_or_init(|| unreachable!());
    /// assert_eq!(value, &92);
    /// ```
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

    /// ```
    /// use wipe_on_fork::WipeOnForkOnceLock;
    ///
    /// let cell = WipeOnForkOnceLock::new();
    /// assert_eq!(cell.get_or_try_init(|| Err(())), Err(()));
    /// assert!(cell.get().is_none());
    /// let value = cell.get_or_try_init(|| -> Result<i32, ()> {
    ///     Ok(92)
    /// });
    /// assert_eq!(value, Ok(&92));
    /// assert_eq!(cell.get(), Some(&92))
    /// ```
    #[inline]
    pub fn get_or_try_init<F, E>(&self, f: F) -> Result<&T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        if let Some(value) = self.get() {
            return Ok(value);
        }
        self._initialize(f)?;

        debug_assert!(self.is_initialized());

        Ok(self.get_unchecked())
    }

    /// ```
    /// use wipe_on_fork::WipeOnForkOnceLock;
    ///
    /// let cell: WipeOnForkOnceLock<String> = WipeOnForkOnceLock::new();
    /// assert_eq!(cell.into_inner(), None);
    ///
    /// let cell = WipeOnForkOnceLock::new();
    /// cell.set("hello".to_string()).unwrap();
    /// assert_eq!(cell.into_inner(), Some("hello".to_string()));
    /// ```
    #[inline]
    pub fn into_inner(mut self) -> Option<T> {
        self.take()
    }

    /// ```
    /// use wipe_on_fork::WipeOnForkOnceLock;
    ///
    /// let mut cell: WipeOnForkOnceLock<String> = WipeOnForkOnceLock::new();
    /// assert_eq!(cell.take(), None);
    ///
    /// let mut cell = WipeOnForkOnceLock::new();
    /// cell.set("hello".to_string()).unwrap();
    /// assert_eq!(cell.take(), Some("hello".to_string()));
    /// assert_eq!(cell.get(), None);
    /// ```
    #[inline]
    pub fn take(&mut self) -> Option<T> {
        if self.is_initialized() {
            self.once = WipeOnForkOnce::new();
            unsafe { (&mut *self.value.get()).take() }
        } else {
            None
        }
    }

    #[inline]
    pub fn is_initialized(&self) -> bool {
        self.once.is_completed()
    }

    #[cold]
    pub(crate) fn _initialize<F, E>(&self, f: F) -> Result<(), E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        let mut res: Result<(), E> = Ok(());
        let slot = &self.value;

        self.once.call_once_force(|p| {
            match f() {
                Ok(value) => unsafe {
                    *slot.get() = Some(value);
                },
                Err(e) => {
                    res = Err(e);

                    // Treat the underlying `Once` as poisoned since we
                    // failed to initialize our value. Calls
                    p.poison();
                }
            }
        });
        res
    }

    #[inline]
    pub(crate) fn get_unchecked(&self) -> &T {
        debug_assert!(self.is_initialized());
        unsafe { (&*self.value.get()).as_ref().unwrap() }
    }

    #[inline]
    pub(crate) fn get_unchecked_mut(&self) -> &mut T {
        debug_assert!(self.is_initialized());
        unsafe { (&mut *self.value.get()).as_mut().unwrap() }
    }
}

unsafe impl<T: Sync + Send> Sync for WipeOnForkOnceLock<T> {}
unsafe impl<T: Send> Send for WipeOnForkOnceLock<T> {}

impl<T: RefUnwindSafe + UnwindSafe> RefUnwindSafe for WipeOnForkOnceLock<T> {}
impl<T: UnwindSafe> UnwindSafe for WipeOnForkOnceLock<T> {}

impl<T> Default for WipeOnForkOnceLock<T> {
    /// ```
    /// use wipe_on_fork::WipeOnForkOnceLock;
    ///
    /// fn main() {
    ///     assert_eq!(WipeOnForkOnceLock::<()>::new(), WipeOnForkOnceLock::default());
    /// }
    /// ```
    #[inline]
    fn default() -> WipeOnForkOnceLock<T> {
        WipeOnForkOnceLock::new()
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for WipeOnForkOnceLock<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut d = f.debug_tuple("WipeOnForkOnceLock");
        match self.get() {
            Some(v) => d.field(v),
            None => d.field(&format_args!("<uninit>")),
        };
        d.finish()
    }
}

impl<T: Clone> Clone for WipeOnForkOnceLock<T> {
    #[inline]
    fn clone(&self) -> WipeOnForkOnceLock<T> {
        let cell = Self::new();
        if let Some(value) = self.get() {
            match cell.set(value.clone()) {
                Ok(()) => (),
                Err(_) => unreachable!(),
            }
        }
        cell
    }
}

impl<T> From<T> for WipeOnForkOnceLock<T> {
    /// ```
    /// use wipe_on_fork::WipeOnForkOnceLock;
    ///
    /// # fn main() -> Result<(), i32> {
    /// let a = WipeOnForkOnceLock::from(3);
    /// let b = WipeOnForkOnceLock::new();
    /// b.set(3)?;
    /// assert_eq!(a, b);
    /// Ok(())
    /// # }
    /// ```
    #[inline]
    fn from(value: T) -> Self {
        let cell = Self::new();
        match cell.set(value) {
            Ok(()) => cell,
            Err(_) => unreachable!(),
        }
    }
}

impl<T: PartialEq> PartialEq for WipeOnForkOnceLock<T> {
    #[inline]
    fn eq(&self, other: &WipeOnForkOnceLock<T>) -> bool {
        self.get() == other.get()
    }
}

impl<T: Eq> Eq for WipeOnForkOnceLock<T> {}
