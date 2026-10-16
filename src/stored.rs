#[cfg(feature = "alloc")]
use alloc::{borrow::ToOwned, boxed::Box, vec::Vec};
#[cfg(feature = "alloc")]
use core::cell::UnsafeCell;
use core::mem::ManuallyDrop;
#[cfg(feature = "alloc")]
use core::ptr::NonNull;

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
use crate::sync::{
    RefDstDecodeStore, RefDstEncodeStore, RefMutDstEncodeStore, RefMutSliceDecodeStore,
};
#[cfg(feature = "alloc")]
use crate::{
    Dst,
    borrow::{BorrowCastMut, borrow_cast_mut},
    boxed::{CBox, CBoxCell, CBoxedSlice, CBoxedSliceCell},
};

use crate::{
    ReprC, Thin,
    borrow::{Borrow, BorrowCast, FromBorrow, borrow_cast},
    ffi::NulTerminatedBuf,
    niche::Niche,
    option::ReprCOption,
    reference::{CRef, CRefMut},
    result::ReprCResult,
    slice::{CSlice, CSliceMut},
    sync::{
        RefMutSizedDecodeStore, RefMutSizedEncodeStore, RefSizedDecodeStore, RefSizedEncodeStore,
    },
    transmute::CheckedTransmute,
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
        Self: RustSpec<Layout = Stable> + CheckedTransmute<CType = CRef<<R as ReprC>::CType>>,
        R: RustSpec<Layout = Stable, Size: Thin, Mutability = Exclusive>,
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
            unsafe { CRef::from_raw(core::mem::transmute_copy::<*const R, *const R::CType>(&ptr)) }
        }
    }
    unsafe impl<R: Wide<Header: WideHeader<Data: CheckedTransmute>, Metadata = usize> + ?Sized>
        EncodeOwned for &R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = Exclusive>,
        <<<R as Wide>::Header as WideHeader>::Data as ReprC>::CType: Sized,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let source = core::ptr::from_ref(self);

            let len = R::metadata(source);
            let data = R::as_ptr(source).cast();

            unsafe { CSlice::from_raw_parts(data, len) }
        }
    }
    unsafe impl<R: NulTerminatedBuf + ?Sized> EncodeOwned for &R
    where
        Self: RustSpec,
        R: RustSpec<Size = NulTerminated, Mutability = Exclusive>,
        <R as NulTerminatedBuf>::Data: CheckedTransmute<CType: Sized>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let ptr = R::as_ptr(core::ptr::from_ref(self)).cast();
            unsafe { CRef::from_raw(ptr) }
        }
    }
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
            CRef::from_ref(store.ctype.insert(ctype))
        }
    }

    unsafe impl<R: ReprC + ?Sized> EncodeOwned for &R
    where
        Self: RustSpec<Layout = Stable> + CheckedTransmute<CType = CRefMut<<R as ReprC>::CType, false>>,
        R: RustSpec<Layout = Stable, Size: Thin, Mutability = WithInterior, Trap = Robust>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let ptr = core::ptr::from_ref(self).cast_mut();

            // TODO: Hack before `Thin` trait is available in stable:
            // https://doc.rust-lang.org/std/ptr/traitalias.Thin.html
            // https://github.com/rust-lang/rust/issues/81513
            unsafe { CRefMut::from_raw(core::mem::transmute_copy::<*mut R, *mut R::CType>(&ptr)) }
        }
    }
    unsafe impl<R: ReprC + ?Sized> EncodeOwned for &R
    where
        Self: RustSpec<Layout = Stable> + CheckedTransmute<CType = CRefMut<<R as ReprC>::CType, false>>,
        R: RustSpec<Layout = Stable, Size: Thin, Mutability = WithInterior, Trap = NonRobust>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            // FIXME: This path has to use the store because it's mutating contents
            unimplemented!()
        }
    }
    unsafe impl<R: Wide<Header: WideHeader<Data: CheckedTransmute>, Metadata = usize> + ?Sized>
        EncodeOwned for &R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = WithInterior, Trap = Robust>,
        <<<R as Wide>::Header as WideHeader>::Data as ReprC>::CType: Sized,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let source = core::ptr::from_ref(self);
            let len = R::metadata(source);
            let ptr = R::as_ptr(source).cast_mut().cast();

            unsafe { CSliceMut::from_raw_parts_mut(ptr, len) }
        }
    }
    unsafe impl<R: Wide<Header: WideHeader<Data: CheckedTransmute>, Metadata = usize> + ?Sized>
        EncodeOwned for &R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = WithInterior, Trap = NonRobust>,
        <<<R as Wide>::Header as WideHeader>::Data as ReprC>::CType: Sized,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            // FIXME: This path has to use the store because it's mutating contents
            unimplemented!()
        }
    }
    unsafe impl<R: NulTerminatedBuf + ?Sized> EncodeOwned for &R
    where
        Self: RustSpec,
        R: RustSpec<Size = NulTerminated, Mutability = WithInterior, Trap = Robust>,
        <R as NulTerminatedBuf>::Data: CheckedTransmute<CType: Sized>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let ptr = R::as_ptr(core::ptr::from_ref(self)).cast_mut().cast();
            unsafe { CRefMut::from_raw(ptr) }
        }
    }
    unsafe impl<R: NulTerminatedBuf + ?Sized> EncodeOwned for &R
    where
        Self: RustSpec,
        R: RustSpec<Size = NulTerminated, Mutability = WithInterior, Trap = NonRobust>,
        <R as NulTerminatedBuf>::Data: CheckedTransmute<CType: Sized>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            // FIXME: This path has to use the store because it's mutating contents
            unimplemented!()
        }
    }
    // FIXME: I'm not sure if this should be split into Robust vs NonRobust case
    unsafe impl<R: EncodeOwned + Clone, S: SizedKind> EncodeOwned for &R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = RustSpecSized<S>, Mutability = WithInterior>,
    {
        type Store = RefSizedEncodeStore<R>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let owned = self.clone();
            let ctype = owned.soft_encode(&mut store.store);
            CRefMut::from_mut(store.ctype.insert(ctype))
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
            encode_ref_dst(self, store)
        }
    }

    unsafe impl<R: ReprC + ?Sized> EncodeOwned for &mut R
    where
        Self: RustSpec<Layout = Stable> + CheckedTransmute<CType = CRefMut<<R as ReprC>::CType, true>>,
        R: RustSpec<Layout = Stable, Size: Thin, Mutability = Exclusive, Trap = Robust>,
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
            unsafe { CRefMut::from_raw(core::mem::transmute_copy::<*mut R, *mut R::CType>(&ptr)) }
        }
    }
    unsafe impl<'a, R: EncodeOwned + DecodeOwned<'a, Store: EmptyStore + 'a> + Clone, S: SizedKind>
        EncodeOwned for &'a mut R
    where
        Self: RustSpec<Layout = Stable> + CheckedTransmute<CType = CRefMut<<R as ReprC>::CType, true>>,
        R: RustSpec<Layout= Stable, Size = RustSpecSized<S>, Mutability = Exclusive, Trap = NonRobust>,
    {
        type Store = RefMutSizedEncodeStore<'a, R>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let ptr = encode_ref_mut_sized(self, store);
            unsafe { CRefMut::from_raw(ptr) }
        }
    }
    unsafe impl<R: Wide<Metadata = usize> + ?Sized> EncodeOwned for &mut R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = Exclusive, Trap = Robust>,
        <<R as Wide>::Header as WideHeader>::Data: CheckedTransmute<CType: Sized>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let len = R::metadata(self);
            let ptr = R::as_mut_ptr(self).cast();

            unsafe { CSliceMut::from_raw_parts_mut(ptr, len) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, R: CheckedTransmute + AssignFromOwned + ?Sized>
        EncodeOwned for &'a mut R
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType = <<<R as ToOwned>::Owned as ReprC>::CType as BorrowCastMut>::AsMut>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = Exclusive, Trap = NonRobust>,
        R: ToOwned<Owned: EncodeOwned<CType: BorrowCastMut<AsMut: Copy> + Copy, Store: EmptyStore> + DecodeOwned<'a, Store: EmptyStore>>,
    {
        type Store = RefMutDstEncodeStore<'a, R>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            encode_ref_mut_dst(self, store)
        }
    }
    unsafe impl<'a, R: EncodeOwned + DecodeOwned<'a, Store: EmptyStore + 'a> + Clone, S: SizedKind>
        EncodeOwned for &'a mut R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = RustSpecSized<S>, Mutability = Exclusive>,
    {
        type Store = RefMutSizedEncodeStore<'a, R>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let ptr = encode_ref_mut_sized(self, store);
            unsafe { CRefMut::from_raw(ptr) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, R: ToOwned<Owned: EncodeOwned<CType: BorrowCastMut<AsMut: Copy> + Copy>> + ?Sized>
        EncodeOwned for &'a mut R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size: Dst, Mutability = Exclusive> + AssignFromOwned,
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

    // FIXME: Confirm the following 3 are correct
    unsafe impl<'a, R: ReprC + ?Sized> EncodeOwned for &'a mut R
    where
        R: RustSpec<Size: Thin, Mutability = WithInterior>,
        &'a R: EncodeOwned<CType = CRefMut<<R as ReprC>::CType, false>>,
    {
        type Store = <&'a R as EncodeOwned>::Store;

        fn soft_encode<'itm>(self, store: &mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let ptr = (&*self).soft_encode(store).as_ptr();
            unsafe { CRefMut::from_raw(ptr) }
        }
    }
    //unsafe impl<'a, R: NulTerminatedBuf<Data: ReprC> + ?Sized> EncodeOwned for &'a mut R
    //where
    //    Self: ReprC<CType = CRefMut<<<R as NulTerminatedBuf>::Data as ReprC>::CType, true>>,
    //    R: RustSpec<Size = NulTerminated, Mutability = WithInterior>,
    //    &'a R: EncodeOwned<CType = CRefMut<<<R as NulTerminatedBuf>::Data as ReprC>::CType, false>>,
    //
    //    type Store = <&'a R as EncodeOwned>::Store;

    //    fn soft_encode<'itm>(self, store: &mut Self::Store) -> Self::CType
    //    where
    //        Self: 'itm,
    //    {
    //        let ptr = (&*self).soft_encode(store);
    //        unsafe { CRefMut::from_raw(ptr) }
    //    }
    //}
    unsafe impl<'a, R: Wide<Header: WideHeader<Data: ReprC<CType: Sized>>, Metadata = usize> + ?Sized>
        EncodeOwned for &'a mut R
    where
        R: RustSpec<Size = MetaSized<SliceLike>, Mutability = WithInterior>,
        &'a R: EncodeOwned<CType = CSliceMut<<<<R as Wide>::Header as WideHeader>::Data as ReprC>::CType, false>>,
    {
        type Store = <&'a R as EncodeOwned>::Store;

        fn soft_encode<'itm>(self, store: &mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let c_slice = (&*self).soft_encode(store);

            let len = c_slice.len();
            let ptr = c_slice.data();

            unsafe { CSliceMut::from_raw_parts_mut(ptr, len) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: EncodeOwned, S: SizedKind> EncodeOwned for Box<R>
    where
        Self: RustSpec<Layout = Stable>,
        R: RustSpec<Layout = Stable, Size = RustSpecSized<S>, Mutability = Exclusive>,
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
                return unsafe { CBox::from_non_null(non_null_ptr) };
            }

            CBox::from_box(Box::new((*self).soft_encode(store)))
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: Wide<Header: WideHeader<Data: CheckedTransmute>, Metadata = usize> + Owned + ?Sized>
        EncodeOwned for Box<R>
    where
        Self: RustSpec<Layout = Unstable> + Into<<R as Owned>::Owned>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = Exclusive>,
        <R as Owned>::Owned: EncodeOwned<CType = CBoxedSlice<<WideData<R> as ReprC>::CType>>,
        <<<R as Wide>::Header as WideHeader>::Data as ReprC>::CType: Sized,
    {
        type Store = <R::Owned as EncodeOwned>::Store;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            // TODO: It's a bit stupid that this is the only conversion
            // where Store != () when type's Layout = Stable
            if impls::impls!(<R::Owned as EncodeOwned>::Store: EmptyStore) {
                let len = R::metadata(core::ptr::from_ref(&*self));
                let data = R::into_non_null(self).cast();

                return unsafe { CBoxedSlice::from_raw_parts(data, len) };
            }

            encode_box_owned::<R>(self, store)
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: NulTerminatedBuf + ?Sized> EncodeOwned for Box<R>
    where
        Self: RustSpec,
        R: RustSpec<Size = NulTerminated, Mutability = Exclusive>,
        <R as NulTerminatedBuf>::Data: CheckedTransmute<CType: Sized>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let ptr = R::into_non_null(self).cast();
            unsafe { CBox::from_non_null(ptr) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: EncodeOwned, S: SizedKind> EncodeOwned for Box<R>
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = RustSpecSized<S>, Mutability = Exclusive>,
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
    unsafe impl<R: EncodeOwned, S: SizedKind> EncodeOwned for Box<R>
    where
        Self: ReprC<CType = CBoxCell<<R as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<S>, Mutability = WithInterior>,
    {
        type Store = Box<R::Store>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            CBoxCell::from_box(Box::new((*self).soft_encode(store)))
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: Wide<Header: WideHeader<Data: ReprC>, Metadata = usize> + Owned + ?Sized>
        EncodeOwned for Box<R>
    where
        Self: RustSpec<Layout = Unstable> + Into<<R as Owned>::Owned>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = WithInterior>,
        <R as Owned>::Owned: EncodeOwned<CType = CBoxedSliceCell<<WideData<R> as ReprC>::CType>>,
        <<<R as Wide>::Header as WideHeader>::Data as ReprC>::CType: Sized,
    {
        type Store = <R::Owned as EncodeOwned>::Store;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            encode_box_owned::<R>(self, store)
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: NulTerminatedBuf<Data: CheckedTransmute<CType: Sized>> + ?Sized> EncodeOwned for Box<R>
    where
        Self: RustSpec,
        R: RustSpec<Size = NulTerminated, Mutability = WithInterior>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let ptr = R::into_non_null(self).cast();
            unsafe { CBoxCell::from_non_null(ptr) }
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
            encode_box_owned::<R>(self, store)
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: CheckedTransmute<CType: Sized>> EncodeOwned for Vec<R>
    where
        R: RustSpec<Mutability = Exclusive, Layout = Stable>,
        Self: ReprC<CType = CBoxedSlice<<R as ReprC>::CType>>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let source = self.into_boxed_slice();

            let len = source.len();
            let data = Box::into_non_null(source).cast();

            unsafe { CBoxedSlice::from_raw_parts(data, len) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: EncodeOwned> EncodeOwned for Vec<R>
    where
        R: RustSpec<Mutability = Exclusive, Layout = Unstable>,
        Self: ReprC<CType = CBoxedSlice<<R as ReprC>::CType>>,
    {
        type Store = Box<[R::Store]>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            CBoxedSlice::from_boxed_slice(encode_vec_elements(self, store))
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: EncodeOwned> EncodeOwned for Vec<R>
    where
        R: RustSpec<Mutability = WithInterior>,
        Self: ReprC<CType = CBoxedSliceCell<<R as ReprC>::CType>>,
    {
        type Store = Box<[R::Store]>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            CBoxedSliceCell::from_boxed_slice(encode_vec_elements(self, store))
        }
    }

    unsafe impl<R: EncodeOwned> EncodeOwned for Option<R>
    where
        R: RustSpec<Niche = WithoutNiche>,
    {
        type Store = R::Store;

        fn soft_encode<'itm>(self, store: &mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            match self {
                Some(value) => ReprCOption::Some(value.soft_encode(store)),
                None => ReprCOption::None(),
            }
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
                    encoded != R::NICHE,
                    "encoding produced the reserved NICHE"
                );

                return encoded;
            }

            R::NICHE
        }
    }

    unsafe impl<R: EncodeOwned<CType: Copy>, E: EncodeOwned<CType: Copy>, K: SizedKind> EncodeOwned
        for Result<R, E>
    where
        Self: ReprC<CType = ReprCResult<<R as ReprC>::CType, <E as ReprC>::CType>>,
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
        Self: ReprC<CType = ReprCResult<<R as ReprC>::CType, <E as ReprC>::CType>>,
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
        Self: ReprC<CType = ReprCResult<<R as ReprC>::CType, <E as ReprC>::CType>>,
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
    unsafe impl<R: EncodeOwned + Niche, E: ReprC, N: NicheStabilityKind> EncodeOwned
        for Result<R, E>
    where
        Self: ReprC<CType = <R as ReprC>::CType>,
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
                Err(_) => R::NICHE,
            }
        }
    }
    unsafe impl<R: ReprC, E: EncodeOwned + Niche, N: NicheStabilityKind> EncodeOwned
        for Result<R, E>
    where
        Self: ReprC<CType = <E as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<Zero>, Alignment = One>,
        E: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
    {
        type Store = E::Store;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            match self {
                Ok(_) => E::NICHE,
                Err(err) => err.soft_encode(store),
            }
        }
    }

    unsafe impl<R: CheckedTransmute<CType: Sized>> EncodeOwned for ManuallyDrop<R>
    where
        Self: RustSpec<Layout = Stable>,
    {
        type Store = ();

        #[inline(always)]
        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            unsafe { core::mem::transmute_copy(&self) }
        }
    }
    unsafe impl<R: EncodeOwned<CType: Sized>> EncodeOwned for ManuallyDrop<R>
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Drop = NoDrop>,
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
    /// Prefer using [`crate::soft_decode`] whenever possible.
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

    unsafe impl<'d, R: ReprC + ?Sized> DecodeOwned<'d> for &'d R
    where
        Self: RustSpec<Layout = Stable> + CheckedTransmute<CType = CRef<<R as ReprC>::CType>>,
        R: RustSpec<Size: Thin, Mutability = Exclusive>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if !unsafe { Self::is_valid(&source) } {
                return None;
            }

            // TODO: Hack before `Thin` trait is available in stable:
            // https://doc.rust-lang.org/std/ptr/traitalias.Thin.html
            // https://github.com/rust-lang/rust/issues/81513
            let ptr = source.as_ptr();
            unsafe { core::mem::transmute_copy::<*const R::CType, *const R>(&ptr).as_ref() }
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            let ptr = source.as_ptr();
            unsafe { &*core::mem::transmute_copy::<*const R::CType, *const R>(&ptr) }
        }
    }
    unsafe impl<'d, R: CheckedTransmute<CType: Wide<Metadata = usize>> + Wide<Metadata = usize> + ?Sized>
        DecodeOwned<'d> for &'d R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = Exclusive>,
        WideData<R>: ReprC<CType = WideData<<R as ReprC>::CType>>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut ()) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            let ctype_slice = unsafe { R::CType::from_raw_parts(source.data().cast(), source.len()) };
            if unsafe { !R::is_valid(ctype_slice) } {
                return None;
            }

            Some(unsafe { Self::soft_decode_unchecked(source, store) })
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            unsafe { R::from_raw_parts(source.data().cast(), source.len()) }
        }
    }
    unsafe impl<'d, R: NulTerminatedBuf + ?Sized> DecodeOwned<'d> for &'d R
    where
        Self: RustSpec,
        R: RustSpec<Size = NulTerminated, Mutability = Exclusive>,
        <R as NulTerminatedBuf>::Data: CheckedTransmute<CType: Sized>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut ()) -> Option<Self> {
            if source.as_ptr().is_null() {
                return None;
            }

            Some(unsafe { Self::soft_decode_unchecked(source, store) })
        }

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

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let ptr = source.as_ptr();
            unsafe { decode_ref_sized(ptr, store) }
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let ptr = source.as_ptr();
            let source = borrow_cast(unsafe { ptr.read() });
            let value = unsafe {
                DecodeOwned::soft_decode_unchecked(source, &mut store.store)
            };
            store.value.insert(FromBorrow::from_borrow(value))
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
        type Store = RefDstDecodeStore<[R], Box<[<<R as Borrow>::Borrowed<'d> as DecodeOwned<'d>>::Store]>>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = unsafe { source.into_rust()? };
            let values = source.iter().map(|value| unsafe { core::ptr::read(value) });
            let value = unsafe { decode_ref_slice_elements(values, &mut store.store)? };
            Some(store.value.insert(value))
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let source = unsafe { source.into_rust().unwrap_unchecked() };
            let value = unsafe {
                decode_ref_slice_elements_unchecked(
                    source.iter().map(|value| core::ptr::read(value)),
                    &mut store.store,
                )
            };
            store.value.insert(value)
        }
    }

    unsafe impl<'d, R: CheckedTransmute<CType: Sized> + ?Sized> DecodeOwned<'d> for &'d R
    where
        Self: RustSpec<Layout = Stable>,
        R: RustSpec<Size: Thin, Mutability = WithInterior>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            let source = source.as_ptr();
            if source.is_null() {
                return None;
            }

            if unsafe { !R::is_valid(&*source) } {
                return None;
            }

            // TODO: Hack before `Thin` trait is available in stable:
            // https://doc.rust-lang.org/std/ptr/traitalias.Thin.html
            // https://github.com/rust-lang/rust/issues/81513
            unsafe { core::mem::transmute_copy::<*mut R::CType, *mut R>(&source).as_ref() }
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            let source = source.as_ptr();
            unsafe { &*core::mem::transmute_copy::<*mut R::CType, *mut R>(&source) }
        }
    }
    unsafe impl<'d, R: CheckedTransmute<CType: Wide<Metadata = usize>> + Wide<Metadata = usize> + ?Sized>
        DecodeOwned<'d> for &'d R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = WithInterior>,
        WideData<R>: ReprC<CType = WideData<<R as ReprC>::CType>>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut ()) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            // TODO: We're creating a temporary reference to inner type of UnsafeCell.
            // That is not in itself a UB if that reference is not mutably aliased
            // However, I'm not entirely comfortale with it
            let ctype_slice = unsafe { R::CType::from_raw_parts(source.data().cast(), source.len()) };
            if unsafe { !R::is_valid(ctype_slice) } {
                return None;
            }

            Some(unsafe { Self::soft_decode_unchecked(source, store) })
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            unsafe { R::from_raw_parts(source.data().cast(), source.len()) }
        }
    }
    unsafe impl<'d, R: NulTerminatedBuf + ?Sized> DecodeOwned<'d> for &'d R
    where
        Self: RustSpec,
        R: RustSpec<Size = NulTerminated, Mutability = WithInterior>,
        <R as NulTerminatedBuf>::Data: CheckedTransmute<CType: Sized>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            let source = source.as_ptr();

            if source.is_null() {
                return None;
            }

            Some(unsafe { R::from_raw(source.cast()) })
        }
    }
    unsafe impl<'d, R: ReprC<CType: BorrowCast<AsConst: Copy> + Copy> + FromBorrow<'d>, S: SizedKind>
        DecodeOwned<'d> for &'d R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = RustSpecSized<S>, Mutability = WithInterior>,
        <R as Borrow>::Borrowed<'d>: DecodeOwned<'d, CType = <<R as ReprC>::CType as BorrowCast>::AsConst>,
    {
        type Store = RefSizedDecodeStore<R, <R::Borrowed<'d> as DecodeOwned<'d>>::Store>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let ptr = source.as_ptr();
            unsafe { decode_ref_sized(ptr, store) }
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let source = source.as_ptr();
            let source = borrow_cast(unsafe { source.read() });
            let value = unsafe {
                DecodeOwned::soft_decode_unchecked(source, &mut store.store)
            };
            store.value.insert(FromBorrow::from_borrow(value))
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: DecodeOwned<'d, CType: BorrowCast<AsConst: Copy> + Copy> + FromBorrow<'d> + Clone>
        DecodeOwned<'d> for &'d [R]
    where
        Self: RustSpec<Layout = Unstable>,
        [R]: RustSpec<Layout = Unstable, Size = MetaSized<SliceLike>, Mutability = WithInterior>,
        <R as Borrow>::Borrowed<'d>: DecodeOwned<'d, CType = <<R as ReprC>::CType as BorrowCast>::AsConst>,
    {
        type Store = RefDstDecodeStore<[R], Box<[<<R as Borrow>::Borrowed<'d> as DecodeOwned<'d>>::Store]>>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            let data = source.data().cast::<UnsafeCell<R::CType>>();
            let cells = unsafe { core::slice::from_raw_parts(data, source.len()) };
            let values = cells.iter().map(|cell| unsafe { cell.get().read() });
            let value = unsafe { decode_ref_slice_elements(values, &mut store.store)? };
            Some(store.value.insert(value))
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let cells = unsafe { core::slice::from_raw_parts(source.data().cast::<core::cell::UnsafeCell<R::CType>>(), source.len()) };
            let values = cells.iter().map(|cell| unsafe { cell.get().read() });
            let value = unsafe {
                decode_ref_slice_elements_unchecked(values, &mut store.store)
            };
            store.value.insert(value)
        }
    }

    unsafe impl<'d, R: ReprC + RustSpec<Size: Thin> + ?Sized> DecodeOwned<'d> for &'d mut R
    where
        Self: RustSpec<Layout = Stable> + CheckedTransmute<CType = CRefMut<<R as ReprC>::CType, true>>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if !unsafe { Self::is_valid(&source) } {
                return None;
            }

            // TODO: Hack before `Thin` trait is available in stable:
            // https://doc.rust-lang.org/std/ptr/traitalias.Thin.html
            // https://github.com/rust-lang/rust/issues/81513
            let source = source.as_ptr();
            unsafe { core::mem::transmute_copy::<*mut R::CType, *mut R>(&source).as_mut() }
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            let source = source.as_ptr();
            unsafe { &mut *core::mem::transmute_copy::<*mut R::CType, *mut R>(&source) }
        }
    }
    unsafe impl<'d, R: CheckedTransmute<CType: Wide<Metadata = usize>> + Wide<Metadata = usize> + ?Sized>
        DecodeOwned<'d> for &'d mut R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>>,
        WideData<R>: ReprC<CType = WideData<<R as ReprC>::CType>>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut ()) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            let ctype_slice = unsafe { R::CType::from_raw_parts(source.data().cast(), source.len()) };

            if unsafe { !R::is_valid(ctype_slice) } {
                return None;
            }

            Some(unsafe { Self::soft_decode_unchecked(source, store) })
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            unsafe { R::from_raw_parts_mut(source.data().cast(), source.len()) }
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
            let source = source.as_ptr();

            if source.is_null() {
                return None;
            }

            store.source = Some(source);
            let source = borrow_cast(unsafe { source.read() });
            let value = unsafe { DecodeOwned::soft_decode(source, &mut store.store)? };
            Some(store.value.insert(FromBorrow::from_borrow(value)))
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let source = source.as_ptr();
            store.source = Some(source);
            let source = borrow_cast(unsafe { source.read() });
            let value = unsafe {
                DecodeOwned::soft_decode_unchecked(source, &mut store.store)
            };
            store.value.insert(FromBorrow::from_borrow(value))
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: DecodeOwned<'d, CType: BorrowCast<AsConst: Copy> + Copy> + FromBorrow<'d> + EncodeOwned<Store: EmptyStore>>
        DecodeOwned<'d> for &'d mut [R]
    where
        Self: RustSpec<Layout = Unstable>,
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
            let values = source.iter().map(|value| unsafe { core::ptr::read(value) });
            let value = unsafe { decode_ref_slice_elements(values, &mut store.store)? };
            Some(store.value.insert(value))
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            store.source = Some(source);
            let source = unsafe { source.into_rust().unwrap_unchecked() };
            let value = unsafe {
                decode_ref_slice_elements_unchecked(
                    source.iter().map(|value| core::ptr::read(value)),
                    &mut store.store,
                )
            };
            store.value.insert(value)
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: ReprC<CType: Sized>, S: SizedKind> DecodeOwned<'d> for Box<R>
    where
        Self: RustSpec<Layout = Stable> + CheckedTransmute<CType = CBox<<R as ReprC>::CType>>,
        R: RustSpec<Layout = Stable, Size = RustSpecSized<S>, Mutability = Exclusive>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if !unsafe { Self::is_valid(&source) } {
                return None;
            }

            Some(unsafe { Self::soft_decode_unchecked(source, &mut ()) })
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            unsafe { Box::from_raw(source.as_ptr().cast()) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: NulTerminatedBuf + ?Sized> DecodeOwned<'d> for Box<R>
    where
        Self: RustSpec,
        R: RustSpec<Size = NulTerminated, Mutability = Exclusive>,
        <R as NulTerminatedBuf>::Data: CheckedTransmute<CType: Sized>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            let ptr = NonNull::new(source.as_ptr())?;
            Some(unsafe { R::from_non_null(ptr.cast()) })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: NulTerminatedBuf + ?Sized> DecodeOwned<'d> for Box<R>
    where
        Self: RustSpec,
        R: RustSpec<Size = NulTerminated, Mutability = WithInterior>,
        <R as NulTerminatedBuf>::Data: CheckedTransmute<CType: Sized>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            let ptr = NonNull::new(source.as_ptr())?;
            Some(unsafe { R::from_non_null(ptr.cast()) })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: DecodeOwned<'d, CType: Sized>, S: SizedKind> DecodeOwned<'d> for Box<R>
    where
        R: RustSpec<Size = RustSpecSized<S>, Mutability = WithInterior>,
    {
        type Store = Box<R::Store>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = unsafe { source.into_rust()? };
            let value = unsafe { R::soft_decode(*source, store)? };
            Some(Box::new(value))
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let source = unsafe { source.into_rust().unwrap_unchecked() };
            Box::new(unsafe { R::soft_decode_unchecked(*source, store) })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: CheckedTransmute<CType: Wide<Metadata = usize>> + Wide<Metadata = usize> + ?Sized>
        DecodeOwned<'d> for Box<R>
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = Exclusive>,
        WideData<R>: ReprC<CType = WideData<<R as ReprC>::CType>>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            let len = source.len();
            let data = source.data();

            if unsafe { !R::is_valid(R::CType::from_raw_parts(data.cast(), len)) } {
                let ptr = unsafe { NonNull::new_unchecked(data) };
                drop(unsafe { R::CType::from_non_null(ptr.cast(), len) });
                return None;
            }

            Some(unsafe { Self::soft_decode_unchecked(source, &mut ()) })
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            let len = source.len();
            let data = source.data().cast();

            unsafe { R::from_non_null(NonNull::new_unchecked(data), len) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: Owned + Wide<Metadata = usize> + ?Sized>
        DecodeOwned<'d> for Box<R>
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = WithInterior>,
        <R as Owned>::Owned: DecodeOwned<'d, CType = CBoxedSliceCell<<WideData<R> as ReprC>::CType>> + Into<Self>,
        <R as Wide>::Header: WideHeader<Data: ReprC<CType: Sized>>,
    {
        type Store = <<R as Owned>::Owned as DecodeOwned<'d>>::Store;

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { decode_box_owned::<R>(source, store) }
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
            unsafe { decode_box_owned_unchecked::<R>(source, store) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: DecodeOwned<'d>, S: SizedKind> DecodeOwned<'d> for Box<R>
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = RustSpecSized<S>, Mutability = Exclusive>,
    {
        type Store = Box<R::Store>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = unsafe { source.into_rust()? };
            let value = unsafe { R::soft_decode(*source, store)? };
            Some(Box::new(value))
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let source = unsafe { source.into_rust().unwrap_unchecked() };
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

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { decode_box_owned::<R>(source, store) }
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            unsafe { decode_box_owned_unchecked::<R>(source, store) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: CheckedTransmute> DecodeOwned<'d> for Vec<R>
    where
        R: RustSpec<Mutability = Exclusive, Layout = Stable>,
        Self: ReprC<CType = CBoxedSlice<<R as ReprC>::CType>>,
        <R as ReprC>::CType: Sized,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            let slice: &[_] = unsafe { borrow_cast(source).into_rust()? };

            if slice.iter().any(|slice| unsafe { !R::is_valid(slice) }) {
                drop(unsafe { source.into_rust() });
                return None;
            }

            Some(unsafe { Self::soft_decode_unchecked(source, &mut ()) })
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            let len = source.len();
            let data = source.data().cast();

            unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(data, len)) }.into()
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for Vec<R>
    where
        R: RustSpec<Mutability = Exclusive, Layout = Unstable>,
        Self: ReprC<CType = CBoxedSlice<<R as ReprC>::CType>>,
    {
        type Store = Box<[R::Store]>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = unsafe { source.into_rust()? };
            unsafe { decode_vec_elements(source, store) }
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let source = unsafe { source.into_rust().unwrap_unchecked() };
            unsafe { decode_vec_elements_unchecked(source, store) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for Vec<R>
    where
        R: RustSpec<Mutability = WithInterior>,
        Self: ReprC<CType = CBoxedSliceCell<<R as ReprC>::CType>>,
    {
        type Store = Box<[R::Store]>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store
        ) -> Option<Self> {
            let source = unsafe { source.into_rust()? };
            unsafe { decode_vec_elements(source, store) }
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let source = unsafe { source.into_rust().unwrap_unchecked() };
            unsafe { decode_vec_elements_unchecked(source, store) }
        }
    }

    unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for Option<R>
    where
        R: RustSpec<Niche = WithoutNiche>,
    {
        type Store = R::Store;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            match source.into_rust()? {
                Some(source) => unsafe { R::soft_decode(source, store) }.map(Some),
                None => Some(None),
            }
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            match unsafe { source.into_option_unchecked() } {
                Some(source) => Some(unsafe { R::soft_decode_unchecked(source, store) }),
                None => None,
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
            if source == R::NICHE {
                return Some(None);
            }

            unsafe { R::soft_decode(source, store) }.map(Some)
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: R::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            if source == R::NICHE {
                return None;
            }

            Some(unsafe { R::soft_decode_unchecked(source, store) })
        }
    }

    unsafe impl<'d, R: DecodeOwned<'d, CType: Copy>, E: DecodeOwned<'d, CType: Copy>, K: SizedKind>
        DecodeOwned<'d> for Result<R, E>
    where
        Self: ReprC<CType = ReprCResult<<R as ReprC>::CType, <E as ReprC>::CType>>,
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
        Self: ReprC<CType = ReprCResult<<R as ReprC>::CType, <E as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = RustSpecSized<Zero>, Alignment = rust_spec::Gt<One>>,
    {
        type Store = Option<Result<R::Store, E::Store>>;

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { decode_result(source, store) }
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
            unsafe { decode_result_unchecked(source, store) }
        }
    }
    unsafe impl<'d, R: DecodeOwned<'d, CType: Copy>, E: DecodeOwned<'d> + Niche, N: NicheStabilityKind>
        DecodeOwned<'d> for Result<R, E>
    where
        Self: ReprC<CType = ReprCResult<<R as ReprC>::CType, <E as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<Zero>, Alignment = rust_spec::Gt<One>>,
        E: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
    {
        type Store = Option<Result<R::Store, E::Store>>;

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { decode_result(source, store) }
        }

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

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            if source == R::NICHE {
                return Some(Err(E::default()));
            }

            unsafe { R::soft_decode(source, store) }.map(Ok)
        }

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

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            if source == E::NICHE {
                return Some(Ok(R::default()));
            }

            unsafe { E::soft_decode(source, store) }.map(Err)
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
            if source == E::NICHE {
                return Ok(R::default());
            }

            Err(unsafe { E::soft_decode_unchecked(source, store) })
        }
    }

    unsafe impl<'d, R: CheckedTransmute<CType: Sized>> DecodeOwned<'d> for ManuallyDrop<R>
    where
        Self: RustSpec<Layout = Stable>,
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
            unsafe { core::mem::transmute_copy(&source) }
        }
    }

    unsafe impl<'d, R: DecodeOwned<'d, CType: Sized>> DecodeOwned<'d> for ManuallyDrop<R>
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Drop = NoDrop>,
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

