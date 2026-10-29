#[cfg(feature = "alloc")]
use crate::stored::Owned;
use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, EncodeUnchecked, ReprC,
    borrow::{Borrow, BorrowCast, BorrowCastMut, Borrowed},
    niche::Niche,
    stored::{DecodeOwned, EmptyStore, EncodeMove, EncodeMoveUnchecked},
    transmute::AbiCompatible,
    utf8::Utf8Str,
};
#[cfg(feature = "alloc")]
use alloc::string::String;
use core::{
    cell::{Cell, UnsafeCell},
    marker::PhantomData,
    mem::{ManuallyDrop, MaybeUninit},
    num::NonZero,
    ptr::NonNull,
};
use rust_spec::RustSpec;

macro_rules! non_zero_derive {
    ($($primitive:ty),+ $(,)?) => {$(
        crate::mutable::impl_copy_mut!([] NonZero<$primitive>);
        unsafe impl Borrow for NonZero<$primitive> {
            type Borrowed = Self;

            #[inline(always)]
            fn borrow(self) -> Self::Borrowed
            {
                self
            }

            #[inline(always)]
            fn from_borrow(source: Self::Borrowed) -> Self {
                source
            }
        }


        impl ReprC for NonZero<$primitive> {
            type CType = $primitive;
        }
        unsafe impl EncodeMove for NonZero<$primitive> {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm
            {
                unsafe { self.soft_encode_unchecked(&mut ()) }
            }
        }
        unsafe impl EncodeMoveUnchecked for NonZero<$primitive> {
            type SoftStore = ();

            unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
                self.get()
            }
        }
        unsafe impl<'d> DecodeOwned<'d> for NonZero<$primitive> {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Self::new(source)
            }

            #[inline(always)]
            unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
                unsafe { Self::new_unchecked(source) }
            }
        }

        impl Decode<'_> for NonZero<$primitive> {}
        impl EncodeUnchecked for NonZero<$primitive> {}
        impl Encode for NonZero<$primitive> {}

        unsafe impl AbiCompatible for NonZero<$primitive> {
            #[inline(always)]
            unsafe fn is_valid(target: &Self::CType) -> bool {
                *target != 0
            }
        }

        impl Niche for NonZero<$primitive> {
            const NICHE: Self::CType = 0;
        }
        )+
    }
}

non_zero_derive! {
    u8, i8, u16, i16, u32, i32, u64, i64, u128, i128,
}
unsafe impl Borrow for () {
    type Borrowed = Self;

    #[inline(always)]
    fn borrow(self) -> Self::Borrowed {}

    #[inline(always)]
    fn from_borrow(_: ()) -> Self {}
}

impl ReprC for () {
    type CType = Self;
}
unsafe impl EncodeMove for () {
    type Store = ();

    #[inline(always)]
    fn soft_encode<'itm>(self, (): &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe { self.soft_encode_unchecked(&mut ()) }
    }
}

unsafe impl EncodeMoveUnchecked for () {
    type SoftStore = ();

    unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
        self
    }
}
unsafe impl<'d> DecodeOwned<'d> for () {
    type Store = ();

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        Some(source)
    }
}

impl Decode<'_> for () {}
impl EncodeUnchecked for () {}
impl Encode for () {}

unsafe impl AbiCompatible for () {
    #[inline(always)]
    unsafe fn is_valid(_: &Self::CType) -> bool {
        true
    }
}

unsafe impl CType for () {}
unsafe impl CFnReturn for () {}
unsafe impl BorrowCast for () {
    type AsConst = Self;
}
unsafe impl BorrowCastMut for () {
    type AsMut = Self;
}

unsafe impl EmptyStore for () {}
unsafe impl<T: ?Sized> Borrow for PhantomData<T> {
    type Borrowed = Self;

    #[inline(always)]
    fn borrow(self) -> Self::Borrowed {
        self
    }

    #[inline(always)]
    fn from_borrow(source: Self::Borrowed) -> Self {
        source
    }
}

impl<T: ?Sized> ReprC for PhantomData<T> {
    type CType = Self;
}
unsafe impl<T: ?Sized> AbiCompatible for PhantomData<T> {
    #[inline(always)]
    unsafe fn is_valid(_: &Self::CType) -> bool {
        true
    }
}
unsafe impl<T: ?Sized> EncodeMove for PhantomData<T> {
    type Store = ();

