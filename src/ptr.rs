//! C-compatible representations and conversions for [`core::ptr`].

use core::ptr::NonNull;

use crate::{
    ReprC,
    convert::{
        AbiValidate, Decode, DecodeMove, DecodeMoveUnchecked, DecodeUnchecked, Encode, EncodeMove,
        EncodeMoveUnchecked, EncodeUnchecked,
    },
    lang::Language,
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
unsafe impl<L: Language, T: AbiCompatible + ?Sized> EncodeMove<L> for NonNull<T> {
    type Store = <*mut T as EncodeMove<L>>::Store;

    #[inline(always)]
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe { crate::convert::EncodeMoveUnchecked::soft_encode_unchecked(self, store) }
    }
}
unsafe impl<T: AbiCompatible + ?Sized> EncodeMoveUnchecked for NonNull<T> {
    type SoftStore = ();

    unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        NonZero(unsafe {
            <*mut T as EncodeMoveUnchecked>::soft_encode_unchecked(self.as_ptr(), &mut ())
        })
    }
}
unsafe impl<'d, T: AbiCompatible + ?Sized> DecodeMoveUnchecked<'d> for NonNull<T> {
    type SoftStore = <*mut T as DecodeMoveUnchecked<'d>>::SoftStore;

    #[inline(always)]
    unsafe fn soft_decode_unchecked<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::SoftStore,
    ) -> Self {
        let ptr =
            unsafe { <*mut T as DecodeMoveUnchecked<'d>>::soft_decode_unchecked(source.0, store) };
        unsafe { NonNull::new_unchecked(ptr) }
    }
}
unsafe impl<'d, L: Language, T: AbiCompatible + ?Sized> DecodeMove<'d, L> for NonNull<T> {
    type Store = <*mut T as DecodeMove<'d, L>>::Store;

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        let ptr = unsafe { <*mut T as DecodeMove<'d, L>>::soft_decode(source.0, store)? };
        if crate::lang::checks::nonzero::<L>(ptr.is_null()) {
            Some(unsafe { NonNull::new_unchecked(ptr) })
        } else {
            None
        }
    }
}

impl<T: AbiCompatible + ?Sized> EncodeUnchecked for NonNull<T> {}
impl<L: Language, T: AbiCompatible + ?Sized> Encode<L> for NonNull<T> {}
impl<T: AbiCompatible + ?Sized> DecodeUnchecked<'_> for NonNull<T> {}
impl<L: Language, T: AbiCompatible + ?Sized> Decode<'_, L> for NonNull<T> {}

unsafe impl<T: AbiCompatible + ?Sized> AbiCompatible for NonNull<T> {}
unsafe impl<L: Language, T: AbiCompatible + ?Sized> AbiValidate<L> for NonNull<T> {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        !target.0.is_null()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CType, lang, marker::Native};

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
            unsafe { crate::decode::<NonNull<u32>, lang::C>(NonZero(core::ptr::null_mut())) },
            None
        );

        let mut value = 7_u32;
        let pointer = NonNull::from(&mut value);
        let encoded = unsafe { crate::encode::<_, lang::C>(pointer) };
        assert_eq!(encoded.0, pointer.as_ptr());
        assert_eq!(
            unsafe { crate::decode::<NonNull<u32>, lang::C>(encoded) },
            Some(pointer)
        );

        let absent = unsafe { crate::encode::<_, lang::C>(None::<NonNull<u32>>) };
        assert!(absent.0.0.0.is_null());
        assert_eq!(
            unsafe { crate::decode::<Option<NonNull<u32>>, lang::C>(absent) },
            Some(None)
        );
    }
}
