#[cfg(feature = "alloc")]
use alloc::string::String;
use core::marker::PhantomData;

#[cfg(feature = "alloc")]
use crate::convert::Owned;
use crate::{
    CFnReturn, CType, ReprC,
    convert::{
        Decode, DecodeMove, EmptyStore, Encode, EncodeMove, EncodeMoveUnchecked, EncodeUnchecked,
    },
    transmute::{AbiCompatible, AbiCompatibleValidate},
    utf8::Utf8Str,
};

crate::__impl_borrowed_identity!([]());

impl ReprC for () {
    type CType = Self;
}
unsafe impl EncodeMove for () {
    type Store = ();

    #[inline(always)]
    fn soft_encode<'itm>(self, (): &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe { self.soft_encode_unchecked(&mut ()) }
    }
}

unsafe impl EncodeMoveUnchecked for () {
    type SoftStore = ();

    unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        self
    }
}
unsafe impl<'d> DecodeMove<'d> for () {
    type Store = ();

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        Some(source)
    }
}

impl Decode<'_> for () {}
impl EncodeUnchecked for () {}
impl Encode for () {}

unsafe impl AbiCompatibleValidate for () {
    #[inline(always)]
    unsafe fn is_valid(_: &Self::CType) -> bool {
        true
    }
}
unsafe impl AbiCompatible for () {}

unsafe impl CType for () {}
crate::reference::impl_direct_mut!([]());
unsafe impl CFnReturn for () {}

unsafe impl EmptyStore for () {}
crate::__impl_borrowed_identity!([T: ?Sized] PhantomData<T>);

impl<T: ?Sized> ReprC for PhantomData<T> {
    type CType = Self;
}
unsafe impl<T: ?Sized> AbiCompatibleValidate for PhantomData<T> {
    #[inline(always)]
    unsafe fn is_valid(_: &Self::CType) -> bool {
        true
    }
}
unsafe impl<T: ?Sized> AbiCompatible for PhantomData<T> {}
unsafe impl<T: ?Sized> EncodeMove for PhantomData<T> {
    type Store = ();

    #[inline(always)]
    fn soft_encode<'itm>(self, (): &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe { self.soft_encode_unchecked(&mut ()) }
    }
}
unsafe impl<T: ?Sized> EncodeMoveUnchecked for PhantomData<T> {
    type SoftStore = ();

    unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        self
    }
}
unsafe impl<'d, T: ?Sized> DecodeMove<'d> for PhantomData<T> {
    type Store = ();

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        Some(source)
    }
}

impl<T: ?Sized> Decode<'_> for PhantomData<T> {}
impl<T: ?Sized> EncodeUnchecked for PhantomData<T> {}
impl<T: ?Sized> Encode for PhantomData<T> {}

unsafe impl<T: ?Sized> CType for PhantomData<T> {}
#[cfg(feature = "alloc")]
impl Owned for str {
    type Owned = String;
}
impl ReprC for str {
    type CType = Utf8Str;
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc::boxed::Box;

    use static_assertions::assert_impl_all;

    use super::*;
    use crate::niche::Niche;
    #[cfg(feature = "alloc")]
    use crate::wide::abi::WideBox as AbiWideBox;

    #[test]
    fn str_is_supported() {
        assert_impl_all!(str: ReprC<CType = Utf8Str>);
        #[cfg(feature = "alloc")]
        assert_impl_all!(String: ReprC<CType = crate::utf8::Utf8String>);

        assert_impl_all!(&str:
            Niche<CType = crate::wide::abi::WideRef<'static, <str as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );

        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut str:
            Niche<CType = crate::wide::abi::SoftWideRefMut<<str as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );

        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<str>:
            Niche<CType = AbiWideBox<<str as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
    }

    #[test]
    fn str_decode_rejects_invalid_utf8() {
        let invalid = [0xff];

        let source =
            unsafe { crate::wide::abi::WideRef::from_raw_parts(invalid.as_ptr(), invalid.len()) };
        let decoded = unsafe { crate::decode::<&str>(source) };
        assert!(decoded.is_none());

        let mut invalid = [0xff];
        let source = unsafe {
            crate::wide::abi::SoftWideRefMut::<_>::from_raw_parts_mut(
                invalid.as_mut_ptr(),
                invalid.len(),
            )
        };
        let decoded = unsafe { crate::decode::<&mut str>(source) };
        assert!(decoded.is_none());

        #[cfg(feature = "alloc")]
        {
            let allocation = Box::into_non_null(Box::new([0xff_u8])).cast();
            let source = unsafe {
                AbiWideBox::<<str as crate::ReprC>::CType>::from_raw_parts(allocation, 1)
            };
            let decoded = unsafe { crate::decode::<Box<str>>(source) };
            assert!(decoded.is_none());
        }
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn owned_strings_round_trip_without_recursive_conversion() {
        let text = String::from("hello 🍃");
        let encoded = crate::encode(text.clone());
        assert_eq!(
            unsafe { crate::decode::<String>(encoded) },
            Some(text.clone())
        );

        let encoded = crate::encode(text.clone().into_boxed_str());
        assert_eq!(
            unsafe { crate::decode::<Box<str>>(encoded) }.as_deref(),
            Some(text.as_str())
        );
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn owned_string_preserves_allocation_and_capacity() {
        let mut text = String::with_capacity(128);
        text.push_str("hello 🍃");
        let pointer = text.as_ptr();
        let capacity = text.capacity();
        let carrier = crate::encode(text);
        assert_eq!(carrier.as_ptr(), pointer.cast_mut());
        assert_eq!(carrier.capacity(), capacity);
        let decoded = unsafe { crate::decode::<String>(carrier) }.unwrap();
        assert_eq!(decoded, "hello 🍃");
        assert_eq!(decoded.as_ptr(), pointer);
        assert_eq!(decoded.capacity(), capacity);

        let invalid = crate::utf8::Utf8String(crate::vec::abi::Vec::from_vec(alloc::vec![0xff]));
        assert!(unsafe { crate::decode::<String>(invalid) }.is_none());
        assert!(unsafe { crate::decode::<String>(crate::utf8::Utf8String::null()) }.is_none());
    }
}
