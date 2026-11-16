//! C-compatible representations of comparison types.
use core::cmp::{Ordering, Reverse};

use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC, Zeroable,
    niche::Niche,
    primitives::{fieldless_enum_derive, primitive_derive},
    shared::{SoftEncodeRef, SoftEncodeRefUnchecked},
    stored::{
        DecodeMove, EncodeMove, EncodeMoveUnchecked, SoftDecodeMut, SoftDecodeRef, SoftEncodeMut,
        SoftEncodeMutUnchecked,
    },
    transmute::{AbiCompatible, AbiCompatibleValidate, transmute_owned},
};

/// C-compatible carrier for [`core::cmp::Ordering`].
///
/// Every byte value is valid for this carrier; conversion to `Ordering` accepts only -1, 0, and 1.
#[derive(Debug, Clone, Copy, Default, RustSpec)]
#[repr(transparent)]
#[doc(hidden)]
pub struct COrdering(pub i8);

unsafe impl Zeroable for COrdering {}

impl COrdering {
    pub const LESS: Self = Self(-1);
    pub const EQUAL: Self = Self(0);
    pub const GREATER: Self = Self(1);
}

impl PartialEq for COrdering {
    fn eq(&self, other: &Self) -> bool {
        matches!(self.0, -1..=1) && self.0 == other.0
    }
}

impl From<Ordering> for COrdering {
    #[inline(always)]
    fn from(value: Ordering) -> Self {
        Self(value as i8)
    }
}

impl TryFrom<COrdering> for Ordering {
    type Error = COrdering;

    #[inline(always)]
    fn try_from(value: COrdering) -> Result<Self, Self::Error> {
        match value.0 {
            -1 => Ok(Self::Less),
            0 => Ok(Self::Equal),
            1 => Ok(Self::Greater),
            _ => Err(value),
        }
    }
}

primitive_derive! { COrdering }
fieldless_enum_derive! {
    Ordering => COrdering: {COrdering(-2)}:
    |i: &COrdering| Ordering::try_from(*i).is_ok();
    |source: COrdering| Ordering::try_from(source).ok();
    |source: Ordering| COrdering::from(source);
    |source: COrdering| unsafe { Ordering::try_from(source).unwrap_unchecked() }
}

impl<T: ReprC<CType: Sized>> ReprC for Reverse<T> {
    type CType = core::cmp::Reverse<T::CType>;
}

impl<T: Niche<CType: Sized>> Niche for Reverse<T> {
    const NICHE: Self::CType = core::cmp::Reverse(T::NICHE);

    fn is_niche(value: &Self::CType) -> bool {
        T::is_niche(&value.0)
    }
}

