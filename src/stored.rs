#[cfg(feature = "alloc")]
use alloc::{borrow::ToOwned, boxed::Box, vec::Vec};
#[cfg(feature = "alloc")]
use core::ptr::NonNull;

use disjoint_impls::disjoint_impls;
use rust_spec::{
    One, RustSpec, Stable, Unstable,
    layout::{NonRobust, Robust},
    mutability::{Exclusive, Interior},
    niche::{NicheStabilityKind, WithNiche, WithoutNiche},
    size::{MetaSized, NulTerminated, Sized as RustSpecSized, SizedKind, SliceLike, Zero},
};

#[cfg(feature = "alloc")]
use crate::{
    Dst,
    borrow::{BorrowCastMut, borrow_cast_mut},
    boxed::{CBox, CBoxedSlice},
};
use crate::{
    ReprC,
    borrow::{Borrow, BorrowCast, FromBorrow, borrow_cast},
    cell::InteriorMut,
    ffi::NulTerminatedRef,
    niche::Niche,
    result::ReprCResult,
    slice::{CSlice, CSliceMut},
    transmute::CheckedTransmute,
    wide::Wide,
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
    /// Facilitates conversion from a Rust type into a corresponding C-compatible representation.
    ///
    /// # Safety
    ///
    /// If [`Self::Store`] implements [`EmptyStore`], [`EncodeOwned::soft_encode`] must not
    /// return references into the store. [`crate::soft_encode`] correctness relies on it.
    ///
    /// Prefer using [`crate::soft_encode`] whenever possible
    pub unsafe trait EncodeOwned: ReprC<CType: Sized> + Sized {
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

    unsafe impl<R: ReprC + ?Sized> EncodeOwned for &R
    where
        Self: RustSpec<Layout = Stable> + CheckedTransmute<CType = *const <R as ReprC>::CType>,
        R: RustSpec<Mutability = Exclusive>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let ptr = core::ptr::from_ref(self);

            // TODO: Hack before `Thin` trait is available in stable:
            // https://doc.rust-lang.org/std/ptr/traitalias.Thin.html
            // https://github.com/rust-lang/rust/issues/81513
            unsafe { core::mem::transmute_copy::<*const R, *const R::CType>(&ptr) }
        }
    }
    unsafe impl<R: InteriorMut + ReprC + ?Sized> EncodeOwned for &R
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType = *mut <R as ReprC>::CType>,
        R: RustSpec<Trap = Robust, Mutability = Interior>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let ptr = self.get();

            // TODO: Hack before `Thin` trait is available in stable:
            // https://doc.rust-lang.org/std/ptr/traitalias.Thin.html
            // https://github.com/rust-lang/rust/issues/81513
            unsafe { core::mem::transmute_copy::<*mut R::Target, *mut R::CType>(&ptr) }
        }
    }
    unsafe impl<'a, R: InteriorMut<Target: DecodeOwned<'a, Store: EmptyStore>> + Clone, S: SizedKind>
        EncodeOwned for &'a R
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType = *mut <R as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<S>, Trap = NonRobust, Mutability = Interior>,
        R: EncodeOwned<CType = <<R as InteriorMut>::Target as ReprC>::CType, Store: EmptyStore>
    {
        type Store = InteriorMutSizedEncodeStore<'a, R>;

        fn soft_encode<'itm>(self, store: &mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let original = store.original.insert(self);
            let owned = (**original).clone();
            let ctype = encode_owned(owned);
            store.ctype.insert(ctype)
        }
    }
    unsafe impl<R: Wide<Data: CheckedTransmute<CType: Sized>, Metadata = usize> + ?Sized> EncodeOwned
        for &R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = Exclusive>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let len = self.metadata();
            let ptr = self.as_ptr().cast();
            CSlice::from_raw_parts(ptr, len)
        }
    }
    // FIXME: There is a deep issue here that surfaces in 0-length slices: https://github.com/mversic/co3/issues/229
    // The canonical example is: &[&UnsafeCell<T>] -> *const &UnsafeCell<T> -> can't call raw_get on the inner cell?
    //#[cfg(feature = "alloc")]
    //unsafe impl<'a, R: ToOwned<Owned: EncodeOwned + DecodeOwned<'a>> + ?Sized> EncodeOwned for &'a R
    //where
    //    Self: RustSpec<Layout = Unstable> + ReprC<CType = <<<R as ToOwned>::Owned as ReprC>::CType as BorrowCastMut>::AsMut>,
    //    R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Trap = NonRobust, Mutability = Interior>,
    //    R: ToOwned<Owned: ReprC<CType: BorrowCastMut<AsMut: Copy> + Copy>>,
    //{
    //    type Store = InteriorMutDstEncodeStore<'a, R>;

    //    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    //    where
    //        Self: 'itm,
    //    {
    //        let original = store.original.insert(self);
    //        let owned = ToOwned::to_owned(&**original);
    //        let ctype = owned.soft_encode(&mut store.store);
    //        borrow_cast_mut(*store.ctype.insert(ctype))
    //    }
    //}
    unsafe impl<R: EncodeOwned + Clone, S: SizedKind> EncodeOwned for &R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = RustSpecSized<S>, Mutability = Exclusive>,
    {
        type Store = RefSizedEncodeStore<R>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let owned = self.clone();
            let ctype = owned.soft_encode(&mut store.store);
            store.ctype.insert(ctype)
        }
    }
    unsafe impl<R: NulTerminatedRef + ?Sized> EncodeOwned for &R
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType = *const <R as ReprC>::CType>,
        R: RustSpec<Size = NulTerminated, Mutability = Exclusive>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            self.as_c_ptr()
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: ToOwned<Owned: EncodeOwned<CType: BorrowCast<AsConst: Copy> + Copy>> + ?Sized>
        EncodeOwned for &R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size: Dst>,
        Self: ReprC<CType = <<<R as ToOwned>::Owned as ReprC>::CType as BorrowCast>::AsConst>,
    {
        type Store = RefDstEncodeStore<R>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let owned = ToOwned::to_owned(self);
            let ctype = owned.soft_encode(&mut store.store);
            borrow_cast(*store.ctype.insert(ctype))
        }
    }

    unsafe impl<'a, R: InteriorMut + ?Sized> EncodeOwned for &'a mut R
    where
        Self: RustSpec + ReprC<CType = <&'a R as ReprC>::CType>,
        R: RustSpec<Layout = Stable, Mutability = Interior>,
        &'a R: EncodeOwned<Store: EmptyStore>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            encode_owned(&*self)
        }
    }
    unsafe impl<R: ReprC + ?Sized> EncodeOwned for &mut R
    where
        Self: RustSpec<Layout = Stable> + CheckedTransmute<CType = *mut <R as ReprC>::CType>,
        R: RustSpec<Trap = Robust, Mutability = Exclusive>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let ptr = core::ptr::from_mut(self);

            // TODO: Hack before `Thin` trait is available in stable:
            // https://doc.rust-lang.org/std/ptr/traitalias.Thin.html
            // https://github.com/rust-lang/rust/issues/81513
            unsafe { core::mem::transmute_copy::<*mut R, *mut R::CType>(&ptr) }
        }
    }
    unsafe impl<'a, R: EncodeOwned + DecodeOwned<'a, Store: EmptyStore + 'a> + Clone, S: SizedKind>
        EncodeOwned for &'a mut R
    where
        Self: RustSpec<Layout = Stable> + CheckedTransmute<CType = *mut <R as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<S>, Trap = NonRobust, Mutability = Exclusive>,
    {
        type Store = RefMutSizedEncodeStore<'a, R>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            // TODO: This impl is identical to later impl for Self: RustSpec<Layout = Unstable>
            encode_ref_mut_sized(self, store)
        }
    }
    unsafe impl<R: Wide<Data: CheckedTransmute<CType: Sized>, Metadata = usize> + ?Sized> EncodeOwned
        for &mut R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Trap = Robust, Mutability = Exclusive>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let len = self.metadata();
            let ptr = self.as_mut_ptr().cast();
            CSliceMut::from_raw_parts_mut(ptr, len)
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, R: CheckedTransmute + AssignFromOwned + ?Sized>
        EncodeOwned for &'a mut R
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType = <<<R as ToOwned>::Owned as ReprC>::CType as BorrowCastMut>::AsMut>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Trap = NonRobust, Mutability = Exclusive>,
        R: ToOwned<Owned: EncodeOwned<CType: BorrowCastMut<AsMut: Copy> + Copy, Store: EmptyStore> + DecodeOwned<'a, Store: EmptyStore>>,
    {
        type Store = RefMutDstEncodeStore<'a, R>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            // TODO: This impl is identical to later impl Self: RustSpec<Layout = Unstable>
            encode_ref_mut_dst(self, store)
        }
    }
    unsafe impl<'a, R: EncodeOwned + DecodeOwned<'a, Store: EmptyStore + 'a> + Clone, S: SizedKind>
        EncodeOwned for &'a mut R
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType = *mut <R as ReprC>::CType>,
        R: RustSpec<Layout = Unstable, Size = RustSpecSized<S>>,
    {
        type Store = RefMutSizedEncodeStore<'a, R>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            encode_ref_mut_sized(self, store)
        }
    }
    // TODO: We should prevent Sized opaque types here because they can't be decoded
    #[cfg(feature = "alloc")]
    unsafe impl<'a, R: ToOwned<Owned: EncodeOwned<CType: BorrowCastMut<AsMut: Copy> + Copy>> + ?Sized>
        EncodeOwned for &'a mut R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size: Dst> + AssignFromOwned,
        <R as ToOwned>::Owned: DecodeOwned<'a, Store: EmptyStore + 'a>,
        Self: ReprC<CType = <<<R as ToOwned>::Owned as ReprC>::CType as BorrowCastMut>::AsMut>,
    {
        type Store = RefMutDstEncodeStore<'a, R>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            encode_ref_mut_dst(self, store)
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: EncodeOwned> EncodeOwned for Box<R>
    where
        Self: RustSpec<Layout = Stable> + CheckedTransmute<CType = CBox<<R as ReprC>::CType>>,
        R: RustSpec<Layout = Stable>,
    {
        type Store = Box<R::Store>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            // TODO: It's a bit stupid that this is the only conversion
            // where Store != () when type's Layout = Stable
            if impls::impls!(R::Store: EmptyStore) {
                let non_null_ptr = Box::into_non_null(self).cast();
                return CBox::from_raw_parts(non_null_ptr);
            }

            CBox::from_box(Box::new((*self).soft_encode(store)))
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: Owned + Wide<Data: CheckedTransmute<CType: Sized>, Metadata = usize> + ?Sized>
        EncodeOwned for Box<R>
    where
        Self: RustSpec<Layout = Unstable> + Into<<R as Owned>::Owned>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>>,
        <R as Owned>::Owned: EncodeOwned<CType = CBoxedSlice<<<R as Wide>::Data as ReprC>::CType>>,
    {
        type Store = <R::Owned as EncodeOwned>::Store;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            // TODO: It's a bit stupid that this is the only conversion
            // where Store != () when type's Layout = Stable
            if impls::impls!(<R::Owned as EncodeOwned>::Store: EmptyStore) {
                let len = self.metadata();
                let data = R::into_non_null(self);

                return CBoxedSlice::from_raw_parts(data.cast(), len);
            }

            self.into().soft_encode(store)
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: EncodeOwned, S: SizedKind> EncodeOwned for Box<R>
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = RustSpecSized<S>>,
    {
        type Store = Box<R::Store>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            CBox::from_box(Box::new((*self).soft_encode(store)))
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: Owned<Owned: EncodeOwned> + ?Sized> EncodeOwned for Box<R>
    where
        Self: RustSpec<Layout = Unstable> + Into<<R as Owned>::Owned>,
        R: RustSpec<Layout = Unstable, Size: Dst>,
        Self: ReprC<CType = <<R as Owned>::Owned as ReprC>::CType>,
    {
        type Store = <R::Owned as EncodeOwned>::Store;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            self.into().soft_encode(store)
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: CheckedTransmute<CType: Copy>> EncodeOwned for Vec<R>
    where
        R: RustSpec<Layout = Stable>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let source = self.into_boxed_slice();
            let len = source.len();
            let data = Box::into_non_null(source).cast();

            CBoxedSlice::from_raw_parts(data, len)
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: EncodeOwned> EncodeOwned for Vec<R>
    where
        R: RustSpec<Layout = Unstable>,
    {
        type Store = Box<[R::Store]>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            *store = (0..self.len()).map(|_| Default::default()).collect();

            let ctypes = self
                .into_iter()
                .zip(store)
                .map(|(item, store)| item.soft_encode(store))
                .collect::<Box<[_]>>();

            CBoxedSlice::from_boxed_slice(ctypes)
        }
    }

    unsafe impl<R: EncodeOwned> EncodeOwned for Option<R>
    where
        R: RustSpec<Niche = WithoutNiche>,
        <Self as ReprC>::CType: Copy,
    {
        type Store = R::Store;

        fn soft_encode<'itm>(self, store: &mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            self.map(|v| v.soft_encode(store)).into()
        }
    }
    // NOTE: PartialEq is only required for decoding, Here it is only used by `debug_assert!`
    unsafe impl<R: EncodeOwned + Niche<CType: PartialEq>, N: NicheStabilityKind> EncodeOwned
        for Option<R>
    where
        R: RustSpec<Niche = WithNiche<N>>,
    {
        type Store = R::Store;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            if let Some(value) = self {
                let encoded = value.soft_encode(store);

                debug_assert!(
                    encoded != R::NICHE_VALUE,
                    "encoding produced the reserved NICHE_VALUE"
                );

                return encoded;
            }

            R::NICHE_VALUE
        }
    }

    unsafe impl<R: EncodeOwned<CType: Copy>, E: EncodeOwned<CType: Copy>, K: SizedKind> EncodeOwned
        for Result<R, E>
    where
        R: RustSpec<Size = RustSpecSized<K>>,
        E: RustSpec<Size = RustSpecSized<K>>,
    {
        type Store = Option<Result<R::Store, E::Store>>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            encode_result(self, store)
        }
    }
    unsafe impl<R: EncodeOwned + Niche, E: EncodeOwned<CType: Copy>, N: NicheStabilityKind>
        EncodeOwned for Result<R, E>
    where
        R: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = RustSpecSized<Zero>, Alignment = rust_spec::Gt<One>>,
    {
        type Store = Option<Result<R::Store, E::Store>>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            encode_result(self, store)
        }
    }
    unsafe impl<R: EncodeOwned<CType: Copy>, E: EncodeOwned<CType: Copy> + Niche, N: NicheStabilityKind>
        EncodeOwned for Result<R, E>
    where
        R: RustSpec<Size = RustSpecSized<Zero>, Alignment = rust_spec::Gt<One>>,
        E: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
    {
        type Store = Option<Result<R::Store, E::Store>>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            encode_result(self, store)
        }
    }
    unsafe impl<R: EncodeOwned + Niche, E, N: NicheStabilityKind> EncodeOwned
        for Result<R, E>
    where
        R: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = RustSpecSized<Zero>, Alignment = One>,
    {
        type Store = R::Store;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            match self {
                Ok(ok) => ok.soft_encode(store),
                Err(_) => R::NICHE_VALUE,
            }
        }
    }
    unsafe impl<R, E: EncodeOwned + Niche, N: NicheStabilityKind> EncodeOwned
        for Result<R, E>
    where
        R: RustSpec<Size = RustSpecSized<Zero>, Alignment = One>,
        E: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
    {
        type Store = E::Store;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            match self {
                Ok(_) => E::NICHE_VALUE,
                Err(err) => err.soft_encode(store),
            }
        }
    }
}

