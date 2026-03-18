#[cfg(feature = "alloc")]
use alloc_crate::{
    string::{String, ToString},
    vec::Vec,
};
use core::{cell::UnsafeCell, ptr::NonNull};

#[cfg(feature = "alloc")]
use crate::vec::CVec;
use crate::{
    borrow::{Borrow, DropFamily, NeedsDrop, NoDrop, ToOwned},
    ir::{ReprFamily, SizeFamily, Sized_, Transmuted},
    niche::{Niche, NicheFamily, StableNiche, WithCustomNiche, WithStableNiche, WithoutNiche},
    reprC,
    transmute::{CheckedTransmute, EncodeTransmuted},
};

// FIXME: Replace with NonZero<T>
macro_rules! non_zero_derive {
    ($($ty:ty => $target:ty),+ $(,)?) => {$(
        reprC! {
            unsafe impl NoDropSizedTransmuted for $ty {
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

impl<T: ?Sized> DropFamily for core::mem::ManuallyDrop<T> {
    type Kind = NoDrop;
}

impl<T: ?Sized> DropFamily for core::cell::Cell<T>
where
    Self: CheckedTransmute<Target: DropFamily>,
{
    type Kind = <<Self as CheckedTransmute>::Target as DropFamily>::Kind;
}

reprC! {
    unsafe impl(T: ?Sized) Transmuted for core::mem::ManuallyDrop<T> {
        type Target = T;
    }
}
reprC! {
    unsafe impl(T: ?Sized) Transmuted for core::cell::Cell<T> {
        type Target = UnsafeCell<T>;
    }
}

impl SizeFamily for str {
    type Kind = <[u8] as SizeFamily>::Kind;
}

impl DropFamily for str {
    type Kind = <[u8] as DropFamily>::Kind;
}

impl NicheFamily for str {
    type Kind = <[u8] as NicheFamily>::Kind;
}

impl ReprFamily for str {
    type Kind = Transmuted;
}

unsafe impl CheckedTransmute for str {
    type Target = [u8];

    #[inline(always)]
    fn is_valid(target: &Self::Target) -> bool {
        core::str::from_utf8(target).is_ok()
    }
}

impl<T: ?Sized + SizeFamily> SizeFamily for UnsafeCell<T> {
    type Kind = T::Kind;
}
impl<T> SizeFamily for NonNull<T> {
    type Kind = Sized_;
}
#[cfg(feature = "alloc")]
impl SizeFamily for String {
    type Kind = Sized_;
}

impl<T: ?Sized> ReprFamily for UnsafeCell<T> {
    type Kind = Transmuted;
}
impl<T> ReprFamily for NonNull<T> {
    type Kind = Transmuted;
}
#[cfg(feature = "alloc")]
impl ReprFamily for String {
    type Kind = Transmuted;
}

impl<T: ?Sized + DropFamily> DropFamily for UnsafeCell<T> {
    type Kind = T::Kind;
}
impl<T> DropFamily for NonNull<T> {
    type Kind = NoDrop;
}
#[cfg(feature = "alloc")]
impl DropFamily for String {
    type Kind = NeedsDrop;
}

impl<T: ?Sized> NicheFamily for UnsafeCell<T> {
    type Kind = WithoutNiche;
}
impl<T> NicheFamily for NonNull<T> {
    type Kind = WithStableNiche;
}
#[cfg(feature = "alloc")]
impl NicheFamily for String {
    type Kind = WithCustomNiche;
}

unsafe impl<T: ?Sized> CheckedTransmute for UnsafeCell<T> {
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

#[cfg(feature = "alloc")]
unsafe impl CheckedTransmute for String {
    // FIXME: String is not guaranteed to be transmutable into Vec<u8>
    // Some trait is required that has as_ptr, len, cap methods which
    // is what `Vec<Transmuted>` types depend on
    type Target = Vec<u8>;

    #[inline(always)]
    fn is_valid(target: &Self::Target) -> bool {
        core::str::from_utf8(target).is_ok()
    }
}

impl<T> Niche for NonNull<T> {
    const NICHE_VALUE: Self::CType = core::ptr::null_mut();
}
#[cfg(feature = "alloc")]
impl Niche for String {
    const NICHE_VALUE: Self::CType = CVec::none();
}

impl<T> Borrow for NonNull<T> {
    type Borrowed<'itm>
        = Self
    where
        Self: 'itm;

    type Store = ();

    fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        self
    }
}
#[cfg(feature = "alloc")]
impl Borrow for String {
    type Borrowed<'itm>
        = &'itm str
    where
        Self: 'itm;

    type Store = Self;

    fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        *store = self;
        store
    }
}

impl<'r, T: 'r> ToOwned<'r> for NonNull<T> {
    #[inline(always)]
    fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
        borrowed
    }
}
#[cfg(feature = "alloc")]
impl<'r> ToOwned<'r> for String {
    #[inline(always)]
    fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
        borrowed.to_string()
    }
}

unsafe impl<R> EncodeTransmuted for UnsafeCell<R>
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
#[cfg(feature = "alloc")]
unsafe impl EncodeTransmuted for String {
    type Store = <Self::Target as crate::Encode>::Store;
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc_crate::boxed::Box;
    use core::num::NonZeroU8;

    use static_assertions::assert_impl_all;

    use super::*;

    #[cfg(feature = "alloc")]
    use crate::boxed::CBoxedSlice;
    use crate::{
        Decode, Encode, ExternC,
        option::COption,
        slice::{CSlice, CSliceMut},
        transmute::FlatTransmute,
    };

    #[test]
    fn str_is_supported() {
        assert_impl_all!(str:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
        );

        assert_impl_all!(&str:
            ReprFamily<Kind = &'static Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            //Niche<CType = CSlice<u8>>,
            //Decode<'static>,
            // FIXME:
            //Encode,
        );
        // TODO: Add more assertions
    }

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
            ReprFamily<Kind = &'static Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [UnsafeCell<NonZeroU8>]:
            ReprFamily<Kind = &'static mut Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<u8>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[UnsafeCell<NonZeroU8>]>:
            ReprFamily<Kind = Box<Transmuted>>,
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