macro_rules! reverse_codecs {
    ($encode:ident, $unchecked:ident, $decode:ident) => {
        unsafe impl<T: $unchecked<CType: Sized>> $unchecked for Reverse<T> {
            type SoftStore = T::SoftStore;

            unsafe fn soft_encode_unchecked<'a>(self, store: &'a mut Self::SoftStore) -> Self::CType
            where
                Self: 'a,
            {
                unsafe { core::cmp::Reverse(self.0.soft_encode_unchecked(store)) }
            }
        }
        unsafe impl<T: $encode<CType: Sized>> $encode for Reverse<T> {
            type Store = T::Store;

            fn soft_encode<'a>(self, store: &'a mut Self::Store) -> Self::CType
            where
                Self: 'a,
            {
                core::cmp::Reverse($encode::soft_encode(self.0, store))
            }
        }
        unsafe impl<'d, T: $decode<'d, CType: Sized>> $decode<'d> for Reverse<T> {
            type Store = T::Store;

            unsafe fn soft_decode<'a: 'd>(
                source: Self::CType,
                store: &'a mut Self::Store,
            ) -> Option<Self> {
                unsafe { T::soft_decode(source.0, store) }.map(Self)
            }

            unsafe fn soft_decode_unchecked<'a: 'd>(
                source: Self::CType,
                store: &'a mut Self::Store,
            ) -> Self {
                Self(unsafe { T::soft_decode_unchecked(source.0, store) })
            }
        }
    };
}
reverse_codecs!(EncodeMove, EncodeMoveUnchecked, DecodeMove);
const _: () = {
    use crate::borrow::{BorrowedDecode, BorrowedEncode, BorrowedEncodeUnchecked};
    unsafe impl<T: BorrowedEncodeUnchecked<CType: Sized>> BorrowedEncodeUnchecked for Reverse<T> {
            type Owner = T::Owner;
            type SoftStore = T::SoftStore;

            unsafe fn soft_encode_unchecked<'a>(self, store: &'a mut Self::SoftStore, owner: &'a mut Self::Owner) -> Self::CType
            where
                Self: 'a,
            {
                unsafe { core::cmp::Reverse(self.0.soft_encode_unchecked(store, owner)) }
            }
        }
        unsafe impl<T: BorrowedEncode<CType: Sized>> BorrowedEncode for Reverse<T> {
            type Store = T::Store;

            fn soft_encode<'a>(self, store: &'a mut Self::Store, owner: &'a mut Self::Owner) -> Self::CType
            where
                Self: 'a,
            {
                core::cmp::Reverse(BorrowedEncode::soft_encode(self.0, store, owner))
            }
        }
        unsafe impl<'d, T: BorrowedDecode<'d, CType: Sized>> BorrowedDecode<'d> for Reverse<T> {
            type Store = T::Store;

            unsafe fn soft_decode<'a: 'd>(
                source: Self::CType,
                store: &'a mut Self::Store,
            ) -> Option<Self> {
                unsafe { T::soft_decode(source.0, store) }.map(Self)
            }

            unsafe fn soft_decode_unchecked<'a: 'd>(
                source: Self::CType,
                store: &'a mut Self::Store,
            ) -> Self {
                Self(unsafe { T::soft_decode_unchecked(source.0, store) })
            }
        }
};

impl<T: Encode<CType: Sized>> Encode for Reverse<T> {}
impl<T: crate::EncodeUnchecked<CType: Sized>> crate::EncodeUnchecked for Reverse<T> {}
impl<'d, T: Decode<'d, CType: Sized>> Decode<'d> for Reverse<T> {}

unsafe impl<T: AbiCompatibleValidate<CType: Sized>> AbiCompatibleValidate for Reverse<T> {
    unsafe fn is_valid(target: &Self::CType) -> bool {
        unsafe { T::is_valid(&target.0) }
    }
}
unsafe impl<T: AbiCompatible<CType: Sized>> AbiCompatible for Reverse<T> {}
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

unsafe impl<'a, T: ReprC + 'a> SoftEncodeRef<'a> for Reverse<T>
where
    T: RustSpec<Size: crate::Thin> + ReprC<CType: Sized>,
    &'a Self: ReprC<CType: Sized>,
    &'a T: EncodeMove + ReprC<CType: Sized>,
{
    type Store = <&'a T as EncodeMove>::Store;

    fn soft_encode_ref<'b>(
        &'a self,
        store: &'b mut Self::Store,
    ) -> <&'a Self as crate::ReprC>::CType
    where
        'a: 'b,
    {
        let carrier = EncodeMove::soft_encode(&self.0, store);
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

unsafe impl<'a, T: ReprC + 'a> SoftEncodeMut<'a> for Reverse<T>
where
    T: RustSpec<Size: crate::Thin> + ReprC<CType: Sized>,
    &'a mut Self: ReprC<CType: Sized>,
    &'a mut T: EncodeMove + ReprC<CType: Sized>,
{
    type Store = <&'a mut T as EncodeMove>::Store;

    fn soft_encode_mut<'b>(
        &'a mut self,
        store: &'b mut Self::Store,
    ) -> <&'a mut Self as crate::ReprC>::CType
    where
        'a: 'b,
    {
        let carrier = EncodeMove::soft_encode(&mut self.0, store);
        unsafe { transmute_owned(carrier) }
    }
}

