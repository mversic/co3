//! FFI-safe equivalent of [`core::option`] related functionality

use crate::{ExternC, FfiReturn, ReprC, borrow::BorrowCast, niche::Niche, reprC};

/// FFI-safe equivalent of [`core::option::Option`] for [`crate::ir::Robust`] types
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(C)]
pub struct COption<T: Copy> {
    tag: u8,
    payload: T,
}

impl<T: Copy> COption<T> {
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

impl<T: Copy> From<Option<T>> for COption<T> {
    fn from(value: Option<T>) -> Self {
        match value {
            Some(value) => Self::Some(value),
            None => Self::None(),
        }
    }
}

impl<T: Copy> TryFrom<COption<T>> for Option<T> {
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
    unsafe impl(T: ReprC + Copy) SizedRobust for COption<T> {}
}

unsafe impl<T: BorrowCast<AsConst: Copy, AsMut: Copy> + Copy> BorrowCast for COption<T> {
    type AsConst = COption<T::AsConst>;
    type AsMut = COption<T::AsMut>;
}

impl<R, C: Copy> Niche for Option<R>
where
    Self: ExternC<CType = COption<C>>,
{
    const NICHE_VALUE: Self::CType = COption::none();
}
