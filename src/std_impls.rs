#[cfg(feature = "alloc")]
use alloc::{string::String, vec::Vec};
use core::{
    cell::{Cell, UnsafeCell},
    marker::PhantomData,
    mem::{ManuallyDrop, MaybeUninit},
    num::NonZero,
    ptr::NonNull,
};

use crate::{
    CFnArg, CFnReturn, Decode, Encode, ExternC, ReprC,
    borrow::{Borrow, BorrowCast, BorrowCastMut, FromBorrow},
    niche::Niche,
    stored::{DecodeOwned, EmptyStore, EncodeOwned},
    transmute::CheckedTransmute,
};
#[cfg(feature = "alloc")]
use crate::{boxed::CBoxedSlice, stored::Owned};

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

        impl ExternC for NonZero<$primitive> {
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
            const NICHE_VALUE: Self::CType = 0;
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

impl ExternC for () {
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

unsafe impl ReprC for () {}
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

impl<T: ?Sized> ExternC for PhantomData<T> {
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

unsafe impl<T: ?Sized> ReprC for PhantomData<T> {}
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

impl<T: ReprC + ?Sized> ExternC for NonNull<T> {
    // TODO: afaik the pointer is not necessarily mutable just non-null
    type CType = <*mut T as ExternC>::CType;
}
unsafe impl<T: ReprC + ?Sized> EncodeOwned for NonNull<T> {
    type Store = <*mut T as EncodeOwned>::Store;

    #[inline(always)]
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        self.as_ptr().soft_encode(store)
    }
}
unsafe impl<'d, T: ReprC + ?Sized> DecodeOwned<'d> for NonNull<T> {
    type Store = <*mut T as DecodeOwned<'d>>::Store;

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        let ptr = unsafe { DecodeOwned::soft_decode(source, store)? };
        NonNull::new(ptr)
    }
}

impl<T: ReprC + ?Sized> Encode for NonNull<T> {}
impl<T: ReprC + ?Sized> Decode<'_> for NonNull<T> {}

unsafe impl<T: ReprC + ?Sized> CheckedTransmute for NonNull<T> {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        !target.is_null()
    }
}

#[cfg(feature = "alloc")]
impl Owned for str {
    type Owned = String;
}

impl ExternC for str {
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
impl ExternC for String {
    type CType = <Vec<u8> as ExternC>::CType;
}
#[cfg(feature = "alloc")]
unsafe impl EncodeOwned for String {
    type Store = ();

    #[inline(always)]
    fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
    where
        Self: 'itm,
    {
        crate::stored::encode_owned(self.into_bytes())
    }
}
#[cfg(feature = "alloc")]
unsafe impl<'d> DecodeOwned<'d> for String {
    type Store = ();

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
        let bytes = unsafe { crate::stored::decode_owned(source)? };
        String::from_utf8(bytes).ok()
    }
}

#[cfg(feature = "alloc")]
impl Encode for String {}
#[cfg(feature = "alloc")]
impl Decode<'_> for String {}

#[cfg(feature = "alloc")]
impl Niche for String {
    const NICHE_VALUE: Self::CType = CBoxedSlice::NICHE_VALUE;
}

unsafe impl<T: Borrow> Borrow for UnsafeCell<T> {
    type Borrowed<'itm>
        = T::Borrowed<'itm>
    where
        Self: 'itm;

    type Owner = T::Owner;

    #[inline(always)]
    fn borrow<'itm>(self, owner: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        self.into_inner().borrow(owner)
    }
}
impl<'itm, T: FromBorrow<'itm>> FromBorrow<'itm> for UnsafeCell<T> {
    #[inline(always)]
    fn from_borrow(source: Self::Borrowed<'itm>) -> Self {
        UnsafeCell::new(T::from_borrow(source))
    }
}

impl<R: ExternC + ?Sized> ExternC for UnsafeCell<R> {
    type CType = R::CType;
}
unsafe impl<R: EncodeOwned<CType: Copy>> EncodeOwned for UnsafeCell<R> {
    type Store = R::Store;

    #[inline(always)]
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        self.into_inner().soft_encode(store)
    }
}
unsafe impl<'d, R: DecodeOwned<'d, CType: Copy>> DecodeOwned<'d> for UnsafeCell<R> {
    type Store = R::Store;

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        unsafe { R::soft_decode(source, store) }.map(Self::new)
    }
}

