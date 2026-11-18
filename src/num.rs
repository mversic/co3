//! C-compatible representations and conversions for [`core::num`].
use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, ReprC, Zeroable,
    convert::{Decode, DecodeMove, Encode, EncodeMove, EncodeMoveUnchecked, EncodeUnchecked},
    transmute,
    transmute::{AbiCompatible, AbiCompatibleValidate},
};
/// Transparent Rust wrapper recording a nonzero value intent.
///
/// This wrapper also permits zero in its C representation.
/// Comparisons require the inner zero value to compare equal to itself; otherwise
/// equality returns false and ordering returns `None`.
#[derive(Debug, Clone, Copy, RustSpec)]
#[repr(transparent)]
pub struct NonZero<C>(pub C);

impl<C: Zeroable + PartialEq> PartialEq for NonZero<C> {
    fn eq(&self, other: &Self) -> bool {
        let zero = unsafe { core::mem::zeroed() };
        PartialEq::eq(&zero, &zero) && self.0 != zero && self.0 == other.0
    }
}

impl<C: Zeroable + PartialOrd> PartialOrd for NonZero<C> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        let zero = unsafe { core::mem::zeroed() };
        if !PartialEq::eq(&zero, &zero) || self.0 == zero || other.0 == zero {
            None
        } else {
            self.0.partial_cmp(&other.0)
        }
    }
}

impl<C: Zeroable + PartialEq> NonZero<C> {
    /// Returns `None` for zero or when zero does not compare equal to itself.
    pub fn new(value: C) -> Option<Self> {
        let zero = unsafe { core::mem::zeroed() };

        // NOTE: Assert the relation is symmetric first
        if !PartialEq::eq(&zero, &zero) || value == zero {
            return None;
        }

        Some(Self(value))
    }
}

impl<R: ReprC<CType: Sized>> ReprC for NonZero<R> {
    type CType = NonZero<R::CType>;
}
unsafe impl<C: CType> CType for NonZero<C> {}
unsafe impl<C: CFnArg> CFnArg for NonZero<C> {}
unsafe impl<C: CFnReturn> CFnReturn for NonZero<C> {}

macro_rules! nonzero_codecs {
    ($encode:ident, $unchecked:ident, $decode:ident) => {
        unsafe impl<R: $unchecked> $unchecked for NonZero<R> {
            type SoftStore = R::SoftStore;

            unsafe fn soft_encode_unchecked<'a>(self, store: &'a mut Self::SoftStore) -> Self::CType
            where
                Self: 'a,
            {
                NonZero(unsafe { self.0.soft_encode_unchecked(store) })
            }
        }
        unsafe impl<R: $encode> $encode for NonZero<R> {
            type Store = R::Store;

            fn soft_encode<'a>(self, store: &'a mut Self::Store) -> Self::CType
            where
                Self: 'a,
            {
                NonZero($encode::soft_encode(self.0, store))
            }
        }
        unsafe impl<'d, R: $decode<'d>> $decode<'d> for NonZero<R> {
            type Store = R::Store;

            unsafe fn soft_decode<'a: 'd>(
                source: Self::CType,
                store: &'a mut Self::Store,
            ) -> Option<Self> {
                Some(NonZero(unsafe { R::soft_decode(source.0, store)? }))
            }

            unsafe fn soft_decode_unchecked<'a: 'd>(
                source: Self::CType,
                store: &'a mut Self::Store,
            ) -> Self {
                NonZero(unsafe { R::soft_decode_unchecked(source.0, store) })
            }
        }
    };
}
nonzero_codecs!(EncodeMove, EncodeMoveUnchecked, DecodeMove);
const _: () = {
    use crate::borrow::{BorrowedDecode, BorrowedEncode, BorrowedEncodeUnchecked};
    unsafe impl<R: BorrowedEncodeUnchecked> BorrowedEncodeUnchecked for NonZero<R> {
        type Owner = R::Owner;
        type SoftStore = R::SoftStore;

        unsafe fn soft_encode_unchecked<'a>(
            self,
            store: &'a mut Self::SoftStore,
            owner: &'a mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'a,
        {
            NonZero(unsafe { self.0.soft_encode_unchecked(store, owner) })
        }
    }
    unsafe impl<R: BorrowedEncode> BorrowedEncode for NonZero<R> {
        type Store = R::Store;

        fn soft_encode<'a>(
            self,
            store: &'a mut Self::Store,
            owner: &'a mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'a,
        {
            NonZero(BorrowedEncode::soft_encode(self.0, store, owner))
        }
    }
    unsafe impl<'d, R: BorrowedDecode<'d>> BorrowedDecode<'d> for NonZero<R> {
        type Store = R::Store;

        unsafe fn soft_decode<'a: 'd>(
            source: Self::CType,
            store: &'a mut Self::Store,
        ) -> Option<Self> {
            Some(NonZero(unsafe { R::soft_decode(source.0, store)? }))
        }

        unsafe fn soft_decode_unchecked<'a: 'd>(
            source: Self::CType,
            store: &'a mut Self::Store,
        ) -> Self {
            NonZero(unsafe { R::soft_decode_unchecked(source.0, store) })
        }
    }
};

impl<R: Encode> Encode for NonZero<R> {}
impl<R: EncodeUnchecked> EncodeUnchecked for NonZero<R> {}
impl<'d, R: Decode<'d>> Decode<'d> for NonZero<R> {}
unsafe impl<R: transmute::AbiCompatibleValidate<CType: Sized>> transmute::AbiCompatibleValidate
    for NonZero<R>
{
    unsafe fn is_valid(target: &Self::CType) -> bool {
        unsafe { R::is_valid(&target.0) }
    }
}
unsafe impl<R: transmute::AbiCompatible<CType: Sized>> transmute::AbiCompatible for NonZero<R> {}

macro_rules! non_zero_derive {
    ($($primitive:ty),+ $(,)?) => {$(
        crate::__impl_borrowed_identity!([] core::num::NonZero<$primitive>);

        impl ReprC for core::num::NonZero<$primitive> {
            type CType = NonZero<$primitive>;
        }
        unsafe impl EncodeMove for core::num::NonZero<$primitive> {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm
            {
                unsafe { self.soft_encode_unchecked(&mut ()) }
            }
        }
        unsafe impl EncodeMoveUnchecked for core::num::NonZero<$primitive> {
            type SoftStore = ();

            unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType where Self: 'itm {
                crate::num::NonZero(self.get())
            }
        }
        unsafe impl<'d> DecodeMove<'d> for core::num::NonZero<$primitive> {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Self::new(source.0)
            }

            #[inline(always)]
            unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
                unsafe { Self::new_unchecked(source.0) }
            }
        }

        impl Decode<'_> for core::num::NonZero<$primitive> {}
        impl EncodeUnchecked for core::num::NonZero<$primitive> {}
        impl Encode for core::num::NonZero<$primitive> {}

        unsafe impl AbiCompatibleValidate for core::num::NonZero<$primitive> {
            #[inline(always)]
            unsafe fn is_valid(target: &Self::CType) -> bool {
                target.0 != 0
            }
        }
        unsafe impl AbiCompatible for core::num::NonZero<$primitive> {}

        impl Niche for core::num::NonZero<$primitive> {
            const NICHE: Self::CType = crate::num::NonZero(0);

            fn is_niche(value: &Self::CType) -> bool {
                value.0 == 0
            }
        })+
    }
}

non_zero_derive! {
    u8, i8, u16, i16, u32, i32, u64, i64, u128, i128,
    usize, isize,
}

use crate::niche::Niche;