/// Perform the conversion from `T` into [`T::CType`] using external storage.
///
/// Prefer using [`crate::encode`] whenever possible.
pub(crate) fn encode_owned<T: EncodeOwned<Store: EmptyStore>>(item: T) -> T::CType {
    let mut store = Default::default();
    T::soft_encode(item, &mut store)
}

pub(crate) unsafe fn decode_owned<'d, T: DecodeOwned<'d, Store: EmptyStore + 'd>>(
    source: T::CType,
) -> Option<T> {
    unsafe { with_empty_decode_store::<T, _>(|store| T::soft_decode(source, store)) }
}

pub(crate) unsafe fn decode_owned_unchecked<'d, T: DecodeOwned<'d, Store: EmptyStore + 'd>>(
    source: T::CType,
) -> T {
    unsafe { with_empty_decode_store::<T, _>(|store| T::soft_decode_unchecked(source, store)) }
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
    let value = unsafe { DecodeOwned::soft_decode(source, &mut store.store)? };
    Some(store.value.insert(R::from_borrow(value)))
}

#[cfg(feature = "alloc")]
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
fn encode_ref_dst<R: ToOwned<Owned: EncodeOwned> + ?Sized>(
    value: &R,
    store: &mut RefDstEncodeStore<R>,
) -> <<R::Owned as ReprC>::CType as BorrowCast>::AsConst
where
    R::Owned: ReprC<CType: BorrowCast<AsConst: Copy> + Copy>,
{
    let owned = ToOwned::to_owned(value);
    let ctype = owned.soft_encode(&mut store.store);
    let ctype = store.ctype.insert(ctype);
    borrow_cast(unsafe { core::ptr::read(ctype) })
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
    let ctype = store.ctype.insert(ctype);
    borrow_cast_mut(unsafe { core::ptr::read(ctype) })
}

