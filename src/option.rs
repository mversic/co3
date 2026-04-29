//! FFI-safe equivalent of [`core::option`] related functionality

use crate::{
    FfiReturn, ReprC,
    borrow::{Borrow, DropFamily, ToOwned},
    reprC,
};

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

    pub(crate) const fn none() -> Self {
        Self {
            tag: 2,
            payload: unsafe { core::mem::zeroed() },
        }
    }
}

impl<R: DropFamily> DropFamily for Option<R> {
    type Kind = R::Kind;
}

impl<R: Borrow<true>> Borrow<true> for Option<R> {
    type Borrowed<'itm>
        = Option<R::Borrowed<'itm>>
    where
        Self: 'itm;

    type Store = R::Store;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        self.map(|value| value.borrow(store))
    }
}

impl<'r, R: ToOwned<'r, true>> ToOwned<'r, true> for Option<R> {
    #[inline(always)]
    fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
        borrowed.map(R::to_owned)
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

reprC! {
    unsafe impl(T: ReprC) SizedRobust for COption<T> {}
}
