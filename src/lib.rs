mod once_cell;
pub use once_cell::WipeOnForkOnceCell;

mod lazy_cell;
pub use lazy_cell::WipeOnForkLazyCell;

#[cfg(std)]
mod once_lock;

#[cfg(std)]
mod lazy_lock;

#[cfg(std)]
mod once;

mod once;
#[cfg(test)]
mod tests;
