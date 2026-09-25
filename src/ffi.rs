//! Conversions for types in [`core::ffi`].

use core::ffi::{CStr, c_char, c_void};

#[cfg(feature = "alloc")]
use alloc::ffi::CString;

use crate::{
    CType, ReprC,
    borrow::{Borrow, BorrowCast, BorrowCastMut, FromBorrow},
    transmute::CheckedTransmute,
    wide::Wide,
};
#[cfg(feature = "alloc")]
use crate::{
    Decode, Encode,
    boxed::CBoxedSlice,
    niche::Niche,
    stored::{DecodeOwned, EncodeOwned, Store},
};

unsafe impl Borrow for c_void {
    type Borrowed<'itm>
        = Self
    where
        Self: 'itm;
    type Owner = ();

    fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        self
    }
}
impl<'itm> FromBorrow<'itm> for c_void {
    fn from_borrow(source: Self) -> Self {
        source
    }
}
impl ReprC for c_void {
    type CType = Self;
}
unsafe impl CType for c_void {}
unsafe impl BorrowCast for c_void {
    type AsConst = Self;
}
unsafe impl BorrowCastMut for c_void {
    type AsMut = Self;
}

impl ReprC for CStr {
    type CType = [c_char];
}
unsafe impl CheckedTransmute for CStr {
    unsafe fn is_valid(value: &[c_char]) -> bool {
        value.last() == Some(&0)
            && value[..value.len().saturating_sub(1)]
                .iter()
                .all(|&c| c != 0)
    }
}
impl Wide for CStr {
    type Data = c_char;
    type Metadata = usize;

    fn metadata(&self) -> Self::Metadata {
        self.to_bytes_with_nul().len()
    }

    fn as_ptr(&self) -> *const Self::Data {
        self.as_ptr()
    }

    fn as_mut_ptr(&mut self) -> *mut Self::Data {
        self.as_ptr().cast_mut()
    }

    #[cfg(feature = "alloc")]
    fn into_non_null(self: alloc::boxed::Box<Self>) -> core::ptr::NonNull<Self::Data> {
        let bytes = self
            .into_c_string()
            .into_bytes_with_nul()
            .into_boxed_slice();
        unsafe { core::ptr::NonNull::new_unchecked(alloc::boxed::Box::into_raw(bytes).cast()) }
    }

    unsafe fn from_raw_parts<'a>(data: *const Self::Data, len: Self::Metadata) -> &'a Self {
        let bytes = unsafe { core::slice::from_raw_parts(data.cast(), len) };
        unsafe { CStr::from_bytes_with_nul_unchecked(bytes) }
    }

    unsafe fn from_raw_parts_mut<'a>(data: *mut Self::Data, len: Self::Metadata) -> &'a mut Self {
        let bytes = core::ptr::slice_from_raw_parts_mut(data.cast::<u8>(), len);
        unsafe { &mut *(bytes as *mut CStr) }
    }

    #[cfg(feature = "alloc")]
    unsafe fn from_non_null(
        data: core::ptr::NonNull<Self::Data>,
        len: Self::Metadata,
    ) -> alloc::boxed::Box<Self> {
        let bytes = unsafe {
            alloc::boxed::Box::from_raw(core::ptr::slice_from_raw_parts_mut(
                data.as_ptr().cast::<u8>(),
                len,
            ))
        };
        unsafe { CString::from_vec_with_nul_unchecked(bytes.into_vec()) }.into_boxed_c_str()
    }
}

#[cfg(feature = "alloc")]
impl ReprC for CString {
    type CType = CBoxedSlice<c_char>;
}

#[cfg(feature = "alloc")]
impl Store for CString {
    fn sync(self) -> Option<()> {
        Some(())
    }
}

#[cfg(feature = "alloc")]
unsafe impl EncodeOwned for CString {
    type Store = ();

    fn soft_encode<'a>(self, (): &'a mut Self::Store) -> Self::CType
    where
        Self: 'a,
    {
        CBoxedSlice::from_boxed_slice(
            self.into_bytes_with_nul()
                .into_iter()
                .map(|byte| byte as c_char)
                .collect(),
        )
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'d> DecodeOwned<'d> for CString {
    type Store = ();

    unsafe fn soft_decode<'a: 'd>(source: Self::CType, _: &mut ()) -> Option<Self> {
        let bytes = unsafe { source.into_rust()? };
        CString::from_vec_with_nul(
            bytes
                .into_vec()
                .into_iter()
                .map(|byte| byte as u8)
                .collect(),
        )
        .ok()
    }
}

#[cfg(feature = "alloc")]
impl Encode for CString {}
#[cfg(feature = "alloc")]
impl Decode<'_> for CString {}
#[cfg(feature = "alloc")]
impl Niche for CString {
    const NICHE_VALUE: Self::CType = CBoxedSlice::NICHE_VALUE;
}
