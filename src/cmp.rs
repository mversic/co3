//! C-compatible representations of comparison types.

use core::cmp::{Ordering, Reverse};

use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, ReprC,
    abi::transmute_owned,
    convert::{
        AbiValidate, Decode, DecodeMove, DecodeMoveUnchecked, DecodeUnchecked, Encode, EncodeMove,
        EncodeMoveUnchecked, SoftEncodeMut, SoftEncodeMutUnchecked, SoftEncodeRef,
        SoftEncodeRefUnchecked,
    },
    lang::Language,
    marker::{AbiCompatible, Zeroable},
    niche::Niche,
    primitive::{fieldless_enum_derive, primitive_derive},
};

#[cfg(feature = "repr-rust")]
use crate::convert::{
    SoftDecodeMut, SoftDecodeMutUnchecked, SoftDecodeRef, SoftDecodeRefUnchecked,
};

impl TryFrom<abi::Ordering> for Ordering {
    type Error = abi::Ordering;

    #[inline(always)]
    fn try_from(value: abi::Ordering) -> Result<Self, Self::Error> {
        match value.0 {
            -1 => Ok(Self::Less),
            0 => Ok(Self::Equal),
            1 => Ok(Self::Greater),
            _ => Err(value),
        }
    }
}

fieldless_enum_derive! {
    Ordering => abi::Ordering: {abi::Ordering(-2)}:
    ordering;
    |source: Ordering| abi::Ordering::from(source);
    |source: abi::Ordering| unsafe { Ordering::try_from(source).unwrap_unchecked() }
}

impl<T: ReprC<CType: Sized>> ReprC for Reverse<T> {
    type CType = core::cmp::Reverse<T::CType>;
}

impl<T: Niche> Niche for Reverse<T> {
    const NICHE: Self::CType = core::cmp::Reverse(T::NICHE);

    fn is_niche(value: &Self::CType) -> bool {
        T::is_niche(&value.0)
    }
}

macro_rules! reverse_codecs {
    ($encode:ident, $unchecked:ident, $decode:ident, $decode_unchecked:ident) => {
        unsafe impl<T: $unchecked<CType: Sized>> $unchecked for Reverse<T> {
            type SoftStore = T::SoftStore;

            unsafe fn soft_encode_unchecked<'a>(self, store: &'a mut Self::SoftStore) -> Self::CType
            where
                Self: 'a,
            {
                unsafe { core::cmp::Reverse(self.0.soft_encode_unchecked(store)) }
            }
        }
        unsafe impl<L: Language, T: $encode<L, CType: Sized>> $encode<L> for Reverse<T> {
            type Store = T::Store;

            fn soft_encode<'a>(self, store: &'a mut Self::Store) -> Self::CType
            where
                Self: 'a,
            {
                core::cmp::Reverse($encode::<L>::soft_encode(self.0, store))
            }
        }
        unsafe impl<'d, T: $decode_unchecked<'d, CType: Sized>> $decode_unchecked<'d>
            for Reverse<T>
        {
            type SoftStore = T::SoftStore;

            unsafe fn soft_decode_unchecked<'a: 'd>(
                source: Self::CType,
                store: &'a mut Self::SoftStore,
            ) -> Self {
                Self(unsafe {
                    <T as $decode_unchecked<'d>>::soft_decode_unchecked(source.0, store)
                })
            }
        }
        unsafe impl<'d, L: Language, T: $decode<'d, L, CType: Sized>> $decode<'d, L>
            for Reverse<T>
        {
            type Store = T::Store;

            unsafe fn soft_decode<'a: 'd>(
                source: Self::CType,
                store: &'a mut Self::Store,
            ) -> Option<Self> {
                unsafe { <T as $decode<'d, L>>::soft_decode(source.0, store) }.map(Self)
            }
        }
    };
}
reverse_codecs!(
    EncodeMove,
    EncodeMoveUnchecked,
    DecodeMove,
    DecodeMoveUnchecked
);
const _: () = {
    use crate::borrow::{
        BorrowedDecode, BorrowedDecodeUnchecked, BorrowedEncode, BorrowedEncodeUnchecked,
    };
    unsafe impl<T: BorrowedEncodeUnchecked<CType: Sized>> BorrowedEncodeUnchecked for Reverse<T> {
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
            unsafe { core::cmp::Reverse(self.0.soft_encode_unchecked(store, owner)) }
        }
    }
    unsafe impl<L: Language, T: BorrowedEncode<L, CType: Sized>> BorrowedEncode<L> for Reverse<T> {
        type Store = T::Store;

        fn soft_encode<'a>(
            self,
            store: &'a mut Self::Store,
            owner: &'a mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'a,
        {
            core::cmp::Reverse(BorrowedEncode::<L>::soft_encode(self.0, store, owner))
        }
    }
    unsafe impl<'d, T: BorrowedDecodeUnchecked<'d, CType: Sized>> BorrowedDecodeUnchecked<'d>
        for Reverse<T>
    {
        type SoftStore = T::SoftStore;

        unsafe fn soft_decode_unchecked<'a: 'd>(
            source: Self::CType,
            store: &'a mut Self::SoftStore,
        ) -> Self {
            Self(unsafe {
                <T as crate::borrow::BorrowedDecodeUnchecked<'d>>::soft_decode_unchecked(
                    source.0, store,
                )
            })
        }
    }
    unsafe impl<'d, L: Language, T: BorrowedDecode<'d, L, CType: Sized>> BorrowedDecode<'d, L>
        for Reverse<T>
    {
        type Store = T::Store;

        unsafe fn soft_decode<'a: 'd>(
            source: Self::CType,
            store: &'a mut Self::Store,
        ) -> Option<Self> {
            unsafe { <T as crate::borrow::BorrowedDecode<'d, L>>::soft_decode(source.0, store) }
                .map(Self)
        }
    }
};