disjoint_impls! {
    /// Perform the conversion from an [`ReprC::CType`] into [`Self`].
    ///
    /// # Safety
    ///
    /// If [`Self::Store`] implements [`EmptyStore`], [`DecodeOwned::soft_decode`] must not
    /// return references into the store. [`crate::soft_decode`] correctness relies on it.
    ///
    /// Prefer using [`crate::soft_decode`] whenever possible
    pub unsafe trait DecodeOwned<'d>: ReprC<CType: Sized> + Sized {
        type Store: Store + Default;

        /// Perform the conversion from an [`ReprC::CType`] into [`Self`].
        ///
        /// Prefer using [`crate::soft_decode`] whenever possible
        ///
        /// # Safety
        ///
        /// - All conversions from a pointer must ensure pointer validity beforehand
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self>;
    }

    unsafe impl<'d, R: NulTerminatedRef + ?Sized> DecodeOwned<'d> for &'d R
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType = *const <R as ReprC>::CType>,
        R: RustSpec<Size = NulTerminated, Mutability = Exclusive>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            Some(unsafe { R::from_c_ptr(source) })
        }
    }

    unsafe impl<'d, R: ReprC + ?Sized> DecodeOwned<'d> for &'d R
    where
        Self: RustSpec<Layout = Stable> + CheckedTransmute<CType = *const <R as ReprC>::CType>,
        R: RustSpec<Mutability = Exclusive>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if !unsafe { Self::is_valid(&source) } {
                return None;
            }

            // TODO: Hack before `Thin` trait is available in stable:
            // https://doc.rust-lang.org/std/ptr/traitalias.Thin.html
            // https://github.com/rust-lang/rust/issues/81513
            unsafe { core::mem::transmute_copy::<*const R::CType, *const R>(&source).as_ref() }
        }
    }
    unsafe impl<'d, R: CheckedTransmute + ?Sized> DecodeOwned<'d> for &'d R
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType = *mut <R as ReprC>::CType>,
        R: RustSpec<Mutability = Interior>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if source.is_null() || unsafe { !R::is_valid(&*source) } {
                return None;
            }

            // TODO: Hack before `Thin` trait is available in stable:
            // https://doc.rust-lang.org/std/ptr/traitalias.Thin.html
            // https://github.com/rust-lang/rust/issues/81513
            unsafe { core::mem::transmute_copy::<*mut R::CType, *mut R>(&source).as_ref() }
        }
    }
    unsafe impl<'d, R: CheckedTransmute<CType: Wide<Metadata = usize>> + Wide<Metadata = usize> + ?Sized>
        DecodeOwned<'d> for &'d R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = Exclusive>,
        <R as Wide>::Data: ReprC<CType = <<R as ReprC>::CType as Wide>::Data>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if source.is_niche() {
                return None;
            }

            let ctype_slice = unsafe { R::CType::from_raw_parts(source.data(), source.len()) };

            if unsafe { !R::is_valid(ctype_slice) } {
                return None;
            }

            let len = source.len();
            let data = source.data().cast();

            Some(unsafe { R::from_raw_parts(data, len) })
        }
    }
    unsafe impl<'d, R: CheckedTransmute<CType: Wide<Metadata = usize>> + Wide<Metadata = usize> + ?Sized>
        DecodeOwned<'d> for &'d R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = Interior>,
        <R as Wide>::Data: ReprC<CType = <<R as ReprC>::CType as Wide>::Data>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if source.is_niche() {
                return None;
            }

            let ctype_slice = unsafe { R::CType::from_raw_parts(source.data(), source.len()) };

            if unsafe { !R::is_valid(ctype_slice) } {
                return None;
            }

            let len = source.len();
            let data = source.data().cast();

            Some(unsafe { R::from_raw_parts(data, len) })
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

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            let source = borrow_cast(unsafe { source.read() });
            let value = unsafe { DecodeOwned::soft_decode(source, &mut store.store)? };
            Some(store.value.insert(FromBorrow::from_borrow(value)))
        }
    }
    unsafe impl<'d, R: ReprC<CType: BorrowCast<AsConst: Copy> + Copy> + FromBorrow<'d>, S: SizedKind>
        DecodeOwned<'d> for &'d R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = RustSpecSized<S>, Mutability = Interior>,
        <R as Borrow>::Borrowed<'d>: DecodeOwned<'d, CType = <<R as ReprC>::CType as BorrowCast>::AsConst>,
    {
        type Store = RefSizedDecodeStore<R, <R::Borrowed<'d> as DecodeOwned<'d>>::Store>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            let source = borrow_cast(unsafe { source.read() });
            let value = unsafe { DecodeOwned::soft_decode(source, &mut store.store)? };
            Some(store.value.insert(FromBorrow::from_borrow(value)))
        }
    }
    #[cfg(feature = "alloc")]
    // TODO: Implement for all R, not just slices. It's quite difficult to unify it under this
    unsafe impl<'d, R: DecodeOwned<'d, CType: BorrowCast<AsConst: Copy> + Copy> + FromBorrow<'d> + Clone>
        DecodeOwned<'d> for &'d [R]
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType = CSlice<<R as ReprC>::CType>>,
        [R]: RustSpec<Layout = Unstable, Size = MetaSized<SliceLike>, Mutability = Exclusive>,
        <R as Borrow>::Borrowed<'d>: DecodeOwned<'d, CType = <<R as ReprC>::CType as BorrowCast>::AsConst>,
    {
        type Store = RefDstDecodeStore<[R], Box<[<<R as Borrow>::Borrowed<'d> as DecodeOwned<'d>>::Store]>>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = unsafe { source.into_rust()? };

            store.store = core::iter::repeat_with(Default::default)
                .take(source.len())
                .collect();

            let mut value = Vec::with_capacity(source.len());
            for (elem, elem_store) in source.iter().zip(&mut store.store) {
                let source = borrow_cast(*elem);

                let decoded = unsafe {
                    DecodeOwned::soft_decode(source, elem_store)?
                };

                value.push(R::from_borrow(decoded));
            }

            Some(store.value.insert(value))
        }
    }
    #[cfg(feature = "alloc")]
    // TODO: Implement for all R, not just slices. It's quite difficult to unify it under this
    unsafe impl<'d, R: DecodeOwned<'d, CType: BorrowCast<AsConst: Copy> + Copy> + FromBorrow<'d> + Clone>
        DecodeOwned<'d> for &'d [R]
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType = CSliceMut<<R as ReprC>::CType>>,
        [R]: RustSpec<Layout = Unstable, Size = MetaSized<SliceLike>, Mutability = Interior>,
        <R as Borrow>::Borrowed<'d>: DecodeOwned<'d, CType = <<R as ReprC>::CType as BorrowCast>::AsConst>,
    {
        type Store = RefDstDecodeStore<[R], Box<[<<R as Borrow>::Borrowed<'d> as DecodeOwned<'d>>::Store]>>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = unsafe { source.into_rust()? };

            store.store = core::iter::repeat_with(Default::default)
                .take(source.len())
                .collect();

            let mut value = Vec::with_capacity(source.len());
            for (elem, elem_store) in source.iter().zip(&mut store.store) {
                let source = borrow_cast(*elem);

                let decoded = unsafe {
                    DecodeOwned::soft_decode(source, elem_store)?
                };

                value.push(R::from_borrow(decoded));
            }

            Some(store.value.insert(value))
        }
    }

    unsafe impl<'d, R: ReprC + ?Sized> DecodeOwned<'d> for &'d mut R
    where
        Self: RustSpec<Layout = Stable> + CheckedTransmute<CType = *mut <R as ReprC>::CType>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if !unsafe { Self::is_valid(&source) } {
                return None;
            }

            // TODO: Hack before `Thin` trait is available in stable:
            // https://doc.rust-lang.org/std/ptr/traitalias.Thin.html
            // https://github.com/rust-lang/rust/issues/81513
            unsafe { core::mem::transmute_copy::<*mut R::CType, *mut R>(&source).as_mut() }
        }
    }
    unsafe impl<'d, R: CheckedTransmute<CType: Wide<Metadata = usize>> + Wide<Metadata = usize> + ?Sized>
        DecodeOwned<'d> for &'d mut R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>>,
        <R as Wide>::Data: ReprC<CType = <<R as ReprC>::CType as Wide>::Data>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if source.is_niche() {
                return None;
            }

            let ctype_slice = unsafe { R::CType::from_raw_parts(source.data(), source.len()) };

            if unsafe { !R::is_valid(ctype_slice) } {
                return None;
            }

            let len = source.len();
            let data = source.data().cast();

            Some(unsafe { R::from_raw_parts_mut(data, len) })
        }
    }
    unsafe impl<'d, R: ReprC<CType: BorrowCast<AsConst: Copy> + Copy> + FromBorrow<'d>, S: SizedKind>
        DecodeOwned<'d> for &'d mut R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = RustSpecSized<S>> + EncodeOwned<Store: EmptyStore>,
        <R as Borrow>::Borrowed<'d>: DecodeOwned<'d, CType = <<R as ReprC>::CType as BorrowCast>::AsConst>,
    {
        type Store = RefMutSizedDecodeStore<R, <R::Borrowed<'d> as DecodeOwned<'d>>::Store>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            store.source = Some(source);
            let source = borrow_cast(unsafe { source.read() });
            let value = unsafe { DecodeOwned::soft_decode(source, &mut store.store)? };
            Some(store.value.insert(FromBorrow::from_borrow(value)))
        }
    }
    #[cfg(feature = "alloc")]
    // TODO: Implement for all R, not just slices. It's quite difficult to unify it under this
    unsafe impl<'d, R: DecodeOwned<'d, CType: BorrowCast<AsConst: Copy> + Copy> + FromBorrow<'d> + EncodeOwned<Store: EmptyStore>>
        DecodeOwned<'d> for &'d mut [R]
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType = CSliceMut<<R as ReprC>::CType>>,
        [R]: RustSpec<Layout = Unstable, Size = MetaSized<SliceLike>>,
        <R as Borrow>::Borrowed<'d>: DecodeOwned<'d, CType = <<R as ReprC>::CType as BorrowCast>::AsConst>,
    {
        type Store = RefMutSliceDecodeStore<R, Box<[<<R as Borrow>::Borrowed<'d> as DecodeOwned<'d>>::Store]>>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            store.source = Some(source);

            let source = unsafe { source.into_rust()? };
            store.store = core::iter::repeat_with(Default::default)
                .take(source.len())
                .collect();

            let mut value = Vec::with_capacity(source.len());
            for (elem, elem_store) in source.iter().zip(&mut store.store) {
                let source = borrow_cast(*elem);

                let decoded = unsafe {
                    DecodeOwned::soft_decode(source, elem_store)?
                };

                value.push(R::from_borrow(decoded));
            }

            Some(store.value.insert(value))
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: CheckedTransmute<CType: Sized>> DecodeOwned<'d> for Box<R>
    where
        Self: RustSpec<Layout = Stable> + CheckedTransmute<CType = CBox<<R as ReprC>::CType>>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if !unsafe { Self::is_valid(&source) } {
                return None;
            }

            let ptr = source.data.cast();
            Some(unsafe { Box::from_raw(ptr) })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: CheckedTransmute<CType: Wide<Metadata = usize>> + Wide<Metadata = usize> + ?Sized>
        DecodeOwned<'d> for Box<R>
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>>,
        <R as Wide>::Data: ReprC<CType = <<R as ReprC>::CType as Wide>::Data>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if source.is_niche() {
                return None;
            }

            let ctype_slice = unsafe { R::CType::from_raw_parts(source.data(), source.len()) };

            if unsafe { !R::is_valid(ctype_slice) } {
                drop(unsafe { source.into_rust() });
                return None;
            }

            let len = source.len();
            let data = source.data().cast();

            Some(unsafe { R::from_non_null(NonNull::new_unchecked(data), len) })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: DecodeOwned<'d, CType: Sized>, S: SizedKind> DecodeOwned<'d> for Box<R>
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = RustSpecSized<S>>,
    {
        type Store = Box<R::Store>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = unsafe { source.read() };
            let value = unsafe { R::soft_decode(source, &mut **store)? };
            Some(Box::new(value))
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

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { R::Owned::soft_decode(source, store) }.map(Into::into)
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: CheckedTransmute> DecodeOwned<'d> for Vec<R>
    where
        R: RustSpec<Layout = Stable>,
        <R as ReprC>::CType: Copy,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            let slice: &[_] = unsafe { borrow_cast(source).into_rust()? };

            if slice.iter().any(|slice| unsafe { !R::is_valid(slice) }) {
                return None;
            }

            let len = source.len();
            let data = source.data().cast();

            Some(unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(data, len)) }.into())
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for Vec<R>
    where
        R: RustSpec<Layout = Unstable>,
    {
        type Store = Box<[R::Store]>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = unsafe { source.into_rust()? };

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
    }

    unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for Option<R>
    where
        R: RustSpec<Niche = WithoutNiche>,
        <Self as ReprC>::CType: Copy,
    {
        type Store = R::Store;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            match source.try_into().ok()? {
                Some(source) => unsafe { R::soft_decode(source, store) }.map(Some),
                None => Some(None),
            }
        }
    }
    unsafe impl<'d, R: DecodeOwned<'d> + Niche<CType: PartialEq>, N: NicheStabilityKind>
        DecodeOwned<'d> for Option<R>
    where
        R: RustSpec<Niche = WithNiche<N>>,
    {
        type Store = <R as DecodeOwned<'d>>::Store;

        unsafe fn soft_decode<'itm: 'd>(
            source: R::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            if source == R::NICHE_VALUE {
                return Some(None);
            }

            unsafe { R::soft_decode(source, store) }.map(Some)
        }
    }

    unsafe impl<'d, R: DecodeOwned<'d, CType: Copy>, E: DecodeOwned<'d, CType: Copy>, K: SizedKind>
        DecodeOwned<'d> for Result<R, E>
    where
        R: RustSpec<Size = RustSpecSized<K>>,
        E: RustSpec<Size = RustSpecSized<K>>,
    {
        type Store = Option<Result<R::Store, E::Store>>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { decode_result(source, store) }
        }
    }
    unsafe impl<'d, R: DecodeOwned<'d> + Niche, E: DecodeOwned<'d, CType: Copy>, N: NicheStabilityKind>
        DecodeOwned<'d> for Result<R, E>
    where
        R: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = RustSpecSized<Zero>, Alignment = rust_spec::Gt<One>>,
    {
        type Store = Option<Result<R::Store, E::Store>>;

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { decode_result(source, store) }
        }
    }
    unsafe impl<'d, R: DecodeOwned<'d, CType: Copy>, E: DecodeOwned<'d> + Niche, N: NicheStabilityKind>
        DecodeOwned<'d> for Result<R, E>
    where
        R: RustSpec<Size = RustSpecSized<Zero>, Alignment = rust_spec::Gt<One>>,
        E: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
    {
        type Store = Option<Result<R::Store, E::Store>>;

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { decode_result(source, store) }
        }
    }
    unsafe impl<'d, R: DecodeOwned<'d> + Niche<CType: PartialEq>, E: Default, N: NicheStabilityKind>
        DecodeOwned<'d> for Result<R, E>
    where
        R: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = RustSpecSized<Zero>, Alignment = One>,
        <R as ReprC>::CType: Copy,
    {
        type Store = R::Store;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            if source == R::NICHE_VALUE {
                return Some(Err(E::default()));
            }

            unsafe { R::soft_decode(source, store) }.map(Ok)
        }
    }
    unsafe impl<'d, R: Default, E: DecodeOwned<'d> + Niche<CType: PartialEq>, N: NicheStabilityKind>
        DecodeOwned<'d> for Result<R, E>
    where
        R: RustSpec<Size = RustSpecSized<Zero>, Alignment = One>,
        E: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
        <E as ReprC>::CType: Copy,
    {
        type Store = E::Store;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            if source == E::NICHE_VALUE {
                return Some(Ok(R::default()));
            }

            unsafe { E::soft_decode(source, store) }.map(Err)
        }
    }
}

