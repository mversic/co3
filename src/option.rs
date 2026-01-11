//! FFI-safe equivalent of [`core::option`] related functionality

use crate::{ReprC, mineral};

pub use private::Option as COption;

mod private {
    use super::*;

    /// FFI-safe equivalent of [`core::option::Option`] for [`crate::ir::Robust`] types
    #[repr(C)]
    pub struct Option<T: ReprC> {
        pub tag: u8,
        pub payload: T,
    }
}

impl<T: ReprC> Copy for COption<T> {}
impl<T: ReprC> Clone for COption<T> {
    fn clone(&self) -> Self {
        *self
    }
}

mineral! {
    unsafe impl(T: ReprC) Robust for COption<T> {}
}
