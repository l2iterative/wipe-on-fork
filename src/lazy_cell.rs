use core::cell::{Cell, UnsafeCell};
use core::marker::PhantomData;
use std::ops::Deref;

enum State<T, F> {
    Uninit(F),
    Init(T, F),
    Poisoned,
}

/// ```
/// use wipe_on_fork::WipeOnForkLazyCell;
///
/// let lazy: WipeOnForkLazyCell<i32> = WipeOnForkLazyCell::new(|| {
///     println!("initializing");
///     92
/// });
/// println!("ready");
/// println!("{}", *lazy);
/// println!("{}", *lazy);
///
/// // Prints:
/// //   ready
/// //   initializing
/// //   92
/// //   92
/// ```
pub struct WipeOnForkLazyCell<T, F = fn() -> T> {
    generation_id: Cell<Option<u64>>,
    state: UnsafeCell<State<T, F>>,
    _not_send_sync: PhantomData<*const ()>,
}

impl<T, F: FnMut() -> T> WipeOnForkLazyCell<T, F> {
    /// ```
    /// use wipe_on_fork::WipeOnForkLazyCell;
    ///
    /// let hello = "Hello, World!".to_string();
    ///
    /// let lazy = WipeOnForkLazyCell::new(|| hello.to_uppercase());
    ///
    /// assert_eq!(&*lazy, "HELLO, WORLD!");
    /// ```
    #[inline]
    pub const fn new(f: F) -> Self {
        WipeOnForkLazyCell {
            generation_id: Cell::new(None),
            state: UnsafeCell::new(State::Uninit(f)),
            _not_send_sync: PhantomData,
        }
    }

    /// ```
    /// use wipe_on_fork::WipeOnForkLazyCell;
    ///
    /// let hello = "Hello, World!".to_string();
    ///
    /// let lazy = WipeOnForkLazyCell::new(|| hello.to_uppercase());
    ///
    /// assert_eq!(&*lazy, "HELLO, WORLD!");
    /// assert_eq!(WipeOnForkLazyCell::into_inner(lazy).ok(), Some("HELLO, WORLD!".to_string()));
    /// ```
    pub fn into_inner(this: Self) -> Result<T, F> {
        this.wipe_if_should_wipe();
        match this.state.into_inner() {
            State::Uninit(f) => Err(f),
            State::Init(data, _) => Ok(data),
            State::Poisoned => panic!("WipeOnForkLazyCell instance has previously been poisoned"),
        }
    }

    /// ```
    /// use wipe_on_fork::WipeOnForkLazyCell;
    ///
    /// let lazy = WipeOnForkLazyCell::new(|| 92);
    ///
    /// assert_eq!(WipeOnForkLazyCell::force(&lazy), &92);
    /// assert_eq!(&*lazy, &92);
    /// ```
    #[inline]
    pub fn force(this: &WipeOnForkLazyCell<T, F>) -> &T {
        this.wipe_if_should_wipe();
        let state = unsafe { &*this.state.get() };
        match state {
            State::Init(data, _) => data,
            State::Uninit(_) => unsafe { WipeOnForkLazyCell::really_init(this) },
            State::Poisoned => panic!("WipeOnForkLazyCell has previously been poisoned"),
        }
    }

    #[cold]
    unsafe fn really_init(this: &WipeOnForkLazyCell<T, F>) -> &T {
        let state = unsafe { &mut *this.state.get() };
        let State::Uninit(mut f) = core::mem::replace(state, State::Poisoned) else {
            unreachable!()
        };

        let data = f();

        unsafe { this.state.get().write(State::Init(data, f)) };

        this.generation_id.set(Some(crate::utils::GENERATION.get()));

        let state = unsafe { &*this.state.get() };
        let State::Init(data, _) = state else {
            unreachable!()
        };
        data
    }
}

impl<T, F> WipeOnForkLazyCell<T, F> {
    #[cfg(unix)]
    #[inline]
    fn check_if_should_wipe(&self) -> bool {
        return match self.generation_id.get() {
            None => false,
            Some(generation_id) => generation_id != crate::utils::GENERATION.get(),
        };
    }

    #[cfg(not(unix))]
    #[inline]
    fn check_if_should_wipe(&self) -> bool {
        false
    }

    #[inline]
    fn wipe_if_should_wipe(&self) {
        if self.check_if_should_wipe() {
            self.generation_id.set(None);

            let is_state_init = unsafe {
                match *self.state.get() {
                    State::Init(_, _) => true,
                    _ => false,
                }
            };

            if is_state_init {
                let state = unsafe { &mut *self.state.get() };
                let State::Init(_, f) = core::mem::replace(state, State::Poisoned) else {
                    unreachable!()
                };

                unsafe { self.state.get().write(State::Uninit(f)) };
            }
        }
    }

    #[inline]
    fn get(&self) -> Option<&T> {
        self.wipe_if_should_wipe();

        let state = unsafe { &*self.state.get() };
        match state {
            State::Init(data, _) => Some(data),
            _ => None,
        }
    }
}

impl<T, F: FnMut() -> T> Deref for WipeOnForkLazyCell<T, F> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T {
        WipeOnForkLazyCell::force(self)
    }
}

impl<T: Default> Default for WipeOnForkLazyCell<T> {
    #[inline]
    fn default() -> WipeOnForkLazyCell<T> {
        WipeOnForkLazyCell::new(T::default)
    }
}

impl<T: core::fmt::Debug, F> core::fmt::Debug for WipeOnForkLazyCell<T, F> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut d = f.debug_tuple("WipeOnForkLazyCell");
        match self.get() {
            Some(data) => d.field(data),
            None => d.field(&format_args!("<uninit>")),
        };
        d.finish()
    }
}