#[cfg(feature = "alloc")]
impl<T: Clone> AssignFromOwned for [T] {
    fn assign_from_owned(&mut self, owned: Self::Owned) -> Option<()> {
        if self.len() != owned.len() {
            unreachable!();
        }

        for (destination, source) in self.iter_mut().zip(owned) {
            *destination = source;
        }

        Some(())
    }
}

#[cfg(feature = "alloc")]
impl AssignFromOwned for str {
    fn assign_from_owned(&mut self, owned: Self::Owned) -> Option<()> {
        if self.len() != owned.len() {
            unreachable!();
        }

        unimplemented!()
        //for (destination, source) in self.iter_mut().zip(owned) {
        //    *destination = source;
        //}

        //Some(())
    }
}

fn encode_ref_mut_sized<'a, R: EncodeOwned + Clone>(
    value: &'a mut R,
    store: &mut RefMutSizedEncodeStore<'a, R>,
) -> *mut <R as ReprC>::CType {
    let original = store.original.insert(value);
    let owned = (**original).clone();
    let ctype = owned.soft_encode(&mut store.store);
    store.ctype.insert(ctype)
}

#[cfg(feature = "alloc")]
fn encode_ref_mut_dst<'a, R: ToOwned<Owned: EncodeOwned> + ?Sized>(
    value: &'a mut R,
    store: &mut RefMutDstEncodeStore<'a, R>,
) -> <<R::Owned as ReprC>::CType as BorrowCastMut>::AsMut
where
    R::Owned: ReprC<CType: BorrowCastMut<AsMut: Copy> + Copy>,
{
    let original = store.original.insert(value);
    let owned = ToOwned::to_owned(&**original);
    let ctype = owned.soft_encode(&mut store.store);
    borrow_cast_mut(*store.ctype.insert(ctype))
}

