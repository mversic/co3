#[cfg(feature = "alloc")]
use alloc::{borrow::ToOwned, boxed::Box, vec::Vec};
use core::mem::ManuallyDrop;
#[cfg(feature = "alloc")]
use core::ptr::NonNull;

pub use crate::mutable::{EncodeMut, EncodeMutUnchecked};
pub use crate::shared::{EncodeRef, EncodeRefUnchecked};
use disjoint_impls::disjoint_impls;
use rust_spec::{
    One, RustSpec, Stable, Unstable,
    drop::NoDrop,
    layout::{NonRobust, Robust},
    mutability::{Exclusive, WithInterior},
    niche::{NicheStabilityKind, WithNiche, WithoutNiche},
    size::{MetaSized, NulTerminated, Sized as RustSpecSized, SizedKind, SliceLike, Zero},
};

#[cfg(feature = "alloc")]
use crate::{
    Dst,
    borrow::{BorrowCastMut, borrow_cast_mut},
    boxed::{FromBox, FromWideBox},
    sync::{RefDstDecodeStore, RefMutDstEncodeStore, RefMutSliceDecodeStore},
};

use crate::{
    CType, ReprC, Thin, WithDrop,
    borrow::{Borrow, BorrowCast, FromBorrow, borrow_cast},
    ffi::NulTerminatedBuf,
    niche::Niche,
    option::COption,
    reference::{CRef, CRefMut, FromRawRef},
    result::CResult,
    slice::{CWideRefMut, FromRawWideRef},
    sync::{RefMutSizedDecodeStore, RefSizedDecodeStore},
    transmute::AbiCompatible,
    wide::{Wide, WideData, WideHeader},
};

// TODO: Could the store just be synced on drop?
// FIXME: Encode types can never error during sync
/// Holds temporary conversion state and applies deferred mutable write-back.
pub trait Store: Sized {
    fn sync(self) -> Option<()>;
}

/// Marker for an empty conversion store.
///
/// # Safety
///
/// Type must not contain any conversion state.
#[doc(hidden)]
pub unsafe trait EmptyStore: Sized {}

#[cfg(feature = "alloc")]
unsafe impl<T: EmptyStore> EmptyStore for Box<T> {}
#[cfg(feature = "alloc")]
unsafe impl<T: EmptyStore> EmptyStore for Box<[T]> {}
#[cfg(feature = "alloc")]
unsafe impl<T: EmptyStore> EmptyStore for Vec<T> {}

/// The encodable owned form of a dynamically-sized value.
#[cfg(feature = "alloc")]
pub trait Owned {
    type Owned;
}

// TODO: Can we remove this trait?
#[cfg(feature = "alloc")]
pub trait AssignFromOwned: ToOwned {
    fn assign_from_owned(&mut self, owned: Self::Owned) -> Option<()>;
}

