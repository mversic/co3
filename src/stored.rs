#[cfg(feature = "alloc")]
use alloc::{borrow::ToOwned, boxed::Box, vec::Vec};
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
    boxed::{CBox, CBoxCell, CWideBox, CWideBoxCell},
};

use crate::{
    CType, ReprC, Thin, WithDrop,
    borrow::{Borrow, BorrowCast, FromBorrow, borrow_cast},
    ffi::NulTerminatedBuf,
    niche::Niche,
    option::ReprCOption,
    reference::{CRef, CRefMut},
    result::ReprCResult,
    slice::{CWideRef, CWideRefMut},
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
    /// Encodes a value using storage chosen independently from [`EncodeOwned::Store`].
    ///
    /// # Safety
    ///
    /// If [`Self::SoftStore`] implements [`EmptyStore`], [`DirectEncodeOwned::soft_encode_unchecked`] must
    /// not return references into the store. [`crate::soft_encode_unchecked`] correctness relies on it.
    pub unsafe trait DirectEncodeOwned: ReprC<CType: Sized> + Sized {
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

    unsafe impl<R: ReprC + ?Sized> DirectEncodeOwned for &R
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType = CRef<<R as ReprC>::CType>>,
        R: RustSpec<Layout = Stable, Size: Thin, Mutability = Exclusive>,
    {
        type SoftStore = ();

        unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
            let ptr = core::ptr::from_ref(self);

            // TODO: Hack before `Thin` trait is available in stable:
            // https://doc.rust-lang.org/std/ptr/traitalias.Thin.html
            // https://github.com/rust-lang/rust/issues/81513
            unsafe { CRef::from_raw(core::mem::transmute_copy::<*const R, *const R::CType>(&ptr)) }
        }
    }

    unsafe impl<R: Wide<Metadata = usize> + ?Sized> DirectEncodeOwned for &R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = Exclusive>,
        R: ReprC<CType: Wide<Header: WideHeader<Data: CType + Sized>, Metadata = usize>>,
    {
        type SoftStore = ();

        unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
            let source = core::ptr::from_ref(self);
            let len = R::metadata(source);
            let data = R::as_ptr(source).cast();
            unsafe { CWideRef::from_raw_parts(data, len) }
        }
    }

    unsafe impl<R: DirectEncodeOwned + Clone, S: SizedKind> DirectEncodeOwned for &R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = RustSpecSized<S>, Mutability = Exclusive>,
    {
        type SoftStore = RefSizedEncodeStore<R, R::SoftStore>;

        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            let ctype = unsafe { self.clone().soft_encode_unchecked(&mut store.store) };
            CRef::from_ref(store.ctype.insert(ctype))
        }
    }

    unsafe impl<R: NulTerminatedBuf<Header: ReprC + CType> + ?Sized> DirectEncodeOwned for &R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = NulTerminated, Mutability = Exclusive>,
        <R as NulTerminatedBuf>::Header: RustSpec<Layout = Stable>,
        <<R as NulTerminatedBuf>::Header as ReprC>::CType: Sized,
    {
        type SoftStore = ();

        unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
            let ptr = R::as_ptr(self).cast();
            unsafe { CRef::from_raw(ptr) }
        }
    }

    unsafe impl<R: ReprC + ?Sized> DirectEncodeOwned for &R
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType = CRefMut<<R as ReprC>::CType, false>>,
        R: RustSpec<Layout = Stable, Size: Thin, Mutability = WithInterior>,
    {
        type SoftStore = ();

        unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
            let ptr = core::ptr::from_ref(self).cast_mut();
            unsafe { CRefMut::from_raw(core::mem::transmute_copy::<*mut R, *mut R::CType>(&ptr)) }
        }
    }

    unsafe impl<R: Wide<Metadata = usize> + ?Sized> DirectEncodeOwned for &R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = WithInterior>,
        R: ReprC<CType: Wide<Header: WideHeader<Data: CType + Sized>, Metadata = usize>>,
    {
        type SoftStore = ();

        unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
            let source = core::ptr::from_ref(self).cast_mut();
            let len = R::metadata(source);
            let ptr = R::as_mut_ptr(source).cast();
            unsafe { CWideRefMut::from_raw_parts_mut(ptr, len) }
        }
    }

    unsafe impl<R: NulTerminatedBuf<Header: ReprC + CType> + ?Sized> DirectEncodeOwned for &R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = NulTerminated, Mutability = WithInterior>,
        <R as NulTerminatedBuf>::Header: RustSpec<Layout = Stable>,
        <<R as NulTerminatedBuf>::Header as ReprC>::CType: Sized,
    {
        type SoftStore = ();

        unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
            let ptr = R::as_ptr(self).cast_mut().cast();
            unsafe { CRefMut::from_raw(ptr) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: ToOwned<Owned: DirectEncodeOwned<CType: BorrowCast<AsConst: Copy> + Copy>> + ?Sized>
        DirectEncodeOwned for &R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size: Dst>,
        Self: ReprC<CType = <<<R as ToOwned>::Owned as ReprC>::CType as BorrowCast>::AsConst>,
    {
        type SoftStore = RefDstEncodeStore<R, <R::Owned as DirectEncodeOwned>::SoftStore>;

        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            let owned = ToOwned::to_owned(self);
            let ctype = unsafe { owned.soft_encode_unchecked(&mut store.store) };
            borrow_cast(unsafe { core::ptr::read(store.ctype.insert(ctype)) })
        }
    }

    unsafe impl<R: ReprC + ?Sized> DirectEncodeOwned for &mut R
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType = CRefMut<<R as ReprC>::CType, true>>,
        R: RustSpec<Layout = Stable, Size: Thin, Mutability = Exclusive>,
    {
        type SoftStore = ();

        unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
            let ptr = core::ptr::from_mut(self);
            unsafe { CRefMut::from_raw(core::mem::transmute_copy::<*mut R, *mut R::CType>(&ptr)) }
        }
    }

    unsafe impl<R: Wide<Metadata = usize> + ?Sized> DirectEncodeOwned for &mut R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = Exclusive>,
        R: ReprC<CType: Wide<Header: WideHeader<Data: CType + Sized>, Metadata = usize>>,
    {
        type SoftStore = ();

        unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
            let len = R::metadata(self);
            let ptr = R::as_mut_ptr(self).cast();
            unsafe { CWideRefMut::from_raw_parts_mut(ptr, len) }
        }
    }

    unsafe impl<'a, R: DirectEncodeOwned + DecodeOwned<'a, Store: EmptyStore + 'a> + Clone, S: SizedKind>
        DirectEncodeOwned for &'a mut R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = RustSpecSized<S>, Mutability = Exclusive>,
    {
        type SoftStore = RefMutSizedEncodeStore<'a, R, R::SoftStore>;

        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            let original = store.original.insert(self);
            let ctype = unsafe { (**original).clone().soft_encode_unchecked(&mut store.store) };
            unsafe { CRefMut::from_raw(store.ctype.insert(ctype)) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'a, R: ToOwned<Owned: DirectEncodeOwned<CType: BorrowCastMut<AsMut: Copy> + Copy>> + ?Sized>
        DirectEncodeOwned for &'a mut R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size: Dst, Mutability = Exclusive> + AssignFromOwned,
        <R as ToOwned>::Owned: DecodeOwned<'a, Store: EmptyStore + 'a>,
        Self: ReprC<CType = <<<R as ToOwned>::Owned as ReprC>::CType as BorrowCastMut>::AsMut>,
    {
        type SoftStore = RefMutDstEncodeStore<'a, R, <R::Owned as DirectEncodeOwned>::SoftStore>;

        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            let original = store.original.insert(self);
            let owned = ToOwned::to_owned(&**original);
            let ctype = unsafe { owned.soft_encode_unchecked(&mut store.store) };
            borrow_cast_mut(unsafe { core::ptr::read(store.ctype.insert(ctype)) })
        }
    }

    unsafe impl<R: ReprC + ?Sized> DirectEncodeOwned for &mut R
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType = CRefMut<<R as ReprC>::CType, true>>,
        R: RustSpec<Layout = Stable, Size: Thin, Mutability = WithInterior>,
    {
        type SoftStore = ();

        unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
            let ptr = core::ptr::from_mut(self);
            unsafe { CRefMut::from_raw(core::mem::transmute_copy::<*mut R, *mut R::CType>(&ptr)) }
        }
    }

    unsafe impl<R: Wide<Metadata = usize> + ?Sized> DirectEncodeOwned for &mut R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = WithInterior>,
        R: ReprC<CType: Wide<Header: WideHeader<Data: CType + Sized>, Metadata = usize>>,
    {
        type SoftStore = ();

        unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
            let len = R::metadata(self);
            let ptr = R::as_mut_ptr(self).cast();
            unsafe { CWideRefMut::from_raw_parts_mut(ptr, len) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: ReprC, S: SizedKind> DirectEncodeOwned for Box<R>
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType = CBox<<R as ReprC>::CType>>,
        R: RustSpec<Layout = Stable, Size = RustSpecSized<S>, Mutability = Exclusive>,
        <R as ReprC>::CType: Sized,
    {
        type SoftStore = ();

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
            let ptr = Box::into_non_null(self).cast();
            unsafe { CBox::from_non_null(ptr) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: Wide<Metadata = usize> + ?Sized> DirectEncodeOwned for Box<R>
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = Exclusive>,
        R: ReprC<CType: Wide<Header: WideHeader<Data: CType + Sized>>>,
    {
        type SoftStore = ();

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
            let len = R::metadata(core::ptr::from_ref(&*self));
            let data = R::into_non_null(self).cast();
            unsafe { CWideBox::from_raw_parts(data, len) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: NulTerminatedBuf<Header: ReprC + CType> + ?Sized> DirectEncodeOwned for Box<R>
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = NulTerminated, Mutability = Exclusive>,
        <R as NulTerminatedBuf>::Header: RustSpec<Layout = Stable>,
        <<R as NulTerminatedBuf>::Header as ReprC>::CType: Sized,
    {
        type SoftStore = ();

        unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
            let ptr = R::into_non_null(self).cast();
            unsafe { CBox::from_non_null(ptr) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: DirectEncodeOwned, S: SizedKind> DirectEncodeOwned for Box<R>
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = RustSpecSized<S>, Mutability = Exclusive>,
    {
        type SoftStore = Box<R::SoftStore>;

        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            CBox::from_box(Box::new(unsafe { (*self).soft_encode_unchecked(store) }))
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: DirectEncodeOwned, S: SizedKind> DirectEncodeOwned for Box<R>
    where
        Self: ReprC<CType = CBoxCell<<R as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<S>, Mutability = WithInterior>,
    {
        type SoftStore = Box<R::SoftStore>;

        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            CBoxCell::from_box(Box::new(unsafe { (*self).soft_encode_unchecked(store) }))
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType + Sized>>> + Owned + ?Sized>
        DirectEncodeOwned for Box<R>
    where
        Self: RustSpec<Layout = Unstable> + Into<<R as Owned>::Owned>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = WithInterior>,
        <R as Owned>::Owned: DirectEncodeOwned<CType = CWideBoxCell<<R as ReprC>::CType>>,
    {
        type SoftStore = <R::Owned as DirectEncodeOwned>::SoftStore;

        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            unsafe { self.into().soft_encode_unchecked(store) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: NulTerminatedBuf<Header: ReprC + CType> + ?Sized> DirectEncodeOwned for Box<R>
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = NulTerminated, Mutability = WithInterior>,
        <R as NulTerminatedBuf>::Header: RustSpec<Layout = Stable>,
        <<R as NulTerminatedBuf>::Header as ReprC>::CType: Sized,
    {
        type SoftStore = ();

        unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
            let ptr = R::into_non_null(self).cast();
            unsafe { CBoxCell::from_non_null(ptr) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: Owned<Owned: DirectEncodeOwned> + ?Sized> DirectEncodeOwned for Box<R>
    where
        Self: RustSpec<Layout = Unstable> + Into<<R as Owned>::Owned>,
        R: RustSpec<Layout = Unstable, Size: Dst>,
        Self: ReprC<CType = <<R as Owned>::Owned as ReprC>::CType>,
    {
        type SoftStore = <R::Owned as DirectEncodeOwned>::SoftStore;

        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            unsafe { self.into().soft_encode_unchecked(store) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: ReprC> DirectEncodeOwned for Vec<R>
    where
        R: RustSpec<Layout = Stable, Mutability = Exclusive>,
        Self: ReprC<CType = CWideBox<[<R as ReprC>::CType]>>,
        <R as ReprC>::CType: Sized,
    {
        type SoftStore = ();

        #[inline(always)]
        unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
            let source = self.into_boxed_slice();

            let len = source.len();
            let data = Box::into_non_null(source).cast();
            unsafe { CWideBox::from_raw_parts(data, len) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: DirectEncodeOwned> DirectEncodeOwned for Vec<R>
    where
        R: RustSpec<Layout = Unstable, Mutability = Exclusive>,
        Self: ReprC<CType = CWideBox<[<R as ReprC>::CType]>>,
    {
        type SoftStore = Box<[R::SoftStore]>;

        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            CWideBox::from_box(unsafe { encode_vec_elements_unchecked(self, store) })
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: DirectEncodeOwned> DirectEncodeOwned for Vec<R>
    where
        R: RustSpec<Mutability = WithInterior>,
        Self: ReprC<CType = CWideBoxCell<[<R as ReprC>::CType]>>,
    {
        type SoftStore = Box<[R::SoftStore]>;

        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            CWideBoxCell::from_box(unsafe { encode_vec_elements_unchecked(self, store) })
        }
    }

    unsafe impl<R: DirectEncodeOwned> DirectEncodeOwned for Option<R>
    where
        R: RustSpec<Niche = WithoutNiche>,
    {
        type SoftStore = Option<R::SoftStore>;

        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            match self {
                Some(value) => ReprCOption::Some(unsafe {
                    value.soft_encode_unchecked(store.insert(Default::default()))
                }),
                None => {
                    *store = None;
                    ReprCOption::None()
                }
            }
        }
    }

    unsafe impl<R: DirectEncodeOwned + Niche, N: NicheStabilityKind> DirectEncodeOwned
        for Option<R>
    where
        R: RustSpec<Niche = WithNiche<N>>,
    {
        type SoftStore = Option<R::SoftStore>;

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

    unsafe impl<R: DirectEncodeOwned<CType: Copy>, E: DirectEncodeOwned<CType: Copy>, K: SizedKind> DirectEncodeOwned
        for Result<R, E>
    where
        Self: ReprC<CType = ReprCResult<<R as ReprC>::CType, <E as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<K>>,
        E: RustSpec<Size = RustSpecSized<K>>,
    {
        type SoftStore = Option<Result<R::SoftStore, E::SoftStore>>;

        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            match self {
                Ok(ok) => {
                    let Result::Ok(inner) = store.insert(Ok(Default::default())) else { unreachable!() };
                    ReprCResult::Ok(unsafe { ok.soft_encode_unchecked(inner) })
                }
                Err(err) => {
                    let Result::Err(inner) = store.insert(Err(Default::default())) else { unreachable!() };
                    ReprCResult::Err(unsafe { err.soft_encode_unchecked(inner) })
                }
            }
        }
    }

    unsafe impl<R: DirectEncodeOwned + Niche, E: DirectEncodeOwned<CType: Copy>, N: NicheStabilityKind>
        DirectEncodeOwned for Result<R, E>
    where
        Self: ReprC<CType = ReprCResult<<R as ReprC>::CType, <E as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = RustSpecSized<Zero>, Alignment = rust_spec::Gt<One>>,
    {
        type SoftStore = Option<Result<R::SoftStore, E::SoftStore>>;

        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            match self {
                Ok(ok) => {
                    let Result::Ok(inner) = store.insert(Ok(Default::default())) else { unreachable!() };
                    ReprCResult::Ok(unsafe { ok.soft_encode_unchecked(inner) })
                }
                Err(err) => {
                    let Result::Err(inner) = store.insert(Err(Default::default())) else { unreachable!() };
                    ReprCResult::Err(unsafe { err.soft_encode_unchecked(inner) })
                }
            }
        }
    }

    unsafe impl<R: DirectEncodeOwned<CType: Copy>, E: DirectEncodeOwned<CType: Copy> + Niche, N: NicheStabilityKind>
        DirectEncodeOwned for Result<R, E>
    where
        Self: ReprC<CType = ReprCResult<<R as ReprC>::CType, <E as ReprC>::CType>>,
        R: RustSpec<Size = RustSpecSized<Zero>, Alignment = rust_spec::Gt<One>>,
        E: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
    {
        type SoftStore = Option<Result<R::SoftStore, E::SoftStore>>;

        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            match self {
                Ok(ok) => {
                    let Result::Ok(inner) = store.insert(Ok(Default::default())) else { unreachable!() };
                    ReprCResult::Ok(unsafe { ok.soft_encode_unchecked(inner) })
                }
                Err(err) => {
                    let Result::Err(inner) = store.insert(Err(Default::default())) else { unreachable!() };
                    ReprCResult::Err(unsafe { err.soft_encode_unchecked(inner) })
                }
            }
        }
    }

    unsafe impl<R: DirectEncodeOwned + Niche, E: ReprC, N: NicheStabilityKind> DirectEncodeOwned
        for Result<R, E>
    where
        Self: ReprC<CType = <R as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = RustSpecSized<Zero>, Alignment = One>,
    {
        type SoftStore = Option<R::SoftStore>;

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

    unsafe impl<R: ReprC, E: DirectEncodeOwned + Niche, N: NicheStabilityKind> DirectEncodeOwned
        for Result<R, E>
    where
        Self: ReprC<CType = <E as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<Zero>, Alignment = One>,
        E: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
    {
        type SoftStore = Option<E::SoftStore>;

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

    unsafe impl<R: DirectEncodeOwned> DirectEncodeOwned for ManuallyDrop<R>
    where
        R: RustSpec<Drop = NoDrop>,
    {
        type SoftStore = R::SoftStore;

        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            unsafe { Self::into_inner(self).soft_encode_unchecked(store) }
        }
    }
    unsafe impl<R: DirectEncodeOwned> DirectEncodeOwned for ManuallyDrop<R>
    where
        R: RustSpec<Drop: WithDrop>,
    {
        type SoftStore = R::SoftStore;

        unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
            ManuallyDrop::new(unsafe { Self::into_inner(self).soft_encode_unchecked(store) })
        }
    }
}

disjoint_impls! {
    /// Facilitates conversion from a Rust type into a corresponding C-compatible representation.
    ///
    /// # Safety
    ///
    /// If [`Self::Store`] implements [`EmptyStore`], [`EncodeOwned::soft_encode`] must not
    /// return references into the store. [`crate::soft_encode`] correctness relies on it.
    pub unsafe trait EncodeOwned: DirectEncodeOwned {
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
            let encoded = unsafe { self.soft_encode_unchecked(&mut ()) };
            debug_assert_encoded_valid::<Self>(&encoded);
            encoded
        }
    }
    unsafe impl<R: Wide<Metadata = usize> + ?Sized> EncodeOwned for &R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = Exclusive>,
        R: CheckedTransmute<CType: Wide<Header: WideHeader<Data: CType + Sized>, Metadata = usize>>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = unsafe { self.soft_encode_unchecked(&mut ()) };
            debug_assert_wide_pointee_valid::<R>(encoded.data(), encoded.len());
            encoded
        }
    }

    unsafe impl<R: EncodeOwned + Clone, S: SizedKind> EncodeOwned for &R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = RustSpecSized<S>, Mutability = Exclusive>,
    {
        type Store = RefSizedEncodeStore<R, R::Store>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let owned = self.clone();
            let ctype = owned.soft_encode(&mut store.store);
            CRef::from_ref(store.ctype.insert(ctype))
        }
    }
    unsafe impl<R: NulTerminatedBuf + ?Sized> EncodeOwned for &R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = NulTerminated, Mutability = Exclusive>,
        <R as NulTerminatedBuf>::Header: RustSpec<Layout = Stable> + ReprC<CType: Sized> + CType,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            unsafe { self.soft_encode_unchecked(&mut ()) }
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
            let encoded = unsafe { self.soft_encode_unchecked(&mut ()) };
            debug_assert_encoded_valid::<Self>(&encoded);
            encoded
        }
    }
    unsafe impl<R: Wide<Metadata = usize> + ?Sized> EncodeOwned for &R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = WithInterior, Trap = Robust>,
        R: CheckedTransmute<CType: Wide<Header: WideHeader<Data: CType + Sized>, Metadata = usize>>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = unsafe { self.soft_encode_unchecked(&mut ()) };
            debug_assert_wide_pointee_valid::<R>(encoded.data(), encoded.len());
            encoded
        }
    }
    unsafe impl<R: NulTerminatedBuf + ?Sized> EncodeOwned for &R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = NulTerminated, Mutability = WithInterior, Trap = Robust>,
        <R as NulTerminatedBuf>::Header: RustSpec<Layout = Stable> + ReprC<CType: Sized> + CType,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            unsafe { self.soft_encode_unchecked(&mut ()) }
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
        type Store = RefDstEncodeStore<R, <R::Owned as EncodeOwned>::Store>;

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
            let encoded = unsafe { self.soft_encode_unchecked(&mut ()) };
            debug_assert_encoded_valid::<Self>(&encoded);
            encoded
        }
    }
    unsafe impl<'a, R: EncodeOwned + DecodeOwned<'a, Store: EmptyStore + 'a> + Clone>
        EncodeOwned for &'a mut R
    where
        Self: RustSpec<Layout = Stable> + CheckedTransmute<CType = CRefMut<<R as ReprC>::CType, true>>,
        R: RustSpec<Layout= Stable, Size: Thin, Mutability = Exclusive, Trap = NonRobust>,
    {
        type Store = RefMutSizedEncodeStore<'a, R, <R as EncodeOwned>::Store>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let ptr = encode_ref_mut_sized(self, store);
            let encoded = unsafe { CRefMut::from_raw(ptr) };
            debug_assert_encoded_valid::<Self>(&encoded);
            encoded
        }
    }
    unsafe impl<R: Wide<Metadata = usize> + ?Sized> EncodeOwned for &mut R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = Exclusive, Trap = Robust>,
        R: CheckedTransmute<CType: Wide<Header: WideHeader<Data: CType + Sized>, Metadata = usize>>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = unsafe { self.soft_encode_unchecked(&mut ()) };
            debug_assert_wide_pointee_valid::<R>(encoded.data(), encoded.len());
            encoded
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, R: Wide<Metadata = usize> + AssignFromOwned + ?Sized> EncodeOwned for &'a mut R
    where
        Self: RustSpec<Layout = Unstable> + DirectEncodeOwned<CType = <<<R as ToOwned>::Owned as ReprC>::CType as BorrowCastMut>::AsMut>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = Exclusive, Trap = NonRobust>,
        R: ToOwned<Owned: EncodeOwned<CType: BorrowCastMut<AsMut: Copy> + Copy> + DecodeOwned<'a>>,
        <<R as ToOwned>::Owned as DecodeOwned<'a>>::Store: EmptyStore,
        <<R as ToOwned>::Owned as EncodeOwned>::Store: EmptyStore,
    {
        type Store = RefMutDstEncodeStore<'a, R, <R::Owned as EncodeOwned>::Store>;

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
        type Store = RefMutSizedEncodeStore<'a, R, <R as EncodeOwned>::Store>;

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
        type Store = RefMutDstEncodeStore<'a, R, <R::Owned as EncodeOwned>::Store>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            encode_ref_mut_dst(self, store)
        }
    }

    // TODO: Verify the following `Mutability = WithInterior` impls are correct.
    // I'm quite sure many impls are missing because previously it was just delegating to `&T`.
    // They should not delegate to &T impls because we certain `&T` impls don't exist for `WithInterior`.
    // These have not been reviewed
    unsafe impl<R: ReprC + ?Sized> EncodeOwned for &mut R
    where
        Self: RustSpec<Layout = Stable> + CheckedTransmute<CType = CRefMut<<R as ReprC>::CType, true>>,
        R: RustSpec<Layout = Stable, Size: Thin, Mutability = WithInterior, Trap = Robust>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = unsafe { self.soft_encode_unchecked(&mut ()) };
            debug_assert_encoded_valid::<Self>(&encoded);
            encoded
        }
    }
    unsafe impl<R: Wide<Metadata = usize> + ?Sized> EncodeOwned for &mut R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = WithInterior, Trap = Robust>,
        R: CheckedTransmute<CType: Wide<Header: WideHeader<Data: CType + Sized>, Metadata = usize>>,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = unsafe { self.soft_encode_unchecked(&mut ()) };
            debug_assert_wide_pointee_valid::<R>(encoded.data(), encoded.len());
            encoded
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: EncodeOwned, S: SizedKind> EncodeOwned for Box<R>
    where
        Self: RustSpec<Layout = Stable> + CheckedTransmute<CType = CBox<<R as ReprC>::CType>>,
        R: RustSpec<Layout = Stable, Size = RustSpecSized<S>, Mutability = Exclusive>,
    {
        type Store = Box<R::Store>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            // FIXME: This doesn't work
            if impls::impls!(R::Store: EmptyStore) {
                let encoded = unsafe { self.soft_encode_unchecked(&mut ()) };
                debug_assert_encoded_valid::<Self>(&encoded);
                encoded
            } else {
                CBox::from_box(Box::new((*self).soft_encode(store)))
            }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: Wide<Metadata = usize> + Owned + ?Sized> EncodeOwned for Box<R>
    where
        Self: RustSpec<Layout = Unstable> + Into<<R as Owned>::Owned>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = Exclusive>,
        R: CheckedTransmute<CType: Wide<Header: WideHeader<Data: CType + Sized>, Metadata = usize>>,
        <R as Owned>::Owned: EncodeOwned<CType = CWideBox<<R as ReprC>::CType>>,
    {
        type Store = <R::Owned as EncodeOwned>::Store;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            // FIXME: This doesn't work
            if impls::impls!(<R::Owned as EncodeOwned>::Store: EmptyStore) {
                let encoded = unsafe { self.soft_encode_unchecked(&mut ()) };
                debug_assert_wide_pointee_valid::<R>(encoded.data(), encoded.len());
                return encoded;
            }

            encode_box_owned::<R>(self, store)
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: NulTerminatedBuf + ?Sized> EncodeOwned for Box<R>
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = NulTerminated, Mutability = Exclusive>,
        <R as NulTerminatedBuf>::Header: RustSpec<Layout = Stable> + ReprC<CType: Sized> + CType,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            unsafe { self.soft_encode_unchecked(&mut ()) }
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
    unsafe impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType + Sized>>> + Owned + ?Sized>
        EncodeOwned for Box<R>
    where
        Self: RustSpec<Layout = Unstable> + Into<<R as Owned>::Owned>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = WithInterior>,
        <R as Owned>::Owned: EncodeOwned<CType = CWideBoxCell<<R as ReprC>::CType>>,
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
    unsafe impl<R: NulTerminatedBuf + ?Sized> EncodeOwned for Box<R>
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = NulTerminated, Mutability = WithInterior>,
        <R as NulTerminatedBuf>::Header: RustSpec<Layout = Stable> + ReprC<CType: Sized> + CType,
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            unsafe { self.soft_encode_unchecked(&mut ()) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: Owned<Owned: EncodeOwned> + ?Sized> EncodeOwned for Box<R>
    where
        Self: RustSpec<Layout = Unstable> + Into<<R as Owned>::Owned>,
        Self: ReprC<CType = <<R as Owned>::Owned as ReprC>::CType>,
        R: RustSpec<Layout = Unstable, Size: Dst>,
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
    unsafe impl<R: EncodeOwned> EncodeOwned for Vec<R>
    where
        R: RustSpec<Layout = Stable, Mutability = Exclusive>,
        Self: ReprC<CType = CWideBox<[<R as ReprC>::CType]>>,
    {
        type Store = Box<[R::Store]>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            CWideBox::from_box(encode_vec_elements(self, store))
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: EncodeOwned> EncodeOwned for Vec<R>
    where
        R: RustSpec<Layout = Unstable, Mutability = Exclusive>,
        Self: ReprC<CType = CWideBox<[<R as ReprC>::CType]>>,
    {
        type Store = Box<[R::Store]>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            CWideBox::from_box(encode_vec_elements(self, store))
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: EncodeOwned> EncodeOwned for Vec<R>
    where
        R: RustSpec<Mutability = WithInterior>,
        Self: ReprC<CType = CWideBoxCell<[<R as ReprC>::CType]>>,
    {
        type Store = Box<[R::Store]>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            CWideBoxCell::from_box(encode_vec_elements(self, store))
        }
    }

    unsafe impl<R: EncodeOwned> EncodeOwned for Option<R>
    where
        R: RustSpec<Niche = WithoutNiche>,
    {
        type Store = Option<R::Store>;

        fn soft_encode<'itm>(self, store: &mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            match self {
                Some(value) => ReprCOption::Some(value.soft_encode(store.insert(Default::default()))),
                None => {
                    *store = None;
                    ReprCOption::None()
                }
            }
        }
    }
    // NOTE: PartialEq is only required for decoding, Here it is only used by `debug_assert!`
    unsafe impl<R: EncodeOwned + Niche<CType: PartialEq>, N: NicheStabilityKind> EncodeOwned
        for Option<R>
    where
        R: RustSpec<Niche = WithNiche<N>>,
    {
        type Store = Option<R::Store>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            if let Some(value) = self {
                let encoded = value.soft_encode(store.insert(Default::default()));

                debug_assert!(
                    encoded != R::NICHE,
                    "encoding produced the reserved NICHE"
                );

                return encoded;
            }

            *store = None;
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
        type Store = Option<R::Store>;

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
    unsafe impl<R: ReprC, E: EncodeOwned + Niche, N: NicheStabilityKind> EncodeOwned
        for Result<R, E>
    where
        Self: ReprC<CType = <E as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<Zero>, Alignment = One>,
        E: RustSpec<Size = RustSpecSized<rust_spec::Gt<Zero>>, Niche = WithNiche<N>>,
    {
        type Store = Option<E::Store>;

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

    unsafe impl<R: EncodeOwned> EncodeOwned for ManuallyDrop<R>
    where
        R: RustSpec<Drop = NoDrop>,
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

    unsafe impl<R: EncodeOwned> EncodeOwned for ManuallyDrop<R>
    where
        R: RustSpec<Drop: WithDrop>,
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

    unsafe impl<'d, R: ReprC + ?Sized> DecodeOwned<'d> for &'d R
    where
        Self: RustSpec<Layout = Stable> + CheckedTransmute<CType = CRef<<R as ReprC>::CType>>,
        R: RustSpec<Layout = Stable, Size: Thin, Mutability = Exclusive>,
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
    unsafe impl<'d, R: CheckedTransmute<CType: Wide<Metadata = usize>> + ?Sized>
        DecodeOwned<'d> for &'d R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = Exclusive>,
        WideData<<R as ReprC>::CType>: CType,
        R: Wide<Metadata = usize>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut ()) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            let ctype_slice = unsafe {
                R::CType::from_raw_parts(
                    source.data().cast::<<R::CType as Wide>::Header>(),
                    source.len(),
                )
            };
            if unsafe { !R::is_valid(ctype_slice) } {
                return None;
            }

            Some(unsafe { Self::soft_decode_unchecked(source, store) })
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            unsafe {
                R::from_raw_parts(source.data().cast::<<R as Wide>::Header>(), source.len())
            }
        }
    }
    unsafe impl<'d, R: NulTerminatedBuf + ?Sized> DecodeOwned<'d> for &'d R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = NulTerminated, Mutability = Exclusive>,
        <R as NulTerminatedBuf>::Header: RustSpec<Layout = Stable> + ReprC<CType: Sized> + CType,
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
            let source = unsafe { source.into_ref()? };
            let values = source.iter().map(|value| unsafe { core::ptr::read(value) });
            let value = unsafe { decode_ref_slice_elements(values, &mut store.store)? };
            Some(store.value.insert(value))
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let source = unsafe { source.into_ref().unwrap_unchecked() };
            let value = unsafe {
                decode_ref_slice_elements_unchecked(
                    source.iter().map(|value| core::ptr::read(value)),
                    &mut store.store,
                )
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
    unsafe impl<'d, R: CheckedTransmute<CType: Wide<Metadata = usize>> + ?Sized> DecodeOwned<'d> for &'d mut R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>>,
        WideData<<R as ReprC>::CType>: CType,
        R: Wide<Metadata = usize>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut ()) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            let ctype_slice = unsafe {
                R::CType::from_raw_parts(
                    source.data().cast::<<R::CType as Wide>::Header>(),
                    source.len(),
                )
            };

            if unsafe { !R::is_valid(ctype_slice) } {
                return None;
            }

            Some(unsafe { Self::soft_decode_unchecked(source, store) })
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            unsafe {
                R::from_raw_parts_mut(source.data().cast::<<R as Wide>::Header>(), source.len())
            }
        }
    }
    // TODO: NulTerminatedBuf needs a sound mutable constructor (from_raw_mut)
    // before this can be enabled; from_raw only creates a shared reference.
    // unsafe impl<'d, R: NulTerminatedBuf + ?Sized> DecodeOwned<'d> for &'d mut R
    // where
    //     R: RustSpec<Size = NulTerminated>,
    //     <R::Header as CheckedTransmute as ReprC>::CType: Sized>,
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

            let source = unsafe { source.into_mut()? };
            let values = source.iter().map(|value| unsafe { core::ptr::read(value) });
            let value = unsafe { decode_ref_slice_elements(values, &mut store.store)? };
            Some(store.value.insert(value))
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            store.source = Some(source);
            let source = unsafe { source.into_mut().unwrap_unchecked() };
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
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Unstable, Size = NulTerminated, Mutability = Exclusive>,
        <R as NulTerminatedBuf>::Header: RustSpec<Layout = Stable> + ReprC<CType: Sized> + CType,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            let ptr = NonNull::new(source.as_ptr())?;
            Some(unsafe { R::from_non_null(ptr.cast()) })
        }
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            let ptr = unsafe { NonNull::new_unchecked(source.as_ptr()) } ;
            unsafe { R::from_non_null(ptr.cast()) }
        }
    }
    // FIXME:
    //#[cfg(feature = "alloc")]
    //unsafe impl<'d, R: NulTerminatedBuf + ?Sized> DecodeOwned<'d> for Box<R>
    //where
    //    Self: RustSpec<Layout = Unstable>,
    //    R: RustSpec<Layout = Unstable, Size = NulTerminated, Mutability = WithInterior>,
    //{
    //    type Store = ();

    //    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
    //    }
    //}
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: DecodeOwned<'d>, S: SizedKind> DecodeOwned<'d> for Box<R>
    where
        R: RustSpec<Size = RustSpecSized<S>, Mutability = WithInterior>,
    {
        type Store = Box<R::Store>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = unsafe { source.into_box()? };
            let value = unsafe { R::soft_decode(*source, store)? };
            Some(Box::new(value))
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let source = unsafe { source.into_box().unwrap_unchecked() };
            Box::new(unsafe { R::soft_decode_unchecked(*source, store) })
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: CheckedTransmute<CType: Wide<Metadata = usize>> + ?Sized>
        DecodeOwned<'d> for Box<R>
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = Exclusive>,
        WideData<R>: ReprC<CType = WideData<<R as ReprC>::CType>>,
        <WideData<R> as ReprC>::CType: CType,
        R: Wide<Metadata = usize>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            let len = source.len();
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

        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            let len = source.len();
            let data = source.data().cast();

            unsafe { R::from_non_null(NonNull::new_unchecked(data), len) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: Owned + ReprC<CType: Wide<Header: WideHeader<Data: CType + Sized>>> + ?Sized>
        DecodeOwned<'d> for Box<R>
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = WithInterior>,
        <R as Owned>::Owned: DecodeOwned<'d, CType = CWideBoxCell<<R as ReprC>::CType>> + Into<Self>,
    {
        type Store = <R::Owned as DecodeOwned<'d>>::Store;

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
            let source = unsafe { source.into_box()? };
            let value = unsafe { R::soft_decode(*source, store)? };
            Some(Box::new(value))
        }

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
    unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for Vec<R>
    where
        R: RustSpec<Mutability = Exclusive>,
        Self: ReprC<CType = CWideBox<[<R as ReprC>::CType]>>,
    {
        type Store = Box<[R::Store]>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = unsafe { source.into_box()? };
            unsafe { decode_vec_elements(source, store) }
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let source = unsafe { source.into_box().unwrap_unchecked() };
            unsafe { decode_vec_elements_unchecked(source, store) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for Vec<R>
    where
        R: RustSpec<Mutability = WithInterior>,
        Self: ReprC<CType = CWideBoxCell<[<R as ReprC>::CType]>>,
    {
        type Store = Box<[R::Store]>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store
        ) -> Option<Self> {
            let source = unsafe { source.into_box()? };
            unsafe { decode_vec_elements(source, store) }
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let source = unsafe { source.into_box().unwrap_unchecked() };
            unsafe { decode_vec_elements_unchecked(source, store) }
        }
    }

    unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for Option<R>
    where
        R: RustSpec<Niche = WithoutNiche>,
    {
        type Store = Option<R::Store>;

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

    unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for ManuallyDrop<R>
    where
        R: RustSpec<Drop = NoDrop>,
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
        R: RustSpec<Drop: WithDrop>,
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
pub fn encode_owned<T: EncodeOwned<Store: EmptyStore>>(item: T) -> T::CType {
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
pub unsafe fn decode_owned<'d, T: DecodeOwned<'d, Store: EmptyStore + 'd>>(
    source: T::CType,
) -> Option<T> {
    unsafe { with_empty_decode_store::<T, _>(|store| T::soft_decode(source, store)) }
}

#[doc(hidden)]
pub(crate) unsafe fn encode_owned_unchecked<T: DirectEncodeOwned<SoftStore: EmptyStore>>(
    item: T,
) -> T::CType {
    let mut store = Default::default();
    unsafe { T::soft_encode_unchecked(item, &mut store) }
}

#[doc(hidden)]
pub(crate) unsafe fn decode_owned_unchecked<'d, T: DecodeOwned<'d, Store: EmptyStore + 'd>>(
    source: T::CType,
) -> T {
    unsafe { with_empty_decode_store::<T, _>(|store| T::soft_decode_unchecked(source, store)) }
}

#[inline(always)]
fn debug_assert_encoded_valid<R: CheckedTransmute>(encoded: &R::CType) {
    debug_assert!(
        unsafe { R::is_valid(encoded) },
        "encoding failed CheckedTransmute::is_valid"
    );
}

#[inline(always)]
fn debug_assert_wide_pointee_valid<R>(data: *const WideData<R::CType>, len: usize)
where
    R: CheckedTransmute<CType: Wide<Metadata = usize>> + ?Sized,
{
    debug_assert!(
        unsafe {
            let ctype = R::CType::from_raw_parts(data.cast(), len);
            R::is_valid(ctype)
        },
        "encoding failed CheckedTransmute::is_valid"
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
        assert!(self.len() == owned.len());

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
    store: &mut RefMutSizedEncodeStore<'a, R, <R as EncodeOwned>::Store>,
) -> *mut <R as ReprC>::CType {
    let original = store.original.insert(value);
    let owned = (**original).clone();
    let ctype = owned.soft_encode(&mut store.store);
    store.ctype.insert(ctype)
}

#[cfg(feature = "alloc")]
fn encode_ref_dst<R: ToOwned<Owned: EncodeOwned> + ?Sized>(
    value: &R,
    store: &mut RefDstEncodeStore<R, <R::Owned as EncodeOwned>::Store>,
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
    store: &mut RefMutDstEncodeStore<'a, R, <R::Owned as EncodeOwned>::Store>,
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
) -> Box<[<R as ReprC>::CType]> {
    *store = (0..value.len()).map(|_| Default::default()).collect();

    value
        .into_iter()
        .zip(store)
        .map(|(item, store)| item.soft_encode(store))
        .collect()
}

#[cfg(feature = "alloc")]
unsafe fn encode_vec_elements_unchecked<R: DirectEncodeOwned>(
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
