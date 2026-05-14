#[cfg(feature = "alloc")]
use alloc_crate::{string::String, vec::Vec};
use core::{
    cell::{Cell, UnsafeCell},
    ffi::c_void,
    mem::ManuallyDrop,
    ptr::NonNull,
};

#[cfg(feature = "alloc")]
use crate::boxed::CBoxedSlice;
use crate::{
    ReprC,
    borrow::{Borrow, BorrowCast, ToOwned},
    ir::EncodeReprFamily,
    ir::{ReprFamily, Robust, Transmuted},
    niche::{Niche, NicheFamily, StableNiche, WithCustomNiche, WithStableNiche, WithoutNiche},
    reprC,
    size::{ExternTypeLike, MetaSized, SizeFamily, SizedType, SliceLike, Zst},
    transmute::CheckedTransmute,
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

        impl Borrow for $ty {
            type Borrowed<'itm>
                = Self
            where
                Self: 'itm;

            type Owner = ();

            #[inline(always)]
            fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
            where
                Self: 'itm,
            {
                self
            }
        }

        impl<'itm> ToOwned<'itm> for $ty {
            #[inline(always)]
            fn to_owned(source: Self::Borrowed<'itm>) -> Self {
                source
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

unsafe impl ReprC for () {}
impl SizeFamily for () {
    type Kind = crate::size::Sized<Zst>;
}
impl NicheFamily for () {
    type Kind = WithoutNiche;
}
unsafe impl BorrowCast for () {
    type AsConst = ();
    type AsMut = ();
}
// TODO: Verify ZST types
impl ReprFamily for () {
    type Kind = Robust;
}
//reprC! {
//    unsafe impl(T: ?Sized) Transmuted for core::marker::PhantomData<T> {
//        type Target = ();
//    }
//}
reprC! {
    // FIXME: This is super wrong because it makes the type Drop
    unsafe impl(T: ?Sized) Transmuted for ManuallyDrop<T> {
        type Target = T;
    }
}
reprC! {
    unsafe impl(T: ?Sized) Transmuted for Cell<T> {
        type Target = UnsafeCell<T>;
    }
}
impl<T> Borrow for ManuallyDrop<T> {
    type Borrowed<'itm>
        = Self
    where
        Self: 'itm;

    type Owner = ();

    #[inline(always)]
    fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        self
    }
}
impl<'itm, T: 'itm> ToOwned<'itm> for ManuallyDrop<T> {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        source
    }
}
impl<T: Borrow> Borrow for Cell<T> {
    type Borrowed<'itm>
        = T::Borrowed<'itm>
    where
        Self: 'itm;

    type Owner = T::Owner;

    #[inline(always)]
    fn borrow<'itm>(self, store: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        T::borrow(Cell::into_inner(self), store)
    }
}
impl<'itm, T: ToOwned<'itm>> ToOwned<'itm> for Cell<T> {
    #[inline(always)]
    fn to_owned(source: Self::Borrowed<'itm>) -> Self {
        Cell::new(T::to_owned(source))
    }
}

unsafe impl ReprC for c_void {}
impl SizeFamily for c_void {
    // NOTE: Although c_void is a ZST
    // it must appear behind a pointer
    type Kind = ExternTypeLike;
}
impl NicheFamily for c_void {
    type Kind = WithoutNiche;
}
impl ReprFamily for c_void {
    type Kind = Robust;
}

impl ReprFamily for str {
    type Kind = Transmuted;
}
impl crate::ir::EncodeReprFamily for str {
    type Kind = Transmuted;
}

impl SizeFamily for str {
    type Kind = MetaSized<SliceLike>;
}

unsafe impl CheckedTransmute for str {
    type Target = [u8];

    #[inline(always)]
    fn is_valid(target: &Self::Target) -> bool {
        core::str::from_utf8(target).is_ok()
    }
}

impl<T: SizeFamily + ?Sized> SizeFamily for UnsafeCell<T> {
    type Kind = T::Kind;
}
impl<T> SizeFamily for NonNull<T> {
    type Kind = crate::size::Sized<SizedType>;
}
#[cfg(feature = "alloc")]
impl SizeFamily for String {
    type Kind = crate::size::Sized<SizedType>;
}