disjoint_impls! {
    /// Encodes a value using storage chosen independently from [`EncodeMove::Store`].
    ///
    /// # Safety
    ///
    /// If [`Self::SoftStore`] implements [`EmptyStore`], [`EncodeMoveUnchecked::soft_encode_unchecked`] must
    /// not return references into the store. [`crate::soft_encode_unchecked`] correctness relies on it.
    pub unsafe trait EncodeMoveUnchecked: ReprC<CType: Sized> + Sized {
        /// Storage for the unchecked encoding path.
        type SoftStore: Store + Default;

        /// Encode using the unchecked path and its independent store.
        ///
        /// Prefer using [`crate::soft_encode_unchecked`] whenever possible
        ///
        /// # Safety
        ///
        /// See safety section in [`crate::soft_encode_unchecked`].
        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType;
    }

    unsafe impl<R: AbiCompatible + ?Sized> EncodeMoveUnchecked for &R
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType: FromRawRef<<R as ReprC>::CType>>,
        R: RustSpec<Layout = Stable, Size: Thin>,
    {
        type SoftStore = ();

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
            let ptr = core::ptr::from_ref(self);

            // TODO: Hack before `Thin` trait is available in stable:
            // https://doc.rust-lang.org/std/ptr/traitalias.Thin.html
            // https://github.com/rust-lang/rust/issues/81513
            let val = unsafe { core::mem::transmute_copy::<*const R, *const R::CType>(&ptr) };

            unsafe { Self::CType::from_raw_reference(val) }
        }
    }
    unsafe impl<R: AbiCompatible + Wide<Metadata = usize> + ?Sized> EncodeMoveUnchecked for &R
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType: FromRawWideRef<<R as ReprC>::CType>>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>>,
        <R as ReprC>::CType: Wide<Header: WideHeader, Metadata = usize>,
        <<<R as ReprC>::CType as Wide>::Header as WideHeader>::Data: CType,
    {
        type SoftStore = ();

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
            let source = core::ptr::from_ref(self);

            let len = R::metadata(source);
            let data = R::as_ptr(source).cast();

            unsafe { Self::CType::from_raw_reference(data, len) }
        }
    }

    unsafe impl<'a, R: EncodeRefUnchecked<'a> + ?Sized> EncodeMoveUnchecked for &'a R
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType: Sized>,
        R: RustSpec<Layout = Unstable, Mutability = Exclusive>,
    {
        type SoftStore = R::SoftStore;

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            unsafe { self.encode_ref_unchecked(store) }
        }
    }

    unsafe impl<R: AbiCompatible + ?Sized> EncodeMoveUnchecked for &mut R
    where
        Self: RustSpec<Layout = Stable>,
        R: RustSpec<Layout = Stable, Size: Thin>,
    {
        type SoftStore = ();

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
            let ptr = core::ptr::from_mut(self);

            // TODO: Hack before `Thin` trait is available in stable:
            // https://doc.rust-lang.org/std/ptr/traitalias.Thin.html
            // https://github.com/rust-lang/rust/issues/81513
            let val = unsafe { core::mem::transmute_copy::<*mut R, *mut R::CType>(&ptr) };

            unsafe { CRefMut::from_raw(val) }
        }
    }
    unsafe impl<R: AbiCompatible + Wide<Metadata = usize> + ?Sized> EncodeMoveUnchecked for &mut R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>>,
        <R as ReprC>::CType: Wide<Header: WideHeader, Metadata = usize>,
        <<<R as ReprC>::CType as Wide>::Header as WideHeader>::Data: CType,
    {
        type SoftStore = ();

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
            let len = R::metadata(self);
            let ptr = R::as_mut_ptr(self).cast();
            unsafe { CWideRefMut::from_raw_parts_mut(ptr, len) }
        }
    }

    unsafe impl<'a, R: EncodeMutUnchecked<'a> + ?Sized> EncodeMoveUnchecked for &'a mut R
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType: Sized>,
        R: RustSpec<Layout = Unstable>,
    {
        type SoftStore = R::SoftStore;
        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            unsafe { self.encode_mut_unchecked(store) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: AbiCompatible, S: SizedKind> EncodeMoveUnchecked for Box<R>
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType: FromBox<<R as ReprC>::CType>>,
        R: RustSpec<Layout = Stable, Size = RustSpecSized<S>>,
        <R as ReprC>::CType: Sized,
    {
        type SoftStore = ();

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
            let ptr = Box::into_non_null(self).cast();
            unsafe { Self::CType::from_non_null(ptr) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: AbiCompatible + Wide<Metadata = usize> + ?Sized> EncodeMoveUnchecked for Box<R>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType: FromWideBox<<R as ReprC>::CType>>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>>,
        <R as ReprC>::CType: Wide<Header: WideHeader, Metadata = usize>,
        <<<R as ReprC>::CType as Wide>::Header as WideHeader>::Data: CType,
    {
        type SoftStore = ();

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
            let len = R::metadata(core::ptr::from_ref(&*self));
            let data = R::into_non_null(self).cast();
            unsafe { Self::CType::from_raw_parts(data, len) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: NulTerminatedBuf<Header: ReprC + CType> + ?Sized> EncodeMoveUnchecked for Box<R>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType: FromBox<<<R as NulTerminatedBuf>::Header as ReprC>::CType>>,
        R: RustSpec<Layout = Unstable, Size = NulTerminated>,
        <R as NulTerminatedBuf>::Header: RustSpec<Layout = Stable>,
        <<R as NulTerminatedBuf>::Header as ReprC>::CType: Sized,
    {
        type SoftStore = ();

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
            let ptr = R::into_non_null(self).cast();
            unsafe { Self::CType::from_non_null(ptr) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: EncodeMoveUnchecked, S: SizedKind> EncodeMoveUnchecked for Box<R>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType: FromBox<<R as ReprC>::CType>>,
        R: RustSpec<Layout = Unstable, Size = RustSpecSized<S>>,
    {
        type SoftStore = Box<R::SoftStore>;

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            let source = Box::new(unsafe { (*self).soft_encode_unchecked(store) });
            unsafe { Self::CType::from_non_null(Box::into_non_null(source)) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: Owned<Owned: EncodeMoveUnchecked> + ?Sized> EncodeMoveUnchecked for Box<R>
    where
        Self: RustSpec<Layout = Unstable> + Into<<R as Owned>::Owned>,
        R: RustSpec<Layout = Unstable, Size: Dst>,
        Self: ReprC<CType = <<R as Owned>::Owned as ReprC>::CType>,
    {
        type SoftStore = <R::Owned as EncodeMoveUnchecked>::SoftStore;

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            unsafe { self.into().soft_encode_unchecked(store) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: AbiCompatible + ReprC> EncodeMoveUnchecked for Vec<R>
    where
        R: RustSpec<Layout = Stable>,
        Self: ReprC<CType: Sized>,
        Box<[R]>: EncodeMoveUnchecked<CType = <Self as ReprC>::CType, SoftStore = ()>,
    {
        type SoftStore = ();

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, store: &mut ()) -> Self::CType {
            unsafe { self.into_boxed_slice().soft_encode_unchecked(store) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: EncodeMoveUnchecked> EncodeMoveUnchecked for Vec<R>
    where
        R: RustSpec<Layout = Unstable>,
        Self: ReprC<CType: FromWideBox<[<R as ReprC>::CType]>>,
    {
        type SoftStore = Box<[R::SoftStore]>;

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            let source = unsafe { encode_vec_elements_unchecked(self, store) };
            let len = source.len();
            let data = Box::into_non_null(source).cast();
            unsafe { Self::CType::from_raw_parts(data, len) }
        }
    }

    unsafe impl<R: EncodeMoveUnchecked> EncodeMoveUnchecked for Option<R>
    where
        R: RustSpec<Niche = WithoutNiche>,
    {
        type SoftStore = Option<R::SoftStore>;

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            match self {
                Some(value) => COption::Some(unsafe {
                    value.soft_encode_unchecked(store.insert(Default::default()))
                }),
                None => {
                    *store = None;
                    COption::None()
                }
            }
        }
    }

    unsafe impl<R: EncodeMoveUnchecked + Niche, N: NicheStabilityKind> EncodeMoveUnchecked
        for Option<R>
    where
        R: RustSpec<Niche = WithNiche<N>>,
    {
        type SoftStore = Option<R::SoftStore>;

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            match self {
                Some(value) => unsafe { value.soft_encode_unchecked(store.insert(Default::default())) },
                None => {
                    *store = None;
                    R::NICHE
                }
            }
        }
    }

    unsafe impl<R: EncodeMoveUnchecked<CType: Copy>, E: EncodeMoveUnchecked<CType: Copy>, K: SizedKind> EncodeMoveUnchecked
        for Result<R, E>
    where
        Self: ReprC<CType = CResult<<R as ReprC>::CType, <E as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<K>>,
        E: RustSpec<Size = RustSpecSized<K>>,
    {
        type SoftStore = Option<Result<R::SoftStore, E::SoftStore>>;

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            match self {
                Ok(ok) => {
                    let Result::Ok(inner) = store.insert(Ok(Default::default())) else { unreachable!() };
                    CResult::Ok(unsafe { ok.soft_encode_unchecked(inner) })
                }
                Err(err) => {
                    let Result::Err(inner) = store.insert(Err(Default::default())) else { unreachable!() };
                    CResult::Err(unsafe { err.soft_encode_unchecked(inner) })
                }
            }
        }
    }

    unsafe impl<R: EncodeMoveUnchecked + Niche, E: EncodeMoveUnchecked<CType: Copy>, N: NicheStabilityKind>
        EncodeMoveUnchecked for Result<R, E>
    where
        Self: ReprC<CType = CResult<<R as ReprC>::CType, <E as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = RustSpecSized<Zero>, Alignment = rust_spec::Gt<One>>,
    {
        type SoftStore = Option<Result<R::SoftStore, E::SoftStore>>;

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            match self {
                Ok(ok) => {
                    let Result::Ok(inner) = store.insert(Ok(Default::default())) else { unreachable!() };
                    CResult::Ok(unsafe { ok.soft_encode_unchecked(inner) })
                }
                Err(err) => {
                    let Result::Err(inner) = store.insert(Err(Default::default())) else { unreachable!() };
                    CResult::Err(unsafe { err.soft_encode_unchecked(inner) })
                }
            }
        }
    }

    unsafe impl<R: EncodeMoveUnchecked<CType: Copy>, E: EncodeMoveUnchecked<CType: Copy> + Niche, N: NicheStabilityKind>
        EncodeMoveUnchecked for Result<R, E>
    where
        Self: ReprC<CType = CResult<<R as ReprC>::CType, <E as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<Zero>, Alignment = rust_spec::Gt<One>>,
        E: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
    {
        type SoftStore = Option<Result<R::SoftStore, E::SoftStore>>;

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            match self {
                Ok(ok) => {
                    let Result::Ok(inner) = store.insert(Ok(Default::default())) else { unreachable!() };
                    CResult::Ok(unsafe { ok.soft_encode_unchecked(inner) })
                }
                Err(err) => {
                    let Result::Err(inner) = store.insert(Err(Default::default())) else { unreachable!() };
                    CResult::Err(unsafe { err.soft_encode_unchecked(inner) })
                }
            }
        }
    }

    unsafe impl<R: EncodeMoveUnchecked + Niche, E: ReprC, N: NicheStabilityKind> EncodeMoveUnchecked
        for Result<R, E>
    where
        Self: ReprC<CType = <R as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = RustSpecSized<Zero>, Alignment = One>,
    {
        type SoftStore = Option<R::SoftStore>;

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            match self {
                Ok(ok) => unsafe { ok.soft_encode_unchecked(store.insert(Default::default())) },
                Err(_) => {
                    *store = None;
                    R::NICHE
                }
            }
        }
    }

    unsafe impl<R: ReprC, E: EncodeMoveUnchecked + Niche, N: NicheStabilityKind> EncodeMoveUnchecked
        for Result<R, E>
    where
        Self: ReprC<CType = <E as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<Zero>, Alignment = One>,
        E: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
    {
        type SoftStore = Option<E::SoftStore>;

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            match self {
                Ok(_) => {
                    *store = None;
                    E::NICHE
                }
                Err(err) => unsafe { err.soft_encode_unchecked(store.insert(Default::default())) },
            }
        }
    }

    unsafe impl<R> EncodeMoveUnchecked for ManuallyDrop<R>
    where R: RustSpec<Layout = Stable>, Self: AbiCompatible<CType: Sized>,
    {
        type SoftStore = ();
        unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
            unsafe { core::mem::transmute_copy(&self) }
        }
    }
    unsafe impl<R: EncodeMoveUnchecked> EncodeMoveUnchecked for ManuallyDrop<R>
    where
        R: RustSpec<Layout = Unstable, Drop = NoDrop>,
    {
        type SoftStore = R::SoftStore;

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            unsafe { Self::into_inner(self).soft_encode_unchecked(store) }
        }
    }
    unsafe impl<R: EncodeMoveUnchecked> EncodeMoveUnchecked for ManuallyDrop<R>
    where
        R: RustSpec<Layout = Unstable, Drop: WithDrop>,
    {
        type SoftStore = R::SoftStore;

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            ManuallyDrop::new(unsafe { Self::into_inner(self).soft_encode_unchecked(store) })
        }
    }
}

disjoint_impls! {
    /// Encodes a value by consuming `Self`, including conversions that rebuild
    /// owned allocations with unstable element layouts.
    ///
    /// `Encode` is the narrower marker for values whose allocation can cross
    /// the FFI boundary in `move` mode without rebuilding it.
    ///
    /// # Safety
    ///
    /// If [`Self::Store`] implements [`EmptyStore`], [`EncodeMove::soft_encode`] must not
    /// return references into the store. [`crate::soft_encode`] correctness relies on it.
    pub unsafe trait EncodeMove: EncodeMoveUnchecked {
        /// Auxiliary storage used during conversion. If storage is not used, set the type to `()`.
        ///
        /// Use cases include:
        /// - Keeping the result of the conversion of references to stored types
        /// - Storing mutable references that need to be updated in [`Store::sync`]
        ///
        /// Conceptually, serves a role similar to the "context" captured by a closure.
        type Store: Store + Default;

        /// Convert from [`Self`] into its [`ReprC::CType`].
        ///
        /// Prefer using [`crate::soft_encode`] whenever possible
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm;
    }

    unsafe impl<R: AbiCompatible + ?Sized> EncodeMove for &R
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType = CRef<<R as ReprC>::CType>>,
        R: RustSpec<Layout = Stable, Size: Thin, Mutability = Exclusive>,
    {
        type Store = ();

        #[inline(always)]
        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = unsafe { self.soft_encode_unchecked(&mut ()) };
            debug_assert_encoded_valid::<Self>(&encoded);
            encoded
        }
    }
    unsafe impl<R: AbiCompatible + Wide<Metadata = usize> + ?Sized> EncodeMove for &R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = Exclusive>,
        <R as ReprC>::CType: Wide<Header: WideHeader<Data: CType>, Metadata = usize>,
    {
        type Store = ();

        #[inline(always)]
        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = unsafe { self.soft_encode_unchecked(&mut ()) };
            debug_assert_wide_pointee_valid::<R>(encoded.data(), encoded.metadata());
            encoded
        }
    }

    unsafe impl<'a, R: EncodeRef<'a> + ?Sized> EncodeMove for &'a R
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType: Sized>,
        R: RustSpec<Layout = Unstable, Mutability = Exclusive>,
    {
        type Store = R::Store;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            self.encode_ref(store)
        }
    }

    unsafe impl<R: AbiCompatible + ?Sized> EncodeMove for &R
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType = CRefMut<<R as ReprC>::CType, false>>,
        R: RustSpec<Layout = Stable, Size: Thin, Mutability = WithInterior, Trap = Robust>,
    {
        type Store = ();

        #[inline(always)]
        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = unsafe { self.soft_encode_unchecked(&mut ()) };
            debug_assert_encoded_valid::<Self>(&encoded);
            encoded
        }
    }
    unsafe impl<R: AbiCompatible + Wide<Metadata = usize> + ?Sized> EncodeMove for &R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = WithInterior, Trap = Robust>,
        <R as ReprC>::CType: Wide<Header: WideHeader<Data: CType>, Metadata = usize>,
    {
        type Store = ();

        #[inline(always)]
        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = unsafe { self.soft_encode_unchecked(&mut ()) };
            debug_assert_wide_pointee_valid::<R>(encoded.data(), encoded.metadata());
            encoded
        }
    }

    unsafe impl<R: AbiCompatible + ?Sized> EncodeMove for &mut R
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType = CRefMut<<R as ReprC>::CType, true>>,
        R: RustSpec<Layout = Stable, Size: Thin, Trap = Robust>,
    {
        type Store = ();

        #[inline(always)]
        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = unsafe { self.soft_encode_unchecked(&mut ()) };
            debug_assert_encoded_valid::<Self>(&encoded);
            encoded
        }
    }
    unsafe impl<'a, R: AbiCompatible + EncodeMut<'a>> EncodeMove for &'a mut R
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType = CRefMut<<R as ReprC>::CType, true>>,
        R: RustSpec<Layout = Stable, Size: Thin, Trap = NonRobust>,
    {
        type Store = R::Store;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = self.encode_mut(store);
            debug_assert_encoded_valid::<Self>(&encoded);
            encoded
        }
    }
    unsafe impl<R: AbiCompatible + Wide<Metadata = usize> + ?Sized> EncodeMove for &mut R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Trap = Robust>,
        <R as ReprC>::CType: Wide<Header: WideHeader<Data: CType>, Metadata = usize>,
    {
        type Store = ();

        #[inline(always)]
        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = unsafe { self.soft_encode_unchecked(&mut ()) };
            debug_assert_wide_pointee_valid::<R>(encoded.data(), encoded.metadata());
            encoded
        }
    }
    // TODO: This looks like a mess of bounds
    #[cfg(feature = "alloc")]
    unsafe impl<'a, R: AbiCompatible + Wide<Metadata = usize> + AssignFromOwned + ?Sized> EncodeMove
        for &'a mut R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Trap = NonRobust>,
        Self: ReprC<CType = <<<R as ToOwned>::Owned as ReprC>::CType as BorrowCastMut>::AsMut>,
        R: ToOwned<Owned: EncodeMove<CType: BorrowCastMut<AsMut: Copy> + Copy> + DecodeOwned<'a>>,
        <R as ReprC>::CType: Wide<Header: WideHeader<Data: CType>, Metadata = usize>,
        <<R as ToOwned>::Owned as DecodeOwned<'a>>::Store: EmptyStore + 'a,
        <<R as ToOwned>::Owned as EncodeMove>::Store: EmptyStore,
    {
        type Store = RefMutDstEncodeStore<'a, R, <R::Owned as EncodeMove>::Store>;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            encode_ref_mut_dst(self, store)
        }
    }

    unsafe impl<'a, R: EncodeMut<'a> + ?Sized> EncodeMove for &'a mut R
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType: Sized>,
        R: RustSpec<Layout = Unstable>,
    {
        type Store = R::Store;
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where Self: 'itm,
        {
            self.encode_mut(store)
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: AbiCompatible + EncodeMove, S: SizedKind> EncodeMove for Box<R>
    where
        Self: RustSpec<Layout = Stable>,
        R: RustSpec<Layout = Stable, Size = RustSpecSized<S>, Trap = Robust>,
        Self: AbiCompatible<CType: FromBox<<R as ReprC>::CType>>,
    {
        type Store = ();

        #[inline(always)]
        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = unsafe { self.soft_encode_unchecked(&mut ()) };
            debug_assert_encoded_valid::<Self>(&encoded);
            encoded
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: AbiCompatible + EncodeMove, S: SizedKind> EncodeMove for Box<R>
    where
        Self: RustSpec<Layout = Stable>,
        R: RustSpec<Layout = Stable, Size = RustSpecSized<S>, Trap = NonRobust>,
        Self: AbiCompatible<CType: FromBox<<R as ReprC>::CType>>,
    {
        type Store = Box<R::Store>;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            // FIXME: This doesn't work
            if impls::impls!(R::Store: EmptyStore) {
                let encoded = unsafe { self.soft_encode_unchecked(&mut ()) };
                debug_assert_encoded_valid::<Self>(&encoded);
                return encoded;
            }

            let source = Box::new((*self).soft_encode(store));
            unsafe { Self::CType::from_non_null(Box::into_non_null(source)) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: AbiCompatible + Wide<Metadata = usize> + ?Sized> EncodeMove for Box<R>
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Trap = Robust>,
        Self: ReprC<CType: FromWideBox<<R as ReprC>::CType>>,
        <R as ReprC>::CType: Wide<Header: WideHeader<Data: CType>, Metadata = usize>,
    {
        type Store = ();

        #[inline(always)]
        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = unsafe { self.soft_encode_unchecked(&mut ()) };
            debug_assert_wide_pointee_valid::<R>(encoded.data(), encoded.metadata());
            encoded
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: AbiCompatible + Wide<Metadata = usize> + Owned + ?Sized> EncodeMove for Box<R>
    where
        Self: RustSpec<Layout = Unstable> + Into<<R as Owned>::Owned>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Trap = NonRobust>,
        <R as ReprC>::CType: Wide<Header: WideHeader<Data: CType>, Metadata = usize>,
        <R as Owned>::Owned: EncodeMove<CType = <Self as ReprC>::CType>,
        Self: ReprC<CType: FromWideBox<<R as ReprC>::CType>>,
    {
        type Store = <R::Owned as EncodeMove>::Store;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            // FIXME: This doesn't work
            if impls::impls!(<R::Owned as EncodeMove>::Store: EmptyStore) {
                let encoded = unsafe { self.soft_encode_unchecked(&mut ()) };
                debug_assert_wide_pointee_valid::<R>(encoded.data(), encoded.metadata());
                return encoded;
            }

            encode_box_owned::<R>(self, store)
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: EncodeMove, S: SizedKind> EncodeMove for Box<R>
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = RustSpecSized<S>>,
        Self: ReprC<CType: FromBox<<R as ReprC>::CType>>,
    {
        type Store = Box<R::Store>;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let source = Box::new((*self).soft_encode(store));
            unsafe { Self::CType::from_non_null(Box::into_non_null(source)) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: NulTerminatedBuf<Header: ReprC + CType> + ?Sized> EncodeMove for Box<R>
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = NulTerminated>,
        <R as NulTerminatedBuf>::Header: RustSpec<Layout = Stable>,
        <<R as NulTerminatedBuf>::Header as ReprC>::CType: Sized,
        Self: ReprC<CType: FromBox<<<R as NulTerminatedBuf>::Header as ReprC>::CType>>,
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

    #[cfg(feature = "alloc")]
    unsafe impl<R: Owned<Owned: EncodeMove> + ?Sized> EncodeMove for Box<R>
    where
        Self: RustSpec<Layout = Unstable> + Into<<R as Owned>::Owned>,
        R: RustSpec<Layout = Unstable, Size: Dst>,
        Self: ReprC<CType = <<R as Owned>::Owned as ReprC>::CType>,
    {
        type Store = <R::Owned as EncodeMove>::Store;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            encode_box_owned::<R>(self, store)
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: AbiCompatible + EncodeMove> EncodeMove for Vec<R>
    where
        R: RustSpec<Layout = Stable, Trap = Robust>,
        Self: ReprC<CType: Sized>,
        Box<[R]>: EncodeMove<CType = <Self as ReprC>::CType, Store = ()>
            + EncodeMoveUnchecked<SoftStore = ()>,
    {
        type Store = ();

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            self.into_boxed_slice().soft_encode(store)
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: AbiCompatible + EncodeMove> EncodeMove for Vec<R>
    where
        R: RustSpec<Layout = Stable, Trap = NonRobust>,
        Self: ReprC<CType: FromWideBox<[<R as ReprC>::CType]>>,
        Box<[R]>: EncodeMoveUnchecked<CType = <Self as ReprC>::CType, SoftStore = ()>,
    {
        type Store = Box<[R::Store]>;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            // TODO: This path can be further optimized like for Box
            let source = encode_vec_elements(self, store);
            let len = source.len();
            let data = Box::into_non_null(source).cast();
            unsafe { Self::CType::from_raw_parts(data, len) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: EncodeMove> EncodeMove for Vec<R>
    where
        R: RustSpec<Layout = Unstable>,
        Self: ReprC<CType: FromWideBox<[<R as ReprC>::CType]>>,
    {
        type Store = Box<[R::Store]>;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let source = encode_vec_elements(self, store);
            let len = source.len();
            let data = Box::into_non_null(source).cast();
            unsafe { Self::CType::from_raw_parts(data, len) }
        }
    }

    unsafe impl<R: EncodeMove> EncodeMove for Option<R>
    where
        R: RustSpec<Niche = WithoutNiche>,
    {
        type Store = Option<R::Store>;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            match self {
                Some(value) => COption::Some(value.soft_encode(store.insert(Default::default()))),
                None => {
                    *store = None;
                    COption::None()
                }
            }
        }
    }
    // NOTE: PartialEq is only required for decoding, Here it is only used by `debug_assert!`
    unsafe impl<R: EncodeMove + Niche<CType: PartialEq>, N: NicheStabilityKind> EncodeMove
        for Option<R>
    where
        R: RustSpec<Niche = WithNiche<N>>,
    {
        type Store = Option<R::Store>;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            if let Some(value) = self {
                let encoded = value.soft_encode(store.insert(Default::default()));
                debug_assert!(encoded != R::NICHE, "encoding produced NICHE value");
                return encoded;
            }

            *store = None;
            R::NICHE
        }
    }

    unsafe impl<R: EncodeMove<CType: Copy>, E: EncodeMove<CType: Copy>, K: SizedKind> EncodeMove
        for Result<R, E>
    where
        Self: ReprC<CType = CResult<<R as ReprC>::CType, <E as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<K>>,
        E: RustSpec<Size = RustSpecSized<K>>,
    {
        type Store = Option<Result<R::Store, E::Store>>;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            encode_result(self, store)
        }
    }
    unsafe impl<R: EncodeMove + Niche, E: EncodeMove<CType: Copy>, N: NicheStabilityKind> EncodeMove
        for Result<R, E>
    where
        Self: ReprC<CType = CResult<<R as ReprC>::CType, <E as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = RustSpecSized<Zero>, Alignment = rust_spec::Gt<One>>,
    {
        type Store = Option<Result<R::Store, E::Store>>;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            encode_result(self, store)
        }
    }
    unsafe impl<R: EncodeMove<CType: Copy>, E: EncodeMove<CType: Copy> + Niche, N: NicheStabilityKind>
        EncodeMove for Result<R, E>
    where
        Self: ReprC<CType = CResult<<R as ReprC>::CType, <E as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<Zero>, Alignment = rust_spec::Gt<One>>,
        E: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
    {
        type Store = Option<Result<R::Store, E::Store>>;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            encode_result(self, store)
        }
    }
    unsafe impl<R: EncodeMove + Niche, E: ReprC, N: NicheStabilityKind> EncodeMove for Result<R, E>
    where
        Self: ReprC<CType = <R as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = RustSpecSized<Zero>, Alignment = One>,
    {
        type Store = Option<R::Store>;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            match self {
                Ok(ok) => ok.soft_encode(store.insert(Default::default())),
                Err(_) => R::NICHE,
            }
        }
    }
    unsafe impl<R: ReprC, E: EncodeMove + Niche, N: NicheStabilityKind> EncodeMove for Result<R, E>
    where
        Self: ReprC<CType = <E as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<Zero>, Alignment = One>,
        E: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
    {
        type Store = Option<E::Store>;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            match self {
                Ok(_) => E::NICHE,
                Err(err) => err.soft_encode(store.insert(Default::default())),
            }
        }
    }

    unsafe impl<R> EncodeMove for ManuallyDrop<R>
    where R: RustSpec<Layout = Stable>, Self: AbiCompatible<CType: Sized>,
    {
        type Store = ();
        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType where Self: 'itm {
            unsafe { self.soft_encode_unchecked(&mut ()) }
        }
    }
    unsafe impl<R: EncodeMove> EncodeMove for ManuallyDrop<R>
    where
        R: RustSpec<Layout = Unstable, Drop = NoDrop>,
    {
        type Store = R::Store;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            Self::into_inner(self).soft_encode(store)
        }
    }

    unsafe impl<R: EncodeMove> EncodeMove for ManuallyDrop<R>
    where
        R: RustSpec<Layout = Unstable, Drop: WithDrop>,
    {
        type Store = R::Store;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            ManuallyDrop::new(Self::into_inner(self).soft_encode(store))
        }
    }
}

disjoint_impls! {
    /// Perform the conversion from a [`ReprC::CType`] into [`Self`].
    ///
    /// # Safety
    ///
    /// If [`Self::Store`] implements [`EmptyStore`], [`DecodeOwned::soft_decode`] must not
    /// return references into the store. [`crate::soft_decode`] correctness relies on it.
    pub unsafe trait DecodeOwned<'d>: ReprC<CType: Sized> + Sized {
        type Store: Store + Default;

        /// Perform the conversion from an [`ReprC::CType`] into [`Self`].
        ///
        /// Prefer using [`crate::soft_decode`] whenever possible
        ///
        /// # Safety
        ///
        /// See the safety section in [`crate::soft_decode`].
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self>;

        /// Perform the unchecked conversion from a [`ReprC::CType`] into [`Self`].
        ///
        /// The default implementation calls `soft_decode` and panics on `None`. Override it to
        /// skip validation for your type
        ///
        /// # Safety
        ///
        /// See the safety section in [`crate::soft_decode_unchecked`].
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            unsafe { Self::soft_decode(source, store) }.unwrap()
        }
    }

    unsafe impl<'d, R: AbiCompatible + ?Sized> DecodeOwned<'d> for &'d R
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType = CRef<<R as ReprC>::CType>>,
        R: RustSpec<Layout = Stable, Size: Thin, Mutability = Exclusive>,
    {
        type Store = ();

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut ()) -> Option<Self> {
            if !unsafe { Self::is_valid(&source) } {
                return None;
            }

            Some(unsafe { Self::soft_decode_unchecked(source, store) })
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            let ptr = source.as_ptr();

            // TODO: Hack before `Thin` trait is available in stable:
            // https://doc.rust-lang.org/std/ptr/traitalias.Thin.html
            // https://github.com/rust-lang/rust/issues/81513
            unsafe { &*core::mem::transmute_copy::<*const R::CType, *const R>(&ptr) }
        }
    }
    unsafe impl<'d, R: AbiCompatible<CType: Wide<Metadata = usize>> + ?Sized>
        DecodeOwned<'d> for &'d R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = Exclusive>,
        WideData<<R as ReprC>::CType>: CType,
        R: Wide<Metadata = usize>,
    {
        type Store = ();

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut ()) -> Option<Self> {
            let ctype = unsafe { source.into_ref()? };

            if unsafe { !R::is_valid(ctype) } {
                return None;
            }

            Some(unsafe { Self::soft_decode_unchecked(source, store) })
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            unsafe {
                R::from_raw_parts(source.data().cast::<R::Header>(), source.metadata())
            }
        }
    }

    unsafe impl<'d, R: NulTerminatedBuf + ?Sized> DecodeOwned<'d> for &'d R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = NulTerminated, Mutability = Exclusive>
            + ReprC<CType = <<R as NulTerminatedBuf>::Header as ReprC>::CType>,
        <R as ReprC>::CType: CType + Sized,
        <R as NulTerminatedBuf>::Header: RustSpec<Layout = Stable> + ReprC<CType: Sized> + CType,
    {
        type Store = ();

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut ()) -> Option<Self> {
            if source.as_ptr().is_null() {
                return None;
            }

            Some(unsafe { Self::soft_decode_unchecked(source, store) })
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            let ptr = source.as_ptr().cast();
            unsafe { R::from_raw(ptr) }
        }
    }
    unsafe impl<'d, R: ReprC<CType: BorrowCast<AsConst: Copy> + Copy> + FromBorrow<'d>, S: SizedKind>
        DecodeOwned<'d> for &'d R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = RustSpecSized<S>, Mutability = Exclusive>,
        <R as Borrow>::Borrowed<'d>: DecodeOwned<'d, CType = <<R as ReprC>::CType as BorrowCast>::AsConst>,
    {
        type Store = RefSizedDecodeStore<R, <R::Borrowed<'d> as DecodeOwned<'d>>::Store>;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            let ptr = source.as_ptr();
            unsafe { decode_ref_sized(ptr, store) }
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
            let ptr = source.as_ptr();
            let source = borrow_cast(unsafe { ptr.read() });
            store.decode(|fields| {
                let value = unsafe { DecodeOwned::soft_decode_unchecked(source, fields) };
                Some(FromBorrow::from_borrow(value))
            }).expect("unchecked decoding produces a value")
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: DecodeOwned<'d, CType: BorrowCast<AsConst: Copy> + Copy> + FromBorrow<'d> + Clone>
        DecodeOwned<'d> for &'d [R]
    where
        Self: RustSpec<Layout = Unstable>,
        [R]: RustSpec<Layout = Unstable, Size = MetaSized<SliceLike>, Mutability = Exclusive>,
        <R as Borrow>::Borrowed<'d>: DecodeOwned<'d, CType = <<R as ReprC>::CType as BorrowCast>::AsConst>,
    {
        type Store = RefDstDecodeStore<[R], Box<[<R::Borrowed<'d> as DecodeOwned<'d>>::Store]>>;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            store.decode(|fields| {
                let source = unsafe { source.into_ref()? };
                let values = source.iter().map(|value| unsafe { core::ptr::read(value) });
                unsafe { decode_ref_slice_elements(values, fields) }
            }).map(Vec::as_slice)
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
            store.decode(|fields| {
                let source = unsafe { source.into_ref().unwrap_unchecked() };
                Some(unsafe {
                    decode_ref_slice_elements_unchecked(
                        source.iter().map(|value| core::ptr::read(value)),
                        fields,
                    )
                })
            }).expect("unchecked decoding produces a value").as_slice()
        }
    }

    unsafe impl<'d, R: AbiCompatible + ?Sized> DecodeOwned<'d> for &'d R
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType = CRefMut<<R as ReprC>::CType, false>>,
        R: RustSpec<Layout = Stable, Size: Thin, Mutability = WithInterior, Trap = Robust>,
    {
        type Store = ();

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut ()) -> Option<Self> {
            if !unsafe { Self::is_valid(&source) } {
                return None;
            }

            Some(unsafe { Self::soft_decode_unchecked(source, store) })
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            let ptr = source.as_ptr().cast_const();

            // TODO: Hack before `Thin` trait is available in stable:
            // https://doc.rust-lang.org/std/ptr/traitalias.Thin.html
            // https://github.com/rust-lang/rust/issues/81513
            unsafe { &*core::mem::transmute_copy::<*const R::CType, *const R>(&ptr) }
        }
    }
    unsafe impl<'d, R: AbiCompatible + Wide<Metadata = usize> + ?Sized> DecodeOwned<'d> for &'d R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = WithInterior, Trap = Robust>,
        <R as ReprC>::CType: Wide<Header: WideHeader<Data: CType>, Metadata = usize>,
    {
        type Store = ();

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut ()) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            Some(unsafe { Self::soft_decode_unchecked(source, store) })
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            unsafe { R::from_raw_parts(source.data().cast::<R::Header>(), source.metadata()) }
        }
    }
    // TODO:
    //unsafe impl<'d, R: NulTerminatedBuf + ?Sized> DecodeOwned<'d> for &'d R
    //where
    //    Self: RustSpec<Layout = Unstable>,
    //    R: RustSpec<Layout = Unstable, Size = NulTerminated, Mutability = WithInterior>,
    //    <R as NulTerminatedBuf>::Header: RustSpec<Layout = Stable> + ReprC<CType: Sized> + CType,
    //{
    //    type Store = ();

    //    #[inline(always)]
    //    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut ()) -> Option<Self> {
    //        if source.as_ptr().is_null() {
    //            return None;
    //        }

    //        Some(unsafe { Self::soft_decode_unchecked(source, store) })
    //    }

    //    #[inline(always)]
    //    unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
    //        let ptr = source.as_ptr().cast();
    //        unsafe { R::from_raw(ptr) }
    //    }
    //}

    unsafe impl<'d, R: AbiCompatible + RustSpec<Size: Thin> + ?Sized> DecodeOwned<'d> for &'d mut R
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType = CRefMut<<R as ReprC>::CType, true>>,
    {
        type Store = ();

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut ()) -> Option<Self> {
            if !unsafe { Self::is_valid(&source) } {
                return None;
            }

            Some(unsafe { Self::soft_decode_unchecked(source, store) })
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            let source = source.as_ptr();

            // TODO: Hack before `Thin` trait is available in stable:
            // https://doc.rust-lang.org/std/ptr/traitalias.Thin.html
            // https://github.com/rust-lang/rust/issues/81513
            unsafe { &mut *core::mem::transmute_copy::<*mut R::CType, *mut R>(&source) }
        }
    }
    unsafe impl<'d, R: AbiCompatible<CType: Wide<Metadata = usize>> + ?Sized> DecodeOwned<'d> for &'d mut R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>>,
        WideData<<R as ReprC>::CType>: CType,
        R: Wide<Metadata = usize>,
    {
        type Store = ();

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut ()) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            let ctype_slice = unsafe {
                R::CType::from_raw_parts(
                    source.data().cast::<<R::CType as Wide>::Header>(),
                    source.metadata(),
                )
            };

            if unsafe { !R::is_valid(ctype_slice) } {
                return None;
            }

            Some(unsafe { Self::soft_decode_unchecked(source, store) })
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            unsafe {
                R::from_raw_parts_mut(source.data().cast::<R::Header>(), source.metadata())
            }
        }
    }
    // TODO: NulTerminatedBuf needs a sound mutable constructor (from_raw_mut)
    // before this can be enabled; from_raw only creates a shared reference.
    // unsafe impl<'d, R: NulTerminatedBuf + ?Sized> DecodeOwned<'d> for &'d mut R
    // where
    //     R: RustSpec<Size = NulTerminated>,
    //     <R::Header as AbiCompatible as ReprC>::CType: Sized>,
    // {
    //     type Store = ();
    //
    //     unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
    //         let ptr = source.as_ptr();
    //         if ptr.is_null() {
    //             return None;
    //         }
    //         Some(unsafe { R::from_raw_mut(ptr.cast()) })
    //     }
    //
    //     unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
    //         unsafe { R::from_raw_mut(source.as_ptr().cast()) }
    //     }
    // }
    unsafe impl<'d, R: ReprC<CType: BorrowCast<AsConst: Copy> + Copy> + FromBorrow<'d>, S: SizedKind>
        DecodeOwned<'d> for &'d mut R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = RustSpecSized<S>> + EncodeMove<Store: EmptyStore>,
        <R as Borrow>::Borrowed<'d>: DecodeOwned<'d, CType = <<R as ReprC>::CType as BorrowCast>::AsConst>,
    {
        type Store = RefMutSizedDecodeStore<R, <R::Borrowed<'d> as DecodeOwned<'d>>::Store>;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = source.as_ptr();
            unsafe {
                store.decode(source, |fields| {
                    let source = borrow_cast(source.read());
                    let value = DecodeOwned::soft_decode(source, fields)?;
                    Some(FromBorrow::from_borrow(value))
                })
            }
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let source = source.as_ptr();
            unsafe {
                store.decode(source, |fields| {
                    let source = borrow_cast(source.read());
                    let value = DecodeOwned::soft_decode_unchecked(source, fields);
                    Some(FromBorrow::from_borrow(value))
                })
            }.expect("unchecked decoding produces a value")
        }
    }
    #[cfg(feature = "alloc")]
    // FIXME: This is implemented only for the slice
    unsafe impl<'d, R: DecodeOwned<'d, CType: BorrowCast<AsConst: Copy> + Copy> + FromBorrow<'d> + EncodeMove<Store: EmptyStore>>
        DecodeOwned<'d> for &'d mut [R]
    where
        Self: RustSpec<Layout = Unstable>,
        [R]: RustSpec<Layout = Unstable, Size = MetaSized<SliceLike>>,
        <R as Borrow>::Borrowed<'d>: DecodeOwned<'d, CType = <<R as ReprC>::CType as BorrowCast>::AsConst>,
    {
        type Store = RefMutSliceDecodeStore<R, Box<[<R::Borrowed<'d> as DecodeOwned<'d>>::Store]>>;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe {
                store.decode(source, |values, fields| {
                    decode_ref_slice_elements(values.iter().copied(), fields)
                })
            }
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            unsafe {
                store.decode(source, |values, fields| {
                    Some(decode_ref_slice_elements_unchecked(values.iter().copied(), fields))
                })
            }.expect("unchecked decoding produces a value")
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: AbiCompatible, S: SizedKind> DecodeOwned<'d> for Box<R>
    where
        Self: RustSpec<Layout = Stable>,
        R: RustSpec<Layout = Stable, Size = RustSpecSized<S>>,
        Self: AbiCompatible<CType: FromBox<<R as ReprC>::CType>>,
        <R as ReprC>::CType: Sized,
    {
        type Store = ();

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if !unsafe { Self::is_valid(&source) } {
                return None;
            }

            Some(unsafe { Self::soft_decode_unchecked(source, &mut ()) })
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            unsafe { Box::from_raw(borrow_cast_mut(source).as_ptr().cast()) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: NulTerminatedBuf<Header: ReprC + CType> + ?Sized> DecodeOwned<'d> for Box<R>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType: FromBox<<<R as NulTerminatedBuf>::Header as ReprC>::CType>>,
        R: RustSpec<Layout = Unstable, Size = NulTerminated>,
        <R as NulTerminatedBuf>::Header: RustSpec<Layout = Stable> + Sized,
        <<R as NulTerminatedBuf>::Header as ReprC>::CType: Sized,
    {
        type Store = ();

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut ()) -> Option<Self> {
            NonNull::new(borrow_cast_mut(source).as_ptr())?;
            Some(unsafe { Self::soft_decode_unchecked(source, store) })
        }
        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            let ptr = unsafe { NonNull::new_unchecked(borrow_cast_mut(source).as_ptr()) } ;
            unsafe { R::from_non_null(ptr.cast()) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: AbiCompatible<CType: Wide<Metadata = usize>> + ?Sized>
        DecodeOwned<'d> for Box<R>
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>>,
        WideData<R>: ReprC<CType = WideData<<R as ReprC>::CType>>,
        Self: ReprC<CType: FromWideBox<<R as ReprC>::CType>>,
        <WideData<R> as ReprC>::CType: CType,
        R: Wide<Metadata = usize>,
    {
        type Store = ();

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if source.data().is_null() {
                return None;
            }

            let len = source.metadata();
            let data = source.data();

            if unsafe {
                !R::is_valid(R::CType::from_raw_parts(
                    data.cast::<<R::CType as Wide>::Header>(),
                    len,
                ))
            } {
                let ptr = unsafe { NonNull::new_unchecked(data) };
                drop(unsafe {
                    R::CType::from_non_null(ptr.cast::<<R::CType as Wide>::Header>(), len)
                });
                return None;
            }

            Some(unsafe { Self::soft_decode_unchecked(source, &mut ()) })
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            let len = source.metadata();
            let data = source.data().cast();

            unsafe { R::from_non_null(NonNull::new_unchecked(data), len) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: DecodeOwned<'d>, S: SizedKind> DecodeOwned<'d> for Box<R>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType: FromBox<<R as ReprC>::CType>>,
        R: RustSpec<Layout = Unstable, Size = RustSpecSized<S>>,
    {
        type Store = Box<R::Store>;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = unsafe { source.into_box()? };
            let value = unsafe { R::soft_decode(*source, store)? };
            Some(Box::new(value))
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let source = unsafe { source.into_box().unwrap_unchecked() };
            Box::new(unsafe { R::soft_decode_unchecked(*source, store) })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: Owned + ?Sized> DecodeOwned<'d> for Box<R>
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType = <<R as Owned>::Owned as ReprC>::CType>,
        R: RustSpec<Layout = Unstable, Size: Dst>,
        <R as Owned>::Owned: DecodeOwned<'d> + Into<Self>,
    {
        type Store = <R::Owned as DecodeOwned<'d>>::Store;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { decode_box_owned::<R>(source, store) }
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            unsafe { decode_box_owned_unchecked::<R>(source, store) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: AbiCompatible> DecodeOwned<'d> for Vec<R>
    where
        R: RustSpec<Layout = Stable>,
        Self: ReprC<CType: Sized>,
        Box<[R]>: DecodeOwned<'d, CType = <Self as ReprC>::CType, Store = ()>,
    {
        type Store = ();

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut ()) -> Option<Self> {
            unsafe { Box::<[R]>::soft_decode(source, store) }.map(|source| source.into_vec())
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut ()) -> Self {
            unsafe { Box::<[R]>::soft_decode_unchecked(source, store) }.into_vec()
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for Vec<R>
    where
        R: RustSpec<Layout = Unstable>,
        Self: ReprC<CType: BorrowCastMut<AsMut = CWideRefMut<[<R as ReprC>::CType], true>> + Sized>,
    {
        type Store = Box<[R::Store]>;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = borrow_cast_mut(source);
            let data = NonNull::new(source.as_ptr())?;
            let ptr = core::ptr::slice_from_raw_parts_mut(data.as_ptr(), source.metadata());
            unsafe { decode_vec_elements(Box::from_raw(ptr), store) }
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let source = borrow_cast_mut(source);
            let ptr = core::ptr::slice_from_raw_parts_mut(source.as_ptr(), source.metadata());
            unsafe { decode_vec_elements_unchecked(Box::from_raw(ptr), store) }
        }
    }

    unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for Option<R>
    where
        R: RustSpec<Niche = WithoutNiche>,
    {
        type Store = Option<R::Store>;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            match source.into_option()? {
                Some(source) => unsafe { R::soft_decode(source, store.insert(Default::default())) }.map(Some),
                None => {
                    *store = None;
                    Some(None)
                }
            }
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            match unsafe { source.into_option_unchecked() } {
                Some(source) => Some(unsafe { R::soft_decode_unchecked(source, store.insert(Default::default())) }),
                None => {
                    *store = None;
                    None
                }
            }
        }
    }
    unsafe impl<'d, R: DecodeOwned<'d> + Niche<CType: PartialEq>, N: NicheStabilityKind>
        DecodeOwned<'d> for Option<R>
    where
        R: RustSpec<Niche = WithNiche<N>>,
    {
        type Store = Option<<R as DecodeOwned<'d>>::Store>;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: R::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            if source == R::NICHE {
                *store = None;
                return Some(None);
            }

            unsafe { R::soft_decode(source, store.insert(Default::default())) }.map(Some)
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: R::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            if source == R::NICHE {
                *store = None;
                return None;
            }

            Some(unsafe { R::soft_decode_unchecked(source, store.insert(Default::default())) })
        }
    }

    unsafe impl<'d, R: DecodeOwned<'d, CType: Copy>, E: DecodeOwned<'d, CType: Copy>, K: SizedKind>
        DecodeOwned<'d> for Result<R, E>
    where
        Self: ReprC<CType = CResult<<R as ReprC>::CType, <E as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<K>>,
        E: RustSpec<Size = RustSpecSized<K>>,
    {
        type Store = Option<Result<R::Store, E::Store>>;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { decode_result(source, store) }
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            unsafe { decode_result_unchecked(source, store) }
        }
    }
    unsafe impl<'d, R: DecodeOwned<'d> + Niche, E: DecodeOwned<'d, CType: Copy>, N: NicheStabilityKind>
        DecodeOwned<'d> for Result<R, E>
    where
        Self: ReprC<CType = CResult<<R as ReprC>::CType, <E as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = RustSpecSized<Zero>, Alignment = rust_spec::Gt<One>>,
    {
        type Store = Option<Result<R::Store, E::Store>>;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { decode_result(source, store) }
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
            unsafe { decode_result_unchecked(source, store) }
        }
    }
    unsafe impl<'d, R: DecodeOwned<'d, CType: Copy>, E: DecodeOwned<'d> + Niche, N: NicheStabilityKind>
        DecodeOwned<'d> for Result<R, E>
    where
        Self: ReprC<CType = CResult<<R as ReprC>::CType, <E as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<Zero>, Alignment = rust_spec::Gt<One>>,
        E: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
    {
        type Store = Option<Result<R::Store, E::Store>>;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { decode_result(source, store) }
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
            unsafe { decode_result_unchecked(source, store) }
        }
    }
    unsafe impl<'d, R: DecodeOwned<'d> + Niche<CType: PartialEq>, E: Default + ReprC, N: NicheStabilityKind>
        DecodeOwned<'d> for Result<R, E>
    where
        Self: ReprC<CType = <R as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = RustSpecSized<Zero>, Alignment = One>,
    {
        type Store = R::Store;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            if source == R::NICHE {
                return Some(Err(E::default()));
            }

            unsafe { R::soft_decode(source, store) }.map(Ok)
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
            if source == R::NICHE {
                return Err(E::default());
            }

            Ok(unsafe { R::soft_decode_unchecked(source, store) })
        }
    }
    unsafe impl<'d, R: Default + ReprC, E: DecodeOwned<'d> + Niche<CType: PartialEq>, N: NicheStabilityKind>
        DecodeOwned<'d> for Result<R, E>
    where
        Self: ReprC<CType = <E as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<Zero>, Alignment = One>,
        E: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
    {
        type Store = E::Store;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            if source == E::NICHE {
                return Some(Ok(R::default()));
            }

            unsafe { E::soft_decode(source, store) }.map(Err)
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
            if source == E::NICHE {
                return Ok(R::default());
            }

            Err(unsafe { E::soft_decode_unchecked(source, store) })
        }
    }

    unsafe impl<'d, R> DecodeOwned<'d> for ManuallyDrop<R>
    where
        R: RustSpec<Layout = Stable, Mutability = Exclusive>,
        Self: AbiCompatible<CType: Sized>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &'itm mut ()) -> Option<Self> {
            if !unsafe { Self::is_valid(&source) } { return None; }
            Some(unsafe { Self::soft_decode_unchecked(source, &mut ()) })
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &'itm mut ()) -> Self {
            let source = ManuallyDrop::new(source);
            unsafe { core::mem::transmute_copy(&source) }
        }
    }

    unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for ManuallyDrop<R>
    where
        R: RustSpec<Layout = Unstable, Drop = NoDrop>,
    {
        type Store = R::Store;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { R::soft_decode(source, store) }.map(Self::new)
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
            Self::new(unsafe { R::soft_decode_unchecked(source, store) })
        }
    }
    unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for ManuallyDrop<R>
    where
        R: RustSpec<Layout = Unstable, Drop: WithDrop>,
    {
        type Store = R::Store;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { R::soft_decode(ManuallyDrop::into_inner(source), store) }.map(Self::new)
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
            Self::new(unsafe { R::soft_decode_unchecked(ManuallyDrop::into_inner(source), store) })
        }
    }
    unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for ManuallyDrop<R>
    where
        R: RustSpec<Layout = Stable, Mutability = WithInterior, Drop = NoDrop>,
    {
        type Store = R::Store;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { R::soft_decode(source, store) }.map(Self::new)
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
            Self::new(unsafe { R::soft_decode_unchecked(source, store) })
        }
    }
    unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for ManuallyDrop<R>
    where
        R: RustSpec<Layout = Stable, Mutability = WithInterior, Drop: WithDrop>,
    {
        type Store = R::Store;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { R::soft_decode(ManuallyDrop::into_inner(source), store) }.map(Self::new)
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
            Self::new(unsafe { R::soft_decode_unchecked(ManuallyDrop::into_inner(source), store) })
        }
    }
}

