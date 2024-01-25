use crate::utils::GENERATION;

#[test]
#[cfg(unix)]
fn generation_test() {
    let father = GENERATION.get();
    assert_eq!(father, 0);

    let mut pipefd_father_son: [libc::c_int; 2] = [libc::c_int::default(), libc::c_int::default()];

    unsafe { libc::pipe(pipefd_father_son.as_mut_ptr()) };

    let res = unsafe { libc::fork() };

    if res == 0 {
        // son
        unsafe {
            libc::close(pipefd_father_son[0]);
        }

        let mut pipefd_son_grandson: [libc::c_int; 2] =
            [libc::c_int::default(), libc::c_int::default()];

        unsafe { libc::pipe(pipefd_son_grandson.as_mut_ptr()) };

        let res = unsafe { libc::fork() };

        if res == 0 {
            // grandson
            unsafe {
                libc::close(pipefd_son_grandson[0]);
            }

            let grandson = GENERATION.get();

            let mut expected_flag = 0u8;
            if grandson != 2 {
                expected_flag = 1u8;
            }

            unsafe {
                libc::write(
                    pipefd_father_son[1],
                    &expected_flag as *const u8 as *const libc::c_void,
                    1,
                );
                libc::close(pipefd_father_son[1]);
                libc::exit(0);
            }
        } else {
            // son
            unsafe {
                libc::close(pipefd_son_grandson[1]);
            }

            let son = GENERATION.get();

            let mut expected_flag = 2u8;
            unsafe {
                libc::read(
                    pipefd_son_grandson[0],
                    (&mut expected_flag) as *mut u8 as *mut libc::c_void,
                    4,
                );
            }

            if son != 1 {
                expected_flag = 1u8;
            }

            unsafe {
                libc::write(
                    pipefd_father_son[1],
                    &expected_flag as *const u8 as *const libc::c_void,
                    1,
                );
                libc::close(pipefd_father_son[1]);
                libc::exit(0);
            }
        }
    } else {
        // father
        unsafe {
            libc::close(pipefd_father_son[1]);
        }

        let mut expected_flag = 2u8;
        unsafe {
            libc::read(
                pipefd_father_son[0],
                (&mut expected_flag) as *mut u8 as *mut libc::c_void,
                4,
            );
        }

        assert_eq!(expected_flag, 0u8);
    }
}