impl<T: ?Sized> ReprFamily for UnsafeCell<T> {
    type Kind = Transmuted;
}
impl<T: EncodeReprFamily + ?Sized> EncodeReprFamily for UnsafeCell<T> {
    type Kind = <T as EncodeReprFamily>::Kind;
}
impl<T> ReprFamily for NonNull<T> {
    type Kind = Transmuted;
}
impl<T> EncodeReprFamily for NonNull<T> {
    type Kind = Transmuted;
}
#[cfg(feature = "alloc")]
impl ReprFamily for String {
    type Kind = Transmuted;
}
#[cfg(feature = "alloc")]
impl EncodeReprFamily for String {
    type Kind = Transmuted;
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

#[cfg(feature = "alloc")]
impl Niche for String {
    const NICHE_VALUE: Self::CType = CBoxedSlice::none();
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
        ExternC, SoftDecode, SoftEncode,
        option::COption,
        slice::{CSlice, CSliceMut},
        transmute::FlatTransmute,
    };

    #[test]
    fn str_is_supported() {
        assert_impl_all!(str:
            ReprFamily<Kind = Transmuted>,
        );

        assert_impl_all!(&str:
            ReprFamily<Kind = &'static str>,
            NicheFamily<Kind = WithCustomNiche>,
            //Niche<CType = CSlice<u8>>,
            //SoftDecode<'static>,
            // FIXME:
            //SoftEncode,
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
            SoftDecode<'static>,
            // FIXME:
            //SoftEncode,
        );
        assert_impl_all!(&UnsafeCell<NonZeroU8>:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const u8>,
            SoftDecode<'static>,
            SoftEncode,
        );
        assert_impl_all!(&mut UnsafeCell<NonZeroU8>:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut u8>,
            SoftDecode<'static>,
        );
        // FIXME:
        //#[cfg(feature = "alloc")]
        //assert_impl_all!(Box<UnsafeCell<NonZeroU8>>:
        //    ReprFamily<Kind = Transmuted>,
        //    NicheFamily<Kind = WithStableNiche>,
        //    StableNiche<CType = *mut u8>,
        //    SoftDecode<'static>,
        //    SoftEncode,
        //);
        assert_impl_all!(&[UnsafeCell<NonZeroU8>]:
            ReprFamily<Kind = &'static [UnsafeCell<NonZeroU8>]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<u8>>,
            SoftDecode<'static>,
            SoftEncode,
        );
        assert_impl_all!(&mut [UnsafeCell<NonZeroU8>]:
            ReprFamily<Kind = &'static mut [UnsafeCell<NonZeroU8>]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<u8>>,
            SoftDecode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[UnsafeCell<NonZeroU8>]>:
            ReprFamily<Kind = Box<[UnsafeCell<NonZeroU8>]>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            // FIXME:
            //SoftDecode<'static>,
            SoftEncode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<UnsafeCell<NonZeroU8>>:
            ReprFamily<Kind = Vec<UnsafeCell<NonZeroU8>>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            // FIXME:
            //SoftDecode<'static>,
            SoftEncode,
        );
        assert_impl_all!([UnsafeCell<NonZeroU8>; 2]:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = [u8; 2]>,
            SoftDecode<'static>,
            SoftEncode,
        );
        assert_impl_all!(Option<UnsafeCell<NonZeroU8>>:
            ReprFamily<Kind = Option<UnsafeCell<NonZeroU8>>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = COption<u8>>,
            SoftDecode<'static>,
            // FIXME:
            //SoftEncode,
        );

        // FIXME:
        //#[cfg(any(
        //    feature = "alloc",
        //))]
        //assert_impl_all!(&mut UnsafeCell<NonZeroU8>: SoftEncode);
        //#[cfg(any(
        //    feature = "alloc",
        //))]
        //assert_impl_all!(&mut [UnsafeCell<NonZeroU8>]: SoftEncode);
        //#[cfg(not(any(
        //    feature = "alloc",
        //)))]
        //assert_not_impl_any!(&mut UnsafeCell<NonZeroU8>: SoftEncode);
        //#[cfg(not(any(
        //    feature = "alloc",
        //)))]
        //assert_not_impl_any!(&mut [UnsafeCell<NonZeroU8>]: SoftEncode);
    }
}