/// Encode using an empty owned-conversion store.
///
/// Prefer using [`crate::encode`] whenever possible.
#[doc(hidden)]
#[inline(always)]
pub fn encode_owned<T: EncodeMove<Store: EmptyStore>>(item: T) -> T::CType {
    let mut store = Default::default();
    T::soft_encode(item, &mut store)
}

/// Decode using an empty owned-conversion store.
///
/// Prefer using [`crate::decode`] whenever possible.
///
/// # Safety
///
/// See the safety section in [`crate::decode`].
#[doc(hidden)]
#[inline(always)]
pub unsafe fn decode_owned<'d, T: DecodeOwned<'d, Store: EmptyStore + 'd>>(
    source: T::CType,
) -> Option<T> {
    unsafe { with_empty_decode_store::<T, _>(|store| T::soft_decode(source, store)) }
}

#[doc(hidden)]
#[inline(always)]
pub(crate) unsafe fn encode_owned_unchecked<T: EncodeMoveUnchecked<SoftStore: EmptyStore>>(
    item: T,
) -> T::CType {
    let mut store = Default::default();
    unsafe { T::soft_encode_unchecked(item, &mut store) }
}

#[doc(hidden)]
#[inline(always)]
pub(crate) unsafe fn decode_owned_unchecked<'d, T: DecodeOwned<'d, Store: EmptyStore + 'd>>(
    source: T::CType,
) -> T {
    unsafe { with_empty_decode_store::<T, _>(|store| T::soft_decode_unchecked(source, store)) }
}

