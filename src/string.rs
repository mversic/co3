//! Stable UTF-8 strings and their C-compatible representation.

#[cfg(feature = "repr-rust")]
use alloc::{boxed::Box, vec::Vec};
#[cfg(feature = "repr-rust")]
use core::ptr::NonNull;

use rust_spec::RustSpec;

#[cfg(feature = "repr-rust")]
use crate::marker::Native;
use crate::{
    CFnArg, CFnReturn, CType, ReprC,
    borrow::{
        BorrowedDecode, BorrowedDecodeUnchecked, BorrowedEncode, BorrowedEncodeUnchecked,
        borrowed_decode_slice,
    },
    convert::{
        AbiValidate, Decode, DecodeMove, DecodeMoveUnchecked, DecodeUnchecked, Encode, EncodeMove,
        EncodeMoveUnchecked,
    },
    lang::Language,
    marker::AbiCompatible,
    niche::Niche,
    primitive::primitive_derive,
    vec::{self, Vec as StableVec},
};

/// An owned UTF-8 string with a stable pointer, length, and capacity layout.
#[derive(RustSpec, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct String(StableVec<u8>);

impl String {
    pub const fn new() -> Self {
        Self(StableVec::new())
    }
    pub fn with_capacity(capacity: usize) -> Self {
        Self(StableVec::with_capacity(capacity))
    }
    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }
    pub fn into_bytes(self) -> StableVec<u8> {
        self.0
    }
    pub fn from_utf8(bytes: StableVec<u8>) -> Result<Self, alloc::string::FromUtf8Error> {
        alloc::string::String::from_utf8(bytes.into()).map(Self::from)
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
impl From<alloc::string::String> for String {
    fn from(value: alloc::string::String) -> Self {
        Self(value.into_bytes().into())
    }
}

impl From<&str> for String {
    fn from(value: &str) -> Self {
        Self(value.as_bytes().to_vec().into())
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
    type CType = abi::String;
}
unsafe impl AbiCompatible for String {}
unsafe impl<L: Language> AbiValidate<L> for String {
    unsafe fn is_valid(value: &Self::CType) -> bool {
        if value.len() > value.capacity() {
            return false;
        }
        let Some(bytes) =
            (unsafe { crate::borrow::borrowed_decode_slice(value.as_ptr(), value.len()) })
        else {
            return false;
        };
        crate::lang::checks::utf8::<L>(bytes)
    }
}
unsafe impl EncodeMoveUnchecked for String {
    type SoftStore = ();
    unsafe fn soft_encode_unchecked<'itm>(self, _: &'itm mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        abi::String::from_string(self.into())
    }
}
unsafe impl<L: Language> EncodeMove<L> for String {
    type Store = ();
    fn soft_encode<'itm>(self, _: &'itm mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        abi::String::from_string(self.into())
    }
}
unsafe impl<'d> DecodeMoveUnchecked<'d> for String {
    type SoftStore = ();

    unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, _: &'itm mut ()) -> Self {
        let bytes = unsafe { source.0.into_vec().unwrap_unchecked() };
        unsafe { alloc::string::String::from_utf8_unchecked(bytes) }.into()
    }
}
unsafe impl<'d, L: Language> DecodeMove<'d, L> for String {
    type Store = ();
    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, _: &'itm mut ()) -> Option<Self> {
        let bytes = unsafe { source.0.into_vec()? };
        crate::lang::checks::utf8::<L>(&bytes)
            .then(|| unsafe { alloc::string::String::from_utf8_unchecked(bytes) }.into())
    }
}
impl crate::convert::EncodeUnchecked for String {}
impl<L: Language> Encode<L> for String {}
impl DecodeUnchecked<'_> for String {}
impl<L: Language> Decode<'_, L> for String {}
impl Niche for String {
    const NICHE: Self::CType = abi::String::null();
    fn is_niche(value: &Self::CType) -> bool {
        value.as_ptr().is_null()
    }
}
unsafe impl BorrowedEncodeUnchecked for String {
    type Owner = <StableVec<u8> as BorrowedEncodeUnchecked>::Owner;
    type SoftStore = ();
    unsafe fn soft_encode_unchecked<'itm>(
        self,
        store: &'itm mut (),
        owner: &'itm mut Self::Owner,
    ) -> Self::CType
    where
        Self: 'itm,
    {
        abi::String(unsafe { BorrowedEncodeUnchecked::soft_encode_unchecked(self.0, store, owner) })
    }
}
unsafe impl<L: Language> BorrowedEncode<L> for String {
    type Store = <StableVec<u8> as BorrowedEncode<L>>::Store;
    fn soft_encode<'itm>(
        self,
        store: &'itm mut Self::Store,
        owner: &'itm mut Self::Owner,
    ) -> Self::CType
    where
        Self: 'itm,
    {
        abi::String(BorrowedEncode::<L>::soft_encode(self.0, store, owner))
    }
}
unsafe impl<'d> BorrowedDecodeUnchecked<'d> for String {
    type SoftStore = ();

    unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, _: &'itm mut ()) -> Self {
        let bytes = unsafe { core::slice::from_raw_parts(source.as_ptr(), source.len()) };
        unsafe { core::str::from_utf8_unchecked(bytes) }.into()
    }
}
unsafe impl<'d, L: Language> BorrowedDecode<'d, L> for String {
    type Store = ();
    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, _: &'itm mut ()) -> Option<Self> {
        if source.len() > source.capacity() {
            return None;
        }
        let bytes = unsafe { borrowed_decode_slice(source.as_ptr(), source.len())? };
        Some(unsafe { crate::lang::checks::decode_utf8::<L>(bytes) }?.into())
    }
}

impl From<String> for alloc::string::String {
    fn from(value: String) -> Self {
        // The private byte vector always contains valid UTF-8.
        unsafe { Self::from_utf8_unchecked(value.0.into()) }
    }
}

#[cfg(feature = "repr-rust")]
impl ReprC for alloc::string::String {
    type CType = Native<abi::String>;
}

#[cfg(feature = "repr-rust")]
unsafe impl<L: Language> EncodeMove<L> for alloc::string::String {
    type Store = ();

    fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe { <Self as EncodeMoveUnchecked>::soft_encode_unchecked(self, &mut ()) }
    }
}
#[cfg(feature = "repr-rust")]
unsafe impl EncodeMoveUnchecked for alloc::string::String {
    type SoftStore = ();

    unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        Native(abi::String::from_string(self))
    }
}

#[cfg(feature = "repr-rust")]
unsafe impl<'d> DecodeMoveUnchecked<'d> for alloc::string::String {
    type SoftStore = ();

    unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
        let bytes = unsafe { source.0.0.into_vec().unwrap_unchecked() };
        unsafe { Self::from_utf8_unchecked(bytes) }
    }
}
#[cfg(feature = "repr-rust")]
unsafe impl<'d, L: Language> DecodeMove<'d, L> for alloc::string::String {
    type Store = ();

    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        let bytes = unsafe { source.0.0.into_vec()? };
        crate::lang::checks::utf8::<L>(&bytes).then(|| unsafe { Self::from_utf8_unchecked(bytes) })
    }
}

#[cfg(feature = "repr-rust")]
impl crate::convert::EncodeUnchecked for alloc::string::String {}
#[cfg(feature = "repr-rust")]
impl<L: Language> Encode<L> for alloc::string::String {}
#[cfg(feature = "repr-rust")]
impl DecodeUnchecked<'_> for alloc::string::String {}
#[cfg(feature = "repr-rust")]
impl<L: Language> Decode<'_, L> for alloc::string::String {}

#[cfg(feature = "repr-rust")]
impl Niche for alloc::string::String {
    const NICHE: Self::CType = Native(abi::String::null());

    fn is_niche(value: &Self::CType) -> bool {
        value.as_ptr().is_null()
    }
}
#[cfg(feature = "repr-rust")]
unsafe impl<'d> BorrowedDecodeUnchecked<'d> for alloc::string::String {
    type SoftStore = <String as BorrowedDecodeUnchecked<'d>>::SoftStore;

