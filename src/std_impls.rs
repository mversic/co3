use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC,
    borrow::{Borrow, BorrowCast, BorrowCastMut, FromBorrow},
    niche::Niche,
    stored::{DecodeOwned, EmptyStore, EncodeOwned},
    transmute::CheckedTransmute,
};
#[cfg(feature = "alloc")]
use crate::{boxed::CWideBox, stored::Owned};
#[cfg(feature = "alloc")]
use alloc::{string::String, vec::Vec};
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
        unsafe impl Borrow for NonZero<$primitive> {
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
        impl<'itm> FromBorrow<'itm> for NonZero<$primitive> {
            #[inline(always)]
            fn from_borrow(source: Self::Borrowed<'itm>) -> Self {
                source
            }
        }

        impl ReprC for NonZero<$primitive> {
            type CType = $primitive;
        }
        unsafe impl EncodeOwned for NonZero<$primitive> {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm
            {
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

        impl Encode for NonZero<$primitive> {}
        impl Decode<'_> for NonZero<$primitive> {}

        unsafe impl CheckedTransmute for NonZero<$primitive> {
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
    type Borrowed<'itm>
        = Self
    where
        Self: 'itm;

    type Owner = ();

    #[inline(always)]
    fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm> {}
}
impl<'itm> FromBorrow<'itm> for () {
    #[inline(always)]
    fn from_borrow(_: ()) -> Self {}
}

impl ReprC for () {
    type CType = Self;
}
unsafe impl EncodeOwned for () {
    type Store = ();

    #[inline(always)]
    fn soft_encode<'itm>(self, (): &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
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

impl Encode for () {}
impl Decode<'_> for () {}

unsafe impl CheckedTransmute for () {
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
impl<'itm, T: ?Sized + 'itm> FromBorrow<'itm> for PhantomData<T> {
    #[inline(always)]
    fn from_borrow(source: Self::Borrowed<'itm>) -> Self {
        source
    }
}

impl<T: ?Sized> ReprC for PhantomData<T> {
    type CType = Self;
}
unsafe impl<T: ?Sized> CheckedTransmute for PhantomData<T> {
    #[inline(always)]
    unsafe fn is_valid(_: &Self::CType) -> bool {
        true
    }
}
unsafe impl<T: ?Sized> EncodeOwned for PhantomData<T> {
    type Store = ();

    #[inline(always)]
    fn soft_encode<'itm>(self, (): &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
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

impl<T: ?Sized> Encode for PhantomData<T> {}
impl<T: ?Sized> Decode<'_> for PhantomData<T> {}

unsafe impl<T: ?Sized> CType for PhantomData<T> {}
unsafe impl<T: ?Sized> BorrowCast for PhantomData<T> {
    type AsConst = Self;
}
unsafe impl<T: ?Sized> BorrowCastMut for PhantomData<T> {
    type AsMut = Self;
}
unsafe impl<T: ?Sized> Borrow for NonNull<T> {
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
impl<'itm, T: ?Sized + 'itm> FromBorrow<'itm> for NonNull<T> {
    #[inline(always)]
    fn from_borrow(source: Self::Borrowed<'itm>) -> Self {
        source
    }
}

impl<T: CType + ?Sized> ReprC for NonNull<T> {
    // TODO: afaik the pointer is not necessarily mutable just non-null
    type CType = <*mut T as ReprC>::CType;
}
unsafe impl<T: CType + ?Sized> EncodeOwned for NonNull<T> {
    type Store = <*mut T as EncodeOwned>::Store;

    #[inline(always)]
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        self.as_ptr().soft_encode(store)
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

impl<T: CType + ?Sized> Encode for NonNull<T> {}
impl<T: CType + ?Sized> Decode<'_> for NonNull<T> {}

unsafe impl<T: CType + ?Sized> CheckedTransmute for NonNull<T>
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
    type CType = [u8];
}
#[cfg(feature = "alloc")]
unsafe impl Borrow for String {
    type Borrowed<'itm>
        = &'itm str
    where
        Self: 'itm;

    type Owner = Self;

    #[inline(always)]
    fn borrow<'itm>(self, owner: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        *owner = self;
        owner
    }
}
#[cfg(feature = "alloc")]
impl<'itm> FromBorrow<'itm> for String {
    #[inline(always)]
    fn from_borrow(source: Self::Borrowed<'itm>) -> Self {
        source.into()
    }
}

#[cfg(feature = "alloc")]
impl ReprC for String {
    type CType = <Vec<u8> as ReprC>::CType;
}
#[cfg(feature = "alloc")]
unsafe impl EncodeOwned for String {
    type Store = ();

    #[inline(always)]
    fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        self.into_bytes().soft_encode(&mut ())
    }
}
#[cfg(feature = "alloc")]
unsafe impl<'d> DecodeOwned<'d> for String {
    type Store = ();

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        let bytes = unsafe { Vec::<u8>::soft_decode(source, &mut ())? };
        String::from_utf8(bytes).ok()
    }

    #[inline(always)]
    unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
        let bytes = unsafe { Vec::<u8>::soft_decode_unchecked(source, &mut ()) };
        unsafe { String::from_utf8_unchecked(bytes) }
    }
}

#[cfg(feature = "alloc")]
impl Encode for String {}
#[cfg(feature = "alloc")]
impl Decode<'_> for String {}

#[cfg(feature = "alloc")]
impl Niche for String {
    const NICHE: Self::CType = CWideBox::null();
}
unsafe impl<T: Borrow> Borrow for UnsafeCell<T> {
    type Borrowed<'itm>
        = UnsafeCell<T::Borrowed<'itm>>
    where
        Self: 'itm;

    type Owner = T::Owner;

    #[inline(always)]
    fn borrow<'itm>(self, owner: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        UnsafeCell::new(self.into_inner().borrow(owner))
    }
}
impl<'itm, T: FromBorrow<'itm>> FromBorrow<'itm> for UnsafeCell<T> {
    #[inline(always)]
    fn from_borrow(source: Self::Borrowed<'itm>) -> Self {
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
unsafe impl<R: EncodeOwned> EncodeOwned for UnsafeCell<R> {
    type Store = R::Store;

    #[inline(always)]
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        UnsafeCell::new(self.into_inner().soft_encode(store))
    }
}
unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for UnsafeCell<R> {
    type Store = R::Store;

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        unsafe { R::soft_decode(source.into_inner(), store) }.map(Self::new)
    }

    #[inline(always)]
    unsafe fn soft_decode_unchecked<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Self {
        Self::new(unsafe { R::soft_decode_unchecked(source.into_inner(), store) })
    }
}