#[inline(always)]
fn debug_assert_encoded_valid<R: AbiCompatible>(encoded: &R::CType) {
    debug_assert!(
        unsafe { R::is_valid(encoded) },
        "encoding failed AbiCompatible::is_valid"
    );
}

#[inline(always)]
fn debug_assert_wide_pointee_valid<R>(data: *const WideData<R::CType>, len: usize)
where
    R: AbiCompatible<CType: Wide<Metadata = usize>> + ?Sized,
{
    debug_assert!(
        unsafe {
            let ctype = R::CType::from_raw_parts(data.cast(), len);
            R::is_valid(ctype)
        },
        "encoding failed AbiCompatible::is_valid"
    );
}

/// Perform the conversion from [`T::CType`](crate::ReprC::CType) into `T` without external storage.
///
/// Prefer using [`crate::decode`] whenever possible.
///
/// # Safety
///
/// The closure must not return references into the temporary store. The
/// `DecodeOwned` contract guarantees this for both decode methods when the
/// store implements `EmptyStore`.
#[inline(always)]
unsafe fn with_empty_decode_store<'d, T, R>(f: impl FnOnce(&'d mut T::Store) -> R) -> R
where
    T: DecodeOwned<'d, Store: EmptyStore + 'd>,
{
    let mut store = T::Store::default();
    // SAFETY: When `T::Store` implements `EmptyStore`, `T::soft_decode` must not return
    // references into the store, so extending this borrow cannot make local backing data escape.
    let store = unsafe { core::mem::transmute::<&mut T::Store, &'d mut T::Store>(&mut store) };
    f(store)
}

#[cfg(feature = "alloc")]
impl<T: Clone> AssignFromOwned for [T] {
    #[inline(always)]
    fn assign_from_owned(&mut self, owned: Self::Owned) -> Option<()> {
        assert!(self.len() == owned.len());

        for (destination, source) in self.iter_mut().zip(owned) {
            *destination = source;
        }

        Some(())
    }
}

#[cfg(feature = "alloc")]
impl AssignFromOwned for str {
    #[inline(always)]
    fn assign_from_owned(&mut self, owned: Self::Owned) -> Option<()> {
        assert!(self.len() == owned.len());

        unimplemented!()
        //for (destination, source) in self.iter_mut().zip(owned) {
        //    *destination = source;
        //}

        //Some(())
    }
}

#[inline(always)]
unsafe fn decode_ref_sized<'d, 'itm: 'd, R>(
    source: *const R::CType,
    store: &'itm mut RefSizedDecodeStore<R, <R::Borrowed<'d> as DecodeOwned<'d>>::Store>,
) -> Option<&'d R>
where
    R: ReprC<CType: BorrowCast<AsConst: Copy> + Copy> + FromBorrow<'d>,
    R::Borrowed<'d>: DecodeOwned<'d, CType = <R::CType as BorrowCast>::AsConst>,
{
    if source.is_null() {
        return None;
    }

    let source = borrow_cast(unsafe { source.read() });
    store.decode(|fields| {
        let value = unsafe { DecodeOwned::soft_decode(source, fields)? };
        Some(R::from_borrow(value))
    })
}