fn encode_result<'itm, R: EncodeOwned<CType: Copy> + 'itm, E: EncodeOwned<CType: Copy> + 'itm>(
    value: Result<R, E>,
    store: &'itm mut Option<Result<R::Store, E::Store>>,
) -> ReprCResult<R::CType, E::CType> {
    match value {
        Ok(ok) => {
            let Result::Ok(store) = store.insert(Ok(Default::default())) else {
                unreachable!()
            };

            ReprCResult::Ok(ok.soft_encode(store))
        }
        Err(err) => {
            let Result::Err(store) = store.insert(Err(Default::default())) else {
                unreachable!()
            };

            ReprCResult::Err(err.soft_encode(store))
        }
    }
}

unsafe fn decode_result<'d, 'i, R: DecodeOwned<'d, CType: Copy>, E: DecodeOwned<'d, CType: Copy>>(
    source: ReprCResult<R::CType, E::CType>,
    store: &'i mut Option<Result<R::Store, E::Store>>,
) -> Option<Result<R, E>>
where
    'i: 'd,
{
    let value = match source.try_into().ok()? {
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

/// Perform the conversion from `T` into [`T::CType`] using external storage.
///
/// Prefer using [`crate::encode`] whenever possible.
pub(crate) fn encode_owned<T: EncodeOwned<Store: EmptyStore>>(item: T) -> T::CType {
    let mut store = Default::default();
    T::soft_encode(item, &mut store)
}

/// Perform the conversion from [`T::CType`](crate::ReprC::CType) into `T` without external storage.
///
/// Prefer using [`crate::decode`] whenever possible.
///
/// # Safety
///
/// - All conversions from a pointer must ensure pointer validity beforehand
pub(crate) unsafe fn decode_owned<'d, T: DecodeOwned<'d, Store: EmptyStore + 'd>>(
    source: T::CType,
) -> Option<T> {
    unsafe fn extend_store_lifetime<'d, S>(store: &mut S) -> &'d mut S {
        unsafe { core::mem::transmute::<&mut S, &'d mut S>(store) }
    }

    let mut store = T::Store::default();
    // SAFETY: When `T::Store` implements `EmptyStore`, `T::soft_decode` must not return
    // references into the store, so extending this borrow cannot make local backing data escape.
    let store = unsafe { extend_store_lifetime(&mut store) };
    unsafe { T::soft_decode(source, store) }
}

impl Store for () {
    fn sync(self) -> Option<()> {
        Some(())
    }
}

#[cfg(feature = "alloc")]
impl<D: Store> Store for Box<D> {
    fn sync(self) -> Option<()> {
        (*self).sync()
    }
}

#[cfg(feature = "alloc")]
impl<D: Store> Store for Box<[D]> {
    fn sync(self) -> Option<()> {
        let mut is_valid = true;

        for store in self {
            if store.sync().is_none() {
                is_valid = false;
            }
        }

        is_valid.then_some(())
    }
}

pub struct RefSizedEncodeStore<R: EncodeOwned> {
    pub(crate) ctype: Option<R::CType>,
    pub(crate) store: R::Store,
}

#[cfg(feature = "alloc")]
pub struct RefDstEncodeStore<R: ToOwned<Owned: EncodeOwned> + ?Sized> {
    pub(crate) ctype: Option<<R::Owned as ReprC>::CType>,
    pub(crate) store: <R::Owned as EncodeOwned>::Store,
}

pub struct InteriorMutSizedEncodeStore<'d, R: ReprC<CType: Sized>> {
    pub(crate) ctype: Option<R::CType>,
    pub(crate) original: Option<&'d R>,
}

// FIXME:
//#[cfg(feature = "alloc")]
//pub struct InteriorMutDstEncodeStore<'d, R: ToOwned<Owned: EncodeOwned> + ?Sized> {
//    pub(crate) ctype: Option<<R::Owned as ReprC>::CType>,
//    pub(crate) original: Option<&'d R>,
//}

pub struct RefMutSizedEncodeStore<'d, R: EncodeOwned> {
    pub(crate) ctype: Option<R::CType>,
    pub(crate) store: R::Store,
    pub(crate) original: Option<&'d mut R>,
}