impl<T: crate::convert::EncodeUnchecked<CType: Sized>> crate::convert::EncodeUnchecked
    for Reverse<T>
{
}
impl<L: Language, T: Encode<L, CType: Sized>> Encode<L> for Reverse<T> {}
impl<'d, T: DecodeUnchecked<'d, CType: Sized>> DecodeUnchecked<'d> for Reverse<T> {}
impl<'d, L: Language, T: Decode<'d, L, CType: Sized>> Decode<'d, L> for Reverse<T> {}

unsafe impl<T: AbiCompatible<CType: Sized>> AbiCompatible for Reverse<T> {}
unsafe impl<L: Language, T: AbiValidate<L, CType: Sized>> AbiValidate<L> for Reverse<T> {
    unsafe fn is_valid(target: &Self::CType) -> bool {
        unsafe { <T as crate::convert::AbiValidate<L>>::is_valid(&target.0) }
    }
}
unsafe impl<T: CType> CType for Reverse<T> {}
unsafe impl<T: CFnArg> CFnArg for Reverse<T> {}
unsafe impl<T: CFnReturn> CFnReturn for Reverse<T> {}
unsafe impl<T: Zeroable> Zeroable for Reverse<T> {}

unsafe impl<'a, T: ReprC + 'a> SoftEncodeRefUnchecked<'a> for Reverse<T>
where
    T: RustSpec<Size: crate::Thin> + ReprC<CType: Sized>,
    &'a Self: ReprC<CType: Sized>,
    &'a T: EncodeMoveUnchecked + ReprC<CType: Sized>,
{
    type SoftStore = <&'a T as EncodeMoveUnchecked>::SoftStore;

    unsafe fn soft_encode_ref_unchecked<'b>(
        &'a self,
        store: &'b mut Self::SoftStore,
    ) -> <&'a Self as ReprC>::CType
    where
        'a: 'b,
    {
        let carrier = unsafe { EncodeMoveUnchecked::soft_encode_unchecked(&self.0, store) };
        // Reverse<T::CType> is transparent over T::CType.
        unsafe { transmute_owned(carrier) }
    }
}

unsafe impl<'a, L: Language, T: ReprC + 'a> SoftEncodeRef<'a, L> for Reverse<T>
where
    T: RustSpec<Size: crate::Thin> + ReprC<CType: Sized>,
    &'a Self: ReprC<CType: Sized>,
    &'a T: EncodeMove<L> + ReprC<CType: Sized>,
{
    type Store = <&'a T as EncodeMove<L>>::Store;

    fn soft_encode_ref<'b>(
        &'a self,
        store: &'b mut Self::Store,
    ) -> <&'a Self as crate::ReprC>::CType
    where
        'a: 'b,
    {
        let carrier = EncodeMove::<L>::soft_encode(&self.0, store);
        unsafe { transmute_owned(carrier) }
    }
}

