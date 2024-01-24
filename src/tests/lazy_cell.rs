use crate::WipeOnForkLazyCell;
use std::ops::Deref;

#[test]
#[cfg(unix)]
fn wipe_on_fork() {
    let a = WipeOnForkLazyCell::new(|| std::process::id());

    let cur_process_id: u32 = a.deref().clone();

    let mut pipefd: [libc::c_int; 2] = [libc::c_int::default(), libc::c_int::default()];

    unsafe { libc::pipe(pipefd.as_mut_ptr()) };

    let res = unsafe { libc::fork() };

    if res == 0 {
        // child
        unsafe {
            libc::close(pipefd[0]);
        }

        let mut expected_flag = 0u8;

        let child_process_id = std::process::id();
        if child_process_id != *a.deref() {
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

        assert_eq!(cur_process_id, std::process::id());
        assert_eq!(*a, std::process::id());

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
