//! Stable UTF-8 strings and their C-compatible representation.

use alloc::{boxed::Box, string::String as RustString};
use core::ptr::NonNull;

use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn,
    borrow::{
        BorrowedDecode, BorrowedEncode, BorrowedEncodeUnchecked, ToOwned, borrowed_decode_slice,
    },
    boxed::OwnedBoxCarrier,
    convert::{Decode, DecodeMove, Encode, EncodeMove, EncodeMoveUnchecked},
    niche::Niche,
    primitive::primitive_derive,
    vec, wide,
};
use crate::{CType, ReprC, convert::AbiCompatibleValidate, marker::AbiCompatible};

#[doc(hidden)]
pub mod abi {
    use super::{
        AbiCompatible, Box, CFnArg, CFnReturn, CType, Decode, DecodeMove, Encode, EncodeMove,
        EncodeMoveUnchecked, NonNull, OwnedBoxCarrier, ReprC, RustSpec, RustString,
        primitive_derive, vec, wide,
    };
    /// Owned UTF-8 string carrier, including the capacity of its allocation.
    ///
    /// The carrier permits any byte values. Decoding as a `String` checks UTF-8.
    #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, RustSpec)]
    #[repr(transparent)]
    #[doc(hidden)]
    pub struct String(pub vec::abi::Vec<u8>);

    impl String {
        pub const fn null() -> Self {
            Self(vec::abi::Vec::null())
        }

        pub fn from_string(value: RustString) -> Self {
            Self(vec::abi::Vec::from_vec(value.into_bytes()))
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

    unsafe impl crate::marker::Zeroable for String {}
    primitive_derive! { String }

    impl OwnedBoxCarrier<wide::abi::WideBox<<str as crate::ReprC>::CType>> for String {
        unsafe fn from_box_carrier(
            value: wide::abi::WideBox<<str as crate::ReprC>::CType>,
        ) -> Option<Self> {
            let data = NonNull::new(value.as_ptr())?;
            let slice = core::ptr::slice_from_raw_parts_mut(data.as_ptr(), value.metadata());
            let bytes = unsafe { Box::from_raw(slice) }.into_vec();
            Some(Self(vec::abi::Vec::from_vec(bytes)))
        }

        unsafe fn into_box_carrier(self) -> wide::abi::WideBox<<str as crate::ReprC>::CType> {
            let bytes = unsafe {
                self.0
                    .into_vec()
                    .expect("encoded string allocation is valid")
            };
            let bytes = bytes.into_boxed_slice();
            let len = bytes.len();
            unsafe { wide::abi::WideBox::from_raw_parts(Box::into_non_null(bytes).cast(), len) }
        }
    }
}

impl ReprC for RustString {
    type CType = abi::String;
}

unsafe impl EncodeMove for RustString {
    type Store = ();

    fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe { <Self as EncodeMoveUnchecked>::soft_encode_unchecked(self, &mut ()) }
    }
}
unsafe impl EncodeMoveUnchecked for RustString {
    type SoftStore = ();

    unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        abi::String::from_string(self)
    }
}

unsafe impl<'d> DecodeMove<'d> for RustString {
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

impl Encode for RustString {}
impl crate::convert::EncodeUnchecked for RustString {}
impl Decode<'_> for RustString {}

impl Niche for RustString {
    const NICHE: Self::CType = abi::String::null();

    fn is_niche(value: &Self::CType) -> bool {
        value.as_ptr().is_null()
    }
}
unsafe impl<'d> BorrowedDecode<'d> for alloc::string::String {
    type Store = ();

    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, _: &'itm mut Self::Store) -> Option<Self> {
        if source.len() > source.capacity() {
            return None;
        }
        let bytes = unsafe { borrowed_decode_slice(source.as_ptr(), source.len())? };
        Some(core::str::from_utf8(bytes).ok()?.to_owned())
    }

