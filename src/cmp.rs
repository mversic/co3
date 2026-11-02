//! C-compatible representations of comparison types.
use core::cmp::{Ordering as RustOrdering, Reverse};

use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC, Zeroable,
    borrow::Borrow,
    niche::Niche,
    primitives::{fieldless_enum_derive, primitive_derive},
    shared::{EncodeRef, EncodeRefUnchecked},
    stored::{
        DecodeMove, DecodeMut, DecodeRef, EncodeMove, EncodeMoveUnchecked, EncodeMut,
        EncodeMutUnchecked,
    },
    transmute::{AbiCompatible, AbiCompatibleValidate, transmute_owned},
};

/// C-compatible carrier for [`core::cmp::Ordering`].
///
/// Every byte value is valid for this carrier; conversion to `Ordering` accepts only -1, 0, and 1.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, RustSpec)]
#[repr(transparent)]
pub struct COrdering(pub i8);

unsafe impl Zeroable for COrdering {}

impl COrdering {
    pub const LESS: Self = Self(-1);
    pub const EQUAL: Self = Self(0);
    pub const GREATER: Self = Self(1);
}

impl From<RustOrdering> for COrdering {
    #[inline(always)]
    fn from(value: RustOrdering) -> Self {
        Self(value as i8)
    }
}

impl TryFrom<COrdering> for RustOrdering {
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
    RustOrdering => COrdering: {COrdering(-2)}:
    |i: &COrdering| RustOrdering::try_from(*i).is_ok();
    |source: COrdering| RustOrdering::try_from(source).ok();
    |source: RustOrdering| COrdering::from(source);
    |source: COrdering| unsafe { RustOrdering::try_from(source).unwrap_unchecked() }
}

impl<T: ReprC<CType: Sized>> ReprC for Reverse<T> {
    type CType = Reverse<T::CType>;
}

unsafe impl<T: Borrow> Borrow for Reverse<T> {
    type Borrowed = Reverse<T::Borrowed>;

    fn borrow(self) -> Self::Borrowed {
        Reverse(self.0.borrow())
    }

    fn from_borrow(source: Self::Borrowed) -> Self {
        Self(T::from_borrow(source.0))
    }
}

impl<T: Niche<CType: Sized>> Niche for Reverse<T> {
    const NICHE: Self::CType = Reverse(T::NICHE);
}

unsafe impl<T: EncodeMoveUnchecked<CType: Sized>> EncodeMoveUnchecked for Reverse<T> {
    type SoftStore = T::SoftStore;

    unsafe fn soft_encode_unchecked<'a>(self, store: &'a mut Self::SoftStore) -> Self::CType
    where
        Self: 'a,
    {
        unsafe { Reverse(self.0.soft_encode_unchecked(store)) }
    }
}

unsafe impl<T: EncodeMove<CType: Sized>> EncodeMove for Reverse<T> {
    type Store = T::Store;

    fn soft_encode<'a>(self, store: &'a mut Self::Store) -> Self::CType
    where
        Self: 'a,
    {
        Reverse(self.0.soft_encode(store))
    }
}

unsafe impl<'d, T: DecodeMove<'d, CType: Sized>> DecodeMove<'d> for Reverse<T> {
    type Store = T::Store;

    unsafe fn soft_decode<'a: 'd>(source: Self::CType, store: &'a mut Self::Store) -> Option<Self> {
        unsafe { T::soft_decode(source.0, store) }.map(Self)
    }

    unsafe fn soft_decode_unchecked<'a: 'd>(
        source: Self::CType,
        store: &'a mut Self::Store,
    ) -> Self {
        Self(unsafe { T::soft_decode_unchecked(source.0, store) })
    }
}

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

unsafe impl<'a, T: EncodeRefUnchecked<'a> + 'a> EncodeRefUnchecked<'a> for Reverse<T>
where
    T: RustSpec<Size: crate::Thin> + ReprC<CType: Sized>,
    &'a Self: ReprC<CType: Sized>,
    &'a T: ReprC<CType: Sized>,
{
    type SoftStore = T::SoftStore;

    unsafe fn encode_ref_unchecked<'b>(
        &'a self,
        store: &'b mut Self::SoftStore,
    ) -> <&'a Self as ReprC>::CType
    where
        'a: 'b,
    {
        let carrier = unsafe { self.0.encode_ref_unchecked(store) };
        // Reverse<T::CType> is transparent over T::CType.
        unsafe { transmute_owned(carrier) }
    }
}

unsafe impl<'a, T: EncodeRef<'a> + 'a> EncodeRef<'a> for Reverse<T>
where
    T: RustSpec<Size: crate::Thin> + ReprC<CType: Sized>,
    &'a Self: ReprC<CType: Sized>,
    &'a T: ReprC<CType: Sized>,
{
    type Store = T::Store;

    fn encode_ref<'b>(&'a self, store: &'b mut Self::Store) -> <&'a Self as ReprC>::CType
    where
        'a: 'b,
    {
        let carrier = self.0.encode_ref(store);
        unsafe { transmute_owned(carrier) }
    }
}