unsafe impl<'a, T: ReprC + 'a> SoftEncodeMutUnchecked<'a> for Reverse<T>
where
    T: RustSpec<Size: crate::Thin> + ReprC<CType: Sized>,
    &'a mut Self: ReprC<CType: Sized>,
    &'a mut T: EncodeMoveUnchecked + ReprC<CType: Sized>,
{
    type SoftStore = <&'a mut T as EncodeMoveUnchecked>::SoftStore;

    unsafe fn soft_encode_mut_unchecked<'b>(
        &'a mut self,
        store: &'b mut Self::SoftStore,
    ) -> <&'a mut Self as ReprC>::CType
    where
        'a: 'b,
    {
        let carrier = unsafe { EncodeMoveUnchecked::soft_encode_unchecked(&mut self.0, store) };
        unsafe { transmute_owned(carrier) }
    }
}

unsafe impl<'a, L: Language, T: ReprC + 'a> SoftEncodeMut<'a, L> for Reverse<T>
where
    T: RustSpec<Size: crate::Thin> + ReprC<CType: Sized>,
    &'a mut Self: ReprC<CType: Sized>,
    &'a mut T: EncodeMove<L> + ReprC<CType: Sized>,
{
    type Store = <&'a mut T as EncodeMove<L>>::Store;

    fn soft_encode_mut<'b>(
        &'a mut self,
        store: &'b mut Self::Store,
    ) -> <&'a mut Self as crate::ReprC>::CType
    where
        'a: 'b,
    {
        let carrier = EncodeMove::<L>::soft_encode(&mut self.0, store);
        unsafe { transmute_owned(carrier) }
    }
}

#[cfg(feature = "repr-rust")]
unsafe impl<'d, T: ReprC + 'd> SoftDecodeRefUnchecked<'d> for Reverse<T>
where
    T: RustSpec<Size: crate::Thin> + ReprC<CType: Sized>,
    &'d Self: ReprC<CType: Sized>,
    &'d T: DecodeMoveUnchecked<'d> + ReprC<CType: Sized>,
{
    type SoftStore = <&'d T as DecodeMoveUnchecked<'d>>::SoftStore;

    unsafe fn soft_decode_ref_unchecked<'a: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'a mut Self::SoftStore,
    ) -> &'d Self {
        let source = unsafe { transmute_owned(source) };
        let value =
            unsafe { <&'d T as DecodeMoveUnchecked<'d>>::soft_decode_unchecked(source, store) };
        unsafe { &*core::ptr::from_ref(value).cast::<Self>() }
    }
}
#[cfg(feature = "repr-rust")]
unsafe impl<'d, L: Language, T: ReprC + 'd> SoftDecodeRef<'d, L> for Reverse<T>
where
    T: RustSpec<Size: crate::Thin> + ReprC<CType: Sized>,
    &'d Self: ReprC<CType: Sized>,
    &'d T: DecodeMove<'d, L> + ReprC<CType: Sized>,
{
    type Store = <&'d T as DecodeMove<'d, L>>::Store;

    unsafe fn soft_decode_ref<'a: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'a mut Self::Store,
    ) -> Option<&'d Self> {
        let source = unsafe { transmute_owned(source) };
        unsafe { <&'d T as DecodeMove<'d, L>>::soft_decode(source, store) }
            .map(|value| unsafe { &*core::ptr::from_ref(value).cast::<Self>() })
    }
}