    unsafe fn soft_decode_unchecked<'itm: 'd>(
        source: Self::CType,
        _: &'itm mut Self::Store,
    ) -> Self {
        let bytes = unsafe { core::slice::from_raw_parts(source.as_ptr(), source.len()) };
        unsafe { core::str::from_utf8_unchecked(bytes) }.to_owned()
    }
}

unsafe impl BorrowedEncodeUnchecked for alloc::string::String {
    type Owner = <alloc::vec::Vec<u8> as BorrowedEncodeUnchecked>::Owner;
    type SoftStore = <alloc::vec::Vec<u8> as BorrowedEncodeUnchecked>::SoftStore;

    unsafe fn soft_encode_unchecked<'itm>(
        self,
        store: &'itm mut Self::SoftStore,
        owner: &'itm mut Self::Owner,
    ) -> Self::CType
    where
        Self: 'itm,
    {
        let carrier = unsafe {
            <alloc::vec::Vec<u8> as BorrowedEncodeUnchecked>::soft_encode_unchecked(
                self.into_bytes(),
                store,
                owner,
            )
        };
        crate::string::abi::String(carrier)
    }
}

unsafe impl BorrowedEncode for alloc::string::String {
    type Store = <alloc::vec::Vec<u8> as BorrowedEncode>::Store;

    fn soft_encode<'itm>(
        self,
        store: &'itm mut Self::Store,
        owner: &'itm mut Self::Owner,
    ) -> Self::CType
    where
        Self: 'itm,
    {
        let carrier =
            <alloc::vec::Vec<u8> as BorrowedEncode>::soft_encode(self.into_bytes(), store, owner);
        crate::string::abi::String(carrier)
    }
}

/// An owned UTF-8 string with a stable pointer, length, and capacity layout.
#[derive(RustSpec, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct String(vec::Vec<u8>);