#[cfg(feature = "alloc")]
#[inline(always)]
unsafe fn decode_ref_slice_elements<'d, 'itm: 'd, R, I>(
    source: I,
    store: &'itm mut Box<[<R::Borrowed<'d> as DecodeOwned<'d>>::Store]>,
) -> Option<Vec<R>>
where
    R: ReprC<CType: BorrowCast<AsConst: Copy> + Copy> + FromBorrow<'d>,
    R::Borrowed<'d>: DecodeOwned<'d, CType = <R::CType as BorrowCast>::AsConst>,
    I: ExactSizeIterator<Item = R::CType>,
{
    *store = core::iter::repeat_with(Default::default)
        .take(source.len())
        .collect();

    let mut value = Vec::with_capacity(source.len());
    for (elem, elem_store) in source.zip(store.iter_mut()) {
        let source = borrow_cast(elem);
        let decoded = unsafe { DecodeOwned::soft_decode(source, elem_store)? };
        value.push(R::from_borrow(decoded));
    }

    Some(value)
}

#[cfg(feature = "alloc")]
#[inline(always)]
unsafe fn decode_ref_slice_elements_unchecked<'d, 'itm: 'd, R, I>(
    source: I,
    store: &'itm mut Box<[<R::Borrowed<'d> as DecodeOwned<'d>>::Store]>,
) -> Vec<R>
where
    R: ReprC<CType: BorrowCast<AsConst: Copy> + Copy> + FromBorrow<'d>,
    R::Borrowed<'d>: DecodeOwned<'d, CType = <R::CType as BorrowCast>::AsConst>,
    I: ExactSizeIterator<Item = R::CType>,
{
    *store = core::iter::repeat_with(Default::default)
        .take(source.len())
        .collect();

    source
        .zip(store.iter_mut())
        .map(|(elem, elem_store)| {
            let source = borrow_cast(elem);
            let decoded = unsafe { DecodeOwned::soft_decode_unchecked(source, elem_store) };
            R::from_borrow(decoded)
        })
        .collect()
}