    unsafe fn soft_decode_unchecked<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::SoftStore,
    ) -> Self {
        unsafe { <String as BorrowedDecodeUnchecked<'d>>::soft_decode_unchecked(source.0, store) }
            .into()
    }
}
#[cfg(feature = "repr-rust")]
unsafe impl<'d, L: Language> BorrowedDecode<'d, L> for alloc::string::String {
    type Store = <String as BorrowedDecode<'d, L>>::Store;

    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        unsafe { <String as BorrowedDecode<'d, L>>::soft_decode(source.0, store) }.map(Into::into)
    }
}

#[cfg(feature = "repr-rust")]
unsafe impl BorrowedEncodeUnchecked for alloc::string::String {
    type Owner = <Vec<u8> as BorrowedEncodeUnchecked>::Owner;
    type SoftStore = <Vec<u8> as BorrowedEncodeUnchecked>::SoftStore;

    unsafe fn soft_encode_unchecked<'itm>(
        self,
        store: &'itm mut Self::SoftStore,
        owner: &'itm mut Self::Owner,
    ) -> Self::CType
    where
        Self: 'itm,
    {
        let carrier = unsafe {
            <Vec<u8> as BorrowedEncodeUnchecked>::soft_encode_unchecked(
                self.into_bytes(),
                store,
                owner,
            )
        };
        Native(crate::string::abi::String(carrier.0))
    }
}

#[cfg(feature = "repr-rust")]
unsafe impl<L: Language> BorrowedEncode<L> for alloc::string::String {
    type Store = <Vec<u8> as BorrowedEncode<L>>::Store;

    fn soft_encode<'itm>(
        self,
        store: &'itm mut Self::Store,
        owner: &'itm mut Self::Owner,
    ) -> Self::CType
    where
        Self: 'itm,
    {
        let carrier = <Vec<u8> as BorrowedEncode<L>>::soft_encode(self.into_bytes(), store, owner);
        Native(crate::string::abi::String(carrier.0))
    }
}

impl crate::convert::AssignFromOwned for str {
    #[inline(always)]
    fn assign_from_owned(&mut self, owned: Self::Owned) -> Option<()> {
        assert!(self.len() == owned.len());

        // The decoded owned string has already been validated as UTF-8.
        unsafe { self.as_bytes_mut() }.copy_from_slice(owned.as_bytes());
        Some(())
    }
}

#[doc(hidden)]
pub mod abi {
    use super::{
        AbiCompatible, CFnArg, CFnReturn, CType, Decode, DecodeMove, DecodeMoveUnchecked,
        DecodeUnchecked, Encode, EncodeMove, ReprC, RustSpec, primitive_derive, vec,
    };
    #[cfg(feature = "repr-rust")]
    use super::{Box, NonNull};
    use crate::lang::Language;
    #[cfg(feature = "repr-rust")]
    use crate::{boxed::OwnedBoxCarrier, marker::Native, wide};

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

        pub fn from_string(value: alloc::string::String) -> Self {
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

    #[cfg(feature = "repr-rust")]
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

    #[cfg(feature = "repr-rust")]
    impl OwnedBoxCarrier<wide::abi::WideBox<<str as crate::ReprC>::CType>> for Native<String> {
        unsafe fn from_box_carrier(
            value: wide::abi::WideBox<<str as crate::ReprC>::CType>,
        ) -> Option<Self> {
            unsafe { String::from_box_carrier(value) }.map(Self)
        }

        unsafe fn into_box_carrier(self) -> wide::abi::WideBox<<str as crate::ReprC>::CType> {
            unsafe { self.0.into_box_carrier() }
        }
    }
}

#[cfg(all(test, feature = "derive"))]
mod tests {
    use super::String;
    use crate::{convert::Store, lang};

    #[test]
    fn raw_mutable_string_writes_back_valid_utf8() {
        fn append(value: &mut String) {
            value.push_str("🦀");
        }
        crate::raw! { fn append(#[soft] value: &mut String); }
        let mut value = String::from("hello");
        let mut store = Default::default();
        let carrier = unsafe { crate::soft_encode::<_, lang::C>(&mut value, &mut store) };
        unsafe { append_raw(crate::marker::Borrowed(carrier)) };
        store.sync().unwrap();
        assert_eq!(&*value, "hello🦀");
    }
}
