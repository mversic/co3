//! C-compatible representations and conversions for [`core::mem`].
use core::mem::{ManuallyDrop, MaybeUninit};

use rust_spec::{RustSpec, Stable};

use crate::{
    CFnArg, CFnReturn, CType, ReprC,
    convert::AbiCompatibleValidate,
    convert::{
        Decode, DecodeMove, EmptyStore, Encode, EncodeMove, EncodeMoveUnchecked, EncodeUnchecked,
    },
    marker::AbiCompatible,
    niche::Niche,
    wide::Wide,
};

impl<R: ReprC + ?Sized> ReprC for ManuallyDrop<R> {
    type CType = core::mem::ManuallyDrop<R::CType>;
}

const _: () = {
    use crate::borrow::{BorrowedDecode, BorrowedEncode, BorrowedEncodeUnchecked};

    unsafe impl<T: BorrowedEncodeUnchecked<CType: Sized>> BorrowedEncodeUnchecked for ManuallyDrop<T> {
        type Owner = T::Owner;
        type SoftStore = T::SoftStore;

        unsafe fn soft_encode_unchecked<'a>(
            self,
            store: &'a mut Self::SoftStore,
            owner: &'a mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'a,
        {
            ManuallyDrop::new(unsafe {
                T::soft_encode_unchecked(ManuallyDrop::into_inner(self), store, owner)
            })
        }
    }

    unsafe impl<T: BorrowedEncode<CType: Sized>> BorrowedEncode for ManuallyDrop<T> {
        type Store = T::Store;

        fn soft_encode<'a>(
            self,
            store: &'a mut Self::Store,
            owner: &'a mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'a,
        {
            ManuallyDrop::new(T::soft_encode(ManuallyDrop::into_inner(self), store, owner))
        }
    }

    unsafe impl<'d, T: BorrowedDecode<'d, CType: Sized>> BorrowedDecode<'d> for ManuallyDrop<T> {
        type Store = T::Store;

        unsafe fn soft_decode<'a: 'd>(
            source: Self::CType,
            store: &'a mut Self::Store,
        ) -> Option<Self> {
            unsafe { T::soft_decode(ManuallyDrop::into_inner(source), store) }
                .map(ManuallyDrop::new)
        }

        unsafe fn soft_decode_unchecked<'a: 'd>(
            source: Self::CType,
            store: &'a mut Self::Store,
        ) -> Self {
            ManuallyDrop::new(unsafe {
                T::soft_decode_unchecked(ManuallyDrop::into_inner(source), store)
            })
        }
    }
};

impl<T: Encode> Encode for ManuallyDrop<T> where Self: EncodeMove {}
impl<T: EncodeUnchecked> EncodeUnchecked for ManuallyDrop<T> where Self: EncodeMoveUnchecked {}
impl<'d, T: Decode<'d>> Decode<'d> for ManuallyDrop<T> where Self: DecodeMove<'d> {}

unsafe impl<T: CType + ?Sized> CType for ManuallyDrop<T> {}
unsafe impl<T: CFnArg> CFnArg for ManuallyDrop<T> where
    Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}
unsafe impl<T: CFnReturn> CFnReturn for ManuallyDrop<T> where
    Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}

unsafe impl<T: EmptyStore> EmptyStore for ManuallyDrop<T> {}

unsafe impl<R: EncodeMove> EncodeMove for ManuallyDrop<R>
where
    Self: EncodeMoveUnchecked + ReprC<CType = core::mem::ManuallyDrop<<R as ReprC>::CType>>,
{
    type Store = R::Store;

    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        core::mem::ManuallyDrop::new(Self::into_inner(self).soft_encode(store))
    }
}

unsafe impl<R: AbiCompatibleValidate + ?Sized> AbiCompatibleValidate for ManuallyDrop<R>
where
    Self: RustSpec<Layout = Stable>,
{
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        unsafe { R::is_valid(target) }
    }
}

unsafe impl<R: AbiCompatible + ?Sized> AbiCompatible for ManuallyDrop<R> where
    Self: RustSpec<Layout = Stable>
{
}

impl<R: Niche> Niche for ManuallyDrop<R> {
    const NICHE: Self::CType = core::mem::ManuallyDrop::new(R::NICHE);

    fn is_niche(value: &Self::CType) -> bool {
        R::is_niche(value)
    }
}

