//! Unsized C representation of a UTF-8 string.

#[cfg(feature = "alloc")]
use alloc::{boxed::Box, string::String};
#[cfg(feature = "alloc")]
use core::ptr::NonNull;

use rust_spec::RustSpec;

#[cfg(feature = "alloc")]
use crate::{
    CFnArg, CFnReturn, Decode, Encode,
    borrow::Borrow,
    boxed::{CWideBox, OwnedBoxCarrier},
    niche::Niche,
    primitives::primitive_derive,
    stored::{DecodeMove, EncodeMove, EncodeMoveUnchecked},
    vec::CVec,
};
use crate::{
    CType, ReprC,
    transmute::{AbiCompatible, AbiCompatibleValidate},
    wide::Wide,
};

/// C-compatible byte region used as the representation of `str`.
///
/// Every byte value is valid for this carrier; conversion to `str` accepts only valid `UTF-8`;
#[derive(RustSpec)]
#[repr(transparent)]
pub struct Utf8Str(pub [u8]);

unsafe impl CType for Utf8Str {}

impl ReprC for Utf8Str {
    type CType = Self;
}

unsafe impl AbiCompatibleValidate for Utf8Str {
    unsafe fn is_valid(_: &Self::CType) -> bool {
        true
    }
}
unsafe impl AbiCompatible for Utf8Str {}

unsafe impl Wide for Utf8Str {
    type Header = [u8; 0];
    type Metadata = usize;

    fn metadata(ptr: *const Self) -> usize {
        (ptr as *const [u8]).len()
    }

    fn as_ptr(ptr: *const Self) -> *const Self::Header {
        ptr.cast()
    }

    fn as_mut_ptr(ptr: *mut Self) -> *mut Self::Header {
        ptr.cast()
    }

    unsafe fn from_raw_parts<'a>(data: *const Self::Header, len: usize) -> &'a Self {
        let bytes = unsafe { core::slice::from_raw_parts(data.cast(), len) };
        unsafe { &*(bytes as *const [u8] as *const Self) }
    }

    unsafe fn from_raw_parts_mut<'a>(data: *mut Self::Header, len: usize) -> &'a mut Self {
        let bytes = unsafe { core::slice::from_raw_parts_mut(data.cast(), len) };
        unsafe { &mut *(bytes as *mut [u8] as *mut Self) }
    }

    #[cfg(feature = "alloc")]
    fn into_non_null(self: Box<Self>) -> NonNull<Self::Header> {
        Box::into_non_null(self).cast()
    }

    #[cfg(feature = "alloc")]
    unsafe fn from_non_null(data: NonNull<Self::Header>, len: usize) -> Box<Self> {
        let ptr = core::ptr::slice_from_raw_parts_mut(data.as_ptr().cast::<u8>(), len);
        unsafe { Box::from_raw(ptr as *mut Self) }
    }
}

/// Owned UTF-8 string carrier, including the capacity of its allocation.
///
/// The carrier permits any byte values. Decoding as a `String` checks UTF-8.
#[cfg(feature = "alloc")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, RustSpec)]
#[repr(transparent)]
pub struct Utf8String(pub CVec<u8>);

#[cfg(feature = "alloc")]
impl Utf8String {
    pub const fn null() -> Self {
        Self(CVec::null())
    }

    pub fn from_string(value: String) -> Self {
        Self(CVec::from_vec(value.into_bytes()))
    }

    pub const fn as_ptr(self) -> *mut u8 {
        self.0.as_ptr()
    }
    pub const fn len(self) -> usize {
        self.0.len()
    }
    pub const fn capacity(self) -> usize {
        self.0.capacity()
    }
    pub const fn is_empty(self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(feature = "alloc")]
unsafe impl crate::Zeroable for Utf8String {}
#[cfg(feature = "alloc")]
primitive_derive! { Utf8String }

#[cfg(feature = "alloc")]
impl OwnedBoxCarrier<CWideBox<str>> for Utf8String {
    unsafe fn from_box_carrier(value: CWideBox<str>) -> Option<Self> {
        let data = NonNull::new(value.as_ptr())?;
        let slice = core::ptr::slice_from_raw_parts_mut(data.as_ptr(), value.metadata());
        let bytes = unsafe { Box::from_raw(slice) }.into_vec();
        Some(Self(CVec::from_vec(bytes)))
    }

    unsafe fn into_box_carrier(self) -> CWideBox<str> {
        let bytes = unsafe {
            self.0
                .into_vec()
                .expect("encoded string allocation is valid")
        };
        let bytes = bytes.into_boxed_slice();
        let len = bytes.len();
        unsafe { CWideBox::from_raw_parts(Box::into_non_null(bytes).cast(), len) }
    }
}

#[cfg(feature = "alloc")]
impl ReprC for String {
    type CType = Utf8String;
}

#[cfg(feature = "alloc")]
unsafe impl EncodeMove for String {
    type Store = ();

    fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe { self.soft_encode_unchecked(&mut ()) }
    }
}
#[cfg(feature = "alloc")]
unsafe impl EncodeMoveUnchecked for String {
    type SoftStore = ();

    unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        Utf8String::from_string(self)
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'d> DecodeMove<'d> for String {
    type Store = ();

    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        let bytes = unsafe { source.0.into_vec()? };
        Self::from_utf8(bytes).ok()
    }

    unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
        let bytes = unsafe { source.0.into_vec().unwrap_unchecked() };
        unsafe { Self::from_utf8_unchecked(bytes) }
    }
}

#[cfg(feature = "alloc")]
impl Encode for String {}
#[cfg(feature = "alloc")]
impl crate::EncodeUnchecked for String {}
#[cfg(feature = "alloc")]
impl Decode<'_> for String {}

#[cfg(feature = "alloc")]
impl Niche for String {
    const NICHE: Self::CType = Utf8String::null();
}