unsafe impl<'d, T: ReprC + 'd> SoftDecodeRef<'d> for Reverse<T>
where
    T: RustSpec<Size: crate::Thin> + ReprC<CType: Sized>,
    &'d Self: ReprC<CType: Sized>,
    &'d T: DecodeMove<'d> + ReprC<CType: Sized>,
{
    type Store = <&'d T as DecodeMove<'d>>::Store;

    unsafe fn soft_decode_ref<'a: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'a mut Self::Store,
    ) -> Option<&'d Self> {
        let source = unsafe { transmute_owned(source) };
        unsafe { <&'d T as DecodeMove<'d>>::soft_decode(source, store) }
            .map(|value| unsafe { &*core::ptr::from_ref(value).cast::<Self>() })
    }

    unsafe fn soft_decode_ref_unchecked<'a: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'a mut Self::Store,
    ) -> &'d Self {
        let source = unsafe { transmute_owned(source) };
        let value = unsafe { <&'d T as DecodeMove<'d>>::soft_decode_unchecked(source, store) };
        unsafe { &*core::ptr::from_ref(value).cast::<Self>() }
    }
}

unsafe impl<'d, T: ReprC + 'd> SoftDecodeMut<'d> for Reverse<T>
where
    T: RustSpec<Size: crate::Thin> + ReprC<CType: Sized>,
    &'d mut T: DecodeMove<'d> + ReprC<CType: Sized>,
    &'d mut Self: ReprC<CType: Sized>,
{
    type Store = <&'d mut T as DecodeMove<'d>>::Store;

    unsafe fn soft_decode_mut<'a: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'a mut Self::Store,
    ) -> Option<&'d mut Self> {
        let source = unsafe { transmute_owned(source) };
        unsafe { <&'d mut T as DecodeMove<'d>>::soft_decode(source, store) }
            .map(|value| unsafe { &mut *core::ptr::from_mut(value).cast::<Self>() })
    }

    unsafe fn soft_decode_mut_unchecked<'a: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'a mut Self::Store,
    ) -> &'d mut Self {
        let source = unsafe { transmute_owned(source) };
        let value = unsafe { <&'d mut T as DecodeMove<'d>>::soft_decode_unchecked(source, store) };
        unsafe { &mut *core::ptr::from_mut(value).cast::<Self>() }
    }
}

#[cfg(test)]
mod tests {
    use static_assertions::{assert_impl_all, assert_type_eq_all};

    use super::*;
    use crate::{decode, encode, option::COptionSome, primitives::CBool};

    #[test]
    fn reverse_reference_codecs_dispatch_stable_inner() {
        use co3::stored::{
            SoftDecodeMut, SoftDecodeRef, SoftEncodeMut, SoftEncodeMutUnchecked, SoftEncodeRef,
            SoftEncodeRefUnchecked,
        };
        use core::cmp::Reverse;

        let mut value = Reverse(7_u32);
        let mut decode_store = ();
        let carrier = SoftEncodeRef::soft_encode_ref(&value, &mut ());
        let decoded =
            unsafe { <Reverse<u32> as SoftDecodeRef>::soft_decode_ref(carrier, &mut decode_store) }
                .unwrap();
        assert!(core::ptr::eq(decoded, &value));

        let carrier = unsafe { SoftEncodeRefUnchecked::soft_encode_ref_unchecked(&value, &mut ()) };
        let decoded = unsafe {
            <Reverse<u32> as SoftDecodeRef>::soft_decode_ref_unchecked(carrier, &mut decode_store)
        };
        assert!(core::ptr::eq(decoded, &value));

        let carrier = SoftEncodeMut::soft_encode_mut(&mut value, &mut ());
        let decoded =
            unsafe { <Reverse<u32> as SoftDecodeMut>::soft_decode_mut(carrier, &mut decode_store) }
                .unwrap();
        decoded.0 = 11;
        assert_eq!(value.0, 11);

        let carrier = unsafe { SoftEncodeMutUnchecked::soft_encode_mut_unchecked(&mut value, &mut ()) };
        let decoded = unsafe {
            <Reverse<u32> as SoftDecodeMut>::soft_decode_mut_unchecked(carrier, &mut decode_store)
        };
        decoded.0 = 13;
        assert_eq!(value.0, 13);
    }