#[cfg(feature = "repr-rust")]
unsafe impl<'d, T: ReprC + 'd> SoftDecodeMutUnchecked<'d> for Reverse<T>
where
    T: RustSpec<Size: crate::Thin> + ReprC<CType: Sized>,
    &'d mut T: DecodeMoveUnchecked<'d> + ReprC<CType: Sized>,
    &'d mut Self: ReprC<CType: Sized>,
{
    type SoftStore = <&'d mut T as DecodeMoveUnchecked<'d>>::SoftStore;

    unsafe fn soft_decode_mut_unchecked<'a: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'a mut Self::SoftStore,
    ) -> &'d mut Self {
        let source = unsafe { transmute_owned(source) };
        let value =
            unsafe { <&'d mut T as DecodeMoveUnchecked<'d>>::soft_decode_unchecked(source, store) };
        unsafe { &mut *core::ptr::from_mut(value).cast::<Self>() }
    }
}
#[cfg(feature = "repr-rust")]
unsafe impl<'d, L: Language, T: ReprC + 'd> SoftDecodeMut<'d, L> for Reverse<T>
where
    T: RustSpec<Size: crate::Thin> + ReprC<CType: Sized>,
    &'d mut T: DecodeMove<'d, L> + ReprC<CType: Sized>,
    &'d mut Self: ReprC<CType: Sized>,
{
    type Store = <&'d mut T as DecodeMove<'d, L>>::Store;

    unsafe fn soft_decode_mut<'a: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'a mut Self::Store,
    ) -> Option<&'d mut Self> {
        let source = unsafe { transmute_owned(source) };
        unsafe { <&'d mut T as DecodeMove<'d, L>>::soft_decode(source, store) }
            .map(|value| unsafe { &mut *core::ptr::from_mut(value).cast::<Self>() })
    }
}

#[doc(hidden)]
pub mod abi {
    use super::*;
    use crate::lang::Language;

    /// C-compatible carrier for [`core::cmp::Ordering`].
    ///
    /// Every byte value is valid for this carrier; conversion to `Ordering` accepts only -1, 0, and 1.
    #[derive(Debug, Clone, Copy, Default, RustSpec)]
    #[repr(transparent)]
    #[doc(hidden)]
    pub struct Ordering(pub i8);

    unsafe impl Zeroable for Ordering {}

    impl Ordering {
        pub const LESS: Self = Self(-1);
        pub const EQUAL: Self = Self(0);
        pub const GREATER: Self = Self(1);
    }

    impl PartialEq for Ordering {
        fn eq(&self, other: &Self) -> bool {
            matches!(self.0, -1..=1) && self.0 == other.0
        }
    }

    impl From<core::cmp::Ordering> for Ordering {
        #[inline(always)]
        fn from(value: core::cmp::Ordering) -> Self {
            Self(value as i8)
        }
    }

    primitive_derive! { Ordering }
}

#[cfg(test)]
mod tests {
    use static_assertions::{assert_impl_all, assert_type_eq_all};

    use super::*;
    use crate::{decode, encode, lang};

    #[test]
    #[cfg(feature = "repr-rust")]
    fn reverse_reference_codecs_dispatch_stable_inner() {
        use core::cmp::Reverse;

        use crate::convert::{
            SoftDecodeMut, SoftDecodeMutUnchecked, SoftDecodeRef, SoftDecodeRefUnchecked,
            SoftEncodeMut, SoftEncodeMutUnchecked, SoftEncodeRef, SoftEncodeRefUnchecked,
        };

        let mut value = Reverse(7_u32);
        let mut decode_store = ();
        let carrier = SoftEncodeRef::<lang::C>::soft_encode_ref(&value, &mut ());
        let decoded =
            unsafe { <Reverse<u32> as SoftDecodeRef>::soft_decode_ref(carrier, &mut decode_store) }
                .unwrap();
        assert!(core::ptr::eq(decoded, &value));

        let carrier = unsafe { SoftEncodeRefUnchecked::soft_encode_ref_unchecked(&value, &mut ()) };
        let decoded = unsafe {
            <Reverse<u32> as SoftDecodeRefUnchecked>::soft_decode_ref_unchecked(
                carrier,
                &mut decode_store,
            )
        };
        assert!(core::ptr::eq(decoded, &value));

        let carrier = SoftEncodeMut::<lang::C>::soft_encode_mut(&mut value, &mut ());
        let decoded =
            unsafe { <Reverse<u32> as SoftDecodeMut>::soft_decode_mut(carrier, &mut decode_store) }
                .unwrap();
        decoded.0 = 11;
        assert_eq!(value.0, 11);

        let carrier =
            unsafe { SoftEncodeMutUnchecked::soft_encode_mut_unchecked(&mut value, &mut ()) };
        let decoded = unsafe {
            <Reverse<u32> as SoftDecodeMutUnchecked>::soft_decode_mut_unchecked(
                carrier,
                &mut decode_store,
            )
        };
        decoded.0 = 13;
        assert_eq!(value.0, 13);
    }