impl<R: Encode> Encode for UnsafeCell<R> {}
impl<'d, R: Decode<'d>> Decode<'d> for UnsafeCell<R> {}

unsafe impl<T: CheckedTransmute + ?Sized> CheckedTransmute for UnsafeCell<T> {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        unsafe { T::is_valid(&*target.get()) }
    }
}

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
    type Borrowed<'itm>
        = UnsafeCell<T::Borrowed<'itm>>
    where
        Self: 'itm;

    type Owner = T::Owner;

    #[inline(always)]
    fn borrow<'itm>(self, owner: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        UnsafeCell::new(Cell::into_inner(self)).borrow(owner)
    }
}
impl<'itm, T: FromBorrow<'itm>> FromBorrow<'itm> for Cell<T> {
    #[inline(always)]
    fn from_borrow(source: Self::Borrowed<'itm>) -> Self {
        Cell::new(T::from_borrow(source.into_inner()))
    }
}

impl<R: ReprC + ?Sized> ReprC for Cell<R> {
    type CType = <UnsafeCell<R> as ReprC>::CType;
}
unsafe impl<R: EncodeOwned> EncodeOwned for Cell<R> {
    type Store = R::Store;

    #[inline(always)]
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        UnsafeCell::new(self.into_inner().soft_encode(store))
    }
}
unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for Cell<R> {
    type Store = R::Store;

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        unsafe { R::soft_decode(source.into_inner(), store) }.map(Self::new)
    }

    #[inline(always)]
    unsafe fn soft_decode_unchecked<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Self {
        Self::new(unsafe { R::soft_decode_unchecked(source.into_inner(), store) })
    }
}