impl<R: Encode<CType: Copy>> Encode for UnsafeCell<R> {}
impl<'d, R: Decode<'d, CType: Copy>> Decode<'d> for UnsafeCell<R> {}

unsafe impl<T: CheckedTransmute + ?Sized> CheckedTransmute for UnsafeCell<T> {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        unsafe { T::is_valid(target) }
    }
}

unsafe impl<T: EmptyStore> EmptyStore for UnsafeCell<T> {}

unsafe impl<T: Borrow> Borrow for Cell<T> {
    type Borrowed<'itm>
        = T::Borrowed<'itm>
    where
        Self: 'itm;

    type Owner = T::Owner;

    #[inline(always)]
    fn borrow<'itm>(self, owner: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        T::borrow(Cell::into_inner(self), owner)
    }
}
impl<'itm, T: FromBorrow<'itm>> FromBorrow<'itm> for Cell<T> {
    #[inline(always)]
    fn from_borrow(source: Self::Borrowed<'itm>) -> Self {
        Cell::new(T::from_borrow(source))
    }
}

impl<R: ExternC + ?Sized> ExternC for Cell<R> {
    type CType = R::CType;
}
unsafe impl<R: EncodeOwned<CType: Copy>> EncodeOwned for Cell<R> {
    type Store = R::Store;

    #[inline(always)]
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        self.into_inner().soft_encode(store)
    }
}
unsafe impl<'d, R: DecodeOwned<'d, CType: Copy>> DecodeOwned<'d> for Cell<R> {
    type Store = R::Store;

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        unsafe { R::soft_decode(source, store) }.map(Self::new)
    }
}

impl<R: Encode<CType: Copy>> Encode for Cell<R> {}
impl<'d, R: Decode<'d, CType: Copy>> Decode<'d> for Cell<R> {}

unsafe impl<T: CheckedTransmute + ?Sized> CheckedTransmute for Cell<T> {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        unsafe { T::is_valid(target) }
    }
}

unsafe impl<T: EmptyStore> EmptyStore for Cell<T> {}

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

impl<T: CheckedTransmute<CType: Sized>> ExternC for MaybeUninit<T> {
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

unsafe impl<T: ReprC> ReprC for MaybeUninit<T> {}
unsafe impl<T: CFnArg> CFnArg for MaybeUninit<T> {}
unsafe impl<T: CFnReturn> CFnReturn for MaybeUninit<T> {}

unsafe impl<T: ReprC> BorrowCast for MaybeUninit<T> {
    type AsConst = Self;
}
unsafe impl<T: ReprC> BorrowCastMut for MaybeUninit<T> {
    type AsMut = Self;
}

unsafe impl<T: EmptyStore> EmptyStore for MaybeUninit<T> {}

unsafe impl<T: Borrow> Borrow for ManuallyDrop<T> {
    type Borrowed<'itm>
        = T::Borrowed<'itm>
    where
        Self: 'itm;

    type Owner = T::Owner;

    #[inline(always)]
    fn borrow<'itm>(self, owner: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        ManuallyDrop::into_inner(self).borrow(owner)
    }
}
impl<'itm, T: FromBorrow<'itm>> FromBorrow<'itm> for ManuallyDrop<T> {
    #[inline(always)]
    fn from_borrow(source: Self::Borrowed<'itm>) -> Self {
        ManuallyDrop::new(T::from_borrow(source))
    }
}

impl<T: ExternC + ?Sized> ExternC for ManuallyDrop<T> {
    type CType = T::CType;
}
// FIXME: I think t's not ok to get owned type and encode it
// ManuallyDrop::into_inner(self).soft_encode(store)
//unsafe impl<R: EncodeOwned<CType: Copy>> EncodeOwned for ManuallyDrop<R> {
//    type Store = R::Store;
//
//    #[inline(always)]
//    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
//    where
//        Self: 'itm,
//    {
//        unimplemented!()
//    }
//}
//unsafe impl<'d, R: DecodeOwned<'d, CType: Copy>> DecodeOwned<'d> for ManuallyDrop<R> {
//    type Store = R::Store;
//
//    #[inline(always)]
//    unsafe fn soft_decode<'itm: 'd>(
//        source: Self::CType,
//        store: &'itm mut Self::Store,
//    ) -> Option<Self> {
//        unimplemented!()
//    }
//}