#[cfg(feature = "alloc")]
#[inline(always)]
fn encode_ref_mut_dst<
    'a,
    R: ToOwned<Owned: EncodeMove + DecodeOwned<'a, Store: EmptyStore + 'a>> + ?Sized,
>(
    value: &'a mut R,
    store: &mut RefMutDstEncodeStore<'a, R, <R::Owned as EncodeMove>::Store>,
) -> <<R::Owned as ReprC>::CType as BorrowCastMut>::AsMut
where
    R::Owned: ReprC<CType: BorrowCastMut<AsMut: Copy> + Copy>,
{
    let ctype = store.encode(value, |original, fields| {
        let owned = ToOwned::to_owned(&*original);
        owned.soft_encode(fields)
    });
    borrow_cast_mut(unsafe { core::ptr::read(ctype) })
}

#[cfg(feature = "alloc")]
#[inline(always)]
fn encode_box_owned<R: Owned + ?Sized>(
    value: Box<R>,
    store: &mut <R::Owned as EncodeMove>::Store,
) -> <R::Owned as ReprC>::CType
where
    Box<R>: Into<R::Owned>,
    R::Owned: EncodeMove,
{
    value.into().soft_encode(store)
}

#[cfg(feature = "alloc")]
#[inline(always)]
unsafe fn decode_box_owned<'d, 'itm: 'd, R: Owned + ?Sized>(
    source: <R::Owned as ReprC>::CType,
    store: &'itm mut <R::Owned as DecodeOwned<'d>>::Store,
) -> Option<Box<R>>
where
    R::Owned: DecodeOwned<'d> + Into<Box<R>>,
{
    unsafe { R::Owned::soft_decode(source, store) }.map(Into::into)
}