macro_rules! impl_wide_for_transparent_wrapper {
    ($($wrapper:ident),+ $(,)?) => {$(
        unsafe impl<R: Wide + ?Sized> Wide for $wrapper<R> {
            type Header = R::Header;
            type Metadata = R::Metadata;

            fn metadata(ptr: *const Self) -> Self::Metadata {
                R::metadata(ptr as *const R)
            }

            fn as_ptr(ptr: *const Self) -> *const Self::Header {
                R::as_ptr(ptr as *const R)
            }

            fn as_mut_ptr(ptr: *mut Self) -> *mut Self::Header {
                R::as_mut_ptr(ptr as *mut R)
            }

            unsafe fn from_raw_parts<'a>(data: *const Self::Header, metadata: Self::Metadata) -> &'a Self {
                let inner = unsafe { R::from_raw_parts(data, metadata) };
                unsafe { &*(inner as *const R as *const Self) }
            }

            unsafe fn from_raw_parts_mut<'a>(
                data: *mut Self::Header,
                metadata: Self::Metadata,
            ) -> &'a mut Self {
                let inner = unsafe { R::from_raw_parts_mut(data, metadata) };
                unsafe { &mut *(inner as *mut R as *mut Self) }
            }

            #[cfg(feature = "alloc")]
            fn into_non_null(self: alloc::boxed::Box<Self>) -> core::ptr::NonNull<Self::Header> {
                alloc::boxed::Box::into_non_null(self).cast()
            }

            #[cfg(feature = "alloc")]
            unsafe fn from_non_null(
                data: core::ptr::NonNull<Self::Header>,
                metadata: Self::Metadata,
            ) -> alloc::boxed::Box<Self> {
                let inner = unsafe { R::from_non_null(data, metadata) };
                let inner = alloc::boxed::Box::into_raw(inner) as *mut Self;
                unsafe { alloc::boxed::Box::from_raw(inner) }
            }
        }
    )+};
}

impl_wide_for_transparent_wrapper!(ManuallyDrop);

crate::borrow::__impl_borrowed_identity!([T] MaybeUninit<T>);

impl<T: AbiCompatible<CType: Sized>> ReprC for MaybeUninit<T> {
    type CType = core::mem::MaybeUninit<T::CType>;
}
unsafe impl<T: AbiCompatible<CType: Sized> + EncodeMove<Store: EmptyStore>> EncodeMove
    for MaybeUninit<T>
{
    type Store = ();

    #[inline(always)]
    fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe { self.soft_encode_unchecked(&mut ()) }
    }
}
unsafe impl<T: AbiCompatible<CType: Sized>> EncodeMoveUnchecked for MaybeUninit<T> {
    type SoftStore = ();

    unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe { core::mem::transmute_copy(&self) }
    }
}
unsafe impl<'d, T: AbiCompatible<CType: Sized>> DecodeMove<'d> for MaybeUninit<T> {
    type Store = ();

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        Some(unsafe { core::mem::transmute_copy(&source) })
    }
}

// TODO: Weird bounds here
impl<'d, T: AbiCompatible<CType: Sized>> Decode<'d> for MaybeUninit<T> {}
impl<T: AbiCompatible<CType: Sized>> EncodeUnchecked for MaybeUninit<T> {}
impl<T: AbiCompatible<CType: Sized> + EncodeMove<Store: EmptyStore>> Encode for MaybeUninit<T> {}

// TODO: Technically all bits are valid, but using T: CType feels safer
unsafe impl<T: AbiCompatible<CType: Sized>> AbiCompatibleValidate for MaybeUninit<T> {
    #[inline(always)]
    unsafe fn is_valid(_: &Self::CType) -> bool {
        true
    }
}
unsafe impl<T: AbiCompatible<CType: Sized>> AbiCompatible for MaybeUninit<T> {}

unsafe impl<T: CType> CType for MaybeUninit<T> {}
unsafe impl<T: CFnArg> CFnArg for MaybeUninit<T> where
    Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}
unsafe impl<T: CFnReturn> CFnReturn for MaybeUninit<T> where
    Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}

