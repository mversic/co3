//! C-compatible representations and conversions for [`core::ptr`].
use core::ptr::NonNull;

use crate::{
    ReprC,
    convert::{
        AbiCompatibleValidate, Decode, DecodeMove, Encode, EncodeMove, EncodeMoveUnchecked,
        EncodeUnchecked,
    },
    marker::{AbiCompatible, NonZero},
};

crate::borrow::__impl_borrowed_identity!([T: ?Sized] NonNull<T>);

impl<T: ?Sized, C: crate::CType> crate::niche::Niche for NonNull<T>
where
    Self: ReprC<CType = NonZero<*mut C>>,
{
    const NICHE: Self::CType = NonZero(core::ptr::null_mut());

    fn is_niche(value: &Self::CType) -> bool {
        value.0.is_null()
    }
}

impl<T: ReprC + ?Sized> ReprC for NonNull<T> {
    type CType = NonZero<*mut T::CType>;
}
unsafe impl<T: AbiCompatible + ?Sized> EncodeMove for NonNull<T> {
    type Store = <*mut T as EncodeMove>::Store;

    #[inline(always)]
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe { self.soft_encode_unchecked(store) }
    }
}
unsafe impl<T: AbiCompatible + ?Sized> EncodeMoveUnchecked for NonNull<T> {
    type SoftStore = ();

    unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        NonZero(unsafe { <*mut T>::soft_encode_unchecked(self.as_ptr(), &mut ()) })
    }
}
unsafe impl<'d, T: AbiCompatible + ?Sized> DecodeMove<'d> for NonNull<T> {
    type Store = <*mut T as DecodeMove<'d>>::Store;

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        let ptr = unsafe { DecodeMove::soft_decode(source.0, store)? };
        NonNull::new(ptr)
    }

    #[inline(always)]
    unsafe fn soft_decode_unchecked<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Self {
        let ptr = unsafe { <*mut T as DecodeMove>::soft_decode_unchecked(source.0, store) };
        unsafe { NonNull::new_unchecked(ptr) }
    }
}

impl<T: AbiCompatible + ?Sized> Decode<'_> for NonNull<T> {}
impl<T: AbiCompatible + ?Sized> EncodeUnchecked for NonNull<T> {}
impl<T: AbiCompatible + ?Sized> Encode for NonNull<T> {}

unsafe impl<T: AbiCompatible + ?Sized> AbiCompatibleValidate for NonNull<T> {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        !target.0.is_null()
    }
}
unsafe impl<T: AbiCompatible + ?Sized> AbiCompatible for NonNull<T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CType, marker::Native};

    #[test]
    fn nonnull_carrier_preserves_nonnull_source_and_null_niche() {
        static_assertions::assert_type_eq_all!(<NonNull<u32> as ReprC>::CType, NonZero<*mut u32>);
        static_assertions::assert_type_eq_all!(
            <Option<NonNull<u32>> as ReprC>::CType,
            Native<crate::option::abi::OptionSome<<NonNull<u32> as crate::ReprC>::CType>>
        );

        static_assertions::assert_not_impl_any!(NonNull<u32>: CType);
        static_assertions::assert_impl_all!(NonZero<*mut u32>: CType);
        assert_eq!(
            unsafe { crate::decode::<NonNull<u32>>(NonZero(core::ptr::null_mut())) },
            None
        );

        let mut value = 7_u32;
        let pointer = NonNull::from(&mut value);
        let encoded = crate::encode(pointer);
        assert_eq!(encoded.0, pointer.as_ptr());
        assert_eq!(
            unsafe { crate::decode::<NonNull<u32>>(encoded) },
            Some(pointer)
        );

        let absent = crate::encode(None::<NonNull<u32>>);
        assert!(absent.0.0.0.is_null());
        assert_eq!(
            unsafe { crate::decode::<Option<NonNull<u32>>>(absent) },
            Some(None)
        );
    }
}