unsafe impl<'a, T: EncodeMutUnchecked<'a> + 'a> EncodeMutUnchecked<'a> for Reverse<T>
where
    T: RustSpec<Size: crate::Thin> + ReprC<CType: Sized>,
    &'a mut Self: ReprC<CType: Sized>,
    &'a mut T: ReprC<CType: Sized>,
{
    type SoftStore = T::SoftStore;

    unsafe fn encode_mut_unchecked<'b>(
        &'a mut self,
        store: &'b mut Self::SoftStore,
    ) -> <&'a mut Self as ReprC>::CType
    where
        'a: 'b,
    {
        let carrier = unsafe { self.0.encode_mut_unchecked(store) };
        unsafe { transmute_owned(carrier) }
    }
}

unsafe impl<'a, T: EncodeMut<'a> + 'a> EncodeMut<'a> for Reverse<T>
where
    T: RustSpec<Size: crate::Thin> + ReprC<CType: Sized>,
    &'a mut Self: ReprC<CType: Sized>,
    &'a mut T: ReprC<CType: Sized>,
{
    type Store = T::Store;

    fn encode_mut<'b>(&'a mut self, store: &'b mut Self::Store) -> <&'a mut Self as ReprC>::CType
    where
        'a: 'b,
    {
        let carrier = self.0.encode_mut(store);
        unsafe { transmute_owned(carrier) }
    }
}

unsafe impl<'d, T: DecodeRef<'d> + 'd> DecodeRef<'d> for Reverse<T>
where
    T: RustSpec<Size: crate::Thin> + ReprC<CType: Sized>,
    &'d Self: ReprC<CType: Sized>,
    &'d T: ReprC<CType: Sized>,
{
    type Store = T::Store;

    unsafe fn decode_ref<'a: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'a mut Self::Store,
    ) -> Option<&'d Self> {
        let source = unsafe { transmute_owned(source) };
        unsafe { T::decode_ref(source, store) }
            .map(|value| unsafe { &*core::ptr::from_ref(value).cast::<Self>() })
    }

    unsafe fn decode_ref_unchecked<'a: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'a mut Self::Store,
    ) -> &'d Self {
        let source = unsafe { transmute_owned(source) };
        let value = unsafe { T::decode_ref_unchecked(source, store) };
        unsafe { &*core::ptr::from_ref(value).cast::<Self>() }
    }
}

unsafe impl<'d, T: DecodeMut<'d> + 'd> DecodeMut<'d> for Reverse<T>
where
    T: RustSpec<Size: crate::Thin> + ReprC<CType: Sized>,
    &'d mut T: ReprC<CType: Sized>,
    &'d mut Self: ReprC<CType: Sized>,
{
    type Store = T::Store;

    unsafe fn decode_mut<'a: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'a mut Self::Store,
    ) -> Option<&'d mut Self> {
        let source = unsafe { transmute_owned(source) };
        unsafe { T::decode_mut(source, store) }
            .map(|value| unsafe { &mut *core::ptr::from_mut(value).cast::<Self>() })
    }

    unsafe fn decode_mut_unchecked<'a: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'a mut Self::Store,
    ) -> &'d mut Self {
        let source = unsafe { transmute_owned(source) };
        let value = unsafe { T::decode_mut_unchecked(source, store) };
        unsafe { &mut *core::ptr::from_mut(value).cast::<Self>() }
    }
}

#[cfg(test)]
mod tests {
    use static_assertions::{assert_impl_all, assert_type_eq_all};

    use super::*;
    use crate::{decode, encode, option::COptionSome, primitives::CBool};

    #[test]
    fn ordering_conversions_validate_the_carrier() {
        for value in [
            RustOrdering::Less,
            RustOrdering::Equal,
            RustOrdering::Greater,
        ] {
            let carrier = COrdering::from(value);
            assert_eq!(RustOrdering::try_from(carrier), Ok(value));
            assert_eq!(encode(value), carrier);
            assert_eq!(unsafe { decode::<RustOrdering>(carrier) }, Some(value));
        }
        for raw in i8::MIN..=i8::MAX {
            let carrier = COrdering(raw);
            assert_eq!(
                RustOrdering::try_from(carrier).is_ok(),
                (-1..=1).contains(&raw)
            );
            assert_eq!(
                unsafe { decode::<RustOrdering>(carrier) }.is_some(),
                (-1..=1).contains(&raw)
            );
        }
    }

    #[test]
    fn reverse_delegates_to_inner_carrier_and_niche() {
        assert_type_eq_all!(<Reverse<bool> as ReprC>::CType, Reverse<CBool>);
        assert_impl_all!(Reverse<bool>: Encode, Decode<'static>, Niche, AbiCompatible);
        assert_impl_all!(&'static Reverse<bool>: Encode, Decode<'static>);
        assert_eq!(encode(Reverse(true)), Reverse(CBool::TRUE));
        assert_eq!(
            unsafe { decode::<Reverse<bool>>(Reverse(CBool::TRUE)) },
            Some(Reverse(true))
        );
        assert_eq!(unsafe { decode::<Reverse<bool>>(Reverse(CBool(2))) }, None);
        let none: COptionSome<Reverse<bool>> = encode(None::<Reverse<bool>>);
        assert_eq!(none.into_carrier(), Reverse(<bool as Niche>::NICHE));
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
