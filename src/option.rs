//! FFI-safe equivalent of [`core::option`] related functionality

use crate::{FfiReturn, ReprC, mineral};

/// FFI-safe equivalent of [`core::option::Option`] for [`crate::ir::Robust`] types
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(C)]
pub struct COption<T> {
    tag: u8,
    payload: T,
}

impl<T> COption<T> {
    /// Construct no value
    #[expect(non_snake_case)]
    pub const fn None() -> Self {
        Self {
            tag: 0,
            // SAFETY: `ReprC` type is robust and can't have any trap representations
            payload: unsafe { core::mem::zeroed() },
        }
    }

    /// Construct some value
    #[expect(non_snake_case)]
    pub const fn Some(value: T) -> Self {
        Self {
            tag: 1,
            payload: value,
        }
    }

    pub(crate) const fn niche() -> Self {
        Self {
            tag: 2,
            payload: unsafe { core::mem::zeroed() },
        }
    }
}

impl<T> From<Option<T>> for COption<T> {
    fn from(value: Option<T>) -> Self {
        match value {
            Some(value) => Self::Some(value),
            None => Self::None(),
        }
    }
}

impl<T> TryFrom<COption<T>> for Option<T> {
    type Error = FfiReturn;

    fn try_from(value: COption<T>) -> Result<Self, Self::Error> {
        match value.tag {
            0 => Ok(None),
            1 => Ok(Some(value.payload)),
            _ => Err(FfiReturn::TrapRepresentation),
        }
    }
}

impl<T: Copy> Copy for COption<T> {}
impl<T: Copy> Clone for COption<T> {
    fn clone(&self) -> Self {
        *self
    }
}

mineral! {
    unsafe impl(T: ReprC) Robust for COption<T> {}
}
