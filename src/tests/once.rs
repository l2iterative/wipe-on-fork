use crate::WipeOnForkOnce;
use std::sync::Once;

static A: WipeOnForkOnce = WipeOnForkOnce::new();
static B: Once = Once::new();

#[test]
#[cfg(unix)]
fn wipe_on_fork() {
    A.call_once(|| {});
    B.call_once(|| {});

    assert_eq!(A.is_completed(), true);
    assert_eq!(B.is_completed(), true);

    let mut pipefd: [libc::c_int; 2] = [libc::c_int::default(), libc::c_int::default()];

    unsafe { libc::pipe(pipefd.as_mut_ptr()) };

    let res = unsafe { libc::fork() };

    if res == 0 {
        // child
        unsafe {
            libc::close(pipefd[0]);
        }

        let mut test_val = 0u8;
        let mut expected_flag = 0u8;

        if A.is_completed() {
            expected_flag = 1u8;
        }

        if !B.is_completed() {
            expected_flag = 1u8;
        }

        A.call_once(|| {
            test_val = 1u8;
        });

        if test_val != 1u8 {
            expected_flag = 1u8;
        }

        B.call_once(|| {
            test_val = 0u8;
        });

        if test_val == 0u8 {
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

#[test]
fn smoke_once() {
    static O: WipeOnForkOnce = WipeOnForkOnce::new();
    let mut a = 0;
    O.call_once(|| a += 1);
    assert_eq!(a, 1);
    O.call_once(|| a += 1);
    assert_eq!(a, 1);
}

#[test]
fn stampede_once() {
    static O: WipeOnForkOnce = WipeOnForkOnce::new();
    static mut RUN: bool = false;

    let (tx, rx) = std::sync::mpsc::channel();
    for _ in 0..10 {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for _ in 0..4 {
                std::thread::yield_now()
            }
            unsafe {
                O.call_once(|| {
                    assert!(!RUN);
                    RUN = true;
                });
                assert!(RUN);
            }
            tx.send(()).unwrap();
        });
    }

    unsafe {
        O.call_once(|| {
            assert!(!RUN);
            RUN = true;
        });
        assert!(RUN);
    }

    for _ in 0..10 {
        rx.recv().unwrap();
    }
}

#[test]
fn poison_bad() {
    static O: WipeOnForkOnce = WipeOnForkOnce::new();

    // poison the once
    let t = std::panic::catch_unwind(|| {
        O.call_once(|| panic!());
    });
    assert!(t.is_err());

    // poisoning propagates
    let t = std::panic::catch_unwind(|| {
        O.call_once(|| {});
    });
    assert!(t.is_err());

    // we can subvert poisoning, however
    let mut called = false;
    O.call_once_force(|p| {
        called = true;
        assert!(p.is_poisoned())
    });
    assert!(called);

    // once any success happens, we stop propagating the poison
    O.call_once(|| {});
}

#[test]
fn wait_for_force_to_finish() {
    static O: WipeOnForkOnce = WipeOnForkOnce::new();

    // poison the once
    let t = std::panic::catch_unwind(|| {
        O.call_once(|| panic!());
    });
    assert!(t.is_err());

    // make sure someone's waiting inside the once via a force
    let (tx1, rx1) = std::sync::mpsc::channel();
    let (tx2, rx2) = std::sync::mpsc::channel();
    let t1 = std::thread::spawn(move || {
        O.call_once_force(|p| {
            assert!(p.is_poisoned());
            tx1.send(()).unwrap();
            rx2.recv().unwrap();
        });
    });

    rx1.recv().unwrap();

    // put another waiter on the once
    let t2 = std::thread::spawn(|| {
        let mut called = false;
        O.call_once(|| {
            called = true;
        });
        assert!(!called);
    });

    tx2.send(()).unwrap();

    assert!(t1.join().is_ok());
    assert!(t2.join().is_ok());
}
