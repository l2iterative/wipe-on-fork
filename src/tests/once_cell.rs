use crate::once_cell::WipeOnForkOnceCell;

#[test]
fn test_once_cell() {
    let a = WipeOnForkOnceCell::<u32>::new();
    a.set(1).unwrap();
    assert_eq!(*a.get().unwrap(), 1);
}

#[should_panic]
#[test]
fn test_once_cell_write_twice() {
    let a = WipeOnForkOnceCell::<u32>::new();
    a.set(1).unwrap();
    a.set(1).unwrap();
}

#[test]
#[cfg(unix)]
fn wipe_on_fork() {
    use core::cell::OnceCell;

    let a = WipeOnForkOnceCell::<u32>::new();
    let b = OnceCell::<u32>::new();

    let _ = a.get_or_init(|| 1u32);
    let _ = b.get_or_init(|| 1u32);

    let mut pipefd: [libc::c_int; 2] = [libc::c_int::default(), libc::c_int::default()];

    unsafe { libc::pipe(pipefd.as_mut_ptr()) };

    let res = unsafe { libc::fork() };

    if res == 0 {
        // child
        unsafe {
            libc::close(pipefd[0]);
        }

        let mut expected_flag = 0u8;

        if !a.get().is_none() {
            expected_flag = 1u8;
        }

        if !b.get().is_some() {
            expected_flag = 1u8;
        }

        let _ = a.get_or_init(|| 2u32);
        let _ = b.get_or_init(|| 2u32);

        if !a.get().unwrap() == 2 {
            expected_flag = 1u8;
        }
        if !b.get().unwrap() == 1 {
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

        assert_eq!(*a.get().unwrap(), 1);
        assert_eq!(*b.get().unwrap(), 1);

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
