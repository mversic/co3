//! Primitive traits and types representing basic properties of types.
use core::marker::PhantomData;

use rust_spec::RustSpec;

pub use crate::abi::AbiCompatible;
use crate::{
    CFnArg, CFnReturn, CType, ReprC,
    convert::{
        AbiCompatibleValidate, Decode, DecodeMove, Encode, EncodeMove, EncodeMoveUnchecked,
        EncodeUnchecked,
    },
    niche::Niche,
};

/// A type for which an all-zero bit pattern is a valid value.
///
/// # Safety
///
/// `core::mem::zeroed::<Self>()` must produce a fully initialized, valid value.
// TODO: Consider using this crate instead:
// https://docs.rs/bytemuck/latest/bytemuck/trait.Zeroable.html
pub unsafe trait Zeroable: Sized {}

macro_rules! impl_zeroable {
    ($($ty:ty),+ $(,)?) => {
        $(unsafe impl Zeroable for $ty {})+
    };
}

impl_zeroable! {
    (), bool, char, f32, f64,
    u8, i8, u16, i16, u32, i32, u64, i64, u128, i128, usize, isize,
}

unsafe impl<T: Zeroable, const N: usize> Zeroable for [T; N] {}
unsafe impl<T: Zeroable> Zeroable for core::mem::MaybeUninit<T> {}
unsafe impl<T: ?Sized> Zeroable for PhantomData<T> {}
// TODO: it is unsettled yet whether dyn trait metadata must be valid
// for a raw pointer. if not, then we can implement for wide pointers
// https://doc.rust-lang.org/reference/behavior-considered-undefined.html#invalid-values
unsafe impl<T: Sized> Zeroable for *const T {}
unsafe impl<T: Sized> Zeroable for *mut T {}

crate::borrow::__impl_borrowed_identity!([T: ReprC + ?Sized] PhantomData<T>);

impl<T: ReprC + ?Sized> ReprC for PhantomData<T> {
    type CType = PhantomData<T::CType>;
}
unsafe impl<T: ReprC + ?Sized> AbiCompatibleValidate for PhantomData<T> {
    #[inline(always)]
    unsafe fn is_valid(_: &Self::CType) -> bool {
        true
    }
}
unsafe impl<T: ReprC + ?Sized> AbiCompatible for PhantomData<T> {}
unsafe impl<T: ReprC + ?Sized> EncodeMove for PhantomData<T> {
    type Store = ();

    #[inline(always)]
    fn soft_encode<'itm>(self, (): &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe { self.soft_encode_unchecked(&mut ()) }
    }
}
unsafe impl<T: ReprC + ?Sized> EncodeMoveUnchecked for PhantomData<T> {
    type SoftStore = ();

    unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        PhantomData
    }
}
unsafe impl<'d, T: ReprC + ?Sized> DecodeMove<'d> for PhantomData<T> {
    type Store = ();

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(_source: Self::CType, (): &mut ()) -> Option<Self> {
        Some(PhantomData)
    }
}

impl<T: ReprC + ?Sized> Decode<'_> for PhantomData<T> {}
impl<T: ReprC + ?Sized> EncodeUnchecked for PhantomData<T> {}
impl<T: ReprC + ?Sized> Encode for PhantomData<T> {}

unsafe impl<T: CType + ?Sized> CType for PhantomData<T> {}
// SAFETY: PhantomData<T> is guaranteed to have size 0 and alignment 1,
// making its return ABI compatible with unit.
unsafe impl<T: CType + ?Sized> CFnReturn for PhantomData<T> {}
crate::mutable::impl_direct_mut!([T: ReprC + ?Sized] PhantomData<T>);

/// Marks a lowered ABI value that is borrowed across the FFI boundary.
#[repr(transparent)]
#[derive(RustSpec)]
pub struct Borrowed<T: CType>(pub T);