    #[inline(always)]
    fn soft_encode<'itm>(self, (): &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe { self.soft_encode_unchecked(&mut ()) }
    }
}
unsafe impl<T: ?Sized> EncodeMoveUnchecked for PhantomData<T> {
    type SoftStore = ();

    unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
        self
    }
}
unsafe impl<'d, T: ?Sized> DecodeOwned<'d> for PhantomData<T> {
    type Store = ();

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        Some(source)
    }
}

impl<T: ?Sized> Decode<'_> for PhantomData<T> {}
impl<T: ?Sized> EncodeUnchecked for PhantomData<T> {}
impl<T: ?Sized> Encode for PhantomData<T> {}

unsafe impl<T: ?Sized> CType for PhantomData<T> {}
unsafe impl<T: ?Sized> BorrowCast for PhantomData<T> {
    type AsConst = Self;
}
unsafe impl<T: ?Sized> BorrowCastMut for PhantomData<T> {
    type AsMut = Self;
}
unsafe impl<T: ?Sized> Borrow for NonNull<T> {
    type Borrowed = Self;

    #[inline(always)]
    fn borrow(self) -> Self::Borrowed {
        self
    }

    #[inline(always)]
    fn from_borrow(source: Self::Borrowed) -> Self {
        source
    }
}

impl<T: CType + ?Sized> ReprC for NonNull<T> {
    // TODO: afaik the pointer is not necessarily mutable just non-null
    type CType = <*mut T as ReprC>::CType;
}
unsafe impl<T: CType + ?Sized> EncodeMove for NonNull<T> {
    type Store = <*mut T as EncodeMove>::Store;

    #[inline(always)]
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe { self.soft_encode_unchecked(store) }
    }
}
unsafe impl<T: CType + ?Sized> EncodeMoveUnchecked for NonNull<T> {
    type SoftStore = ();

    unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
        unsafe { self.as_ptr().soft_encode_unchecked(&mut ()) }
    }
}
unsafe impl<'d, T: CType + ?Sized> DecodeOwned<'d> for NonNull<T> {
    type Store = <*mut T as DecodeOwned<'d>>::Store;

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        let ptr = unsafe { DecodeOwned::soft_decode(source, store)? };
        NonNull::new(ptr)
    }

    #[inline(always)]
    unsafe fn soft_decode_unchecked<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Self {
        let ptr = unsafe { <*mut T as DecodeOwned>::soft_decode_unchecked(source, store) };
        unsafe { NonNull::new_unchecked(ptr) }
    }
}

impl<T: CType + ?Sized> Decode<'_> for NonNull<T> {}
impl<T: CType + ?Sized> EncodeUnchecked for NonNull<T> {}
impl<T: CType + ?Sized> Encode for NonNull<T> {}

unsafe impl<T: CType + ?Sized> AbiCompatible for NonNull<T>
where
    T: RustSpec<Size: crate::Thin>,
    Self: ReprC<CType = *mut T>,
{
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        !target.is_null()
    }
}
#[cfg(feature = "alloc")]
impl Owned for str {
    type Owned = String;
}
impl ReprC for str {
    type CType = Utf8Str;
}
#[cfg(feature = "alloc")]
unsafe impl Borrow for String {
    type Borrowed = Borrowed<Self>;

    #[inline(always)]
    fn borrow(self) -> Self::Borrowed {
        Borrowed::new(self)
    }

    #[inline(always)]
    fn from_borrow(source: Self::Borrowed) -> Self {
        Borrowed::into_inner(source)
    }
}

unsafe impl<T: Borrow> Borrow for UnsafeCell<T> {
    type Borrowed = UnsafeCell<T::Borrowed>;

    #[inline(always)]
    fn borrow(self) -> Self::Borrowed {
        UnsafeCell::new(self.into_inner().borrow())
    }

    #[inline(always)]
    fn from_borrow(source: Self::Borrowed) -> Self {
        UnsafeCell::new(T::from_borrow(source.into_inner()))
    }
}

unsafe impl<R: BorrowCast + ?Sized> BorrowCast for UnsafeCell<R> {
    type AsConst = UnsafeCell<R::AsConst>;
}
unsafe impl<R: BorrowCastMut + ?Sized> BorrowCastMut for UnsafeCell<R> {
    type AsMut = UnsafeCell<R::AsMut>;
}

impl<R: ReprC + ?Sized> ReprC for UnsafeCell<R> {
    type CType = UnsafeCell<R::CType>;
}

