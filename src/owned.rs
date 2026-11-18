//! Ownership markers for lowered ABI values.

use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, ReprC,
    convert::{Decode, DecodeMove, Encode, EncodeMove, EncodeMoveUnchecked, EncodeUnchecked},
    transmute::{AbiCompatible, AbiCompatibleValidate},
};

/// Marks an ABI payload that transfers ownership when passed by value.
///
/// The Rust owner defines how to release the resource. This carrier has the
/// same ABI as its payload and does not destroy the resource itself.
/// Copies of a carrier still refer to one resource and can only be consumed once in total.
#[derive(RustSpec, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct Owned<T: CType + ?Sized>(pub T);

impl<T: CType + ?Sized> ReprC for Owned<T> {
    type CType = Self;
}

unsafe impl<T: CType + ?Sized> CType for Owned<T> {}
unsafe impl<T: CType + ?Sized> AbiCompatibleValidate for Owned<T> {
    unsafe fn is_valid(_: &Self) -> bool {
        true
    }
}
unsafe impl<T: CType + ?Sized> AbiCompatible for Owned<T> {}
unsafe impl<T: CFnArg> CFnArg for Owned<T> {}
unsafe impl<T: CFnReturn> CFnReturn for Owned<T> {}

unsafe impl<T: CType> EncodeMoveUnchecked for Owned<T> {
    type SoftStore = ();

    unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self
    where
        Self: 'itm,
    {
        self
    }
}

unsafe impl<T: CType> EncodeMove for Owned<T> {
    type Store = ();

    fn soft_encode<'itm>(self, (): &'itm mut ()) -> Self
    where
        Self: 'itm,
    {
        self
    }
}

unsafe impl<'d, T: CType> DecodeMove<'d> for Owned<T> {
    type Store = ();

    unsafe fn soft_decode<'itm: 'd>(source: Self, (): &'itm mut ()) -> Option<Self> {
        Some(source)
    }
}

impl<T: CType> Encode for Owned<T> {}
impl<T: CType> EncodeUnchecked for Owned<T> {}
impl<T: CType> Decode<'_> for Owned<T> {}
crate::__impl_borrowed_identity!([T: CType] Owned<T>);
