#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, string::String, vec::Vec};
use core::{cell::UnsafeCell, ptr::NonNull};

#[cfg(feature = "alloc")]
use crate::{boxed::CBoxedSlice, vec::CVec};
use crate::{
    ir::{ReprFamily, Transmuted},
    niche::{Niche, NicheFamily, StableNiche, WithCustomNiche, WithStableNiche, WithoutNiche},
    reprC,
    slice::{CSlice, CSliceMut},
    transmute::CheckedTransmute,
};

// FIXME: Replace with NonZero<T>
macro_rules! non_zero_derive {
    ($($ty:ty => $target:ty),+ $(,)?) => {$(
        reprC! {
            unsafe impl Transparent for $ty {
                type Target = <$target as $crate::ExternC>::CType;

                const NICHE_VALUE: Self::CType = 0;
                fn is_valid(target: &Self::Target) -> bool {
                    *target != 0
                }
            }
        }

        unsafe impl StableNiche for $ty {})+
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

reprC! {
    unsafe impl(T) Transparent for core::mem::ManuallyDrop<T> {
        type Target = T;
    }
}
reprC! {
    unsafe impl(T) Transparent for core::cell::Cell<T> {
        type Target = T;
    }
}

impl<T> ReprFamily for UnsafeCell<T> {
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
#[cfg(feature = "alloc")]
impl ReprFamily for Box<str> {
    type Kind = Transmuted;
}
#[cfg(feature = "alloc")]
impl ReprFamily for String {
    type Kind = Transmuted;
}

impl<T> NicheFamily for UnsafeCell<T> {
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
#[cfg(feature = "alloc")]
impl NicheFamily for Box<str> {
    type Kind = WithCustomNiche;
}
#[cfg(feature = "alloc")]
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
#[cfg(feature = "alloc")]
unsafe impl CheckedTransmute for Box<str> {
    // WARN: `core::str::as_bytes` uses transmute internally which means that
    // even though it's a string slice it can be transmuted into byte slice.
    type Target = Box<[u8]>;

    #[inline(always)]
    fn is_valid(target: &Self::Target) -> bool {
        core::str::from_utf8(target).is_ok()
    }
}
#[cfg(feature = "alloc")]
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
#[cfg(feature = "alloc")]
impl Niche for String {
    const NICHE_VALUE: Self::CType = CVec::none();
}
#[cfg(feature = "alloc")]
impl Niche for Box<str> {
    const NICHE_VALUE: Self::CType = CBoxedSlice::none();
}

//unsafe impl<R> EncodeTransmuted for UnsafeCell<R>
//where
//    Self: CheckedTransmute<Target: crate::Encode>,
//{
//    type Store = <Self::Target as crate::Encode>::Store;
//}
//unsafe impl<R> EncodeTransmuted for NonNull<R>
//where
//    Self: CheckedTransmute<Target: crate::Encode>,
//{
//    type Store = <Self::Target as crate::Encode>::Store;
//}
//unsafe impl EncodeTransmuted for &str {
//    type Store = <Self::Target as crate::Encode>::Store;
//}
//unsafe impl EncodeTransmuted for &mut str {
//    type Store = <Self::Target as crate::Encode>::Store;
//}
//#[cfg(feature = "alloc")]
//unsafe impl EncodeTransmuted for Box<str> {
//    type Store = <Self::Target as crate::Encode>::Store;
//}
//#[cfg(feature = "alloc")]
//unsafe impl EncodeTransmuted for String {
//    type Store = <Self::Target as crate::Encode>::Store;
//}

#[cfg(test)]
mod tests {
    use core::num::NonZeroU8;

    use static_assertions::assert_impl_all;

    use super::*;
    use crate::{
        Decode, Encode, ExternC,
        option::COption,
        slice::{CSlice, CSliceMut},
        transmute::FlatTransmute,
    };

    #[test]
    fn unsafe_cell_is_without_niche() {
        assert_impl_all!(UnsafeCell<NonZeroU8>:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithoutNiche>,
            FlatTransmute<Target = u8>,
            ExternC<CType = u8>,
            Decode<'static>,
            // FIXME:
            //Encode,
        );
        assert_impl_all!(&UnsafeCell<NonZeroU8>:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut UnsafeCell<NonZeroU8>:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut u8>,
            Decode<'static>,
        );
        // FIXME:
        //#[cfg(feature = "alloc")]
        //assert_impl_all!(Box<UnsafeCell<NonZeroU8>>:
        //    ReprFamily<Kind = Transmuted>,
        //    NicheFamily<Kind = WithStableNiche>,
        //    StableNiche<CType = *mut u8>,
        //    Decode<'static>,
        //    Encode,
        //);
        assert_impl_all!(&[UnsafeCell<NonZeroU8>]:
            ReprFamily<Kind = &'static [Transmuted]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [UnsafeCell<NonZeroU8>]:
            ReprFamily<Kind = &'static mut [Transmuted]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<u8>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[UnsafeCell<NonZeroU8>]>:
            ReprFamily<Kind = Box<[Transmuted]>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            // FIXME:
            //Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<UnsafeCell<NonZeroU8>>:
            ReprFamily<Kind = Vec<Transmuted>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CVec<u8>>,
            // FIXME:
            //Decode<'static>,
            Encode,
        );
        assert_impl_all!([UnsafeCell<NonZeroU8>; 2]:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = [u8; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<UnsafeCell<NonZeroU8>>:
            ReprFamily<Kind = Option<WithoutNiche>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = COption<u8>>,
            Decode<'static>,
            // FIXME:
            //Encode,
        );

        // FIXME:
        //#[cfg(any(
        //    feature = "unsafe-optimizations",
        //    all(feature = "alloc", feature = "unstable-refs"),
        //))]
        //assert_impl_all!(&mut UnsafeCell<NonZeroU8>: Encode);
        //#[cfg(any(
        //    feature = "unsafe-optimizations",
        //    all(feature = "alloc", feature = "unstable-refs"),
        //))]
        //assert_impl_all!(&mut [UnsafeCell<NonZeroU8>]: Encode);
        //#[cfg(not(any(
        //    feature = "unsafe-optimizations",
        //    all(feature = "alloc", feature = "unstable-refs"),
        //)))]
        //assert_not_impl_any!(&mut UnsafeCell<NonZeroU8>: Encode);
        //#[cfg(not(any(
        //    feature = "unsafe-optimizations",
        //    all(feature = "alloc", feature = "unstable-refs"),
        //)))]
        //assert_not_impl_any!(&mut [UnsafeCell<NonZeroU8>]: Encode);
    }
}