impl<T: CType> ReprC for Borrowed<T> {
    type CType = Self;
}
unsafe impl<T: CType> CType for Borrowed<T> {}
unsafe impl<T: CType> AbiCompatibleValidate for Borrowed<T>
where
    Self: crate::rust_spec::RustSpec<Layout = crate::rust_spec::Specified>,
{
    unsafe fn is_valid(_: &Self) -> bool {
        true
    }
}
unsafe impl<T: CType> AbiCompatible for Borrowed<T> where
    Self: crate::rust_spec::RustSpec<Layout = crate::rust_spec::Specified>
{
}
unsafe impl<T: CType> CFnArg for Borrowed<T> where T: CFnArg {}
// SAFETY: repr(transparent) preserves T's return ABI, including unit.
unsafe impl<T: CFnReturn> CFnReturn for Borrowed<T> {}
impl<T: CType> Borrowed<T> {
    pub const fn new(value: T) -> Self {
        Self(value)
    }
    pub fn into_inner(self) -> T {
        self.0
    }
}
unsafe impl<T: CType> EncodeMoveUnchecked for Borrowed<T> {
    type SoftStore = ();
    unsafe fn soft_encode_unchecked<'a>(self, _: &'a mut ()) -> Self
    where
        Self: 'a,
    {
        self
    }
}
unsafe impl<T: CType> EncodeMove for Borrowed<T> {
    type Store = ();
    fn soft_encode<'a>(self, _: &'a mut ()) -> Self
    where
        Self: 'a,
    {
        self
    }
}
unsafe impl<'d, T: CType> DecodeMove<'d> for Borrowed<T> {
    type Store = ();
    unsafe fn soft_decode<'a: 'd>(value: Self, _: &'a mut ()) -> Option<Self> {
        Some(value)
    }
}
impl<T: CType> Encode for Borrowed<T> {}
impl<T: CType> crate::convert::EncodeUnchecked for Borrowed<T> {}
impl<'d, T: CType> Decode<'d> for Borrowed<T> {}
crate::borrow::__impl_borrowed_identity!([T: CType] Borrowed<T>);
impl<T: CType + Copy> Copy for Borrowed<T> {}
impl<T: CType + Copy> Clone for Borrowed<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: CType + core::fmt::Debug> core::fmt::Debug for Borrowed<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Borrowed").field(&self.0).finish()
    }
}
impl<T: CType + PartialEq> PartialEq for Borrowed<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<T: CType + Eq> Eq for Borrowed<T> {}

/// Marks the lowered carrier of the native Rust form of a type.
///
/// The marker has the same layout and calling convention as its payload.
#[derive(RustSpec, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Native<T: CType + ?Sized>(pub T);

impl<T: CType + ?Sized> ReprC for Native<T> {
    type CType = Self;
}

unsafe impl<T: CType + ?Sized> CType for Native<T> {}
unsafe impl<T: CType + ?Sized> AbiCompatibleValidate for Native<T>
where
    Self: crate::rust_spec::RustSpec<Layout = crate::rust_spec::Specified>,
{
    unsafe fn is_valid(_: &Self) -> bool {
        true
    }
}
unsafe impl<T: CType + ?Sized> AbiCompatible for Native<T> where
    Self: crate::rust_spec::RustSpec<Layout = crate::rust_spec::Specified>
{
}
unsafe impl<T: CFnArg> CFnArg for Native<T> {}
unsafe impl<T: CFnReturn> CFnReturn for Native<T> {}

unsafe impl<T: CType> EncodeMoveUnchecked for Native<T> {
    type SoftStore = ();

    unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self
    where
        Self: 'itm,
    {
        self
    }
}

unsafe impl<T: CType> EncodeMove for Native<T> {
    type Store = ();

    fn soft_encode<'itm>(self, (): &'itm mut ()) -> Self
    where
        Self: 'itm,
    {
        self
    }
}

unsafe impl<'d, T: CType> DecodeMove<'d> for Native<T> {
    type Store = ();

    unsafe fn soft_decode<'itm: 'd>(source: Self, (): &'itm mut ()) -> Option<Self> {
        Some(source)
    }
}

impl<T: CType> Encode for Native<T> {}
impl<T: CType> EncodeUnchecked for Native<T> {}
impl<T: CType> Decode<'_> for Native<T> {}
crate::borrow::__impl_borrowed_identity!([T: CType] Native<T>);

impl<T: CType> From<T> for Native<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

impl<T: CType + ?Sized> core::ops::Deref for Native<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}
impl<T: CType + ?Sized> core::ops::DerefMut for Native<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}
unsafe impl<T: CType + Zeroable> Zeroable for Native<T> {}

impl<T: CType + crate::wide::WideHeader> crate::wide::WideHeader for Native<T> {
    type Data = T::Data;
}

