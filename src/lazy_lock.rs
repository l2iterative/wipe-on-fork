use std::cell::UnsafeCell;
use std::mem::ManuallyDrop;
use std::ops::Deref;
use std::panic::{RefUnwindSafe, UnwindSafe};
use crate::once::ExclusiveState;
use crate::WipeOnForkOnce;

/// ```
/// use std::collections::HashMap;
///
/// use wipe_on_fork::WipeOnForkLazyLock;
///
/// static HASHMAP: WipeOnForkLazyLock<HashMap<i32, String>> = WipeOnForkLazyLock::new(|| {
///     println!("initializing");
///     let mut m = HashMap::new();
///     m.insert(13, "Spica".to_string());
///     m.insert(74, "Hoyten".to_string());
///     m
/// });
///
/// fn main() {
///     println!("ready");
///     std::thread::spawn(|| {
///         println!("{:?}", HASHMAP.get(&13));
///     }).join().unwrap();
///     println!("{:?}", HASHMAP.get(&74));
///
///     // Prints:
///     //   ready
///     //   initializing
///     //   Some("Spica")
///     //   Some("Hoyten")
/// }
/// ```
/// Initialize fields with `LazyLock`.
/// ```
/// use wipe_on_fork::WipeOnForkLazyLock;
///
/// #[derive(Debug)]
/// struct UseCellLock {
///     number: WipeOnForkLazyLock<u32>,
/// }
/// fn main() {
///     let lock: WipeOnForkLazyLock<u32> = WipeOnForkLazyLock::new(|| 0u32);
///
///     let data = UseCellLock { number: lock };
///     println!("{}", *data.number);
/// }
/// ```

pub struct WipeOnForkLazyLock<T, F = fn() -> T> {
    once: WipeOnForkOnce,
    func: UnsafeCell<ManuallyDrop<F>>,
    data: UnsafeCell<ManuallyDrop<Option<T>>>,
}

impl<T, F: FnMut() -> T> WipeOnForkLazyLock<T, F> {
    #[inline]
    pub const fn new(f: F) -> WipeOnForkLazyLock<T, F> {
        WipeOnForkLazyLock {
            once: WipeOnForkOnce::new(),
            func: UnsafeCell::new(ManuallyDrop::new(f)),
            data: UnsafeCell::new(ManuallyDrop::new(None))
        }
    }

    /// ```
    /// use wipe_on_fork::WipeOnForkLazyLock;
    ///
    /// let hello = "Hello, World!".to_string();
    ///
    /// let lazy = WipeOnForkLazyLock::new(|| hello.to_uppercase());
    ///
    /// assert_eq!(&*lazy, "HELLO, WORLD!");
    /// assert_eq!(WipeOnForkLazyLock::into_inner(lazy).ok(), Some("HELLO, WORLD!".to_string()));
    /// ```
    pub fn into_inner(mut this: Self) -> Result<T, F> {
        let state = this.once.state();
        match state {
            ExclusiveState::Poisoned => panic!("LazyLock instance has previously been poisoned"),
            state => unsafe {
                let this = ManuallyDrop::new(this);
                match state {
                    ExclusiveState::Incomplete => Err(
                        ManuallyDrop::into_inner(std::ptr::read(&this.func).into_inner())
                    ),
                    ExclusiveState::Complete => Ok(
                        ManuallyDrop::into_inner(std::ptr::read(&this.data).into_inner()).unwrap()
                    ),
                    ExclusiveState::Poisoned => unreachable!(),
                }
            }
        }
    }

    /// ```
    /// use wipe_on_fork::WipeOnForkLazyLock;
    ///
    /// let lazy = WipeOnForkLazyLock::new(|| 92);
    ///
    /// assert_eq!(WipeOnForkLazyLock::force(&lazy), &92);
    /// assert_eq!(&*lazy, &92);
    /// ```
    #[inline]
    pub fn force(this: &WipeOnForkLazyLock<T, F>) -> &T {
        this.once.call_once(|| unsafe {
            let mut f = ManuallyDrop::take(&mut *this.func.get());
            let value = f();
            *this.data.get() = ManuallyDrop::new(Some(value));
        });

        unsafe { &*(*this.data.get()).as_ref().unwrap() }
    }
}

impl<T, F> WipeOnForkLazyLock<T, F> {
    fn get(&self) -> Option<&T> {
        if self.once.is_completed() {
            Some(unsafe { &*(*self.data.get()).as_ref().unwrap() })
        } else {
            None
        }
    }
}

impl<T, F> Drop for WipeOnForkLazyLock<T, F> {
    fn drop(&mut self) {
        match self.once.state() {
            ExclusiveState::Incomplete => unsafe { ManuallyDrop::drop(&mut self.func.get_mut()) },
            ExclusiveState::Complete => unsafe {
                ManuallyDrop::drop(&mut self.data.get_mut())
            }
            ExclusiveState::Poisoned => {}
        }
    }
}

impl<T, F: FnMut() -> T> Deref for WipeOnForkLazyLock<T, F> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        WipeOnForkLazyLock::force(self)
    }
}

impl<T: Default> Default for WipeOnForkLazyLock<T> {
    #[inline]
    fn default() -> WipeOnForkLazyLock<T> {
        WipeOnForkLazyLock::new(T::default)
    }
}

impl<T: core::fmt::Debug, F> core::fmt::Debug for WipeOnForkLazyLock<T, F> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut d = f.debug_tuple("WipeOnForkLazyLock");
        match self.get() {
            Some(v) => d.field(v),
            None => d.field(&format_args!("<uninit>")),
        };
        d.finish()
    }
}

unsafe impl<T: Sync + Send, F: Send> Sync for WipeOnForkLazyLock<T, F> {}

impl<T: RefUnwindSafe + UnwindSafe, F: UnwindSafe> RefUnwindSafe for WipeOnForkLazyLock<T, F> {}
impl<T: UnwindSafe, F: UnwindSafe> UnwindSafe for WipeOnForkLazyLock<T, F> {}