impl<R: Encode> Encode for UnsafeCell<R> where Self: EncodeMove {}
impl<R: EncodeUnchecked> EncodeUnchecked for UnsafeCell<R> where Self: EncodeMoveUnchecked {}
impl<'d, R: Decode<'d>> Decode<'d> for UnsafeCell<R> where Self: DecodeOwned<'d> {}

unsafe impl<T: EmptyStore> EmptyStore for UnsafeCell<T> {}

unsafe impl<R: CType + ?Sized> CType for UnsafeCell<R> {}
unsafe impl<T: CFnArg> CFnArg for UnsafeCell<T> where
    Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}
unsafe impl<T: CFnReturn> CFnReturn for UnsafeCell<T> where
    Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}

unsafe impl<T: Borrow> Borrow for Cell<T> {
    type Borrowed = UnsafeCell<T::Borrowed>;

    #[inline(always)]
    fn borrow(self) -> Self::Borrowed {
        UnsafeCell::new(Cell::into_inner(self)).borrow()
    }

    #[inline(always)]
    fn from_borrow(source: Self::Borrowed) -> Self {
        Cell::new(T::from_borrow(source.into_inner()))
    }
}

impl<R: ReprC + ?Sized> ReprC for Cell<R> {
    type CType = <UnsafeCell<R> as ReprC>::CType;
}

impl<R: Encode> Encode for Cell<R> where Self: EncodeMove {}
impl<R: EncodeUnchecked> EncodeUnchecked for Cell<R> where Self: EncodeMoveUnchecked {}
impl<'d, R: Decode<'d>> Decode<'d> for Cell<R> where Self: DecodeOwned<'d> {}

unsafe impl<T: EmptyStore> EmptyStore for Cell<T> {}

unsafe impl<R: CType + ?Sized> CType for Cell<R> {}
unsafe impl<T: CFnArg> CFnArg for Cell<T> where
    Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}
unsafe impl<T: CFnReturn> CFnReturn for Cell<T> where
    Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}
unsafe impl<T: BorrowCast + ?Sized> BorrowCast for Cell<T> {
    type AsConst = UnsafeCell<T::AsConst>;
}
unsafe impl<T: BorrowCastMut + ?Sized> BorrowCastMut for Cell<T> {
    type AsMut = UnsafeCell<T::AsMut>;
}

unsafe impl<T> Borrow for MaybeUninit<T> {
    type Borrowed = Self;

    #[inline(always)]
    fn borrow(self) -> Self::Borrowed {
        self
    }

    #[inline(always)]
    fn from_borrow(source: Self::Borrowed) -> Self {
        source
    }
}

impl<T: AbiCompatible<CType: Sized>> ReprC for MaybeUninit<T> {
    type CType = MaybeUninit<T::CType>;
}
unsafe impl<T: AbiCompatible<CType: Sized> + EncodeMove<Store: EmptyStore>> EncodeMove
    for MaybeUninit<T>
{
    type Store = ();

    #[inline(always)]
    fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe { self.soft_encode_unchecked(&mut ()) }
    }
}
unsafe impl<T: AbiCompatible<CType: Sized>> EncodeMoveUnchecked for MaybeUninit<T> {
    type SoftStore = ();

    unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
        unsafe { core::mem::transmute_copy(&self) }
    }
}
unsafe impl<'d, T: AbiCompatible<CType: Sized>> DecodeOwned<'d> for MaybeUninit<T> {
    type Store = ();

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        Some(unsafe { core::mem::transmute_copy(&source) })
    }
}

// TODO: Weird bounds here
impl<'d, T: AbiCompatible<CType: Sized>> Decode<'d> for MaybeUninit<T> {}
impl<T: AbiCompatible<CType: Sized>> EncodeUnchecked for MaybeUninit<T> {}
impl<T: AbiCompatible<CType: Sized> + EncodeMove<Store: EmptyStore>> Encode for MaybeUninit<T> {}

// TODO: Technically all bits are valid, but using T: CType feels safer
unsafe impl<T: AbiCompatible<CType: Sized>> AbiCompatible for MaybeUninit<T> {
    #[inline(always)]
    unsafe fn is_valid(_: &Self::CType) -> bool {
        true
    }
}

unsafe impl<T: CType> CType for MaybeUninit<T> {}
unsafe impl<T: CFnArg> CFnArg for MaybeUninit<T> where
    Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}