#[cfg(feature = "alloc")]
fn encode_box_owned<R: Owned + ?Sized>(
    value: Box<R>,
    store: &mut <R::Owned as EncodeOwned>::Store,
) -> <R::Owned as ReprC>::CType
where
    Box<R>: Into<R::Owned>,
    R::Owned: EncodeOwned,
{
    value.into().soft_encode(store)
}

#[cfg(feature = "alloc")]
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
fn encode_vec_elements<R: EncodeOwned>(
    value: Vec<R>,
    store: &mut Box<[R::Store]>,
) -> Box<[R::CType]> {
    *store = (0..value.len()).map(|_| Default::default()).collect();

    value
        .into_iter()
        .zip(store)
        .map(|(item, store)| item.soft_encode(store))
        .collect()
}

#[cfg(feature = "alloc")]
unsafe fn decode_vec_elements<'d, 'itm: 'd, R: DecodeOwned<'d>>(
    source: Box<[R::CType]>,
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
unsafe fn decode_vec_elements_unchecked<'d, 'itm: 'd, R: DecodeOwned<'d>>(
    source: Box<[R::CType]>,
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
    let value = match source.into_rust()? {
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

unsafe fn decode_result_unchecked<
    'd,
    'i,
    R: DecodeOwned<'d, CType: Copy>,
    E: DecodeOwned<'d, CType: Copy>,
>(
    source: ReprCResult<R::CType, E::CType>,
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
