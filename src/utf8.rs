//! Unsized C representation of a UTF-8 string.

#[cfg(feature = "alloc")]
use alloc::{boxed::Box, string::String};
use core::ptr::NonNull;

use rust_spec::RustSpec;

use crate::{CType, ReprC, transmute::CheckedTransmute, wide::Wide};
#[cfg(feature = "alloc")]
use crate::{
    Decode, Encode,
    boxed::CWideBox,
    niche::Niche,
    stored::{DecodeOwned, DirectEncodeOwned, EncodeOwned},
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

unsafe impl CheckedTransmute for Utf8Str {
    unsafe fn is_valid(_: &Self::CType) -> bool {
        true
    }
}

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

#[cfg(feature = "alloc")]
impl ReprC for String {
    type CType = CWideBox<Utf8Str>;
}

#[cfg(feature = "alloc")]
unsafe impl EncodeOwned for String {
    type Store = ();

    fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe { self.soft_encode_unchecked(&mut ()) }
    }
}
#[cfg(feature = "alloc")]
unsafe impl DirectEncodeOwned for String {
    type SoftStore = ();

    unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
        let bytes = self.into_bytes().into_boxed_slice();
        let len = bytes.len();
        let data = Box::into_non_null(bytes).cast::<u8>();
        unsafe { CWideBox::from_raw_parts(data, len) }
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'d> DecodeOwned<'d> for String {
    type Store = ();

    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        if source.as_ptr().is_null() {
            return None;
        }
        let bytes = unsafe {
            Box::from_raw(core::ptr::slice_from_raw_parts_mut(
                source.as_ptr(),
                source.len(),
            ))
        };
        String::from_utf8(bytes.into_vec()).ok()
    }

    unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
        let bytes = unsafe {
            Box::from_raw(core::ptr::slice_from_raw_parts_mut(
                source.as_ptr(),
                source.len(),
            ))
        };
        unsafe { String::from_utf8_unchecked(bytes.into_vec()) }
    }
}

#[cfg(feature = "alloc")]
impl Encode for String {}
#[cfg(feature = "alloc")]
impl crate::DirectEncode for String {}
#[cfg(feature = "alloc")]
impl Decode<'_> for String {}

#[cfg(feature = "alloc")]
impl Niche for String {
    const NICHE: Self::CType = CWideBox::null();
}