unsafe impl<T: CFnReturn> CFnReturn for MaybeUninit<T> where
    Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}

unsafe impl<T: BorrowCast<AsConst: Sized>> BorrowCast for MaybeUninit<T> {
    type AsConst = MaybeUninit<T::AsConst>;
}
unsafe impl<T: BorrowCastMut<AsMut: Sized>> BorrowCastMut for MaybeUninit<T> {
    type AsMut = MaybeUninit<T::AsMut>;
}

unsafe impl<T: EmptyStore> EmptyStore for MaybeUninit<T> {}

unsafe impl<T> Borrow for ManuallyDrop<T> {
    type Borrowed = Self;

    #[inline(always)]
    fn borrow(self) -> Self::Borrowed {
        self
    }

    #[inline(always)]
    fn from_borrow(source: Self::Borrowed) -> Self {
        source
    }
}

impl<T: Encode> Encode for ManuallyDrop<T> where Self: EncodeMove {}
impl<T: EncodeUnchecked> EncodeUnchecked for ManuallyDrop<T> where Self: EncodeMoveUnchecked {}
impl<'d, T: Decode<'d>> Decode<'d> for ManuallyDrop<T> where Self: DecodeOwned<'d> {}

unsafe impl<T: CType + ?Sized> CType for ManuallyDrop<T> {}
unsafe impl<T: CFnArg> CFnArg for ManuallyDrop<T> where
    Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}
unsafe impl<T: CFnReturn> CFnReturn for ManuallyDrop<T> where
    Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}
unsafe impl<T: BorrowCast + ?Sized> BorrowCast for ManuallyDrop<T> {
    type AsConst = T::AsConst;
}
unsafe impl<T: BorrowCastMut + ?Sized> BorrowCastMut for ManuallyDrop<T> {
    type AsMut = T::AsMut;
}