#[cfg(feature = "alloc")]
#[inline(always)]
unsafe fn decode_box_owned_unchecked<'d, 'itm: 'd, R: Owned + ?Sized>(
    source: <R::Owned as ReprC>::CType,
    store: &'itm mut <R::Owned as DecodeOwned<'d>>::Store,
) -> Box<R>
where
    R::Owned: DecodeOwned<'d> + Into<Box<R>>,
{
    unsafe { R::Owned::soft_decode_unchecked(source, store) }.into()
}

#[cfg(feature = "alloc")]
#[inline(always)]
fn encode_vec_elements<R: EncodeMove>(
    value: Vec<R>,
    store: &mut Box<[R::Store]>,
) -> Box<[<R as ReprC>::CType]> {
    *store = (0..value.len()).map(|_| Default::default()).collect();

    value
        .into_iter()
        .zip(store)
        .map(|(item, store)| item.soft_encode(store))
        .collect()
}

#[cfg(feature = "alloc")]
#[inline(always)]
unsafe fn encode_vec_elements_unchecked<R: EncodeMoveUnchecked>(
    value: Vec<R>,
    store: &mut Box<[R::SoftStore]>,
) -> Box<[<R as ReprC>::CType]> {
    *store = core::iter::repeat_with(Default::default)
        .take(value.len())
        .collect();

    value
        .into_iter()
        .zip(store.iter_mut())
        .map(|(item, store)| unsafe { item.soft_encode_unchecked(store) })
        .collect()
}

