//! C-compatible representations of comparison types.

use core::cmp::{Ordering as RustOrdering, Reverse};
use rust_spec::RustSpec;

use crate::primitives::{fieldless_enum_derive, primitive_derive};
use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC,
    borrow::Borrow,
    niche::Niche,
    stored::{DecodeMove, EncodeMove, EncodeMoveUnchecked},
    transmute::AbiCompatible,
};

/// C-compatible carrier for [`core::cmp::Ordering`].
///
/// Every byte value is valid for this carrier; conversion to `Ordering` accepts only -1, 0, and 1.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, RustSpec)]
#[repr(transparent)]
pub struct Ordering(pub i8);

unsafe impl crate::Zeroable for Ordering {}

impl Ordering {
    pub const LESS: Self = Self(-1);
    pub const EQUAL: Self = Self(0);
    pub const GREATER: Self = Self(1);
}

impl From<RustOrdering> for Ordering {
    #[inline(always)]
    fn from(value: RustOrdering) -> Self {
        Self(value as i8)
    }
}

impl TryFrom<Ordering> for RustOrdering {
    type Error = Ordering;

    #[inline(always)]
    fn try_from(value: Ordering) -> Result<Self, Self::Error> {
        match value.0 {
            -1 => Ok(RustOrdering::Less),
            0 => Ok(RustOrdering::Equal),
            1 => Ok(RustOrdering::Greater),
            _ => Err(value),
        }
    }
}

primitive_derive! { Ordering }
fieldless_enum_derive! {
    RustOrdering => Ordering: {Ordering(-2)}:
    |i: &Ordering| RustOrdering::try_from(*i).is_ok();
    |source: Ordering| RustOrdering::try_from(source).ok();
    |source: RustOrdering| Ordering::from(source);
    |source: Ordering| unsafe { RustOrdering::try_from(source).unwrap_unchecked() }
}

impl<T: ReprC> ReprC for Reverse<T> {
    type CType = T::CType;
}

unsafe impl<T: Borrow> Borrow for Reverse<T> {
    type Borrowed = Reverse<T::Borrowed>;

    fn borrow(self) -> Self::Borrowed {
        Reverse(self.0.borrow())
    }

    fn from_borrow(source: Self::Borrowed) -> Self {
        Reverse(T::from_borrow(source.0))
    }
}

impl<T: Niche> Niche for Reverse<T> {
    const NICHE: Self::CType = T::NICHE;
}

unsafe impl<T: EncodeMoveUnchecked> EncodeMoveUnchecked for Reverse<T> {
    type SoftStore = T::SoftStore;

    unsafe fn soft_encode_unchecked<'a>(self, store: &'a mut Self::SoftStore) -> Self::CType
    where
        Self: 'a,
    {
        unsafe { self.0.soft_encode_unchecked(store) }
    }
}

unsafe impl<T: EncodeMove> EncodeMove for Reverse<T> {
    type Store = T::Store;

    fn soft_encode<'a>(self, store: &'a mut Self::Store) -> Self::CType
    where
        Self: 'a,
    {
        self.0.soft_encode(store)
    }
}

unsafe impl<'d, T: DecodeMove<'d>> DecodeMove<'d> for Reverse<T> {
    type Store = T::Store;

    unsafe fn soft_decode<'a: 'd>(source: Self::CType, store: &'a mut Self::Store) -> Option<Self> {
        unsafe { T::soft_decode(source, store) }.map(Reverse)
    }

    unsafe fn soft_decode_unchecked<'a: 'd>(
        source: Self::CType,
        store: &'a mut Self::Store,
    ) -> Self {
        Self(unsafe { T::soft_decode_unchecked(source, store) })
    }
}

impl<T: Encode> Encode for Reverse<T> {}
impl<T: crate::EncodeUnchecked> crate::EncodeUnchecked for Reverse<T> {}
impl<'d, T: Decode<'d>> Decode<'d> for Reverse<T> {}

unsafe impl<T: crate::transmute::AbiCompatibleValidate> crate::transmute::AbiCompatibleValidate
    for Reverse<T>
{
    unsafe fn is_valid(target: &Self::CType) -> bool {
        unsafe { T::is_valid(target) }
    }
}
unsafe impl<T: AbiCompatible> AbiCompatible for Reverse<T> {}
unsafe impl<T: CType> CType for Reverse<T> {}
unsafe impl<T: CFnArg> CFnArg for Reverse<T> {}
unsafe impl<T: CFnReturn> CFnReturn for Reverse<T> {}
unsafe impl<T: crate::Zeroable> crate::Zeroable for Reverse<T> {}

unsafe impl<'a, T: crate::shared::EncodeRefUnchecked<'a> + 'a> crate::shared::EncodeRefUnchecked<'a>
    for Reverse<T>
where
    &'a T: ReprC<CType: Sized>,
    &'a Reverse<T>: ReprC<CType = <&'a T as ReprC>::CType>,
{
    type SoftStore = T::SoftStore;

    unsafe fn encode_ref_unchecked<'b>(
        &'a self,
        store: &'b mut Self::SoftStore,
    ) -> <&'a Self as ReprC>::CType
    where
        'a: 'b,
    {
        unsafe { self.0.encode_ref_unchecked(store) }
    }
}