impl<R: Encode> Encode for Cell<R> {}
impl<'d, R: Decode<'d>> Decode<'d> for Cell<R> {}

unsafe impl<T: CheckedTransmute<CType: Copy>> CheckedTransmute for Cell<T> {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        unsafe { T::is_valid(&*target.get()) }
    }
}

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
impl<'itm, T: 'itm> FromBorrow<'itm> for MaybeUninit<T> {
    #[inline(always)]
    fn from_borrow(source: Self::Borrowed<'itm>) -> Self {
        source
    }
}

impl<T: CheckedTransmute<CType: Sized>> ReprC for MaybeUninit<T> {
    type CType = MaybeUninit<T::CType>;
}
unsafe impl<T: CheckedTransmute<CType: Sized>> EncodeOwned for MaybeUninit<T> {
    type Store = ();

    #[inline(always)]
    fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe { core::mem::transmute_copy(&self) }
    }
}
unsafe impl<'d, T: CheckedTransmute<CType: Sized>> DecodeOwned<'d> for MaybeUninit<T> {
    type Store = ();

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        Some(unsafe { core::mem::transmute_copy(&source) })
    }
}

impl<T: CheckedTransmute<CType: Sized>> Encode for MaybeUninit<T> {}
impl<'d, T: CheckedTransmute<CType: Sized>> Decode<'d> for MaybeUninit<T> {}

unsafe impl<T: CheckedTransmute<CType: Sized>> CheckedTransmute for MaybeUninit<T> {
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

unsafe impl<T: CType> BorrowCast for MaybeUninit<T> {
    type AsConst = Self;
}
unsafe impl<T: CType> BorrowCastMut for MaybeUninit<T> {
    type AsMut = Self;
}

unsafe impl<T: EmptyStore> EmptyStore for MaybeUninit<T> {}

unsafe impl<T> Borrow for ManuallyDrop<T> {
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
impl<'itm, T: 'itm> FromBorrow<'itm> for ManuallyDrop<T> {
    #[inline(always)]
    fn from_borrow(source: Self::Borrowed<'itm>) -> Self {
        source
    }
}

impl<T: ReprC + ?Sized> ReprC for ManuallyDrop<T> {
    type CType = ManuallyDrop<T::CType>;
}

impl<T: ReprC> Encode for ManuallyDrop<T> where Self: EncodeOwned {}
impl<'d, T: ReprC> Decode<'d> for ManuallyDrop<T> where Self: DecodeOwned<'d> {}

unsafe impl<T: CheckedTransmute + ?Sized> CheckedTransmute for ManuallyDrop<T> {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        unsafe { T::is_valid(target) }
    }
}

impl<T: Niche> Niche for ManuallyDrop<T> {
    const NICHE: Self::CType = ManuallyDrop::new(T::NICHE);
}

unsafe impl<T: CType + ?Sized> CType for ManuallyDrop<T> {}
unsafe impl<T: CFnArg> CFnArg for ManuallyDrop<T> where
    Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}
unsafe impl<T: CFnReturn> CFnReturn for ManuallyDrop<T> where
    Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}
unsafe impl<T: CType + ?Sized> BorrowCast for ManuallyDrop<T> {
    type AsConst = Self;
}
unsafe impl<T: CType + ?Sized> BorrowCastMut for ManuallyDrop<T> {
    type AsMut = Self;
}