impl String {
    pub const fn new() -> Self {
        Self(vec::Vec::new())
    }
    pub fn with_capacity(capacity: usize) -> Self {
        Self(vec::Vec::with_capacity(capacity))
    }
    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }
    pub fn into_bytes(self) -> vec::Vec<u8> {
        self.0
    }
    pub fn from_utf8(bytes: vec::Vec<u8>) -> Result<Self, alloc::string::FromUtf8Error> {
        RustString::from_utf8(bytes.into()).map(Self::from)
    }
    pub fn push_str(&mut self, value: &str) {
        self.0.extend(value.bytes());
    }
    pub fn push(&mut self, value: char) {
        self.push_str(value.encode_utf8(&mut [0; 4]));
    }
    pub fn pop(&mut self) -> Option<char> {
        let value = self.chars().next_back()?;
        for _ in 0..value.len_utf8() {
            self.0.pop();
        }
        Some(value)
    }
}
impl From<RustString> for String {
    fn from(value: RustString) -> Self {
        Self(value.into_bytes().into())
    }
}
impl From<String> for RustString {
    fn from(value: String) -> Self {
        // The private byte vector always contains valid UTF-8.
        unsafe { Self::from_utf8_unchecked(value.0.into()) }
    }
}
impl From<&str> for String {
    fn from(value: &str) -> Self {
        Self(value.bytes().collect())
    }
}
impl core::ops::Deref for String {
    type Target = str;
    fn deref(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.0) }
    }
}
impl core::ops::DerefMut for String {
    fn deref_mut(&mut self) -> &mut str {
        unsafe { core::str::from_utf8_unchecked_mut(&mut self.0) }
    }
}
impl core::fmt::Debug for String {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&**self, f)
    }
}
impl core::fmt::Display for String {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self)
    }
}
impl core::hash::Hash for String {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::hash::Hash::hash(&**self, state);
    }
}
impl AsRef<str> for String {
    fn as_ref(&self) -> &str {
        self
    }
}
impl AsMut<str> for String {
    fn as_mut(&mut self) -> &mut str {
        self
    }
}
impl core::str::FromStr for String {
    type Err = core::convert::Infallible;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(value.into())
    }
}
impl ReprC for String {
    type CType = crate::marker::Stable<abi::String>;
}
unsafe impl AbiCompatible for String {}
unsafe impl AbiCompatibleValidate for String {
    unsafe fn is_valid(value: &Self::CType) -> bool {
        if value.len() > value.capacity() {
            return false;
        }
        let Some(bytes) =
            (unsafe { crate::borrow::borrowed_decode_slice(value.as_ptr(), value.len()) })
        else {
            return false;
        };
        core::str::from_utf8(bytes).is_ok()
    }
}
unsafe impl EncodeMoveUnchecked for String {
    type SoftStore = ();
    unsafe fn soft_encode_unchecked<'itm>(self, _: &'itm mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        crate::marker::Stable(abi::String::from_string(self.into()))
    }
}
unsafe impl EncodeMove for String {
    type Store = ();
    fn soft_encode<'itm>(self, _: &'itm mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        crate::marker::Stable(abi::String::from_string(self.into()))
    }
}
unsafe impl<'d> DecodeMove<'d> for String {
    type Store = ();
    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, _: &'itm mut ()) -> Option<Self> {
        unsafe { <RustString as DecodeMove>::soft_decode(source.0, &mut ()) }.map(Self::from)
    }
    unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, _: &'itm mut ()) -> Self {
        unsafe { <RustString as DecodeMove>::soft_decode_unchecked(source.0, &mut ()) }.into()
    }
}
impl Encode for String {}
impl crate::convert::EncodeUnchecked for String {}
impl Decode<'_> for String {}
impl Niche for String {
    const NICHE: Self::CType = crate::marker::Stable(abi::String::null());
    fn is_niche(value: &Self::CType) -> bool {
        value.as_ptr().is_null()
    }
}
unsafe impl BorrowedEncodeUnchecked for String {
    type Owner = <RustString as BorrowedEncodeUnchecked>::Owner;
    type SoftStore = ();
    unsafe fn soft_encode_unchecked<'itm>(
        self,
        store: &'itm mut (),
        owner: &'itm mut Self::Owner,
    ) -> Self::CType
    where
        Self: 'itm,
    {
        crate::marker::Stable(unsafe {
            <RustString as BorrowedEncodeUnchecked>::soft_encode_unchecked(
                self.into(),
                store,
                owner,
            )
        })
    }
}
unsafe impl BorrowedEncode for String {
    type Store = <RustString as BorrowedEncode>::Store;
    fn soft_encode<'itm>(
        self,
        store: &'itm mut Self::Store,
        owner: &'itm mut Self::Owner,
    ) -> Self::CType
    where
        Self: 'itm,
    {
        crate::marker::Stable(<RustString as BorrowedEncode>::soft_encode(
            self.into(),
            store,
            owner,
        ))
    }
}
unsafe impl<'d> BorrowedDecode<'d> for String {
    type Store = ();
    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut ()) -> Option<Self> {
        unsafe { <RustString as BorrowedDecode>::soft_decode(source.0, store) }.map(Self::from)
    }
    unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut ()) -> Self {
        unsafe { <RustString as BorrowedDecode>::soft_decode_unchecked(source.0, store) }.into()
    }
}

#[cfg(all(test, feature = "derive"))]
mod tests {
    use super::String;
    use crate::convert::Store;

    #[test]
    fn raw_mutable_string_writes_back_valid_utf8() {
        fn append(value: &mut String) {
            value.push_str("🦀");
        }
        crate::raw! { fn append(#[soft] value: &mut String); }
        let mut value = String::from("hello");
        let mut store = Default::default();
        let carrier = crate::soft_encode(&mut value, &mut store);
        unsafe { append_raw(crate::marker::Borrowed(carrier)) };
        store.sync().unwrap();
        assert_eq!(&*value, "hello🦀");
    }
}
