use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Mutex;
use std::thread;
use crate::{WipeOnForkLazyCell, WipeOnForkLazyLock, WipeOnForkOnceLock};


static A: WipeOnForkLazyLock<u32> = WipeOnForkLazyLock::new(|| std::process::id());

#[test]
#[cfg(unix)]
fn wipe_on_fork() {
    assert_eq!(*A, std::process::id());

    let mut pipefd: [libc::c_int; 2] = [libc::c_int::default(), libc::c_int::default()];

    unsafe { libc::pipe(pipefd.as_mut_ptr()) };

    let res = unsafe { libc::fork() };

    if res == 0 {
        // child
        unsafe {
            libc::close(pipefd[0]);
        }

        let mut expected_flag = 0u8;

        if *A != std::process::id() {
            expected_flag = 1u8;
        }

        unsafe {
            libc::write(
                pipefd[1],
                &expected_flag as *const u8 as *const libc::c_void,
                1,
            );
            libc::close(pipefd[1]);
            libc::exit(0);
        }
    } else {
        // parent
        unsafe {
            libc::close(pipefd[1]);
        }

        let mut expected_flag = 2u8;
        unsafe {
            libc::read(
                pipefd[0],
                (&mut expected_flag) as *mut u8 as *mut libc::c_void,
                4,
            );
        }

        assert_eq!(expected_flag, 0u8);
    }
}

fn spawn_and_wait<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
    thread::spawn(f).join().unwrap()
}

#[test]
fn lazy_default() {
    static CALLED: AtomicUsize = AtomicUsize::new(0);

    struct Foo(u8);
    impl Default for Foo {
        fn default() -> Self {
            CALLED.fetch_add(1, SeqCst);
            Foo(42)
        }
    }

    let lazy: WipeOnForkLazyCell<Mutex<Foo>> = <_>::default();

    assert_eq!(CALLED.load(SeqCst), 0);

    assert_eq!(lazy.lock().unwrap().0, 42);
    assert_eq!(CALLED.load(SeqCst), 1);

    lazy.lock().unwrap().0 = 21;

    assert_eq!(lazy.lock().unwrap().0, 21);
    assert_eq!(CALLED.load(SeqCst), 1);
}

#[test]
fn lazy_poisoning() {
    let x: WipeOnForkLazyCell<String> = WipeOnForkLazyCell::new(|| panic!("kaboom"));
    for _ in 0..2 {
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| x.len()));
        assert!(res.is_err());
    }
}

#[test]
#[cfg_attr(target_os = "emscripten", ignore)]
fn sync_lazy_new() {
    static CALLED: AtomicUsize = AtomicUsize::new(0);
    static SYNC_LAZY: WipeOnForkLazyLock<i32> = WipeOnForkLazyLock::new(|| {
        CALLED.fetch_add(1, SeqCst);
        92
    });

    assert_eq!(CALLED.load(SeqCst), 0);

    spawn_and_wait(|| {
        let y = *SYNC_LAZY - 30;
        assert_eq!(y, 62);
        assert_eq!(CALLED.load(SeqCst), 1);
    });

    let y = *SYNC_LAZY - 30;
    assert_eq!(y, 62);
    assert_eq!(CALLED.load(SeqCst), 1);
}

#[test]
fn sync_lazy_default() {
    static CALLED: AtomicUsize = AtomicUsize::new(0);

    struct Foo(u8);
    impl Default for Foo {
        fn default() -> Self {
            CALLED.fetch_add(1, SeqCst);
            Foo(42)
        }
    }

    let lazy: WipeOnForkLazyLock<Mutex<Foo>> = <_>::default();

    assert_eq!(CALLED.load(SeqCst), 0);

    assert_eq!(lazy.lock().unwrap().0, 42);
    assert_eq!(CALLED.load(SeqCst), 1);

    lazy.lock().unwrap().0 = 21;

    assert_eq!(lazy.lock().unwrap().0, 21);
    assert_eq!(CALLED.load(SeqCst), 1);
}

#[test]
fn static_sync_lazy() {
    static XS: WipeOnForkLazyLock<Vec<i32>> = WipeOnForkLazyLock::new(|| {
        let mut xs = Vec::new();
        xs.push(1);
        xs.push(2);
        xs.push(3);
        xs
    });

    spawn_and_wait(|| {
        assert_eq!(&*XS, &vec![1, 2, 3]);
    });

    assert_eq!(&*XS, &vec![1, 2, 3]);
}

#[test]
fn static_sync_lazy_via_fn() {
    fn xs() -> &'static Vec<i32> {
        static XS: WipeOnForkOnceLock<Vec<i32>> = WipeOnForkOnceLock::new();
        XS.get_or_init(|| {
            let mut xs = Vec::new();
            xs.push(1);
            xs.push(2);
            xs.push(3);
            xs
        })
    }
    assert_eq!(xs(), &vec![1, 2, 3]);
}

#[test]
fn sync_lazy_poisoning() {
    let x: WipeOnForkLazyLock<String> = WipeOnForkLazyLock::new(|| panic!("kaboom"));
    for _ in 0..2 {
        let res = std::panic::catch_unwind(|| x.len());
        assert!(res.is_err());
    }
}

// Check that we can infer `T` from closure's type.
#[test]
fn lazy_type_inference() {
    let _ = WipeOnForkLazyCell::new(|| ());
}

#[test]
fn is_sync_send() {
    fn assert_traits<T: Send + Sync>() {}
    assert_traits::<WipeOnForkLazyLock<String>>();
}