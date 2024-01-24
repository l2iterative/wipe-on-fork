use core::cell::OnceCell;
#[cfg(not(unix))]
compile_error!("This crate should only be compiled with a Unix target.");

pub struct WipeOnForkOnceCell<T>
{
    pid: Option<u32>,
    once_cell: OnceCell<T>
}

impl<T> WipeOnForkOnceCell<T> {
    #[inline]
    fn check_if_should_wiped(&self) -> bool {
        return match self.pid {
            None => false,
            Some(pid) => {
                pid != std::process::id()
            }
        }
    }


}

impl<T> Default for WipeOnForkOnceCell<T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for WipeOnForkOnceCell<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {

    }
}