    #[test]
    fn ordering_conversions_validate_the_carrier() {
        for value in [Ordering::Less, Ordering::Equal, Ordering::Greater] {
            let carrier = COrdering::from(value);
            assert_eq!(Ordering::try_from(carrier), Ok(value));
            assert_eq!(encode(value), carrier);
            assert_eq!(unsafe { decode::<Ordering>(carrier) }, Some(value));
        }
        for raw in i8::MIN..=i8::MAX {
            let carrier = COrdering(raw);
            assert_eq!(carrier == carrier, (-1..=1).contains(&raw));
            assert_eq!(Ordering::try_from(carrier).is_ok(), (-1..=1).contains(&raw));
            assert_eq!(
                unsafe { decode::<Ordering>(carrier) }.is_some(),
                (-1..=1).contains(&raw)
            );
        }
        assert_ne!(COrdering::LESS, COrdering::EQUAL);
        assert_ne!(COrdering::EQUAL, COrdering::GREATER);
    }

    #[test]
    fn reverse_delegates_to_inner_carrier_and_niche() {
        assert_type_eq_all!(
            <Reverse<bool> as ReprC>::CType,
            core::cmp::Reverse<crate::primitives::CBool>
        );
        assert_impl_all!(Reverse<bool>: Encode, Decode<'static>, Niche, AbiCompatible);
        assert_impl_all!(&'static Reverse<bool>: Encode, Decode<'static>);
        assert_eq!(encode(Reverse(true)), core::cmp::Reverse(CBool::TRUE));
        assert_eq!(
            unsafe { decode::<Reverse<bool>>(core::cmp::Reverse(CBool::TRUE)) },
            Some(Reverse(true))
        );
        assert_eq!(
            unsafe { decode::<Reverse<bool>>(core::cmp::Reverse(CBool(2))) },
            None
        );
        let none: COptionSome<Reverse<bool>> = encode(None::<Reverse<bool>>);
        assert!(<Reverse<bool> as Niche>::is_niche(&none.0));
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn reverse_round_trips_owned_values() {
        use alloc::{boxed::Box, vec::Vec};
        assert_impl_all!(&'static Reverse<Vec<(u8,)>>: Encode, Decode<'static>);
        assert_impl_all!(&'static mut Reverse<Vec<bool>>: Encode, Decode<'static>);
        let value = Reverse(Box::new(7_u32));
        let carrier = encode(value);
        assert_eq!(
            unsafe { decode::<Reverse<Box<u32>>>(carrier) },
            Some(Reverse(Box::new(7)))
        );
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn reverse_shared_unstable_value_uses_inner_stores() {
        use alloc::{vec, vec::Vec};

        use crate::stored::Store;

        let value = Reverse(vec![(3_u8,), (7,)]);
        let mut encode_store = Default::default();
        let carrier = crate::soft_encode(&value, &mut encode_store);
        let mut decode_store = Default::default();
        let decoded =
            unsafe { crate::soft_decode::<&Reverse<Vec<(u8,)>>>(carrier, &mut decode_store) }
                .unwrap();
        assert_eq!(decoded, &value);
        decode_store.sync().unwrap();
        encode_store.sync().unwrap();
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn reverse_mutable_value_synchronizes_changes() {
        use alloc::{vec, vec::Vec};

        use crate::stored::Store;

        let mut value = Reverse(vec![false, true]);
        let mut encode_store = Default::default();
        let carrier = crate::soft_encode(&mut value, &mut encode_store);
        let mut decode_store = Default::default();
        let decoded =
            unsafe { crate::soft_decode::<&mut Reverse<Vec<bool>>>(carrier, &mut decode_store) }
                .unwrap();
        decoded.0[0] = true;
        decode_store.sync().unwrap();
        encode_store.sync().unwrap();
        assert_eq!(value, Reverse(vec![true, true]));
    }
}