unsafe impl<T: CType + crate::wide::Wide + ?Sized> crate::wide::Wide for Native<T>
where
    T::Header: CType,
{
    type Header = Native<T::Header>;
    type Metadata = T::Metadata;

    fn metadata(pointer: *const Self) -> Self::Metadata {
        T::metadata(pointer as *const T)
    }
    fn as_ptr(pointer: *const Self) -> *const Self::Header {
        T::as_ptr(pointer as *const T).cast()
    }
    fn as_mut_ptr(pointer: *mut Self) -> *mut Self::Header {
        T::as_mut_ptr(pointer as *mut T).cast()
    }
    unsafe fn from_raw_parts<'a>(data: *const Self::Header, metadata: Self::Metadata) -> &'a Self {
        let value = unsafe { T::from_raw_parts(data.cast(), metadata) };
        unsafe { &*(core::ptr::from_ref(value) as *const Self) }
    }
    unsafe fn from_raw_parts_mut<'a>(
        data: *mut Self::Header,
        metadata: Self::Metadata,
    ) -> &'a mut Self {
        let value = unsafe { T::from_raw_parts_mut(data.cast(), metadata) };
        unsafe { &mut *(core::ptr::from_mut(value) as *mut Self) }
    }
    #[cfg(feature = "alloc")]
    fn into_non_null(self: alloc::boxed::Box<Self>) -> core::ptr::NonNull<Self::Header> {
        let value =
            unsafe { alloc::boxed::Box::from_raw(alloc::boxed::Box::into_raw(self) as *mut T) };
        T::into_non_null(value).cast()
    }
    #[cfg(feature = "alloc")]
    unsafe fn from_non_null(
        data: core::ptr::NonNull<Self::Header>,
        metadata: Self::Metadata,
    ) -> alloc::boxed::Box<Self> {
        let value = unsafe { T::from_non_null(data.cast(), metadata) };
        unsafe { alloc::boxed::Box::from_raw(alloc::boxed::Box::into_raw(value) as *mut Self) }
    }
}

/// Marks nonzero intent for a value, including pointer and other non-numeric payloads.
///
/// The transparent carrier permits zero. The source type supplies any nonzero
/// validation and niche semantics during conversion.
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
unsafe impl<R: AbiCompatibleValidate<CType: Sized>> AbiCompatibleValidate for NonZero<R> {
    unsafe fn is_valid(target: &Self::CType) -> bool {
        unsafe { R::is_valid(&target.0) }
    }
}
unsafe impl<R: AbiCompatible<CType: Sized>> AbiCompatible for NonZero<R> {}

macro_rules! non_zero_derive {
    ($($primitive:ty),+ $(,)?) => {$(
        crate::borrow::__impl_borrowed_identity!([] core::num::NonZero<$primitive>);

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
                crate::marker::NonZero(self.get())
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
            const NICHE: Self::CType = crate::marker::NonZero(0);

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

#[cfg(test)]
mod tests {
    use super::*;
    use static_assertions::{assert_impl_all, assert_not_impl_any, assert_type_eq_all};

    struct Unmapped;
    assert_type_eq_all!(
        <PhantomData<bool> as ReprC>::CType,
        PhantomData<<bool as ReprC>::CType>
    );
    assert_type_eq_all!(
        <PhantomData<[bool]> as ReprC>::CType,
        PhantomData<<[bool] as ReprC>::CType>
    );
    assert_impl_all!(PhantomData<u8>: CType, CFnReturn);
    assert_not_impl_any!(PhantomData<bool>: CType, CFnArg);
    assert_not_impl_any!(PhantomData<Unmapped>: ReprC);

    #[cfg(feature = "derive")]
    #[test]
    fn named_markers_lower_their_fields() {
        #[derive(RustSpec, crate::ReprC)]
        #[repr(transparent)]
        struct Marker<T: ?Sized>(PhantomData<T>);

        let encoded: <Marker<bool> as ReprC>::CType = crate::encode(Marker::<bool>(PhantomData));
        let _: PhantomData<<bool as ReprC>::CType> = encoded.0;
        let encoded: <Marker<[bool]> as ReprC>::CType =
            crate::encode(Marker::<[bool]>(PhantomData));
        let _: PhantomData<<[bool] as ReprC>::CType> = encoded.0;
    }

    #[test]
    fn phantom_data_round_trip() {
        let encoded: PhantomData<<bool as ReprC>::CType> = crate::encode(PhantomData::<bool>);
        assert_eq!(
            unsafe { crate::decode::<PhantomData<bool>>(encoded) },
            Some(PhantomData)
        );
        let encoded = crate::encode(PhantomData::<[bool]>);
        assert_eq!(
            unsafe { crate::decode::<PhantomData<[bool]>>(encoded) },
            Some(PhantomData)
        );
    }
}