unsafe impl<T: EmptyStore> EmptyStore for MaybeUninit<T> {}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc::{boxed::Box, string::String, vec::Vec};

    use rust_spec::niche::WithNiche;
    use static_assertions::assert_impl_all;
    #[cfg(feature = "alloc")]
    use static_assertions::assert_not_impl_any;
    #[cfg(feature = "alloc")]
    use static_assertions::assert_type_eq_all;

    use super::*;
    #[cfg(feature = "alloc")]
    use crate::{boxed::abi::Box as AbiBox, wide::abi::WideBox as AbiWideBox};
    use crate::{option::abi::Option as AbiOption, reference::abi::Ref};

    #[test]
    #[cfg(feature = "alloc")]
    fn manually_drop_borrowed_box_retains_owner_and_decodes_independently() {
        use crate::borrow::{BorrowedDecode, BorrowedEncode, BorrowedEncodeUnchecked};

        assert_type_eq_all!(
            <ManuallyDrop<Box<u8>> as BorrowedEncodeUnchecked>::Owner,
            <Box<u8> as BorrowedEncodeUnchecked>::Owner
        );

        for unchecked in [false, true] {
            let value = Box::new(42_u8);
            let original = core::ptr::from_ref(&*value);
            let mut owner = Default::default();
            let mut encode_store = Default::default();
            let value = ManuallyDrop::new(value);
            let encoded = if unchecked {
                unsafe {
                    BorrowedEncodeUnchecked::soft_encode_unchecked(value, &mut (), &mut owner)
                }
            } else {
                BorrowedEncode::soft_encode(value, &mut encode_store, &mut owner)
            };
            assert_eq!(encoded.as_ptr().cast_const(), original);
            let mut decode_store = Default::default();
            let decoded: ManuallyDrop<Box<u8>> = if unchecked {
                unsafe { BorrowedDecode::soft_decode_unchecked(encoded, &mut decode_store) }
            } else {
                unsafe { BorrowedDecode::soft_decode(encoded, &mut decode_store) }.unwrap()
            };
            assert_eq!(**decoded, 42);
            assert_ne!(core::ptr::from_ref(&**decoded), original);
            drop(ManuallyDrop::into_inner(decoded));
            // The retained encode owner still owns the original allocation.
            assert_eq!(unsafe { *encoded.as_ptr() }, 42);
        }
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn manually_drop_stable_and_unstable_conversions() {
        assert_type_eq_all!(<ManuallyDrop<bool> as EncodeMove>::Store, ());
        assert_type_eq_all!(<ManuallyDrop<Box<u8>> as EncodeMove>::Store, ());
        assert_type_eq_all!(<ManuallyDrop<Box<u8>> as DecodeMove<'static>>::Store, ());
        assert_type_eq_all!(
            <ManuallyDrop<Vec<(u8,)>> as EncodeMove>::Store,
            <Vec<(u8,)> as EncodeMove>::Store
        );
        let mut encode_store = Default::default();
        let encoded = crate::soft_encode(
            ManuallyDrop::new(alloc::vec![(3_u8,), (5_u8,)]),
            &mut encode_store,
        );
        let mut decode_store = Default::default();
        let decoded =
            unsafe { crate::soft_decode::<ManuallyDrop<Vec<(u8,)>>>(encoded, &mut decode_store) }
                .unwrap();
        assert_eq!(ManuallyDrop::into_inner(decoded), alloc::vec![(3,), (5,)]);
    }

    #[test]
    fn manually_drop_decode_rejects_invalid_inner_representation() {
        assert_impl_all!(ManuallyDrop<bool>: Decode<'static>, Encode);

        let invalid = core::mem::ManuallyDrop::new(bool::NICHE);
        assert!(unsafe { crate::decode::<ManuallyDrop<bool>>(invalid) }.is_none());

        let encoded = crate::encode(ManuallyDrop::new(true));
        let decoded = unsafe { crate::decode::<ManuallyDrop<bool>>(encoded) }.unwrap();
        assert!(*decoded);
    }

    #[test]
    fn maybe_uninit_lowers_without_validating_or_encoding_the_inner_value() {
        assert_impl_all!(MaybeUninit<bool>:
            ReprC<CType = core::mem::MaybeUninit<crate::primitive::abi::bool>>,
            AbiCompatible,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(MaybeUninit<u8>: CType, CFnArg, CFnReturn);

        let source = core::mem::MaybeUninit::new(bool::NICHE);
        assert!(unsafe { MaybeUninit::<bool>::is_valid(&source) });

        let decoded = unsafe { crate::decode::<MaybeUninit<bool>>(source) }.unwrap();
        let encoded = crate::encode(decoded);

        assert!(bool::is_niche(&unsafe { encoded.assume_init() }));
    }

    #[test]
    fn manually_drop_inner_without_drop() {
        assert_impl_all!(ManuallyDrop<u8>:
            ReprC<CType = core::mem::ManuallyDrop<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&ManuallyDrop<u8>:
            RustSpec<Niche = WithNiche<rust_spec::Stable>>,
            Niche<CType = Ref<'static, <ManuallyDrop<u8> as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut ManuallyDrop<u8>:
            RustSpec<Niche = WithNiche<rust_spec::Stable>>,
            Niche<CType = crate::reference::abi::RefMut<'static, <ManuallyDrop<u8> as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<ManuallyDrop<u8>>:
            Niche<CType = AbiBox<<ManuallyDrop<u8> as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[ManuallyDrop<u8>]:
            Niche<CType = crate::wide::abi::WideRef<'static, <[ManuallyDrop<u8>] as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [ManuallyDrop<u8>]:
            Niche<CType = crate::wide::abi::WideRef<'static, <[ManuallyDrop<u8>] as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[ManuallyDrop<u8>]>:
            Niche<CType = AbiWideBox<<[ManuallyDrop<u8>] as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<ManuallyDrop<u8>>:
            Niche<CType = crate::vec::abi::Vec<<ManuallyDrop<u8> as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([ManuallyDrop<u8>; 2]:
            ReprC<CType = [core::mem::ManuallyDrop<u8>; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<ManuallyDrop<u8>>:
            Niche<CType = AbiOption<<ManuallyDrop<u8> as crate::ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );

        assert_impl_all!(ManuallyDrop<u8>: CType);
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn manually_drop_inner_with_drop() {
        assert_impl_all!(ManuallyDrop<String>:
            ReprC<CType = core::mem::ManuallyDrop<<String as ReprC>::CType>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(ManuallyDrop<Box<u8>>:
            Niche<CType = core::mem::ManuallyDrop<crate::boxed::abi::Box<u8>>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&ManuallyDrop<Box<u8>>:
            Niche<CType = Ref<'static, <ManuallyDrop<Box<u8>> as crate::ReprC>::CType>>,
        );
        assert_impl_all!(&mut ManuallyDrop<Box<u8>>:
            Niche<CType = crate::reference::abi::SoftRefMut<<ManuallyDrop<Box<u8>> as crate::ReprC>::CType>>,
        );
        assert_impl_all!(Box<ManuallyDrop<Box<u8>>>:
            RustSpec<Niche = WithNiche<rust_spec::Stable>>,
            Niche<CType = AbiBox<<ManuallyDrop<Box<u8>> as crate::ReprC>::CType>>,
        );
        assert_impl_all!(&[ManuallyDrop<Box<u8>>]:
            Niche<CType = crate::wide::abi::WideRef<'static, <[ManuallyDrop<Box<u8>>] as crate::ReprC>::CType>>,
        );
        assert_impl_all!(&mut [ManuallyDrop<Box<u8>>]:
            Niche<CType = crate::wide::abi::SoftWideRefMut<<[ManuallyDrop<Box<u8>>] as crate::ReprC>::CType>>,
        );
        assert_impl_all!(Box<[ManuallyDrop<Box<u8>>]>:
            Niche<CType = AbiWideBox<<[ManuallyDrop<Box<u8>>] as crate::ReprC>::CType>>,
        );
        assert_impl_all!(Vec<ManuallyDrop<Box<u8>>>:
            Niche<CType = crate::vec::abi::Vec<<ManuallyDrop<Box<u8>> as crate::ReprC>::CType>>,
        );
        assert_impl_all!([ManuallyDrop<Box<u8>>; 2]:
            Niche<CType = [core::mem::ManuallyDrop<crate::boxed::abi::Box<u8>>; 2]>,
        );
        assert_impl_all!(Option<ManuallyDrop<Box<u8>>>:
            ReprC<CType = crate::option::abi::OptionSome<<ManuallyDrop<Box<u8>> as crate::ReprC>::CType>>,
        );
        assert_not_impl_any!(ManuallyDrop<Box<u8>>: CType);

        let encoded = crate::encode(ManuallyDrop::new(Box::new(42u8)));
        let decoded = unsafe { crate::decode::<ManuallyDrop<Box<u8>>>(encoded) }.unwrap();
        let decoded = ManuallyDrop::into_inner(decoded);
        assert_eq!(*decoded, 42);

        let encoded = crate::encode(ManuallyDrop::new(String::from("hello")));
        let decoded = unsafe { crate::decode::<ManuallyDrop<String>>(encoded) }.unwrap();
        assert_eq!(ManuallyDrop::into_inner(decoded), "hello");
    }
}