#[cfg(feature = "alloc")]
pub struct RefMutDstEncodeStore<'d, R: ToOwned<Owned: EncodeOwned> + ?Sized> {
    pub(crate) ctype: Option<<R::Owned as ReprC>::CType>,
    pub(crate) store: <R::Owned as EncodeOwned>::Store,
    pub(crate) original: Option<&'d mut R>,
}

pub struct RefSizedDecodeStore<R, S> {
    pub(crate) value: Option<R>,
    pub(crate) store: S,
}

pub struct RefMutSizedDecodeStore<R: ReprC, S> {
    pub(crate) value: Option<R>,
    pub(crate) store: S,
    pub(crate) source: Option<*mut R::CType>,
}

#[cfg(feature = "alloc")]
pub struct RefDstDecodeStore<R: ToOwned + ?Sized, S> {
    pub(crate) value: Option<R::Owned>,
    pub(crate) store: S,
}

#[cfg(feature = "alloc")]
pub struct RefMutSliceDecodeStore<R: ReprC<CType: Sized>, S> {
    pub(crate) value: Option<Vec<R>>,
    pub(crate) store: S,
    pub(crate) source: Option<CSliceMut<R::CType>>,
}

/// This struct exists only because [arrays don't yet implement Default](https://github.com/rust-lang/rust/issues/61415)
pub struct ArrayStore<D, const N: usize>(pub(crate) [D; N]);