unsafe impl<'a, T: crate::shared::EncodeRef<'a> + 'a> crate::shared::EncodeRef<'a> for Reverse<T>
where
    &'a T: ReprC<CType: Sized>,
    &'a Reverse<T>: ReprC<CType = <&'a T as ReprC>::CType>,
{
    type Store = T::Store;

    fn encode_ref<'b>(&'a self, store: &'b mut Self::Store) -> <&'a Self as ReprC>::CType
    where
        'a: 'b,
    {
        self.0.encode_ref(store)
    }
}

unsafe impl<'a, T: crate::mutable::EncodeMutUnchecked<'a> + 'a>
    crate::mutable::EncodeMutUnchecked<'a> for Reverse<T>
where
    &'a mut T: ReprC<CType: Sized>,
    &'a mut Reverse<T>: ReprC<CType = <&'a mut T as ReprC>::CType>,
{
    type SoftStore = T::SoftStore;

    unsafe fn encode_mut_unchecked<'b>(
        &'a mut self,
        store: &'b mut Self::SoftStore,
    ) -> <&'a mut Self as ReprC>::CType
    where
        'a: 'b,
    {
        unsafe { self.0.encode_mut_unchecked(store) }
    }
}

unsafe impl<'a, T: crate::mutable::EncodeMut<'a> + 'a> crate::mutable::EncodeMut<'a> for Reverse<T>
where
    &'a mut T: ReprC<CType: Sized>,
    &'a mut Reverse<T>: ReprC<CType = <&'a mut T as ReprC>::CType>,
{
    type Store = T::Store;

    fn encode_mut<'b>(&'a mut self, store: &'b mut Self::Store) -> <&'a mut Self as ReprC>::CType
    where
        'a: 'b,
    {
        self.0.encode_mut(store)
    }
}

unsafe impl<'d, T: crate::decoded::DecodeRef<'d> + 'd> crate::decoded::DecodeRef<'d> for Reverse<T>
where
    &'d T: ReprC<CType: Sized>,
    &'d Reverse<T>: ReprC<CType = <&'d T as ReprC>::CType>,
{
    type Store = T::Store;

    unsafe fn decode_ref<'a: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'a mut Self::Store,
    ) -> Option<&'d Self> {
        unsafe { T::decode_ref(source, store) }
            .map(|value| unsafe { &*core::ptr::from_ref(value).cast::<Self>() })
    }

    unsafe fn decode_ref_unchecked<'a: 'd>(
        source: <&'d Self as ReprC>::CType,
        store: &'a mut Self::Store,
    ) -> &'d Self {
        let value = unsafe { T::decode_ref_unchecked(source, store) };
        unsafe { &*core::ptr::from_ref(value).cast::<Self>() }
    }
}

unsafe impl<'d, T: crate::decoded::DecodeMut<'d> + 'd> crate::decoded::DecodeMut<'d> for Reverse<T>
where
    &'d mut T: ReprC<CType: Sized>,
    &'d mut Reverse<T>: ReprC<CType = <&'d mut T as ReprC>::CType>,
{
    type Store = T::Store;

    unsafe fn decode_mut<'a: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'a mut Self::Store,
    ) -> Option<&'d mut Self> {
        unsafe { T::decode_mut(source, store) }
            .map(|value| unsafe { &mut *core::ptr::from_mut(value).cast::<Self>() })
    }

    unsafe fn decode_mut_unchecked<'a: 'd>(
        source: <&'d mut Self as ReprC>::CType,
        store: &'a mut Self::Store,
    ) -> &'d mut Self {
        let value = unsafe { T::decode_mut_unchecked(source, store) };
        unsafe { &mut *core::ptr::from_mut(value).cast::<Self>() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{decode, encode, option::COptionSome, primitives::Bool};
    use static_assertions::{assert_impl_all, assert_type_eq_all};

    #[test]
    fn ordering_conversions_validate_the_carrier() {
        for value in [
            RustOrdering::Less,
            RustOrdering::Equal,
            RustOrdering::Greater,
        ] {
            let carrier = Ordering::from(value);
            assert_eq!(RustOrdering::try_from(carrier), Ok(value));
            assert_eq!(encode(value), carrier);
            assert_eq!(unsafe { decode::<RustOrdering>(carrier) }, Some(value));
        }
        for raw in i8::MIN..=i8::MAX {
            let carrier = Ordering(raw);
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
        assert_type_eq_all!(<Reverse<bool> as ReprC>::CType, Bool);
        assert_impl_all!(Reverse<bool>: Encode, Decode<'static>, Niche, AbiCompatible);
        assert_impl_all!(&'static Reverse<bool>: Encode, Decode<'static>);
        assert_eq!(encode(Reverse(true)), Bool::TRUE);
        assert_eq!(
            unsafe { decode::<Reverse<bool>>(Bool::TRUE) },
            Some(Reverse(true))
        );
        assert_eq!(unsafe { decode::<Reverse<bool>>(Bool(2)) }, None);
        let none: COptionSome<Reverse<bool>> = encode(None::<Reverse<bool>>);
        assert_eq!(none.into_carrier(), <bool as Niche>::NICHE);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn reverse_round_trips_owned_values() {
        use alloc::boxed::Box;
        use alloc::vec::Vec;
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
        use crate::stored::Store;
        use alloc::{vec, vec::Vec};

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
        use crate::stored::Store;
        use alloc::{vec, vec::Vec};

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
