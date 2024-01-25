use crate::WipeOnForkOnceLock;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::OnceLock;
use std::thread;

static A: WipeOnForkOnceLock<u32> = WipeOnForkOnceLock::<u32>::new();
static B: OnceLock<u32> = OnceLock::new();

#[test]
#[cfg(unix)]
fn wipe_on_fork() {
    A.get_or_init(|| 1u32);
    B.get_or_init(|| 1u32);

    assert_eq!(A.get().is_some(), true);
    assert_eq!(B.get().is_some(), true);

    let mut pipefd: [libc::c_int; 2] = [libc::c_int::default(), libc::c_int::default()];

    unsafe { libc::pipe(pipefd.as_mut_ptr()) };

    let res = unsafe { libc::fork() };

    if res == 0 {
        // child
        unsafe {
            libc::close(pipefd[0]);
        }

        let mut expected_flag = 0u8;

        if A.get().is_some() {
            expected_flag = 1u8;
        }

        if !B.get().is_some() {
            expected_flag = 1u8;
        }

        A.get_or_init(|| 2u32);

        if *A.get().unwrap() != 2u32 {
            expected_flag = 1u8;
        }

        B.get_or_init(|| 2u32);

        if *B.get().unwrap() != 1u32 {
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
fn sync_once_cell() {
    static ONCE_CELL: WipeOnForkOnceLock<i32> = WipeOnForkOnceLock::new();

    assert!(ONCE_CELL.get().is_none());

    spawn_and_wait(|| {
        ONCE_CELL.get_or_init(|| 92);
        assert_eq!(ONCE_CELL.get(), Some(&92));
    });

    ONCE_CELL.get_or_init(|| panic!("Kaboom!"));
    assert_eq!(ONCE_CELL.get(), Some(&92));
}

#[test]
fn sync_once_cell_get_mut() {
    let mut c = WipeOnForkOnceLock::new();
    assert!(c.get_mut().is_none());
    c.set(90).unwrap();
    *c.get_mut().unwrap() += 2;
    assert_eq!(c.get_mut(), Some(&mut 92));
}

#[test]
fn sync_once_cell_get_unchecked() {
    let c = WipeOnForkOnceLock::new();
    c.set(92).unwrap();
    assert_eq!(c.get_unchecked(), &92);
}

#[test]
fn sync_once_cell_drop() {
    static DROP_CNT: AtomicUsize = AtomicUsize::new(0);
    struct Dropper;
    impl Drop for Dropper {
        fn drop(&mut self) {
            DROP_CNT.fetch_add(1, SeqCst);
        }
    }

    let x = WipeOnForkOnceLock::new();
    spawn_and_wait(move || {
        x.get_or_init(|| Dropper);
        assert_eq!(DROP_CNT.load(SeqCst), 0);
        drop(x);
    });

    assert_eq!(DROP_CNT.load(SeqCst), 1);
}

#[test]
fn sync_once_cell_drop_empty() {
    let x = WipeOnForkOnceLock::<String>::new();
    drop(x);
}

#[test]
fn clone() {
    let s = WipeOnForkOnceLock::new();
    let c = s.clone();
    assert!(c.get().is_none());

    s.set("hello".to_string()).unwrap();
    let c = s.clone();
    assert_eq!(c.get().map(String::as_str), Some("hello"));
}

#[test]
fn get_or_try_init() {
    let cell: WipeOnForkOnceLock<String> = WipeOnForkOnceLock::new();
    assert!(cell.get().is_none());

    let res = std::panic::catch_unwind(|| cell.get_or_try_init(|| -> Result<_, ()> { panic!() }));
    assert!(res.is_err());
    assert!(!cell.is_initialized());
    assert!(cell.get().is_none());

    assert_eq!(cell.get_or_try_init(|| Err(())), Err(()));

    assert_eq!(
        cell.get_or_try_init(|| Ok::<_, ()>("hello".to_string())),
        Ok(&"hello".to_string())
    );
    assert_eq!(cell.get(), Some(&"hello".to_string()));
}

#[test]
fn from_impl() {
    assert_eq!(WipeOnForkOnceLock::from("value").get(), Some(&"value"));
    assert_ne!(WipeOnForkOnceLock::from("foo").get(), Some(&"bar"));
}

#[test]
fn partialeq_impl() {
    assert!(WipeOnForkOnceLock::from("value") == WipeOnForkOnceLock::from("value"));
    assert!(WipeOnForkOnceLock::from("foo") != WipeOnForkOnceLock::from("bar"));

    assert!(WipeOnForkOnceLock::<String>::new() == WipeOnForkOnceLock::new());
    assert!(WipeOnForkOnceLock::<String>::new() != WipeOnForkOnceLock::from("value".to_owned()));
}

#[test]
fn into_inner() {
    let cell: WipeOnForkOnceLock<String> = WipeOnForkOnceLock::new();
    assert_eq!(cell.into_inner(), None);
    let cell = WipeOnForkOnceLock::new();
    cell.set("hello".to_string()).unwrap();
    assert_eq!(cell.into_inner(), Some("hello".to_string()));
}

#[test]
fn is_sync_send() {
    fn assert_traits<T: Send + Sync>() {}
    assert_traits::<WipeOnForkOnceLock<String>>();
}

#[test]
fn eval_once_macro() {
    macro_rules! eval_once {
        (|| -> $ty:ty {
            $($body:tt)*
        }) => {{
            static ONCE_CELL: WipeOnForkOnceLock<$ty> = WipeOnForkOnceLock::new();
            fn init() -> $ty {
                $($body)*
            }
            ONCE_CELL.get_or_init(init)
        }};
    }

    let fib: &'static Vec<i32> = eval_once! {
        || -> Vec<i32> {
            let mut res = vec![1, 1];
            for i in 0..10 {
                let next = res[i] + res[i + 1];
                res.push(next);
            }
            res
        }
    };
    assert_eq!(fib[5], 8)
}

#[test]
fn sync_once_cell_does_not_leak_partially_constructed_boxes() {
    static ONCE_CELL: WipeOnForkOnceLock<String> = WipeOnForkOnceLock::new();

    let n_readers = 10;
    let n_writers = 3;
    const MSG: &str = "Hello, World";

    let (tx, rx) = std::sync::mpsc::channel();

    for _ in 0..n_readers {
        let tx = tx.clone();
        thread::spawn(move || loop {
            if let Some(msg) = ONCE_CELL.get() {
                tx.send(msg).unwrap();
                break;
            }
        });
    }
    for _ in 0..n_writers {
        thread::spawn(move || {
            let _ = ONCE_CELL.set(MSG.to_owned());
        });
    }

    for _ in 0..n_readers {
        let msg = rx.recv().unwrap();
        assert_eq!(msg, MSG);
    }
}

#[test]
fn dropck() {
    let cell = WipeOnForkOnceLock::new();
    {
        let s = String::new();
        cell.set(&s).unwrap();
    }
}
