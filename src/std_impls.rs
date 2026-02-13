use core::{
    cell::{Cell, UnsafeCell},
    ptr::NonNull,
};

#[cfg(feature = "owned-types")]
use alloc::{boxed::Box, string::String, vec::Vec};

#[cfg(feature = "owned-types")]
use crate::VecCType;
use crate::{
    ReprC,
    ir::{ReprFamily, Transmuted},
    mineral,
    niche::{Niche, NicheFamily, StableNiche, WithCustomNiche, WithStableNiche, WithoutNiche},
    slice::{CSlice, CSliceMut},
    transmute::{CheckedTransmute, EncodeTransmuted},
};

macro_rules! non_zero_derive {
    ($($ty:ty => $target:ty),+ $(,)?) => {$(
        mineral! {
            unsafe impl Transparent for $ty {
                type Target = $target;

                const NICHE_VALUE: Self::CType = 0;
                fn is_valid(target: &Self::Target) -> bool {
                    *target != <Self as crate::niche::Niche>::NICHE_VALUE
                }
            }
        }

        unsafe impl StableNiche for $ty {}
        unsafe impl ReprC for Option<$ty> {})+
    }
}

non_zero_derive! {
    core::num::NonZeroU8 => u8,
    core::num::NonZeroI8 => i8,
    core::num::NonZeroU16 => u16,
    core::num::NonZeroI16 => i16,
    core::num::NonZeroU32 => u32,
    core::num::NonZeroI32 => i32,
    core::num::NonZeroU64 => u64,
    core::num::NonZeroI64 => i64,
    core::num::NonZeroU128 => u128,
    core::num::NonZeroI128 => i128,
}

mineral! {
    unsafe impl(T,) Transparent for core::mem::ManuallyDrop<T> {
        type Target = T;
    }
}

impl<T> ReprFamily for UnsafeCell<T> {
    type Kind = Transmuted;
}
impl<T> ReprFamily for Cell<T> {
    type Kind = Transmuted;
}
impl<T> ReprFamily for NonNull<T> {
    type Kind = Transmuted;
}
impl ReprFamily for &str {
    type Kind = Transmuted;
}
impl ReprFamily for &mut str {
    type Kind = Transmuted;
}
#[cfg(feature = "owned-types")]
impl ReprFamily for Box<str> {
    type Kind = Transmuted;
}
#[cfg(feature = "owned-types")]
impl ReprFamily for String {
    type Kind = Transmuted;
}

impl<T> NicheFamily for UnsafeCell<T> {
    type Kind = WithoutNiche;
}
impl<T> NicheFamily for Cell<T> {
    type Kind = WithoutNiche;
}
impl<T> NicheFamily for NonNull<T> {
    type Kind = WithStableNiche;
}
impl NicheFamily for &str {
    type Kind = WithCustomNiche;
}
impl NicheFamily for &mut str {
    type Kind = WithCustomNiche;
}
#[cfg(feature = "owned-types")]
impl NicheFamily for Box<str> {
    type Kind = WithCustomNiche;
}
#[cfg(feature = "owned-types")]
impl NicheFamily for String {
    type Kind = WithCustomNiche;
}

unsafe impl<T> CheckedTransmute for UnsafeCell<T> {
    type Target = T;

    #[inline(always)]
    fn is_valid(_: &Self::Target) -> bool {
        true
    }
}
unsafe impl<T> CheckedTransmute for Cell<T> {
    type Target = UnsafeCell<T>;

    #[inline(always)]
    fn is_valid(_: &Self::Target) -> bool {
        true
    }
}
unsafe impl<T> CheckedTransmute for NonNull<T> {
    type Target = *mut T;

    #[inline(always)]
    fn is_valid(target: &Self::Target) -> bool {
        !target.is_null()
    }
}
unsafe impl<'a> CheckedTransmute for &'a str {
    type Target = &'a [u8];

    #[inline(always)]
    fn is_valid(target: &Self::Target) -> bool {
        core::str::from_utf8(target).is_ok()
    }
}
unsafe impl<'a> CheckedTransmute for &'a mut str {
    // WARN: `core::str::as_bytes` uses transmute internally which means that
    // even though it's a string slice it can be transmuted into byte slice.
    type Target = &'a mut [u8];

    #[inline(always)]
    fn is_valid(target: &Self::Target) -> bool {
        core::str::from_utf8(target).is_ok()
    }
}
#[cfg(feature = "owned-types")]
unsafe impl CheckedTransmute for Box<str> {
    // WARN: `core::str::as_bytes` uses transmute internally which means that
    // even though it's a string slice it can be transmuted into byte slice.
    type Target = Box<[u8]>;

    #[inline(always)]
    fn is_valid(target: &Self::Target) -> bool {
        core::str::from_utf8(target).is_ok()
    }
}
#[cfg(feature = "owned-types")]
unsafe impl CheckedTransmute for String {
    // WARN: This can be contested as it is nowhere documented that String is
    // actually transmutable into Vec<u8>, but implicitly it should be
    type Target = Vec<u8>;

    #[inline(always)]
    fn is_valid(target: &Self::Target) -> bool {
        core::str::from_utf8(target).is_ok()
    }
}

impl<T> Niche for NonNull<T> {
    const NICHE_VALUE: Self::CType = core::ptr::null_mut();
}
impl Niche for &str {
    const NICHE_VALUE: Self::CType = CSlice::none();
}
impl Niche for &mut str {
    const NICHE_VALUE: Self::CType = CSliceMut::none();
}
#[cfg(feature = "owned-types")]
impl Niche for String {
    const NICHE_VALUE: Self::CType = VecCType::none();
}
#[cfg(feature = "owned-types")]
impl Niche for Box<str> {
    const NICHE_VALUE: Self::CType = VecCType::none();
}

unsafe impl<R> EncodeTransmuted for UnsafeCell<R>
where
    Self: CheckedTransmute<Target: crate::Encode>,
{
    type Store = <Self::Target as crate::Encode>::Store;
}
unsafe impl<R> EncodeTransmuted for Cell<R>
where
    Self: CheckedTransmute<Target: crate::Encode>,
{
    type Store = <Self::Target as crate::Encode>::Store;
}
unsafe impl<R> EncodeTransmuted for NonNull<R>
where
    Self: CheckedTransmute<Target: crate::Encode>,
{
    type Store = <Self::Target as crate::Encode>::Store;
}
unsafe impl EncodeTransmuted for &str
where
    Self: CheckedTransmute<Target: crate::Encode>,
{
    type Store = <Self::Target as crate::Encode>::Store;
}
unsafe impl EncodeTransmuted for &mut str
where
    Self: CheckedTransmute<Target: crate::Encode>,
{
    type Store = <Self::Target as crate::Encode>::Store;
}
#[cfg(feature = "owned-types")]
unsafe impl EncodeTransmuted for Box<str>
where
    Self: CheckedTransmute<Target: crate::Encode>,
{
    type Store = <Self::Target as crate::Encode>::Store;
}
#[cfg(feature = "owned-types")]
unsafe impl EncodeTransmuted for String
where
    Self: CheckedTransmute<Target: crate::Encode>,
{
    type Store = <Self::Target as crate::Encode>::Store;
}