impl<R: EncodeOwned> Default for RefSizedEncodeStore<R> {
    fn default() -> Self {
        Self {
            ctype: None,
            store: Default::default(),
        }
    }
}

impl<R: EncodeOwned> Store for RefSizedEncodeStore<R> {
    fn sync(self) -> Option<()> {
        self.store.sync()
    }
}

#[cfg(feature = "alloc")]
impl<R: ToOwned<Owned: EncodeOwned> + ?Sized> Default for RefDstEncodeStore<R> {
    fn default() -> Self {
        Self {
            ctype: None,
            store: Default::default(),
        }
    }
}

#[cfg(feature = "alloc")]
impl<R: ToOwned<Owned: EncodeOwned> + ?Sized> Store for RefDstEncodeStore<R> {
    fn sync(self) -> Option<()> {
        self.store.sync()
    }
}

impl<'d, R: EncodeOwned> Default for RefMutSizedEncodeStore<'d, R> {
    fn default() -> Self {
        Self {
            ctype: Default::default(),
            store: Default::default(),
            original: Default::default(),
        }
    }
}

// TODO: I'd bet the store doesn't have to be empty on decode during sync
// it should be possible to traverse the ctype and update current type.
impl<'d, R> Store for RefMutSizedEncodeStore<'d, R>
where
    R: EncodeOwned + DecodeOwned<'d, Store: EmptyStore + 'd>,
{
    fn sync(self) -> Option<()> {
        let original = self.original?;
        self.store.sync()?;
        let ctype = self.ctype?;

        let value = unsafe { decode_owned(ctype)? };
        *original = value;
        Some(())
    }
}