//impl<R: Encode<CType: Copy>> Encode for ManuallyDrop<R> {}
//impl<'d, R: Decode<'d, CType: Copy>> Decode<'d> for ManuallyDrop<R> {}

unsafe impl<T: CheckedTransmute + ?Sized> CheckedTransmute for ManuallyDrop<T> {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        unsafe { T::is_valid(target) }
    }
}

impl<T: Niche> Niche for ManuallyDrop<T> {
    const NICHE_VALUE: Self::CType = T::NICHE_VALUE;
}

unsafe impl<T: EmptyStore> EmptyStore for ManuallyDrop<T> {}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc::{boxed::Box, vec};

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    #[cfg(feature = "alloc")]
    use crate::boxed::{CBox, CBoxedSlice};
    use crate::{
        CFnArg, CFnReturn, Decode, Encode,
        option::ReprCOption,
        slice::{CSlice, CSliceMut},
    };

    #[test]
    fn maybe_uninit_lowers_without_validating_or_encoding_the_inner_value() {
        assert_impl_all!(MaybeUninit<bool>:
            ExternC<CType = MaybeUninit<u8>>,
            CheckedTransmute,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(MaybeUninit<u8>: ReprC, CFnArg, CFnReturn);

        let source = MaybeUninit::new(2_u8);
        assert!(unsafe { <MaybeUninit<bool> as CheckedTransmute>::is_valid(&source) });

        let decoded = unsafe { crate::decode::<MaybeUninit<bool>>(source) }.unwrap();
        let encoded = crate::encode(decoded);

        assert_eq!(unsafe { encoded.assume_init() }, 2);
    }

    // TODO: Enable
    //#[test]
    //fn manually_drop_inner_without_drop() {
    //    assert_impl_all!(ManuallyDrop<u8>:
    //        ExternC<CType = u8>,
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
    //        Niche<CType = *mut u8>,
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
    //        Niche<CType = CSlice<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(&mut [ManuallyDrop<u8>]:
    //        Niche<CType = CSliceMut<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    #[cfg(feature = "alloc")]
    //    assert_impl_all!(Box<[ManuallyDrop<u8>]>:
    //        Niche<CType = CBoxedSlice<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    #[cfg(feature = "alloc")]
    //    assert_impl_all!(Vec<ManuallyDrop<u8>>:
    //        Niche<CType = CBoxedSlice<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!([ManuallyDrop<u8>; 2]:
    //        ExternC<CType = [u8; 2]>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(Option<ManuallyDrop<u8>>:
    //        Niche<CType = ReprCOption<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );

    //    assert_not_impl_any!(ManuallyDrop<u8>: ReprC);
    //}

    //#[test]
    //#[cfg(feature = "alloc")]
    //fn manually_drop_inner_with_drop() {
    //    assert_impl_all!(ManuallyDrop<String>:
    //        Niche<CType = CBoxedSlice<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(&ManuallyDrop<String>:
    //        Niche<CType = *const CBoxedSlice<u8>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(&mut ManuallyDrop<String>:
    //        Niche<CType = *mut CBoxedSlice<u8>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(Box<ManuallyDrop<String>>:
    //        Niche<CType = CBox<CBoxedSlice<u8>>,
    //        RustSpec<Niche = rust_spec::niche::WithNiche<rust_spec::Stable>>>>,
    //        DecodeOwned<'static>,
    //        EncodeOwned,
    //    );
    //    assert_impl_all!(&[ManuallyDrop<String>]:
    //        Niche<CType = CSlice<CBoxedSlice<u8>>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(&mut [ManuallyDrop<String>]:
    //        Niche<CType = CSliceMut<CBoxedSlice<u8>>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(Box<[ManuallyDrop<String>]>:
    //        Niche<CType = CBoxedSlice<CBoxedSlice<u8>>>,
    //        DecodeOwned<'static>,
    //        EncodeOwned,
    //    );
    //    assert_impl_all!(Vec<ManuallyDrop<String>>:
    //        Niche<CType = CBoxedSlice<CBoxedSlice<u8>>>,
    //        DecodeOwned<'static>,
    //        EncodeOwned,
    //    );
    //    assert_impl_all!([ManuallyDrop<String>; 2]:
    //        Niche<CType = [CBoxedSlice<u8>; 2]>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_impl_all!(Option<ManuallyDrop<String>>:
    //        //Niche<CType = CBoxedSlice<u8>>,
    //        ExternC<CType = CBoxedSlice<u8>>,
    //        Decode<'static>,
    //        Encode,
    //    );
    //    assert_not_impl_any!(ManuallyDrop<String>: ReprC);

    //    #[cfg(feature = "alloc")]
    //    assert_not_impl_any!(Box<[ManuallyDrop<String>]>: Encode, Decode<'static>);
    //    #[cfg(feature = "alloc")]
    //    assert_not_impl_any!(Vec<ManuallyDrop<String>>: Encode, Decode<'static>);
    //}

    #[test]
    fn str_is_supported() {
        assert_impl_all!(&str:
            Niche<CType = CSlice<u8>>,
            Decode<'static>,
            Encode,
        );

        #[cfg(feature = "alloc")]
        assert_impl_all!(&mut str:
            Niche<CType = CSliceMut<u8>>,
            Decode<'static>,
            Encode,
        );

        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<str>:
            Niche<CType = CBoxedSlice<u8>>,
            Decode<'static>,
            Encode,
        );
    }

    #[test]
    fn str_decode_rejects_invalid_utf8() {
        let invalid = [0xff];

        let source = CSlice::from_raw_parts(invalid.as_ptr(), invalid.len());
        let decoded = unsafe { crate::decode::<&str>(source) };
        assert!(decoded.is_none());

        let mut invalid = [0xff];
        let source = CSliceMut::from_raw_parts_mut(invalid.as_mut_ptr(), invalid.len());
        let decoded = unsafe { crate::decode::<&mut str>(source) };
        assert!(decoded.is_none());

        #[cfg(feature = "alloc")]
        {
            let source = CBoxedSlice::from_boxed_slice(vec![0xff].into_boxed_slice());
            let decoded = unsafe { crate::decode::<Box<str>>(source) };
            assert!(decoded.is_none());
        }
    }

    #[test]
    fn robust_unsafe_cell() {
        assert_impl_all!(UnsafeCell<u8>:
            ExternC<CType = u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&UnsafeCell<u8>:
            Niche<CType = *mut u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut UnsafeCell<u8>:
            Niche<CType = *mut u8>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<UnsafeCell<u8>>:
            Niche<CType = CBox<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[UnsafeCell<u8>]:
            Niche<CType = CSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [UnsafeCell<u8>]:
            Niche<CType = CSliceMut<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[UnsafeCell<u8>]>:
            Niche<CType = CBoxedSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<UnsafeCell<u8>>:
            Niche<CType = CBoxedSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([UnsafeCell<u8>; 2]:
            ExternC<CType = [u8; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<UnsafeCell<u8>>:
            Niche<CType = ReprCOption<u8>>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(UnsafeCell<u8>: ReprC);
    }

    #[test]
    fn non_robust_unsafe_cell() {
        assert_impl_all!(UnsafeCell<NonZero<u8>>:
            ExternC<CType = u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&UnsafeCell<NonZero<u8>>:
            Niche<CType = *mut u8>,
            Decode<'static>,
            //Encode,
        );
        assert_impl_all!(&mut UnsafeCell<NonZero<u8>>:
            Niche<CType = *mut u8>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<UnsafeCell<NonZero<u8>>>:
            Niche<CType = CBox<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&[UnsafeCell<NonZero<u8>>]:
            Niche<CType = CSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [UnsafeCell<NonZero<u8>>]:
            Niche<CType = CSliceMut<u8>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[UnsafeCell<NonZero<u8>>]>:
            Niche<CType = CBoxedSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<UnsafeCell<NonZero<u8>>>:
            Niche<CType = CBoxedSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([UnsafeCell<NonZero<u8>>; 2]:
            ExternC<CType = [u8; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<UnsafeCell<NonZero<u8>>>:
            Niche<CType = ReprCOption<u8>>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(UnsafeCell<NonZero<u8>>: ReprC);
    }
}