#[cfg(feature = "alloc")]
#[inline(always)]
unsafe fn decode_vec_elements<'d, 'itm: 'd, R: DecodeOwned<'d>>(
    source: Box<[<R as ReprC>::CType]>,
    store: &'itm mut Box<[R::Store]>,
) -> Option<Vec<R>> {
    let mut is_valid = true;

    *store = core::iter::repeat_with(Default::default)
        .take(source.len())
        .collect();

    let mut decoded = Vec::with_capacity(source.len());
    for (item, store) in source.into_vec().into_iter().zip(store.iter_mut()) {
        if let Some(item) = unsafe { R::soft_decode(item, store) } {
            decoded.push(item);
        } else {
            is_valid = false;
        }
    }
    is_valid.then_some(decoded)
}

#[cfg(feature = "alloc")]
#[inline(always)]
unsafe fn decode_vec_elements_unchecked<'d, 'itm: 'd, R: DecodeOwned<'d>>(
    source: Box<[<R as ReprC>::CType]>,
    store: &'itm mut Box<[R::Store]>,
) -> Vec<R> {
    *store = core::iter::repeat_with(Default::default)
        .take(source.len())
        .collect();

    source
        .into_vec()
        .into_iter()
        .zip(store.iter_mut())
        .map(|(item, store)| unsafe { R::soft_decode_unchecked(item, store) })
        .collect()
}

#[inline(always)]
fn encode_result<'itm, R: EncodeMove<CType: Copy> + 'itm, E: EncodeMove<CType: Copy> + 'itm>(
    value: Result<R, E>,
    store: &'itm mut Option<Result<R::Store, E::Store>>,
) -> CResult<R::CType, E::CType> {
    match value {
        Ok(ok) => {
            let Result::Ok(store) = store.insert(Ok(Default::default())) else {
                unreachable!()
            };

            CResult::Ok(ok.soft_encode(store))
        }
        Err(err) => {
            let Result::Err(store) = store.insert(Err(Default::default())) else {
                unreachable!()
            };

            CResult::Err(err.soft_encode(store))
        }
    }
}

#[inline(always)]
unsafe fn decode_result<'d, 'i, R: DecodeOwned<'d, CType: Copy>, E: DecodeOwned<'d, CType: Copy>>(
    source: CResult<R::CType, E::CType>,
    store: &'i mut Option<Result<R::Store, E::Store>>,
) -> Option<Result<R, E>>
where
    'i: 'd,
{
    let value = match source.into_result()? {
        Ok(ok) => {
            let ok_store = store.insert(Ok(Default::default()));
            let ok_store = unsafe { ok_store.as_mut().unwrap_unchecked() };
            Ok(unsafe { R::soft_decode(ok, ok_store)? })
        }
        Err(err) => {
            let err_store = store.insert(Err(Default::default()));
            let err_store = unsafe { err_store.as_mut().unwrap_err_unchecked() };
            Err(unsafe { E::soft_decode(err, err_store)? })
        }
    };

    Some(value)
}

#[inline(always)]
unsafe fn decode_result_unchecked<
    'd,
    'i,
    R: DecodeOwned<'d, CType: Copy>,
    E: DecodeOwned<'d, CType: Copy>,
>(
    source: CResult<R::CType, E::CType>,
    store: &'i mut Option<Result<R::Store, E::Store>>,
) -> Result<R, E>
where
    'i: 'd,
{
    match unsafe { source.into_result_unchecked() } {
        Ok(ok) => {
            let ok_store = store.insert(Ok(Default::default()));
            let ok_store = unsafe { ok_store.as_mut().unwrap_unchecked() };
            Ok(unsafe { R::soft_decode_unchecked(ok, ok_store) })
        }
        Err(err) => {
            let err_store = store.insert(Err(Default::default()));
            let err_store = unsafe { err_store.as_mut().unwrap_err_unchecked() };
            Err(unsafe { E::soft_decode_unchecked(err, err_store) })
        }
    }
}

/// Copy an exclusively borrowed field representation into an aggregate's temporary value.
/// Its backing storage remains borrowed and must be kept alive by the aggregate store.
#[doc(hidden)]
pub fn encode_ref_field<'a, R: ReprC<CType: Copy> + 'a>(
    value: &'a R,
    store: &mut <&'a R as EncodeMove>::Store,
) -> R::CType
where
    &'a R: EncodeMove<CType = CRef<R::CType>>,
{
    let encoded = value.soft_encode(store);
    // CRef guarantees shared access to initialized storage, and Copy permits
    // duplicating the field representation without moving out of that storage.
    unsafe { encoded.as_ptr().read() }
}

/// Unchecked counterpart of `encode_ref_field`.
///
/// # Safety
/// The caller must uphold the unchecked encoding contract for the field.
#[doc(hidden)]
pub unsafe fn encode_ref_field_unchecked<'a, R: ReprC<CType: Copy> + 'a>(
    value: &'a R,
    store: &mut <&'a R as EncodeMoveUnchecked>::SoftStore,
) -> R::CType
where
    &'a R: EncodeMoveUnchecked<CType = CRef<R::CType>>,
{
    let encoded = unsafe { value.soft_encode_unchecked(store) };
    unsafe { encoded.as_ptr().read() }
}