    #[test]
    fn ordering_conversions_validate_the_carrier() {
        for value in [Ordering::Less, Ordering::Equal, Ordering::Greater] {
            let carrier = abi::Ordering::from(value);
            assert_eq!(Ordering::try_from(carrier), Ok(value));
            assert_eq!(unsafe { encode::<_, lang::C>(value) }, carrier);
            assert_eq!(unsafe { decode::<Ordering, lang::C>(carrier) }, Some(value));
        }
        for raw in i8::MIN..=i8::MAX {
            let carrier = abi::Ordering(raw);
            assert_eq!(carrier == carrier, (-1..=1).contains(&raw));
            assert_eq!(Ordering::try_from(carrier).is_ok(), (-1..=1).contains(&raw));
            assert_eq!(
                unsafe { decode::<Ordering, lang::C>(carrier) }.is_some(),
                (-1..=1).contains(&raw)
            );
        }
        assert_ne!(abi::Ordering::LESS, abi::Ordering::EQUAL);
        assert_ne!(abi::Ordering::EQUAL, abi::Ordering::GREATER);
    }

    #[test]
    fn reverse_delegates_to_inner_carrier_and_niche() {
        assert_type_eq_all!(
            <Reverse<bool> as ReprC>::CType,
            core::cmp::Reverse<crate::primitive::abi::bool>
        );
        assert_impl_all!(Reverse<bool>: Encode, Decode<'static>, Niche, AbiCompatible);
        assert_impl_all!(&'static Reverse<bool>: Encode, Decode<'static>);
        assert_eq!(
            unsafe { encode::<_, lang::C>(Reverse(true)) },
            core::cmp::Reverse(crate::primitive::abi::bool::TRUE)
        );
        assert_eq!(
            unsafe {
                decode::<Reverse<bool>, lang::C>(core::cmp::Reverse(
                    crate::primitive::abi::bool::TRUE,
                ))
            },
            Some(Reverse(true))
        );
        assert_eq!(
            unsafe {
                decode::<Reverse<bool>, lang::C>(core::cmp::Reverse(crate::primitive::abi::bool(2)))
            },
            None
        );
        assert!(<Reverse<bool> as Niche>::is_niche(
            &unsafe { encode::<_, lang::C>(None::<Reverse<bool>>) }.0.0
        ));
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn reverse_round_trips_owned_values() {
        use alloc::{boxed::Box, vec::Vec};
        assert_impl_all!(&'static Reverse<Vec<(u8,)>>: Encode, Decode<'static>);
        assert_impl_all!(&'static mut Reverse<Vec<bool>>: Encode, Decode<'static>);
        let value = Reverse(Box::new(7_u32));
        let carrier = unsafe { encode::<_, lang::C>(value) };
        assert_eq!(
            unsafe { decode::<Reverse<Box<u32>>, lang::C>(carrier) },
            Some(Reverse(Box::new(7)))
        );
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn reverse_shared_unstable_value_uses_inner_stores() {
        use alloc::vec::Vec;

        use crate::convert::Store;

        let value = Reverse(alloc::vec![(3_u8,), (7,)]);
        let mut encode_store = Default::default();
        let carrier = unsafe { crate::soft_encode::<_, lang::C>(&value, &mut encode_store) };
        let mut decode_store = Default::default();
        let decoded = unsafe {
            crate::soft_decode::<&Reverse<Vec<(u8,)>>, lang::C>(carrier, &mut decode_store)
        }
        .unwrap();
        assert_eq!(decoded, &value);
        decode_store.sync().unwrap();
        encode_store.sync().unwrap();
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn reverse_mutable_value_synchronizes_changes() {
        use alloc::vec::Vec;

        use crate::convert::Store;

        let mut value = Reverse(alloc::vec![false, true]);
        let mut encode_store = Default::default();
        let carrier = unsafe { crate::soft_encode::<_, lang::C>(&mut value, &mut encode_store) };
        let mut decode_store = Default::default();
        let decoded = unsafe {
            crate::soft_decode::<&mut Reverse<Vec<bool>>, lang::C>(carrier, &mut decode_store)
        }
        .unwrap();
        decoded.0[0] = true;
        decode_store.sync().unwrap();
        encode_store.sync().unwrap();
        assert_eq!(value, Reverse(alloc::vec![true, true]));
    }
}
