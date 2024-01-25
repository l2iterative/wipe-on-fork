mod once_cell;

pub use once_cell::WipeOnForkOnceCell;

mod lazy_cell;
pub use lazy_cell::WipeOnForkLazyCell;

mod once_lock;
pub use once_lock::WipeOnForkOnceLock;
mod lazy_lock;
pub use lazy_lock::WipeOnForkLazyLock;

mod once;
pub use once::{WipeOnForkOnce, WIPE_ON_FORK_ONCE_INIT};

mod utils;

#[cfg(test)]
mod tests;