unsafe impl<T: EmptyStore> EmptyStore for ManuallyDrop<T> {}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc::{boxed::Box, vec};

    #[cfg(feature = "alloc")]
    use static_assertions::assert_type_eq_all;
    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    #[cfg(feature = "alloc")]
    use crate::boxed::{CBoxCell, CWideBox, CWideBoxCell};
    use crate::{
        CFnArg, CFnReturn, Decode, Encode,
        option::ReprCOption,
        reference::CRefMut,
        slice::{CWideRef, CWideRefMut},
    };

    // FIXME:
    //#[test]
    //fn manually_drop_checked_transmute_encodes_without_dropping() {
    //    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    //    #[repr(transparent)]
    //    struct DropProbe(u8);

    //    impl Drop for DropProbe {
    //        fn drop(&mut self) {
    //            DROP_COUNT.fetch_add(1, Ordering::Relaxed);
    //        }
    //    }

    //    impl ReprC for DropProbe {
    //        type CType = u8;
    //    }

    //    unsafe impl CheckedTransmute for DropProbe {
    //        unsafe fn is_valid(_: &Self::CType) -> bool {
    //            true
    //        }
    //    }

    //    assert_impl_all!(ManuallyDrop<DropProbe>: CheckedTransmute, Encode, Decode<'static>);

    //    let before = DROP_COUNT.load(Ordering::Relaxed);
    //    let encoded = crate::encode(ManuallyDrop::new(DropProbe(42)));

    //    assert_eq!(encoded, 42);
    //    assert_eq!(DROP_COUNT.load(Ordering::Relaxed), before);

    //    {
    //        let decoded = unsafe { crate::decode::<ManuallyDrop<DropProbe>>(encoded) }.unwrap();
    //        assert_eq!(decoded.0, 42);
    //        assert_eq!(DROP_COUNT.load(Ordering::Relaxed), before);
    //    }
    //    assert_eq!(DROP_COUNT.load(Ordering::Relaxed), before);
    //}

    #[test]
    fn manually_drop_decode_rejects_invalid_inner_representation() {
        assert_impl_all!(ManuallyDrop<bool>: Decode<'static>, Encode);

        let invalid = bool::NICHE;
        assert!(
            unsafe { crate::decode::<ManuallyDrop<bool>>(ManuallyDrop::new(invalid)) }.is_none()
        );

        let encoded = crate::encode(ManuallyDrop::new(true));
        let decoded = unsafe { crate::decode::<ManuallyDrop<bool>>(encoded) }.unwrap();
        assert!(*decoded);
    }

    #[test]
    fn maybe_uninit_lowers_without_validating_or_encoding_the_inner_value() {
        assert_impl_all!(MaybeUninit<bool>:
            ReprC<CType = MaybeUninit<crate::primitives::CBool>>,
            CheckedTransmute,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(MaybeUninit<u8>: CType, CFnArg, CFnReturn);

        let source = MaybeUninit::new(bool::NICHE);
        assert!(unsafe { <MaybeUninit<bool> as CheckedTransmute>::is_valid(&source) });

        let decoded = unsafe { crate::decode::<MaybeUninit<bool>>(source) }.unwrap();
        let encoded = crate::encode(decoded);

        assert_eq!(unsafe { encoded.assume_init() }, bool::NICHE);
    }

    // TODO: Enable
    //#[test]
    //fn manually_drop_inner_without_drop() {
    //    assert_impl_all!(ManuallyDrop<u8>:
    //        ReprC<CType = u8>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(&ManuallyDrop<u8>:
    //        Niche<CType = *const u8>,
    //        RustSpec<Niche = rust_spec::niche::WithNiche<rust_spec::Stable>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(&mut ManuallyDrop<u8>:
    //        Niche<CType = CRefMut<u8, true>>,
    //        RustSpec<Niche = rust_spec::niche::WithNiche<rust_spec::Stable>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    #[cfg(feature = "alloc")]
    //    assert_impl_all!(Box<ManuallyDrop<u8>>:
    //        Niche<CType = CBox<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(&[ManuallyDrop<u8>]:
    //        Niche<CType = CWideRef<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(&mut [ManuallyDrop<u8>]:
    //        Niche<CType = CWideRefMut<u8, true>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    #[cfg(feature = "alloc")]
    //    assert_impl_all!(Box<[ManuallyDrop<u8>]>:
    //        Niche<CType = CWideBox<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    #[cfg(feature = "alloc")]
    //    assert_impl_all!(Vec<ManuallyDrop<u8>>:
    //        Niche<CType = CWideBox<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!([ManuallyDrop<u8>; 2]:
    //        ReprC<CType = [u8; 2]>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(Option<ManuallyDrop<u8>>:
    //        Niche<CType = ReprCOption<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );

    //    assert_not_impl_any!(ManuallyDrop<u8>: CType);
    //}

    //#[test]
    //#[cfg(feature = "alloc")]
    //fn manually_drop_inner_with_drop() {
    //    assert_impl_all!(ManuallyDrop<String>:
    //        Niche<CType = CWideBox<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(&ManuallyDrop<String>:
    //        Niche<CType = *const CWideBox<u8>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(&mut ManuallyDrop<String>:
    //        Niche<CType = CRefMut<CWideBox<u8>, true>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(Box<ManuallyDrop<String>>:
    //        Niche<CType = CBox<CWideBox<u8>>,
    //        RustSpec<Niche = rust_spec::niche::WithNiche<rust_spec::Stable>>>>,
    //        DecodeOwned<'static>,
    //        EncodeOwned,
    //    );
    //    assert_impl_all!(&[ManuallyDrop<String>]:
    //        Niche<CType = CWideRef<CWideBox<u8>>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(&mut [ManuallyDrop<String>]:
    //        Niche<CType = CWideRefMut<CWideBox<u8>, true>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(Box<[ManuallyDrop<String>]>:
    //        Niche<CType = CWideBox<CWideBox<u8>>>,
    //        DecodeOwned<'static>,
    //        EncodeOwned,
    //    );
    //    assert_impl_all!(Vec<ManuallyDrop<String>>:
    //        Niche<CType = CWideBox<CWideBox<u8>>>,
    //        DecodeOwned<'static>,
    //        EncodeOwned,
    //    );
    //    assert_impl_all!([ManuallyDrop<String>; 2]:
    //        Niche<CType = [CWideBox<u8>; 2]>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(Option<ManuallyDrop<String>>:
    //        //Niche<CType = CWideBox<u8>>,
    //        ReprC<CType = CWideBox<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_not_impl_any!(ManuallyDrop<String>: CType);

    //    #[cfg(feature = "alloc")]
    //    assert_not_impl_any!(Box<[ManuallyDrop<String>]>: Encode, Decode<'static>);
    //    #[cfg(feature = "alloc")]
    //    assert_not_impl_any!(Vec<ManuallyDrop<String>>: Encode, Decode<'static>);
    //}

    #[test]
    fn str_is_supported() {
        assert_impl_all!(str: ReprC<CType = [u8]>);
        #[cfg(feature = "alloc")]
        assert_impl_all!(String: ReprC<CType = CWideBox<u8>>);

        assert_impl_all!(&str:
            Niche<CType = CWideRef<u8>>,
            Decode<'static>,
            Encode,
        );

        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut str:
            Niche<CType = CWideRefMut<u8, true>>,
            Decode<'static>,
            Encode,
        );

        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<str>:
            Niche<CType = CWideBox<u8>>,
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
            let source = CWideBox::from_boxed_slice(vec![0xff].into_boxed_slice());
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
        assert_impl_all!(UnsafeCell<u8>: CheckedTransmute);
        assert_impl_all!(Cell<u8>: CheckedTransmute);
        assert_impl_all!([UnsafeCell<u8>; 2]: CheckedTransmute);
        assert_not_impl_any!((UnsafeCell<u8>, u8): CheckedTransmute);
        assert_impl_all!(ManuallyDrop<UnsafeCell<u8>>: CheckedTransmute);

        assert_impl_all!(UnsafeCell<u8>:
            ReprC<CType = UnsafeCell<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&UnsafeCell<u8>:
            Niche<CType = CRefMut<UnsafeCell<u8>, false>>,
            Decode<'static>,
            Encode,
        );
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
        assert_not_impl_any!(Box<UnsafeCell<u8>>: CheckedTransmute);
        assert_impl_all!(&[UnsafeCell<u8>]:
            Niche<CType = CWideRefMut<UnsafeCell<u8>, false>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [UnsafeCell<u8>]:
            Niche<CType = CWideRefMut<UnsafeCell<u8>, true>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[UnsafeCell<u8>]>:
            Niche<CType = CWideBoxCell<UnsafeCell<u8>>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<UnsafeCell<u8>>:
            Niche<CType = CWideBoxCell<UnsafeCell<u8>>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([UnsafeCell<u8>; 2]:
            ReprC<CType = [UnsafeCell<u8>; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<UnsafeCell<u8>>:
            ReprC<CType = ReprCOption<UnsafeCell<u8>>>,
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
    fn shared_slice_pointers_cover_cells_and_empty_slices() {
        let cells = [UnsafeCell::new(1_u8)];
        let view = crate::encode(&cells[..]);
        unsafe { view.data().write(UnsafeCell::new(2)) };
        assert_eq!(unsafe { *cells[0].get() }, 2);

        let cells = [Cell::new(3_u8)];
        let view = crate::encode(&cells[..]);
        unsafe { view.data().write(UnsafeCell::new(4)) };
        assert_eq!(cells[0].get(), 4);

        let empty_unsafe_cells: [UnsafeCell<u8>; 0] = [];
        assert_eq!(crate::encode(&empty_unsafe_cells[..]).len(), 0);
        let empty_cells: [Cell<u8>; 0] = [];
        assert_eq!(crate::encode(&empty_cells[..]).len(), 0);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn interior_mutable_slice_carriers_round_trip() {
        assert_type_eq_all!(
            <CWideBoxCell<UnsafeCell<u8>> as BorrowCast>::AsConst,
            <&[UnsafeCell<u8>] as ReprC>::CType
        );

        let cells = [UnsafeCell::new(3_u8), UnsafeCell::new(4_u8)];
        let view = crate::encode(&cells[..]);
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

    // FIXME:
    //#[test]
    //fn non_robust_mutable_cell_syncs_after_soft_encode() {
    //    use crate::stored::Store;

    //    let mut value = Cell::new(NonZero::new(3_u8).unwrap());
    //    let mut store = <&mut Cell<NonZero<u8>> as EncodeOwned>::Store::default();
    //    let encoded = <&mut Cell<NonZero<u8>> as EncodeOwned>::soft_encode(&mut value, &mut store);
    //    unsafe { encoded.as_ptr().write(UnsafeCell::new(4)) };
    //    assert_eq!(store.sync(), Some(()));
    //    assert_eq!(value.get().get(), 4);
    //}

    #[test]
    fn non_robust_unsafe_cell() {
        assert_impl_all!(UnsafeCell<NonZero<u8>>:
            ReprC<CType = UnsafeCell<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&UnsafeCell<NonZero<u8>>:
            Niche<CType = CRefMut<UnsafeCell<u8>, false>>,
            Decode<'static>,
            //Encode,
        );
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
            Niche<CType = CWideRefMut<UnsafeCell<u8>, false>>,
            Decode<'static>,
        );
        assert_impl_all!(&mut [UnsafeCell<NonZero<u8>>]:
            Niche<CType = CWideRefMut<UnsafeCell<u8>, true>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[UnsafeCell<NonZero<u8>>]>:
            Niche<CType = CWideBoxCell<UnsafeCell<u8>>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<UnsafeCell<NonZero<u8>>>:
            Niche<CType = CWideBoxCell<UnsafeCell<u8>>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        {
            let invalid =
                CWideBoxCell::from_boxed_slice(vec![UnsafeCell::new(0_u8)].into_boxed_slice());
            assert!(unsafe { crate::decode::<Vec<UnsafeCell<NonZero<u8>>>>(invalid) }.is_none());

            let invalid =
                CWideBoxCell::from_boxed_slice(vec![UnsafeCell::new(0_u8)].into_boxed_slice());
            assert!(unsafe { crate::decode::<Box<[UnsafeCell<NonZero<u8>>]>>(invalid) }.is_none());
        }
        assert_impl_all!([UnsafeCell<NonZero<u8>>; 2]:
            ReprC<CType = [UnsafeCell<u8>; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<UnsafeCell<NonZero<u8>>>:
            ReprC<CType = ReprCOption<UnsafeCell<u8>>>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(UnsafeCell<NonZero<u8>>: CType);
    }
}
