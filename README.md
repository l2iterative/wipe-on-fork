## Wipe-on-fork `OnceCell`, `LazyCell`, `Once`, `OnceLock`, `LazyLock` for Rust

<img src="https://github.com/l2iterative/wipe-on-fork/raw/main/title.png" align="right" alt="a group of people cleaning the room" width="320"/>

There has been a conspiracy theory on who created the pyramids: Egyptians or aliens. Similarly, thousands of years 
later, we can expect futurelings to ask who invented the Internet: humans or aliens?

A historical, at that time, can cite the HTTP status code [418 I'm a teapot](https://en.wikipedia.org/wiki/Hyper_Text_Coffee_Pot_Control_Protocol), which was originally 
an April Fools' prank, to prove that HTTP was invented by living creatures that drink tea, make teapots, and carry
a sense of humors. Shane Brunswick, who created the save418.com website that was crucial in the effort to not discard 418, 
said "It's a reminder that the underlying processes of computers are still made by humans."

Similar things happen in other areas of computer science. `fork()` being one of them. It is a way for one process to 
create another process. In HotOS 2019, four highly reputable computer systems researchers—Andrew Baumann, Jonathan Appavoo, 
Orran Krieger, and Timothy Roscoe—in their paper [A fork() in the road](https://www.microsoft.com/en-us/research/uploads/prod/2019/04/fork-hotos19.pdf), 
have discussed why people should avoid `fork()`, despite that it has been the core of POSIX and operating systems 
designs and has been widely used. A parallel universe may not have `fork()`.

Since `fork()` is a low-level operating systems primitive that performs changes in a way that applications are close to 
being transparent to, it always has a compatability issue. For Rust, it has been a headache (see https://github.com/rust-lang/rust/issues/6930,
https://github.com/rust-lang/rust/issues/9373, https://github.com/rust-lang/rust/issues/9568, https://github.com/rust-lang/rust/issues/16799). 

This is also an issue that involves CUDA. I believe that NVIDIA engineers have been routinely in the position to remind people 
that child processes cannot use the parent processes' CUDA contexts after `fork()`. This now, however, is related to Rust.
In RISC Zero's implementation of CUDA, it has a static global CUDA context that is lazy-initialized.
```rust
lazy_static! {
    static ref CONTEXT: Context = {
        let device = Device::get_device(0).unwrap();
        let context = Context::new(device).unwrap();
        context.set_flags(ContextFlags::SCHED_AUTO).unwrap();
        context
    };
}
```

Now, if the child process, who would have the exact same copy of `CONTEXT` from the parent, uses this context, chances 
are that the child would encounter segment faults. In general, "contexts" just cannot be shared across processes.

This calls for a replacement to the `lazy_static!` above that child processes would need to run their own rather than 
inherit it from the parent. This is closely related to a concept called "wipe-on-fork" in [Linux madvise function](https://man7.org/linux/man-pages/man2/madvise.2.html),
which allows a program to advise the operating system to wipe a page upon being forked:

> MADV_WIPEONFORK (since Linux 4.14)
>
> Present the child process with zero-filled memory in this
range after a fork(2).  This is useful in forking servers
in order to ensure that sensitive per-process data (for
example, PRNG seeds, cryptographic secrets, and so on) is
not handed to child processes.

Therefore, we adopt this naming convention and creates a number of data structures. 

This repository re-implements (copy-pastes, but with some modifications) the following structures from Rust's `std` library:

| Rust `std` Library    | This library                       |
|-----------------------|------------------------------------|
| `std::cell::OnceCell` | `wipe_on_fork::WipeOnForkOnceCell` |
| `std::cell::LazyCell` | `wipe_on_fork::WipeOnForkLazyCell` |
| `std::sync::Once`     | `wipe_on_fork::WipeOnForkOnce`     |
| `std::sync::OnceLock` | `wipe_on_fork::WipeOnForkOnceLock` |
| `std::sync::LazyLock` | `wipe_on_fork::WipeOnForkLazyLock` |

Most of the code, including the [documentation tests](https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html),
are copy-and-pasted from Rust std library in [rust-lang/rust](https://github.com/rust-lang/rust). We did so rather than 
using the existing primitives in a black-box manner—which would always be the preferred choice—because (1) some are still 
pending stabilization, (2) some necessary types or functions are only accessible within the `std` crate, (3) certain changes 
from us require more low-level manipulation. 

The usage fo the wipe-on-fork versions of `OnceCell`, `LazyCell`, `Once`, `OnceLock`, `LazyLock` resembles their keep-on-fork 
counterparts. It is necessary to note that these wipe-on-fork versions are not "better" or "more general-purpose" implementations. 
Some applications would **_specifically_** require wipe-on-fork, while other applications would **_specifically_** require keep-on-fork. 
This is why we include the prefix `WipeOnFork*` to remind that they are related but fundamentally different upon `fork()`.

Note that `fork()` is not the only solution to create child processes. Indeed, a more favorable solution, though less convenient,
is to `posix_spawn()` new processes. This has been used in [Dask](https://www.dask.org/), but not in [Ray](https://github.com/ray-project/ray) (see discussion in https://github.com/ray-project/ray/issues/13568).
The use of wipe-on-fork primitives is to offer compatibility upon composability, as anywhere, any thread of a process can 
make a call to `fork()`, and the less destructive solution is, like [thread safety](https://en.wikipedia.org/wiki/Thread_safety), 
to write code with **fork safety**.

### Fork detection

There are two approaches to detect a fork on the background.
- check if the code is running under a different [process ID (PID)](https://en.wikipedia.org/wiki/Process_identifier), obtainable from `std::process::id()`
- register a fork handler through [pthread_atfork()](https://man7.org/linux/man-pages/man3/pthread_atfork.3.html)

We eventually did not go with the PID approach because it has an inherent limitation. In Unix, there is no guarantee that 
PID does not repeat. In fact, assuming that the entire operating system already has `pid_max - 3` processes (note: since `pid_max` is 2^22 in 64-bit systems, this is very unlikely):
- The father has PID `a`
- The father forks and creates the son with PID `b`
- The son forks and creates the grandson with PID `c`
- The father passes away, leaving `a` available to be reused by the operating system
- The grandson forks and can expect to obtain PID `a` for the great-grandson
- If the father creates and initializes an `Once`, and this `Once` remains untouched by the son and the grandson, when 
the great-grandson first uses it, it cannot distinguish whether this `Once` should be wiped or not.

Although this is extremely niche, as most consumer memory is unlikely capable to have `pid_max` processes, we choose to 
go with a more resilient approach.

We introduce a notion of **generations**. When a wipe-on-fork object is being initialized for the very first time, it would 
be the first generation (i.e., with generation ID `0`). This generation ID is stored in a global variable.

```rust
pub struct GenerationCounter {
    pub(crate) gen: Mutex<Option<u64>>,
}

// implementations of `GenerationCounter`

pub(crate) static GENERATION: GenerationCounter = GenerationCounter::new();
```

We register a fork handler using `pthread_atfork()`. Each time it is being forked, we ask to increment this counter. Since 
the future generations would inherit this fork handler.
```rust
unsafe extern "C" fn update_generations() {
    let mut lock = GENERATION.gen.lock().unwrap();
    if lock.is_some() {
        *lock = Some(lock.unwrap() + 1);
    } else {
        panic!("The generation counter is expected to have started.");
    }
}

impl GenerationCounter {
    pub fn get(&self) -> u64 {
        // code before pthread_atfork
        unsafe {
            libc::pthread_atfork(None, None, Some(update_generations));
        }
        // code after pthread_atfork
    }
}
```

This fixes the problem because the great-grandson here is guaranteed to have a generation ID of `3`.


### Implementation detail

We now highlight how each is being implemented.

#### OnceCell

Recall that the `std::cell::OnceCell` is implemented as follows.
```rust
pub struct OnceCell<T> {
    inner: UnsafeCell<Option<T>>,
}
```

Our implementation is as follows.
```rust
pub struct WipeOnForkOnceCell<T> {
    generation_id: Cell<Option<u64>>,
    inner: UnsafeCell<Option<T>>,
    _not_send_sync: PhantomData<*const ()>,
}
```

We use `Cell` to host `Option<u64>` so that we can modify the generation ID even if we only have a non-mutable reference 
to `WipeOnForkOnceCell<T>`. The use of `_not_send_sync` is a hack to implement the negative trait `!Sync` which is not supported 
outside the standard library. See discussion [here](https://users.rust-lang.org/t/negative-trait-bounds-are-not-yet-fully-implemented-use-marker-types-for-now/64495) for this hack.

We run the check `wipe_if_should_wipe` on `get()`, `get_mut()`, `set()`, `try_insert()`, and `into_inner()`. If it is time to wipe, 
it clears the cell.
```rust
#[inline]
fn wipe_if_should_wipe(&self) {
    if self.check_if_should_wipe() {
        self.generation_id.set(None);
        unsafe {
            *self.inner.get() = None;
        }
    }
}
```

#### LazyCell

Recall that the `std::cell::LazyCell` is implemented as follows.
```rust
enum State<T, F> {
    Uninit(F),
    Init(T),
    Poisoned,
}

pub struct LazyCell<T, F = fn() -> T> {
    state: UnsafeCell<State<T, F>>,
}
```

Our implementation is as follows.
```rust
enum State<T, F> {
    Uninit(F),
    Init(T, F),
    Poisoned,
}

pub struct WipeOnForkLazyCell<T, F = fn() -> T> {
    generation_id: Cell<Option<u64>>,
    state: UnsafeCell<State<T, F>>,
    _not_send_sync: PhantomData<*const ()>,
}
```

Most of the design considerations are the same as in `OnceCell`. We modify the state to keep `F` in `Init` because initialization
may need to be redone in the child. We also change the type of functions in the implementation from `FnOnce` to `FnMut` to allow 
the function to be called several times.

A more careful `wipe_if_should_wipe()`, presented below, is run before `get()`, `into_inner()`, and `force()`.
```rust
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
```

#### Once

Now we turn our attention to the thread-safe primitives. The core one is `std::sync::Once`, which is the enabler of `std::sync::OnceLock`
and `std::sync::LazyLock` we are going to discuss next.

The `std::sync::Once` is implemented as follows.
```rust
pub struct Once {
    inner: sys::Once,
}
```
where `sys::Once` may be resolved to:
```rust
pub struct Once {
    state: AtomicU32,
}
```

Our implementation takes a different approach, which is as follows.
```rust
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum State {
    Incomplete,
    Poisoned,
    Running,
    Complete,
}

pub struct WipeOnForkOnce {
    generation_id: Mutex<Option<u64>>,
    state: Mutex<State>,
}
```
which is based on the non-thread-safe implementation in https://github.com/rust-lang/rust/blob/master/library/std/src/sys/pal/unsupported/once.rs, 
which is as simple as follows:
```rust
pub struct Once {
    state: Cell<State>,
}
```
Because the `std::sync::Once` default implementation in Linux or Unix would involve a lot of low-level primitives. 
To make the construction above thread-safe, we leverage `Mutex`. It is fortunate that `Mutex::new()` is a const function
(see https://github.com/rust-lang/rust/pull/97791). 

Instead of `Cell<Option<u64>>` that we use in `OnceCell` and `LazyCell`,
here we use `Mutex<Option<u64>>` for thread safety. 

The `wipe_if_should_wipe()` function is run before `call_once()`, `call_once_force()`, `is_completed()`, 
and `state()`.
```rust
#[inline]
fn wipe_if_should_wipe(&self) {
    let mut lock = self.generation_id.lock().unwrap();

    let res = match *lock {
        None => false,
        Some(generation_id) => generation_id != crate::utils::GENERATION.get(),
    };

    if res {
        *lock = None;
        *self.state.lock().unwrap() = State::Incomplete;
    }
}
```

#### OnceLock

The `std::sync::OnceLock` is implemented as follows.
```rust
pub struct OnceLock<T> {
    once: Once,
    value: UnsafeCell<MaybeUninit<T>>,
    _marker: PhantomData<T>,
}
```

Our implementation has a few differences.
```rust
pub struct WipeOnForkOnceLock<T> {
    once: WipeOnForkOnce,
    value: UnsafeCell<Option<T>>,
    _marker: PhantomData<T>,
}
```

We replace `Once` with `WipeOnForkOnce`. This seems to be sufficient for most of the features to work. We, however, 
use `UnsafeCell<Option<T>>` instead of `UnsafeCell<MaybeUninit<T>>` because we cannot use the `#[may_dangle]`, which is 
unstable, in our code. However, without this attribute, the behavior of the program changes. 

```rust
unsafe impl<#[may_dangle] T> Drop for OnceLock<T>  {
    // the implementation 
}
```

We would rather delegate back to Rust to handle the dropper. So, we change to `Option<T>` and let Rust enum implemenetation
to handle the detail, rather than using `MaybeUninit` which is harder.

#### LazyLock

The `std::sync::LazyLock` is implemented as follows, with the use of `union`.
```rust
union Data<T, F> {
    value: ManuallyDrop<T>,
    f: ManuallyDrop<F>,
}

pub struct LazyLock<T, F = fn() -> T> {
    once: Once,
    data: UnsafeCell<Data<T, F>>,
}
```

But, since we need to retain the function (for initialization in the child processes), we do need to depart from this implementation.
Our implementation is as follows.
```rust
pub struct WipeOnForkLazyLock<T, F = fn() -> T> {
    once: WipeOnForkOnce,
    func: UnsafeCell<ManuallyDrop<F>>,
    data: UnsafeCell<ManuallyDrop<Option<T>>>,
}
```

We also let the data be `Option<T>` because the data does not exist before lazy initialization.
The rest of the code is, therefore, modified accordingly, including the dropper. Some smaller changes are made to 
handle taking (i.e., `take()`).

### Behaviors not in Unix
We have not extensively test our implementation when it is used in pure Windows (not WSL, not Cygwin), but we expect it to work correctly. 
We basically disable the wipe-on-fork check, so that they always assume that no fork happens (which is the case since Windows does not have fork).

### License
Rust is under the [MIT](LICENSE-MIT) and [Apache 2](LICENSE-APACHE) licenses (with the conjunction "or"). This repository, which copy-pastes most of the code 
from it, inherits the same license. 