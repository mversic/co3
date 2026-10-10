//! C-compatible carriers for Rust UTF-8 strings.

use rust_spec::RustSpec;

use crate::CType;
#[cfg(feature = "alloc")]
use crate::{
    CFnArg, CFnReturn, Decode, Encode, ReprC,
    borrow::{BorrowCast, BorrowCastMut},
    boxed::CBoxedSlice,
    slice::{CSlice, CSliceMut},
    stored::{DecodeOwned, EncodeOwned},
    transmute::CheckedTransmute,
};

/// Unsized byte representation of `str`.
#[derive(RustSpec)]
#[repr(transparent)]
pub struct Utf8Str(pub [u8]);

/// Owning, length-delimited UTF-8 string carrier.
#[cfg(feature = "alloc")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, RustSpec)]
#[repr(transparent)]
pub struct Utf8String(pub CBoxedSlice<u8>);

unsafe impl CType for Utf8Str {}

#[cfg(feature = "alloc")]
impl Utf8String {
    /// Construct a carrier with a null data pointer.
    pub const fn null() -> Self {
        Self(CBoxedSlice::null())
    }
}

#[cfg(feature = "alloc")]
impl ReprC for Utf8String {
    type CType = Self;
}

#[cfg(feature = "alloc")]
unsafe impl EncodeOwned for Utf8String {
    type Store = ();

    fn soft_encode<'itm>(self, _: &mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        self
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'d> DecodeOwned<'d> for Utf8String {
    type Store = ();

    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, _: &mut ()) -> Option<Self> {
        Some(source)
    }
}

#[cfg(feature = "alloc")]
impl Encode for Utf8String {}
#[cfg(feature = "alloc")]
impl Decode<'_> for Utf8String {}

#[cfg(feature = "alloc")]
unsafe impl CheckedTransmute for Utf8String {
    unsafe fn is_valid(_: &Self::CType) -> bool {
        true
    }
}

#[cfg(feature = "alloc")]
unsafe impl CType for Utf8String {}
#[cfg(feature = "alloc")]
unsafe impl CFnArg for Utf8String {}
#[cfg(feature = "alloc")]
unsafe impl CFnReturn for Utf8String {}

#[cfg(feature = "alloc")]
unsafe impl BorrowCast for Utf8String {
    type AsConst = CSlice<u8>;
}
#[cfg(feature = "alloc")]
unsafe impl BorrowCastMut for Utf8String {
    type AsMut = CSliceMut<u8>;
}