unsafe impl<T: EmptyStore> EmptyStore for ManuallyDrop<T> {}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc::{boxed::Box, vec, vec::Vec};

    #[cfg(feature = "alloc")]
    use static_assertions::assert_type_eq_all;
    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    #[cfg(feature = "alloc")]
    use crate::boxed::{CBox, CBoxCell, CWideBox, CWideBoxCell};
    use crate::{
        CFnArg, CFnReturn, Decode, Encode,
        option::COption,
        reference::{CRef, CRefMut},
        slice::{CWideRef, CWideRefMut},
    };

    #[test]
    #[cfg(feature = "alloc")]
    fn manually_drop_stable_and_unstable_conversions() {
        assert_type_eq_all!(<ManuallyDrop<bool> as EncodeMove>::Store, ());
        assert_type_eq_all!(<ManuallyDrop<Box<u8>> as EncodeMove>::Store, ());
        assert_type_eq_all!(<ManuallyDrop<Box<u8>> as DecodeOwned<'static>>::Store, ());
        assert_type_eq_all!(
            <ManuallyDrop<Vec<(u8,)>> as EncodeMove>::Store,
            <Vec<(u8,)> as EncodeMove>::Store
        );
        let mut encode_store = Default::default();
        let encoded = ManuallyDrop::new(vec![(3_u8,), (5_u8,)]).soft_encode(&mut encode_store);
        let mut decode_store = Default::default();
        let decoded = unsafe {
            <ManuallyDrop<Vec<(u8,)>> as DecodeOwned>::soft_decode(encoded, &mut decode_store)
        }
        .unwrap();
        assert_eq!(ManuallyDrop::into_inner(decoded), vec![(3,), (5,)]);
    }

    #[test]
    fn manually_drop_decode_rejects_invalid_inner_representation() {
        assert_impl_all!(ManuallyDrop<bool>: Decode<'static>, Encode);

        let invalid = bool::NICHE;
        assert!(unsafe { crate::decode::<ManuallyDrop<bool>>(invalid) }.is_none());

        let encoded = crate::encode(ManuallyDrop::new(true));
        let decoded = unsafe { crate::decode::<ManuallyDrop<bool>>(encoded) }.unwrap();
        assert!(*decoded);
    }

    #[test]
    fn maybe_uninit_lowers_without_validating_or_encoding_the_inner_value() {
        assert_impl_all!(MaybeUninit<bool>:
            ReprC<CType = MaybeUninit<crate::primitives::CBool>>,
            AbiCompatible,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(MaybeUninit<u8>: CType, CFnArg, CFnReturn);

        let source = MaybeUninit::new(bool::NICHE);
        assert!(unsafe { MaybeUninit::<bool>::is_valid(&source) });

        let decoded = unsafe { crate::decode::<MaybeUninit<bool>>(source) }.unwrap();
        let encoded = crate::encode(decoded);

        assert_eq!(unsafe { encoded.assume_init() }, bool::NICHE);
    }

    #[test]
    fn manually_drop_inner_without_drop() {
        assert_impl_all!(ManuallyDrop<u8>:
            ReprC<CType = u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&ManuallyDrop<u8>:
            Niche<CType = CRef<u8>>,
            RustSpec<Niche = rust_spec::niche::WithNiche<rust_spec::Stable>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut ManuallyDrop<u8>:
            Niche<CType = CRefMut<u8, true>>,
            RustSpec<Niche = rust_spec::niche::WithNiche<rust_spec::Stable>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<ManuallyDrop<u8>>:
            Niche<CType = CBox<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[ManuallyDrop<u8>]:
            Niche<CType = CWideRef<[u8]>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [ManuallyDrop<u8>]:
            Niche<CType = CWideRefMut<[u8], true>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[ManuallyDrop<u8>]>:
            Niche<CType = CWideBox<[u8]>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<ManuallyDrop<u8>>:
            Niche<CType = crate::vec::CVec<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([ManuallyDrop<u8>; 2]:
            ReprC<CType = [u8; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<ManuallyDrop<u8>>:
            Niche<CType = COption<u8>>,
            Decode<'static>,
            Encode,
        );

        assert_impl_all!(ManuallyDrop<u8>: CType);
    }

    #[test]
    #[cfg(feature = "alloc")]
    fn manually_drop_inner_with_drop() {
        assert_type_eq_all!(<ManuallyDrop<CBox<u32>> as BorrowCast>::AsConst, CRef<u32>);
        assert_type_eq_all!(<ManuallyDrop<CBox<u32>> as BorrowCastMut>::AsMut, CRefMut<u32, true>);
        assert_type_eq_all!(<ManuallyDrop<CRef<u32>> as BorrowCast>::AsConst, CRef<u32>);
        assert_impl_all!(ManuallyDrop<String>:
            ReprC<CType = ManuallyDrop<CWideBox<Utf8Str>>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(ManuallyDrop<Box<u8>>:
            Niche<CType = ManuallyDrop<CBox<u8>>>,
            ReprC<CType = ManuallyDrop<CBox<u8>>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&ManuallyDrop<Box<u8>>:
            Niche<CType = CRef<ManuallyDrop<CBox<u8>>>>,
        );
        assert_impl_all!(&mut ManuallyDrop<Box<u8>>:
            Niche<CType = CRefMut<ManuallyDrop<CBox<u8>>, true>>,
        );
        assert_impl_all!(Box<ManuallyDrop<Box<u8>>>:
            Niche<CType = CBox<ManuallyDrop<CBox<u8>>>>,
            RustSpec<Niche = rust_spec::niche::WithNiche<rust_spec::Stable>>,
        );
        assert_impl_all!(&[ManuallyDrop<Box<u8>>]:
            Niche<CType = CWideRef<[ManuallyDrop<CBox<u8>>]>>,
        );
        assert_impl_all!(&mut [ManuallyDrop<Box<u8>>]:
            Niche<CType = CWideRefMut<[ManuallyDrop<CBox<u8>>], true>>,
        );
        assert_impl_all!(Box<[ManuallyDrop<Box<u8>>]>:
            Niche<CType = CWideBox<[ManuallyDrop<CBox<u8>>]>>,
        );
        assert_impl_all!(Vec<ManuallyDrop<Box<u8>>>:
            Niche<CType = crate::vec::CVec<ManuallyDrop<CBox<u8>>>>,
        );
        assert_impl_all!([ManuallyDrop<Box<u8>>; 2]:
            Niche<CType = [ManuallyDrop<CBox<u8>>; 2]>,
        );
        assert_impl_all!(Option<ManuallyDrop<Box<u8>>>:
            ReprC<CType = ManuallyDrop<CBox<u8>>>,
        );
        assert_not_impl_any!(ManuallyDrop<Box<u8>>: CType);

        let encoded = crate::encode(ManuallyDrop::new(Box::new(42u8)));
        let decoded = unsafe { crate::decode::<ManuallyDrop<Box<u8>>>(encoded) }.unwrap();
        let decoded = ManuallyDrop::into_inner(decoded);
        assert_eq!(*decoded, 42);

        let encoded = crate::encode(ManuallyDrop::new(String::from("hello")));
        let decoded = unsafe { crate::decode::<ManuallyDrop<String>>(encoded) }.unwrap();
        assert_eq!(ManuallyDrop::into_inner(decoded), "hello");
    }

    #[test]
    fn str_is_supported() {
        assert_impl_all!(str: ReprC<CType = Utf8Str>);
        #[cfg(feature = "alloc")]
        assert_impl_all!(String: ReprC<CType = CWideBox<Utf8Str>>);

        assert_impl_all!(&str:
            Niche<CType = CWideRef<Utf8Str>>,
            Decode<'static>,
            Encode,
        );

        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut str:
            Niche<CType = CWideRefMut<Utf8Str, true>>,
            Decode<'static>,
            Encode,
        );

        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<str>:
            Niche<CType = CWideBox<Utf8Str>>,
            Decode<'static>,
            Encode,
        );
    }

    #[test]
    fn str_decode_rejects_invalid_utf8() {
        let invalid = [0xff];

        let source = unsafe { CWideRef::from_raw_parts(invalid.as_ptr(), invalid.len()) };
        let decoded = unsafe { crate::decode::<&str>(source) };
        assert!(decoded.is_none());

        let mut invalid = [0xff];
        let source = unsafe {
            CWideRefMut::<_, true>::from_raw_parts_mut(invalid.as_mut_ptr(), invalid.len())
        };
        let decoded = unsafe { crate::decode::<&mut str>(source) };
        assert!(decoded.is_none());

        #[cfg(feature = "alloc")]
        {
            let allocation = Box::into_non_null(Box::new([0xff_u8])).cast();
            let source = unsafe { CWideBox::<Utf8Str>::from_raw_parts(allocation, 1) };
            let decoded = unsafe { crate::decode::<Box<str>>(source) };
            assert!(decoded.is_none());
        }
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn owned_strings_round_trip_without_recursive_conversion() {
        let text = String::from("hello 🍃");
        let encoded = crate::encode(text.clone());
        assert_eq!(
            unsafe { crate::decode::<String>(encoded) },
            Some(text.clone())
        );

        let encoded = crate::encode(text.clone().into_boxed_str());
        assert_eq!(
            unsafe { crate::decode::<Box<str>>(encoded) }.as_deref(),
            Some(text.as_str())
        );
    }

    #[test]
    fn robust_unsafe_cell() {
        assert_impl_all!(UnsafeCell<u8>: AbiCompatible);
        assert_impl_all!(Cell<u8>: AbiCompatible);
        assert_impl_all!([UnsafeCell<u8>; 2]: AbiCompatible);
        assert_not_impl_any!((UnsafeCell<u8>, u8): AbiCompatible);
        assert_impl_all!(ManuallyDrop<UnsafeCell<u8>>: AbiCompatible);

        assert_impl_all!(UnsafeCell<u8>:
            ReprC<CType = UnsafeCell<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&UnsafeCell<u8>:
            Niche<CType = CRefMut<UnsafeCell<u8>, false>>,
            Encode,
            AbiCompatible,
        );
        assert_impl_all!(&UnsafeCell<u8>: Decode<'static>);
        assert_impl_all!(&mut UnsafeCell<u8>:
            Niche<CType = CRefMut<UnsafeCell<u8>, true>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<UnsafeCell<u8>>:
            Niche<CType = CBoxCell<UnsafeCell<u8>>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<UnsafeCell<u8>>: AbiCompatible);
        assert_impl_all!(&[UnsafeCell<u8>]:
            Niche<CType = CWideRefMut<[UnsafeCell<u8>], false>>,
            Encode,
        );
        assert_impl_all!(&[UnsafeCell<u8>]: Decode<'static>);
        assert_impl_all!(&mut [UnsafeCell<u8>]:
            Niche<CType = CWideRefMut<[UnsafeCell<u8>], true>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[UnsafeCell<u8>]>:
            Niche<CType = CWideBoxCell<[UnsafeCell<u8>]>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<UnsafeCell<u8>>:
            Niche<CType = crate::vec::CVec<UnsafeCell<u8>>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([UnsafeCell<u8>; 2]:
            ReprC<CType = [UnsafeCell<u8>; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<UnsafeCell<u8>>:
            ReprC<CType = COption<UnsafeCell<u8>>>,
            Decode<'static>,
            Encode,
        );

        assert_impl_all!(UnsafeCell<u8>: CType);
    }

    #[test]
    fn mutable_unsafe_cell_reference_round_trip() {
        let mut cell = UnsafeCell::new(7_u8);
        let encoded = crate::encode(&mut cell);
        unsafe { encoded.as_ptr().write(UnsafeCell::new(9)) };
        assert_eq!(*cell.get_mut(), 9);

        let source = unsafe { CRefMut::from_raw(cell.get().cast::<UnsafeCell<u8>>()) };
        let decoded = unsafe { crate::decode::<&mut UnsafeCell<u8>>(source) }.unwrap();
        *decoded.get_mut() = 11;
        assert_eq!(*cell.get_mut(), 11);

        let value = UnsafeCell::new(NonZero::new(3_u8).unwrap());
        let source = unsafe { CRefMut::from_raw(value.get().cast::<UnsafeCell<u8>>()) };
        let decoded = unsafe { crate::decode::<&mut UnsafeCell<NonZero<u8>>>(source) }.unwrap();
        assert_eq!(decoded.get_mut().get(), 3);

        let invalid = UnsafeCell::new(0_u8);
        let source = unsafe { CRefMut::from_raw(invalid.get().cast::<UnsafeCell<u8>>()) };
        assert!(unsafe { crate::decode::<&mut UnsafeCell<NonZero<u8>>>(source) }.is_none());
    }

    #[test]
    fn exclusive_slice_pointers_cover_cells_and_empty_slices() {
        let mut cells = [UnsafeCell::new(1_u8)];
        let view = crate::encode(&mut cells[..]);
        unsafe { view.data().write(UnsafeCell::new(2)) };
        assert_eq!(unsafe { *cells[0].get() }, 2);

        let mut cells = [Cell::new(3_u8)];
        let view = crate::encode(&mut cells[..]);
        unsafe { view.data().write(UnsafeCell::new(4)) };
        assert_eq!(cells[0].get(), 4);

        let mut empty_unsafe_cells: [UnsafeCell<u8>; 0] = [];
        assert_eq!(crate::encode(&mut empty_unsafe_cells[..]).metadata(), 0);
        let mut empty_cells: [Cell<u8>; 0] = [];
        assert_eq!(crate::encode(&mut empty_cells[..]).metadata(), 0);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn interior_mutable_slice_carriers_round_trip() {
        assert_type_eq_all!(
            <CWideBoxCell<[UnsafeCell<u8>]> as BorrowCast>::AsConst,
            <&[UnsafeCell<u8>] as ReprC>::CType
        );

        let mut cells = [UnsafeCell::new(3_u8), UnsafeCell::new(4_u8)];
        let view = crate::encode(&mut cells[..]);
        unsafe { view.data().write(UnsafeCell::new(9)) };
        assert_eq!(unsafe { *cells[0].get() }, 9);

        let boxed = crate::encode(vec![UnsafeCell::new(5_u8)].into_boxed_slice());
        unsafe {
            crate::borrow::borrow_cast(boxed)
                .data()
                .write(UnsafeCell::new(7))
        };
        let boxed = unsafe { crate::decode::<Box<[UnsafeCell<u8>]>>(boxed) }.unwrap();
        assert_eq!(unsafe { *boxed[0].get() }, 7);

        let vector = crate::encode(vec![UnsafeCell::new(6_u8)]);
        let vector = unsafe { crate::decode::<Vec<UnsafeCell<u8>>>(vector) }.unwrap();
        assert_eq!(unsafe { *vector[0].get() }, 6);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn boxed_cell_borrow_and_round_trip() {
        assert_type_eq_all!(
            <CBoxCell<UnsafeCell<u8>> as BorrowCast>::AsConst,
            <&UnsafeCell<u8> as ReprC>::CType,
        );
        assert_type_eq_all!(
            <CBoxCell<UnsafeCell<u8>> as BorrowCast>::AsConst,
            <&Cell<u8> as ReprC>::CType
        );

        let encoded = crate::encode(Box::new(UnsafeCell::new(4u8)));
        let borrowed = crate::borrow::borrow_cast(encoded);
        unsafe { borrowed.as_ptr().write(UnsafeCell::new(8)) };
        let decoded = unsafe { crate::decode::<Box<UnsafeCell<u8>>>(encoded) }.unwrap();
        assert_eq!((*decoded).into_inner(), 8);

        let encoded = crate::encode(Box::new(Cell::new(4u8)));
        unsafe {
            crate::borrow::borrow_cast(encoded)
                .as_ptr()
                .write(UnsafeCell::new(6))
        };
        let decoded = unsafe { crate::decode::<Box<Cell<u8>>>(encoded) }.unwrap();
        assert_eq!(decoded.get(), 6);

        let none = crate::encode(None::<Box<UnsafeCell<u8>>>);
        assert!(none.as_ptr().is_null());
        assert!(matches!(
            unsafe { crate::decode::<Option<Box<UnsafeCell<u8>>>>(none) },
            Some(None)
        ));
    }

    #[test]
    fn non_robust_mutable_cell_syncs_after_soft_encode() {
        use crate::stored::Store;

        let mut value = Cell::new(NonZero::new(3_u8).unwrap());
        let mut store = <&mut Cell<NonZero<u8>> as EncodeMove>::Store::default();
        let encoded = <&mut Cell<NonZero<u8>> as EncodeMove>::soft_encode(&mut value, &mut store);
        unsafe { encoded.as_ptr().write(UnsafeCell::new(4)) };
        assert_eq!(store.sync(), Some(()));
        assert_eq!(value.get().get(), 4);
    }

    #[test]
    fn non_robust_unsafe_cell() {
        assert_impl_all!(UnsafeCell<NonZero<u8>>:
            ReprC<CType = UnsafeCell<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&UnsafeCell<NonZero<u8>>:
            Niche<CType = CRefMut<UnsafeCell<u8>, false>>,
        );
        assert_not_impl_any!(&UnsafeCell<NonZero<u8>>: Decode<'static>, AbiCompatible);
        assert_impl_all!(&mut UnsafeCell<NonZero<u8>>:
            Niche<CType = CRefMut<UnsafeCell<u8>, true>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<UnsafeCell<NonZero<u8>>>:
            Niche<CType = CBoxCell<UnsafeCell<u8>>>,
            Decode<'static>,
            Encode,
        );

        #[cfg(feature = "alloc")]
        {
            // SAFETY: this test constructs the carrier for an interior-mutable cell value.
            let valid = CBoxCell::from_box(Box::new(UnsafeCell::new(7u8)));
            let decoded = unsafe { crate::decode::<Box<UnsafeCell<NonZero<u8>>>>(valid) }.unwrap();
            assert_eq!((*decoded).into_inner().get(), 7);

            // SAFETY: this test constructs the carrier for an interior-mutable cell value.
            let invalid = CBoxCell::from_box(Box::new(UnsafeCell::new(0u8)));
            assert!(unsafe { crate::decode::<Box<UnsafeCell<NonZero<u8>>>>(invalid) }.is_none());
        }
        assert_impl_all!(&[UnsafeCell<NonZero<u8>>]:
            Niche<CType = CWideRefMut<[UnsafeCell<u8>], false>>,
        );
        assert_not_impl_any!(&[UnsafeCell<NonZero<u8>>]: Decode<'static>);
        assert_impl_all!(&mut [UnsafeCell<NonZero<u8>>]:
            Niche<CType = CWideRefMut<[UnsafeCell<u8>], true>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[UnsafeCell<NonZero<u8>>]>:
            Niche<CType = CWideBoxCell<[UnsafeCell<u8>]>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<UnsafeCell<NonZero<u8>>>:
            Niche<CType = crate::vec::CVec<UnsafeCell<u8>>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        {
            let invalid = crate::vec::CVec::from_vec(vec![UnsafeCell::new(0_u8)]);
            assert!(unsafe { crate::decode::<Vec<UnsafeCell<NonZero<u8>>>>(invalid) }.is_none());

            let invalid = CWideBoxCell::from_box(vec![UnsafeCell::new(0_u8)].into_boxed_slice());
            assert!(unsafe { crate::decode::<Box<[UnsafeCell<NonZero<u8>>]>>(invalid) }.is_none());
        }
        assert_impl_all!([UnsafeCell<NonZero<u8>>; 2]:
            ReprC<CType = [UnsafeCell<u8>; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<UnsafeCell<NonZero<u8>>>:
            ReprC<CType = COption<UnsafeCell<u8>>>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(UnsafeCell<NonZero<u8>>: CType);
    }
}