impl<'d, R: ReprC<CType: Sized>> Default for InteriorMutSizedEncodeStore<'d, R> {
    fn default() -> Self {
        Self {
            ctype: Default::default(),
            original: Default::default(),
        }
    }
}

// &mut &(u32,)
// TODO: I'd bet the store doesn't have to be empty on decode during sync
// it should be possible to traverse the ctype and update current type.
impl<'d, R: InteriorMut<Target: DecodeOwned<'d, Store: EmptyStore>>> Store
    for InteriorMutSizedEncodeStore<'d, R>
where
    R: ReprC<CType = <R::Target as ReprC>::CType>,
{
    fn sync(self) -> Option<()> {
        let original = self.original?;
        let ctype = self.ctype?;

        let value = unsafe { decode_owned::<'d, R::Target>(ctype)? };
        unsafe { original.get().write(value) };
        Some(())
    }
}

#[cfg(feature = "alloc")]
impl<'d, R: ToOwned<Owned: EncodeOwned> + ?Sized> Default for RefMutDstEncodeStore<'d, R> {
    fn default() -> Self {
        Self {
            ctype: Default::default(),
            store: Default::default(),
            original: Default::default(),
        }
    }
}

#[cfg(feature = "alloc")]
impl<'d, R: AssignFromOwned> Store for RefMutDstEncodeStore<'d, R>
where
    R: ToOwned<Owned: EncodeOwned + DecodeOwned<'d, Store: EmptyStore>> + ?Sized,
{
    fn sync(self) -> Option<()> {
        let original = self.original?;
        self.store.sync()?;
        let ctype = self.ctype?;

        let owned = unsafe { decode_owned::<'d, R::Owned>(ctype)? };
        original.assign_from_owned(owned)
    }
}

impl<R, S: Default> Default for RefSizedDecodeStore<R, S> {
    fn default() -> Self {
        Self {
            value: None,
            store: Default::default(),
        }
    }
}

impl<R: ReprC, S: Default> Default for RefMutSizedDecodeStore<R, S> {
    fn default() -> Self {
        Self {
            value: None,
            store: Default::default(),
            source: None,
        }
    }
}

impl<R, S: Store> Store for RefSizedDecodeStore<R, S> {
    fn sync(self) -> Option<()> {
        self.store.sync()
    }
}

impl<R: EncodeOwned<Store: EmptyStore>, S: Store> Store for RefMutSizedDecodeStore<R, S> {
    fn sync(self) -> Option<()> {
        let source = self.source?;
        self.store.sync()?;
        let value = self.value?;
        unsafe { source.write(encode_owned(value)) };
        Some(())
    }
}

#[cfg(feature = "alloc")]
impl<R: ToOwned + ?Sized, S: Default> Default for RefDstDecodeStore<R, S> {
    fn default() -> Self {
        Self {
            value: None,
            store: Default::default(),
        }
    }
}

#[cfg(feature = "alloc")]
impl<R: ReprC<CType: Sized>, S: Default> Default for RefMutSliceDecodeStore<R, S> {
    fn default() -> Self {
        Self {
            value: None,
            store: Default::default(),
            source: None,
        }
    }
}

#[cfg(feature = "alloc")]
impl<R: ToOwned + ?Sized, S: Store> Store for RefDstDecodeStore<R, S> {
    fn sync(self) -> Option<()> {
        self.store.sync()
    }
}

#[cfg(feature = "alloc")]
impl<R: EncodeOwned<CType: Sized, Store: EmptyStore>, S: Store> Store
    for RefMutSliceDecodeStore<R, S>
{
    fn sync(self) -> Option<()> {
        let source = self.source?;
        self.store.sync()?;
        let value = self.value?;

        if source.len() != value.len() {
            return None;
        }

        let source = unsafe { source.into_rust()? };
        for (destination, value) in source.iter_mut().zip(value) {
            *destination = encode_owned(value);
        }
        Some(())
    }
}

impl<D: Default, const N: usize> Default for ArrayStore<D, N> {
    fn default() -> Self {
        Self(core::array::from_fn(|_| D::default()))
    }
}

impl<D: Store, const N: usize> Store for ArrayStore<D, N> {
    fn sync(self) -> Option<()> {
        let mut is_valid = true;

        for store in self.0 {
            if store.sync().is_none() {
                is_valid = false;
            }
        }

        is_valid.then_some(())
    }
}

unsafe impl<D: EmptyStore, const N: usize> EmptyStore for ArrayStore<D, N> {}

impl<T: Store> Store for Option<T> {
    fn sync(self) -> Option<()> {
        match self {
            Some(store) => store.sync(),
            None => Some(()),
        }
    }
}

impl<T: Store, E: Store> Store for Result<T, E> {
    fn sync(self) -> Option<()> {
        match self {
            Ok(store) => store.sync(),
            Err(store) => store.sync(),
        }
    }
}
