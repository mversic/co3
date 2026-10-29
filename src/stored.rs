#[cfg(feature = "alloc")]
use crate::vec::CVec;
#[cfg(feature = "alloc")]
use alloc::{borrow::ToOwned, boxed::Box, vec::Vec};
#[cfg(feature = "alloc")]
use core::ptr::NonNull;
use core::{
    cell::{Cell, UnsafeCell},
    mem::ManuallyDrop,
};
#[cfg(feature = "alloc")]
use rust_spec::size::NulTerminated;

pub use crate::decoded::{DecodeMut, DecodeRef};
pub use crate::mutable::{EncodeMut, EncodeMutUnchecked};
pub use crate::shared::{EncodeRef, EncodeRefUnchecked};
use disjoint_impls::disjoint_impls;
use rust_spec::{
    Gt, RustSpec, Stable, Unstable,
    drop::NoDrop,
    layout::{LayoutKind, NonRobust, Robust},
    mutability::{Exclusive, ExclusiveKind, WithInterior},
    niche::{WithNiche, WithoutNiche},
    size::{MetaSized, Sized as RustSpecSized, SizedKind, SliceLike, Zero},
};

#[cfg(feature = "alloc")]
use crate::{
    Dst,
    borrow::borrow_cast_mut,
    boxed::{FromBox, FromWideBox, OwnedBoxCarrier, OwnedMutView},
    ffi::NulTerminatedBuf,
    sync::RefMutDstEncodeStore,
};

use crate::{
    CType, NeedsDrop, ReprC, Thin, assert_arr_has_non_zero_len,
    borrow::{Borrowed, BorrowedDecodeStore, BorrowedEncodeStore, BorrowedOwnerEncode},
    niche::Niche,
    reference::{CRef, CRefMut, FromRawRef},
    result::CResult,
    slice::{CWideRefMut, FromRawWideRef},
    sync::ArrayStore,
    transmute::{AbiCompatible, AbiCompatibleValidate, InteriorTrap},
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
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
        where
            Self: 'itm;
    }

    unsafe impl<R: AbiCompatible + ?Sized> EncodeMoveUnchecked for &R
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType: FromRawRef<<R as ReprC>::CType>>,
        R: RustSpec<Layout = Stable, Size: Thin>,
    {
        type SoftStore = ();

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType where Self: 'itm {
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
        unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType where Self: 'itm {
            let source = core::ptr::from_ref(self);

            let len = R::metadata(source);
            let data = R::as_ptr(source).cast();

            unsafe { Self::CType::from_raw_reference(data, len) }
        }
    }

    unsafe impl<'a, R: EncodeRefUnchecked<'a> + ?Sized, __Interior: ExclusiveKind> EncodeMoveUnchecked
        for &'a R
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType: Sized>,
        R: RustSpec<Layout = Unstable, Mutability = Exclusive<__Interior>>,
    {
        type SoftStore = R::SoftStore;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
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
        unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType where Self: 'itm {
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
        unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType where Self: 'itm {
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
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
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
        unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType where Self: 'itm {
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
        unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType where Self: 'itm {
            let len = R::metadata(core::ptr::from_ref(&*self));
            let data = R::into_non_null(self).cast();
            unsafe { Self::CType::from_raw_parts(data, len) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: NulTerminatedBuf<Header: ReprC + CType> + ?Sized> EncodeMoveUnchecked for Box<R>
    where
        Self: RustSpec<Layout = Unstable>
            + ReprC<CType: FromBox<<<R as NulTerminatedBuf>::Header as ReprC>::CType>>,
        R: RustSpec<Layout = Unstable, Size = NulTerminated>,
        <R as NulTerminatedBuf>::Header: RustSpec<Layout = Stable>,
        <<R as NulTerminatedBuf>::Header as ReprC>::CType: Sized,
    {
        type SoftStore = ();

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType where Self: 'itm {
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
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
            let source = Box::new(unsafe { (*self).soft_encode_unchecked(store) });
            unsafe { Self::CType::from_non_null(Box::into_non_null(source)) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: Owned<Owned: EncodeMoveUnchecked> + ?Sized> EncodeMoveUnchecked for Box<R>
    where
        Self: RustSpec<Layout = Unstable> + Into<<R as Owned>::Owned>,
        R: RustSpec<Layout = Unstable, Size: Dst>,
        Self: ReprC<CType: Sized>,
        <<R as Owned>::Owned as ReprC>::CType: OwnedBoxCarrier<<Self as ReprC>::CType>,
    {
        type SoftStore = <R::Owned as EncodeMoveUnchecked>::SoftStore;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
            unsafe { self.into().soft_encode_unchecked(store).into_box_carrier() }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: AbiCompatible> EncodeMoveUnchecked for crate::vec::Vec<R>
    where
        R: RustSpec<Layout = Stable>,
        R: ReprC<CType: CType + Sized>,
    {
        type SoftStore = ();
        unsafe fn soft_encode_unchecked<'itm>(self, _store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
            unsafe { crate::transmute::transmute_owned(self) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: EncodeMoveUnchecked> EncodeMoveUnchecked for crate::vec::Vec<R>
    where
        R: RustSpec<Layout = Unstable>,
        R: ReprC<CType: CType + Sized>,
    {
        type SoftStore = Box<[R::SoftStore]>;
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
            CVec::from_vec(unsafe { encode_vec_elements_unchecked(self.into(), store) })
        }
    }

    unsafe impl<R: EncodeMoveUnchecked> EncodeMoveUnchecked for Option<R>
    where
        R: RustSpec<Niche = WithoutNiche>,
    {
        type SoftStore = <crate::option::Option<R> as EncodeMoveUnchecked>::SoftStore;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
            unsafe { crate::option::Option::from(self).soft_encode_unchecked(store) }
        }
    }

    unsafe impl<R: EncodeMoveUnchecked + Niche, N: LayoutKind> EncodeMoveUnchecked
        for Option<R>
    where
        R: RustSpec<Niche = WithNiche<N>>,
    {
        type SoftStore = Option<R::SoftStore>;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
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
        type SoftStore = <crate::result::Result<R, E> as EncodeMoveUnchecked>::SoftStore;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
            unsafe { crate::result::Result::from(self).soft_encode_unchecked(store) }
        }
    }

    unsafe impl<R: EncodeMoveUnchecked + Niche, E: ReprC, N: LayoutKind> EncodeMoveUnchecked
        for Result<R, E>
    where
        Self: ReprC<CType = <R as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<Gt<Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = RustSpecSized<Zero>>,
    {
        type SoftStore = Option<R::SoftStore>;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
            match self {
                Ok(ok) => unsafe { ok.soft_encode_unchecked(store.insert(Default::default())) },
                Err(_) => {
                    *store = None;
                    R::NICHE
                }
            }
        }
    }

    unsafe impl<R: ReprC, E: EncodeMoveUnchecked + Niche, N: LayoutKind> EncodeMoveUnchecked
        for Result<R, E>
    where
        Self: ReprC<CType = <E as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<Zero>>,
        E: RustSpec<Size = RustSpecSized<Gt<Zero>>, Niche = WithNiche<N>>,
    {
        type SoftStore = Option<E::SoftStore>;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
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
    where
        R: RustSpec<Layout = Stable>,
        Self: AbiCompatible<CType: Sized>,
    {
        type SoftStore = ();
        unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType where Self: 'itm {
            unsafe { core::mem::transmute_copy(&self) }
        }
    }
    unsafe impl<R: EncodeMoveUnchecked> EncodeMoveUnchecked for ManuallyDrop<R>
    where
        R: RustSpec<Layout = Unstable, Drop = NoDrop>,
    {
        type SoftStore = R::SoftStore;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
            unsafe { Self::into_inner(self).soft_encode_unchecked(store) }
        }
    }
    unsafe impl<R: EncodeMoveUnchecked> EncodeMoveUnchecked for ManuallyDrop<R>
    where
        R: RustSpec<Layout = Unstable, Drop: NeedsDrop>,
    {
        type SoftStore = R::SoftStore;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
            ManuallyDrop::new(unsafe { Self::into_inner(self).soft_encode_unchecked(store) })
        }
    }

    unsafe impl<R: EncodeMoveUnchecked + AbiCompatible<CType: Sized>, const N: usize>
        EncodeMoveUnchecked for [R; N]
    where
        R: RustSpec<Layout = Stable, Mutability: InteriorTrap<Trap = Robust>>,
    {
        type SoftStore = ();
        unsafe fn soft_encode_unchecked<'itm>(self, _: &'itm mut ()) -> Self::CType where Self: 'itm {
            assert_arr_has_non_zero_len::<N>();
            // SAFETY: The owned wrapper/array preserves its ABI-compatible payload layout.
            unsafe { crate::transmute::transmute_owned(self) }
        }
    }
    unsafe impl<R: EncodeMoveUnchecked, const N: usize> EncodeMoveUnchecked for [R; N]
    where
        R: RustSpec<Layout = Stable, Mutability: InteriorTrap<Trap = NonRobust>>,
    {
        type SoftStore = ArrayStore<R::SoftStore, N>;

        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
            unsafe { encode_unchecked_array_elements(self, store) }
        }
    }
    unsafe impl<R: EncodeMoveUnchecked, const N: usize> EncodeMoveUnchecked for [R; N]
    where
        R: RustSpec<Layout = Unstable>,
    {
        type SoftStore = ArrayStore<R::SoftStore, N>;

        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
            unsafe { encode_unchecked_array_elements(self, store) }
        }
    }

    unsafe impl<R: EncodeMoveUnchecked + AbiCompatible<CType: Sized>> EncodeMoveUnchecked
        for UnsafeCell<R>
    where
        R: RustSpec<Layout = Stable, Mutability: InteriorTrap<Trap = Robust>>,
    {
        type SoftStore = ();
        unsafe fn soft_encode_unchecked<'itm>(self, _: &'itm mut ()) -> Self::CType where Self: 'itm {
            // SAFETY: The owned wrapper/array preserves its ABI-compatible payload layout.
            unsafe { crate::transmute::transmute_owned(self) }
        }
    }
    unsafe impl<R: EncodeMoveUnchecked> EncodeMoveUnchecked for UnsafeCell<R>
    where
        R: RustSpec<Layout = Stable, Mutability: InteriorTrap<Trap = NonRobust>>,
    {
        type SoftStore = R::SoftStore;

        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
            UnsafeCell::new(unsafe { self.into_inner().soft_encode_unchecked(store) })
        }
    }
    unsafe impl<R: EncodeMoveUnchecked> EncodeMoveUnchecked for UnsafeCell<R>
    where
        R: RustSpec<Layout = Unstable>,
    {
        type SoftStore = R::SoftStore;

        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
            UnsafeCell::new(unsafe { self.into_inner().soft_encode_unchecked(store) })
        }
    }

    unsafe impl<R: EncodeMoveUnchecked + AbiCompatible<CType: Sized>> EncodeMoveUnchecked for Cell<R>
    where
        R: RustSpec<Layout = Stable, Mutability: InteriorTrap<Trap = Robust>>,
    {
        type SoftStore = ();
        unsafe fn soft_encode_unchecked<'itm>(self, _: &'itm mut ()) -> Self::CType where Self: 'itm {
            // SAFETY: The owned wrapper/array preserves its ABI-compatible payload layout.
            unsafe { crate::transmute::transmute_owned(self) }
        }
    }
    unsafe impl<R: EncodeMoveUnchecked> EncodeMoveUnchecked for Cell<R>
    where
        R: RustSpec<Layout = Stable, Mutability: InteriorTrap<Trap = NonRobust>>,
    {
        type SoftStore = R::SoftStore;

        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
            UnsafeCell::new(unsafe { self.into_inner().soft_encode_unchecked(store) })
        }
    }
    unsafe impl<R: EncodeMoveUnchecked> EncodeMoveUnchecked for Cell<R>
    where
        R: RustSpec<Layout = Unstable>,
    {
        type SoftStore = R::SoftStore;

        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
            UnsafeCell::new(unsafe { self.into_inner().soft_encode_unchecked(store) })
        }
    }

    unsafe impl<T: BorrowedOwnerEncode<CType: Copy>> EncodeMoveUnchecked for Borrowed<T>
    where
        T: RustSpec<Drop: NeedsDrop>,
    {
        type SoftStore = BorrowedEncodeStore<T, <T as BorrowedOwnerEncode>::ReferenceStore>;

        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
            Borrowed::into_inner(self).encode_borrowed(store)
        }
    }
    unsafe impl<'r, T: ?Sized> EncodeMoveUnchecked for Borrowed<&'r mut T>
    where
        &'r mut T: RustSpec<Drop = NoDrop>,
        &'r mut T: EncodeMoveUnchecked,
    {
        type SoftStore = <&'r mut T as EncodeMoveUnchecked>::SoftStore;

        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
            // SAFETY: Borrowed carriers are read-only; delegate without changing the reference.
            Borrowed::new(unsafe { Borrowed::into_inner(self).soft_encode_unchecked(store) })
        }
    }
    unsafe impl<'r, T: ?Sized> EncodeMoveUnchecked for Borrowed<&'r T>
    where
        &'r T: RustSpec<Drop = NoDrop>,
        &'r T: EncodeMoveUnchecked,
    {
        type SoftStore = <&'r T as EncodeMoveUnchecked>::SoftStore;

        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm {
            // SAFETY: Borrowed carriers are read-only; delegate without changing the reference.
            Borrowed::new(unsafe { Borrowed::into_inner(self).soft_encode_unchecked(store) })
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

    unsafe impl<R: AbiCompatibleValidate + ?Sized, __Interior: ExclusiveKind> EncodeMove for &R
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType = CRef<<R as ReprC>::CType>>,
        R: RustSpec<Layout = Stable, Size: Thin, Mutability = Exclusive<__Interior>>,
        <R as RustSpec>::Mutability: InteriorTrap<Trap = Robust>,
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
    unsafe impl<R: AbiCompatibleValidate + Wide<Metadata = usize> + ?Sized, __Interior: ExclusiveKind>
        EncodeMove for &R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = Exclusive<__Interior>>,
        <R as ReprC>::CType: Wide<Header: WideHeader<Data: CType>, Metadata = usize>,
        <R as RustSpec>::Mutability: InteriorTrap<Trap = Robust>,
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

    unsafe impl<'a, R: EncodeRef<'a> + ?Sized, __Interior: ExclusiveKind> EncodeMove for &'a R
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType: Sized>,
        R: RustSpec<Layout = Unstable, Mutability = Exclusive<__Interior>>,
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

    unsafe impl<R: AbiCompatibleValidate + ?Sized, __Trap> EncodeMove for &R
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType = CRefMut<<R as ReprC>::CType, false>>,
        R: RustSpec<Layout = Stable, Size: Thin, Mutability = WithInterior<__Trap>>,
        <R as RustSpec>::Mutability: InteriorTrap<Trap = Robust>,
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
    unsafe impl<R: AbiCompatibleValidate + Wide<Metadata = usize> + ?Sized, __Trap> EncodeMove for &R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = WithInterior<__Trap>>,
        <R as ReprC>::CType: Wide<Header: WideHeader<Data: CType>, Metadata = usize>,
        <R as RustSpec>::Mutability: InteriorTrap<Trap = Robust>,
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

    unsafe impl<R: AbiCompatibleValidate + ?Sized> EncodeMove for &mut R
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
    unsafe impl<'a, R: AbiCompatibleValidate + EncodeMut<'a>> EncodeMove for &'a mut R
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
    unsafe impl<R: AbiCompatibleValidate + Wide<Metadata = usize> + ?Sized> EncodeMove for &mut R
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
    unsafe impl<'a, R: AbiCompatibleValidate + Wide<Metadata = usize> + AssignFromOwned + ?Sized> EncodeMove
        for &'a mut R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Trap = NonRobust>,
        Self: ReprC<CType = <<<R as ToOwned>::Owned as ReprC>::CType as OwnedMutView>::View>,
        R: ToOwned<Owned: EncodeMove<CType: OwnedMutView> + DecodeOwned<'a>>,
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
            let ctype = store.encode(self, |original, fields| {
                let owned = ToOwned::to_owned(&*original);
                owned.soft_encode(fields)
            });

            unsafe { core::ptr::read(ctype) }.mut_view()
        }
    }

    unsafe impl<'a, R: EncodeMut<'a> + ?Sized> EncodeMove for &'a mut R
    where
        Self: RustSpec<Layout = Unstable> + ReprC<CType: Sized>,
        R: RustSpec<Layout = Unstable>,
    {
        type Store = R::Store;
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            self.encode_mut(store)
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: AbiCompatibleValidate + EncodeMove, S: SizedKind> EncodeMove for Box<R>
    where
        Self: RustSpec<Layout = Stable>,
        R: RustSpec<Layout = Stable, Size = RustSpecSized<S>, Trap = Robust>,
        Self: AbiCompatibleValidate<CType: FromBox<<R as ReprC>::CType>>,
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
    unsafe impl<R: AbiCompatibleValidate + EncodeMove, S: SizedKind> EncodeMove for Box<R>
    where
        Self: RustSpec<Layout = Stable>,
        R: RustSpec<Layout = Stable, Size = RustSpecSized<S>, Trap = NonRobust>,
        Self: AbiCompatibleValidate<CType: FromBox<<R as ReprC>::CType>>,
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
    unsafe impl<R: AbiCompatibleValidate + Wide<Metadata = usize> + ?Sized> EncodeMove for Box<R>
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
    unsafe impl<R: AbiCompatibleValidate + Wide<Metadata = usize> + Owned + ?Sized> EncodeMove for Box<R>
    where
        Self: RustSpec<Layout = Unstable> + Into<<R as Owned>::Owned>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Trap = NonRobust>,
        <R as ReprC>::CType: Wide<Header: WideHeader<Data: CType>, Metadata = usize>,
        <R as Owned>::Owned: EncodeMove,
        <<R as Owned>::Owned as ReprC>::CType: OwnedBoxCarrier<<Self as ReprC>::CType>,
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
        Self: ReprC<CType: Sized>,
        <<R as Owned>::Owned as ReprC>::CType: OwnedBoxCarrier<<Self as ReprC>::CType>,
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
    unsafe impl<R: AbiCompatibleValidate + EncodeMove> EncodeMove for crate::vec::Vec<R>
    where
        R: RustSpec<Layout = Stable, Trap = Robust>,
        R: ReprC<CType: CType + Sized>,
    {
        type Store = ();
        fn soft_encode<'itm>(self, _store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            unsafe { crate::transmute::transmute_owned(self) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: AbiCompatibleValidate + EncodeMove> EncodeMove for crate::vec::Vec<R>
    where
        R: RustSpec<Layout = Stable, Trap = NonRobust>,
        R: ReprC<CType: CType + Sized>,
    {
        type Store = Box<[R::Store]>;
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            CVec::from_vec(encode_vec_elements(self.into(), store))
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: EncodeMove> EncodeMove for crate::vec::Vec<R>
    where
        R: RustSpec<Layout = Unstable>,
        R: ReprC<CType: CType + Sized>,
    {
        type Store = Box<[R::Store]>;
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            CVec::from_vec(encode_vec_elements(self.into(), store))
        }
    }

    unsafe impl<R: EncodeMove> EncodeMove for Option<R>
    where
        R: RustSpec<Niche = WithoutNiche>,
    {
        type Store = <crate::option::Option<R> as EncodeMove>::Store;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            crate::option::Option::from(self).soft_encode(store)
        }
    }
    // NOTE: PartialEq is only required for decoding, Here it is only used by `debug_assert!`
    unsafe impl<R: EncodeMove + Niche<CType: PartialEq>, N: LayoutKind> EncodeMove
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
        type Store = <crate::result::Result<R, E> as EncodeMove>::Store;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            crate::result::Result::from(self).soft_encode(store)
        }
    }
    unsafe impl<R: EncodeMove + Niche, E: ReprC, N: LayoutKind> EncodeMove for Result<R, E>
    where
        Self: ReprC<CType = <R as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<Gt<Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = RustSpecSized<Zero>>,
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
    unsafe impl<R: ReprC, E: EncodeMove + Niche, N: LayoutKind> EncodeMove for Result<R, E>
    where
        Self: ReprC<CType = <E as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<Zero>>,
        E: RustSpec<Size = RustSpecSized<Gt<Zero>>, Niche = WithNiche<N>>,
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
    where
        R: RustSpec<Layout = Stable>,
        Self: AbiCompatibleValidate<CType: Sized>,
    {
        type Store = ();
        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
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
        R: RustSpec<Layout = Unstable, Drop: NeedsDrop>,
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

    unsafe impl<R: EncodeMove + AbiCompatibleValidate<CType: Sized>, const N: usize> EncodeMove for [R; N]
    where
        R: RustSpec<Layout = Stable, Trap = Robust>,
        Self: EncodeMoveUnchecked + ReprC<CType = [<R as ReprC>::CType; N]>,
    {
        type Store = ();
        fn soft_encode<'itm>(self, _: &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            assert_arr_has_non_zero_len::<N>();
            // SAFETY: Ownership is transferred and robust payloads require no validity conversion.
            unsafe { crate::transmute::transmute_owned(self) }
        }
    }
    unsafe impl<R: EncodeMove, const N: usize> EncodeMove for [R; N]
    where
        R: RustSpec<Layout = Stable, Trap = NonRobust>,
        Self: EncodeMoveUnchecked + ReprC<CType = [<R as ReprC>::CType; N]>,
    {
        type Store = ArrayStore<R::Store, N>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            encode_array_elements(self, store)
        }
    }
    unsafe impl<R: EncodeMove, const N: usize> EncodeMove for [R; N]
    where
        R: RustSpec<Layout = Unstable>,
    {
        type Store = ArrayStore<R::Store, N>;

        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            encode_array_elements(self, store)
        }
    }

    unsafe impl<R: EncodeMove + AbiCompatibleValidate<CType: Sized>> EncodeMove for UnsafeCell<R>
    where
        R: RustSpec<Layout = Stable, Trap = Robust>,
        Self: EncodeMoveUnchecked + ReprC<CType = UnsafeCell<<R as ReprC>::CType>>,
    {
        type Store = ();
        fn soft_encode<'itm>(self, _: &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            // SAFETY: Ownership is transferred and robust payloads require no validity conversion.
            unsafe { crate::transmute::transmute_owned(self) }
        }
    }
    unsafe impl<R: EncodeMove> EncodeMove for UnsafeCell<R>
    where
        R: RustSpec<Layout = Stable, Trap = NonRobust>,
        Self: EncodeMoveUnchecked + ReprC<CType = UnsafeCell<<R as ReprC>::CType>>,
    {
        type Store = R::Store;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            UnsafeCell::new(self.into_inner().soft_encode(store))
        }
    }
    unsafe impl<R: EncodeMove> EncodeMove for UnsafeCell<R>
    where
        R: RustSpec<Layout = Unstable>,
    {
        type Store = R::Store;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            UnsafeCell::new(self.into_inner().soft_encode(store))
        }
    }

    unsafe impl<R> EncodeMove for Cell<R>
    where
        R: EncodeMove + AbiCompatibleValidate<CType: Sized> + RustSpec<Layout = Stable, Trap = Robust>,
        Self: EncodeMoveUnchecked + ReprC<CType = UnsafeCell<<R as ReprC>::CType>>,
    {
        type Store = ();
        fn soft_encode<'itm>(self, _: &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            // SAFETY: Ownership is transferred and robust payloads require no validity conversion.
            unsafe { crate::transmute::transmute_owned(self) }
        }
    }
    unsafe impl<R: EncodeMove> EncodeMove for Cell<R>
    where
        R: RustSpec<Layout = Stable, Trap = NonRobust>,
        Self: EncodeMoveUnchecked + ReprC<CType = UnsafeCell<<R as ReprC>::CType>>,
    {
        type Store = R::Store;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            UnsafeCell::new(self.into_inner().soft_encode(store))
        }
    }
    unsafe impl<R: EncodeMove> EncodeMove for Cell<R>
    where
        R: RustSpec<Layout = Unstable>,
    {
        type Store = R::Store;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            UnsafeCell::new(self.into_inner().soft_encode(store))
        }
    }

    unsafe impl<T: BorrowedOwnerEncode<CType: Copy>> EncodeMove for Borrowed<T>
    where
        T: RustSpec<Drop: NeedsDrop>,
    {
        type Store = BorrowedEncodeStore<T, <T as BorrowedOwnerEncode>::ReferenceStore>;
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            Borrowed::into_inner(self).encode_borrowed(store)
        }
    }
    unsafe impl<'r, T: ?Sized> EncodeMove for Borrowed<&'r mut T>
    where
        &'r mut T: RustSpec<Drop = NoDrop>,
        &'r mut T: EncodeMoveUnchecked,
    {
        type Store = <&'r mut T as EncodeMoveUnchecked>::SoftStore;
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            // SAFETY: Borrowed carriers are read-only; delegate without changing the reference.
            Borrowed::new(unsafe { Borrowed::into_inner(self).soft_encode_unchecked(store) })
        }
    }
    unsafe impl<'r, T: ?Sized> EncodeMove for Borrowed<&'r T>
    where
        &'r T: RustSpec<Drop = NoDrop>,
        &'r T: EncodeMoveUnchecked,
    {
        type Store = <&'r T as EncodeMoveUnchecked>::SoftStore;
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            // SAFETY: Borrowed carriers are read-only; delegate without changing the reference.
            Borrowed::new(unsafe { Borrowed::into_inner(self).soft_encode_unchecked(store) })
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

    unsafe impl<'d, R: AbiCompatibleValidate + ?Sized, __Interior: ExclusiveKind> DecodeOwned<'d> for &'d R
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType = CRef<<R as ReprC>::CType>>,
        R: RustSpec<Layout = Stable, Size: Thin, Mutability = Exclusive<__Interior>>,
        <R as RustSpec>::Mutability: InteriorTrap<Trap = Robust>,
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
    unsafe impl<'d, R: AbiCompatibleValidate<CType: Wide<Metadata = usize>> + ?Sized, __Interior: ExclusiveKind>
        DecodeOwned<'d> for &'d R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = Exclusive<__Interior>>,
        WideData<<R as ReprC>::CType>: CType,
        R: Wide<Metadata = usize>,
        <R as RustSpec>::Mutability: InteriorTrap<Trap = Robust>,
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
            unsafe { R::from_raw_parts(source.data().cast::<R::Header>(), source.metadata()) }
        }
    }
    unsafe impl<'d, R: AbiCompatibleValidate + ?Sized, __Trap> DecodeOwned<'d> for &'d R
    where
        Self: RustSpec<Layout = Stable> + ReprC<CType = CRefMut<<R as ReprC>::CType, false>>,
        R: RustSpec<Layout = Stable, Size: Thin, Mutability = WithInterior<__Trap>>,
        <R as RustSpec>::Mutability: InteriorTrap<Trap = Robust>,
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
    unsafe impl<'d, R: AbiCompatibleValidate + Wide<Metadata = usize> + ?Sized, __Trap> DecodeOwned<'d>
        for &'d R
    where
        Self: RustSpec<Layout = Unstable>,
        R: RustSpec<Layout = Stable, Size = MetaSized<SliceLike>, Mutability = WithInterior<__Trap>>,
        <R as ReprC>::CType: Wide<Header: WideHeader<Data: CType>, Metadata = usize>,
        <R as RustSpec>::Mutability: InteriorTrap<Trap = Robust>,
    {
        type Store = ();

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut ()) -> Option<Self> {
            if source.is_null() {
                return None;
            }

            if !unsafe { wide_pointee_is_valid::<R>(source.data(), source.metadata()) } {
                return None;
            }
            Some(unsafe { Self::soft_decode_unchecked(source, store) })
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            unsafe { R::from_raw_parts(source.data().cast::<R::Header>(), source.metadata()) }
        }
    }
    unsafe impl<'d, R: AbiCompatibleValidate + RustSpec<Layout = Stable, Size: Thin> + ?Sized> DecodeOwned<'d>
        for &'d mut R
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
    unsafe impl<'d, R: AbiCompatibleValidate<CType: Wide<Metadata = usize>> + ?Sized> DecodeOwned<'d>
        for &'d mut R
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
            unsafe { R::from_raw_parts_mut(source.data().cast::<R::Header>(), source.metadata()) }
        }
    }

    unsafe impl<'d, R: DecodeRef<'d> + ?Sized> DecodeOwned<'d> for &'d R
    where
        R: RustSpec<Layout = Unstable>,
        Self: RustSpec<Layout = Unstable> + ReprC<CType: Sized>,
    {
        type Store = <R as DecodeRef<'d>>::Store;

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { R::decode_ref(source, store) }
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
            unsafe { R::decode_ref_unchecked(source, store) }
        }
    }

    unsafe impl<'d, R: DecodeMut<'d> + ?Sized> DecodeOwned<'d> for &'d mut R
    where
        R: RustSpec<Layout = Unstable>,
        Self: RustSpec<Layout = Unstable> + ReprC<CType: Sized>,
    {
        type Store = <R as DecodeMut<'d>>::Store;

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            unsafe { R::decode_mut(source, store) }
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
            unsafe { R::decode_mut_unchecked(source, store) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: AbiCompatibleValidate, S: SizedKind> DecodeOwned<'d> for Box<R>
    where
        Self: RustSpec<Layout = Stable>,
        R: RustSpec<Layout = Stable, Size = RustSpecSized<S>>,
        Self: AbiCompatibleValidate<CType: FromBox<<R as ReprC>::CType>>,
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
        Self: RustSpec<Layout = Unstable>
            + ReprC<CType: FromBox<<<R as NulTerminatedBuf>::Header as ReprC>::CType>>,
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
            let ptr = unsafe { NonNull::new_unchecked(borrow_cast_mut(source).as_ptr()) };
            unsafe { R::from_non_null(ptr.cast()) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: AbiCompatibleValidate<CType: Wide<Metadata = usize>> + ?Sized> DecodeOwned<'d> for Box<R>
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
                drop(unsafe { R::CType::from_non_null(ptr.cast::<<R::CType as Wide>::Header>(), len) });
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
        Self: RustSpec<Layout = Unstable> + ReprC<CType: Sized>,
        <<R as Owned>::Owned as ReprC>::CType: OwnedBoxCarrier<<Self as ReprC>::CType>,
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
    unsafe impl<'d, R: AbiCompatibleValidate> DecodeOwned<'d> for crate::vec::Vec<R>
    where
        R: RustSpec<Layout = Stable, Mutability: rust_spec::mutability::PointerMutability>,
        R: ReprC<CType: CType + Sized>,
    {
        type Store = ();
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, _: &'itm mut ()) -> Option<Self> {
            if !unsafe { <Self as crate::transmute::AbiCompatibleValidate>::is_valid(&source) } {
                // Reclaim the carrier allocation without constructing invalid R values.
                drop(unsafe { source.into_vec() });
                return None;
            }
            Some(unsafe { crate::transmute::transmute_owned(source) })
        }
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, _: &'itm mut ()) -> Self {
            unsafe { crate::transmute::transmute_owned(source) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for crate::vec::Vec<R>
    where
        R: RustSpec<Layout = Unstable>,
        R: ReprC<CType: CType + Sized>,
    {
        type Store = Box<[R::Store]>;
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = unsafe { source.into_vec()? };
            unsafe { decode_vec_elements(source, store) }.map(Into::into)
        }
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let source = unsafe { source.into_vec().unwrap_unchecked() };
            unsafe { decode_vec_elements_unchecked(source, store) }.into()
        }
    }

    unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for Option<R>
    where
        R: RustSpec<Niche = WithoutNiche>,
    {
        type Store = <crate::option::Option<R> as DecodeOwned<'d>>::Store;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { <crate::option::Option<R> as DecodeOwned<'d>>::soft_decode(source, store) }.map(Into::into)
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            unsafe { <crate::option::Option<R> as DecodeOwned<'d>>::soft_decode_unchecked(source, store) }.into()
        }
    }
    unsafe impl<'d, R: DecodeOwned<'d> + Niche<CType: PartialEq>, N: LayoutKind>
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
        type Store = <crate::result::Result<R, E> as DecodeOwned<'d>>::Store;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { <crate::result::Result<R, E> as DecodeOwned<'d>>::soft_decode(source, store) }.map(Into::into)
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            unsafe { <crate::result::Result<R, E> as DecodeOwned<'d>>::soft_decode_unchecked(source, store) }.into()
        }
    }
    unsafe impl<'d, R: DecodeOwned<'d> + Niche<CType: PartialEq>, E: Default + ReprC, N: LayoutKind>
        DecodeOwned<'d> for Result<R, E>
    where
        Self: ReprC<CType = <R as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<Gt<Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = RustSpecSized<Zero>>,
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
    unsafe impl<'d, R: Default + ReprC, E: DecodeOwned<'d> + Niche<CType: PartialEq>, N: LayoutKind>
        DecodeOwned<'d> for Result<R, E>
    where
        Self: ReprC<CType = <E as ReprC>::CType>,
        R: RustSpec<Size = RustSpecSized<Zero>>,
        E: RustSpec<Size = RustSpecSized<Gt<Zero>>, Niche = WithNiche<N>>,
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

    unsafe impl<'d, R, __Interior: ExclusiveKind> DecodeOwned<'d> for ManuallyDrop<R>
    where
        R: RustSpec<Layout = Stable, Mutability = Exclusive<__Interior>>,
        Self: AbiCompatibleValidate<CType: Sized>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &'itm mut ()) -> Option<Self> {
            if !unsafe { Self::is_valid(&source) } {
                return None;
            }
            Some(unsafe { Self::soft_decode_unchecked(source, &mut ()) })
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &'itm mut ()) -> Self {
            let source = ManuallyDrop::new(source);
            unsafe { core::mem::transmute_copy(&source) }
        }
    }

    unsafe impl<'d, R, __Interior> DecodeOwned<'d> for ManuallyDrop<R>
    where
        R: RustSpec<Layout = Stable, Trap = Robust, Mutability = WithInterior<__Interior>>,
        Self: AbiCompatibleValidate<CType: Sized>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &'itm mut ()) -> Option<Self> {
            // SAFETY: Robust, ABI-compatible values need no validation or payload reads.
            Some(unsafe { crate::transmute::transmute_owned(source) })
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &'itm mut ()) -> Self {
            // SAFETY: Ownership is transferred using the ABI-compatible wrapper layout.
            unsafe { crate::transmute::transmute_owned(source) }
        }
    }

    unsafe impl<'d, R: DecodeOwned<'d>, __Trap> DecodeOwned<'d> for ManuallyDrop<R>
    where
        R: RustSpec<
                Layout = Stable,
                Trap = NonRobust,
                Mutability = WithInterior<__Trap>,
                Drop = NoDrop,
            >,
    {
        type Store = R::Store;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { R::soft_decode(source, store) }.map(Self::new)
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            Self::new(unsafe { R::soft_decode_unchecked(source, store) })
        }
    }
    unsafe impl<'d, R: DecodeOwned<'d>, __Trap> DecodeOwned<'d> for ManuallyDrop<R>
    where
        R: RustSpec<
                Layout = Stable,
                Trap = NonRobust,
                Mutability = WithInterior<__Trap>,
                Drop: NeedsDrop,
            >,
    {
        type Store = R::Store;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { R::soft_decode(ManuallyDrop::into_inner(source), store) }.map(Self::new)
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            Self::new(unsafe { R::soft_decode_unchecked(ManuallyDrop::into_inner(source), store) })
        }
    }
    unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for ManuallyDrop<R>
    where
        R: RustSpec<Layout = Unstable, Drop = NoDrop>,
    {
        type Store = R::Store;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { R::soft_decode(source, store) }.map(Self::new)
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            Self::new(unsafe { R::soft_decode_unchecked(source, store) })
        }
    }
    unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for ManuallyDrop<R>
    where
        R: RustSpec<Layout = Unstable, Drop: NeedsDrop>,
    {
        type Store = R::Store;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { R::soft_decode(ManuallyDrop::into_inner(source), store) }.map(Self::new)
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            Self::new(unsafe { R::soft_decode_unchecked(ManuallyDrop::into_inner(source), store) })
        }
    }

    unsafe impl<'d, R: DecodeOwned<'d> + AbiCompatibleValidate<CType: Sized>, const N: usize> DecodeOwned<'d>
        for [R; N]
    where
        R: RustSpec<Layout = Stable, Trap = Robust>,
    {
        type Store = ();
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, _: &'itm mut ()) -> Option<Self> {
            assert_arr_has_non_zero_len::<N>();
            // SAFETY: Robust, ABI-compatible payloads have no invalid bit patterns to inspect.
            Some(unsafe { crate::transmute::transmute_owned(source) })
        }
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, _: &'itm mut ()) -> Self {
            assert_arr_has_non_zero_len::<N>();
            unsafe { crate::transmute::transmute_owned(source) }
        }
    }
    unsafe impl<'d, R: DecodeOwned<'d>, const N: usize> DecodeOwned<'d> for [R; N]
    where
        R: RustSpec<Layout = Stable, Trap = NonRobust>,
    {
        type Store = ArrayStore<R::Store, N>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { decode_array_elements(source, store) }
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            unsafe { decode_unchecked_array_elements(source, store) }
        }
    }
    unsafe impl<'d, R: DecodeOwned<'d>, const N: usize> DecodeOwned<'d> for [R; N]
    where
        R: RustSpec<Layout = Unstable>,
    {
        type Store = ArrayStore<R::Store, N>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { decode_array_elements(source, store) }
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            unsafe { decode_unchecked_array_elements(source, store) }
        }
    }

    unsafe impl<'d, R: DecodeOwned<'d> + AbiCompatibleValidate<CType: Sized>> DecodeOwned<'d> for UnsafeCell<R>
    where
        R: RustSpec<Layout = Stable, Trap = Robust>,
    {
        type Store = ();
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, _: &'itm mut ()) -> Option<Self> {
            // SAFETY: Robust, ABI-compatible payloads have no invalid bit patterns to inspect.
            Some(unsafe { crate::transmute::transmute_owned(source) })
        }
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, _: &'itm mut ()) -> Self {
            unsafe { crate::transmute::transmute_owned(source) }
        }
    }
    unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for UnsafeCell<R>
    where
        R: RustSpec<Layout = Stable, Trap = NonRobust>,
    {
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
    unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for UnsafeCell<R>
    where
        R: RustSpec<Layout = Unstable>,
    {
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

    unsafe impl<'d, R: DecodeOwned<'d> + AbiCompatibleValidate<CType: Sized>> DecodeOwned<'d> for Cell<R>
    where
        R: RustSpec<Layout = Stable, Trap = Robust>,
    {
        type Store = ();
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, _: &'itm mut ()) -> Option<Self> {
            // SAFETY: Robust, ABI-compatible payloads have no invalid bit patterns to inspect.
            Some(unsafe { crate::transmute::transmute_owned(source) })
        }
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, _: &'itm mut ()) -> Self {
            unsafe { crate::transmute::transmute_owned(source) }
        }
    }
    unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for Cell<R>
    where
        R: RustSpec<Layout = Stable, Trap = NonRobust>,
    {
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
    unsafe impl<'d, R: DecodeOwned<'d>> DecodeOwned<'d> for Cell<R>
    where
        R: RustSpec<Layout = Unstable>,
    {
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

    unsafe impl<'d, T: Clone + ReprC<CType: CType + Sized> + 'd> DecodeOwned<'d> for Borrowed<T>
    where
        T: RustSpec<Drop: NeedsDrop>,
        &'d T: DecodeOwned<'d, CType = CRef<<T as ReprC>::CType>>,
    {
        type Store = BorrowedDecodeStore<T::CType, <&'d T as DecodeOwned<'d>>::Store>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let (carrier, reference) = store.insert(Borrowed::into_inner(source));
            let carrier = CRef::from_ref(carrier);
            // SAFETY: The caller supplies a valid borrowed carrier; storage survives for 'd.
            let value = unsafe { <&'d T>::soft_decode(carrier, reference)? };
            Some(Borrowed::new(T::clone(value)))
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let (carrier, reference) = store.insert(Borrowed::into_inner(source));
            let carrier = CRef::from_ref(carrier);
            // SAFETY: The unchecked contract guarantees validity; the carrier is retained.
            let value = unsafe { <&'d T>::soft_decode_unchecked(carrier, reference) };
            Borrowed::new(T::clone(value))
        }
    }
    unsafe impl<'d, T: ?Sized> DecodeOwned<'d> for Borrowed<&'d mut T>
    where
        &'d mut T: RustSpec<Drop = NoDrop>,
        &'d mut T: DecodeOwned<'d>,
    {
        type Store = <&'d mut T as DecodeOwned<'d>>::Store;
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { <&'d mut T>::soft_decode(Borrowed::into_inner(source), store) }.map(Borrowed::new)
        }
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            Borrowed::new(unsafe {
                <&'d mut T>::soft_decode_unchecked(Borrowed::into_inner(source), store)
            })
        }
    }
    unsafe impl<'d, T: ?Sized> DecodeOwned<'d> for Borrowed<&'d T>
    where
        &'d T: RustSpec<Drop = NoDrop>,
        &'d T: DecodeOwned<'d>,
    {
        type Store = <&'d T as DecodeOwned<'d>>::Store;
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { <&'d T>::soft_decode(Borrowed::into_inner(source), store) }.map(Borrowed::new)
        }
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            Borrowed::new(unsafe {
                <&'d T>::soft_decode_unchecked(Borrowed::into_inner(source), store)
            })
        }
    }
}

#[cfg(feature = "alloc")]
unsafe impl<R: ReprC<CType: CType + Sized>> EncodeMoveUnchecked for Vec<R>
where
    crate::vec::Vec<R>: EncodeMoveUnchecked,
{
    type SoftStore = <crate::vec::Vec<R> as EncodeMoveUnchecked>::SoftStore;

    unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe { crate::vec::Vec::from(self).soft_encode_unchecked(store) }
    }
}

#[cfg(feature = "alloc")]
unsafe impl<R: ReprC<CType: CType + Sized>> EncodeMove for Vec<R>
where
    crate::vec::Vec<R>: EncodeMove,
{
    type Store = <crate::vec::Vec<R> as EncodeMove>::Store;

    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        crate::vec::Vec::from(self).soft_encode(store)
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'d, R: ReprC<CType: CType + Sized>> DecodeOwned<'d> for Vec<R>
where
    crate::vec::Vec<R>: DecodeOwned<'d>,
{
    type Store = <crate::vec::Vec<R> as DecodeOwned<'d>>::Store;
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        unsafe { crate::vec::Vec::<R>::soft_decode(source, store) }.map(Into::into)
    }
    unsafe fn soft_decode_unchecked<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Self {
        unsafe { crate::vec::Vec::<R>::soft_decode_unchecked(source, store) }.into()
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
fn debug_assert_encoded_valid<R: AbiCompatibleValidate>(encoded: &R::CType) {
    debug_assert!(
        unsafe { R::is_valid(encoded) },
        "encoding failed AbiCompatibleValidate::is_valid"
    );
}

#[inline(always)]
fn debug_assert_wide_pointee_valid<R>(data: *const WideData<R::CType>, len: usize)
where
    R: AbiCompatibleValidate<CType: Wide<Metadata = usize>> + ?Sized,
{
    debug_assert!(
        unsafe { wide_pointee_is_valid::<R>(data, len) },
        "encoding failed AbiCompatibleValidate::is_valid"
    );
}

#[inline(always)]
unsafe fn wide_pointee_is_valid<R>(data: *const WideData<R::CType>, len: usize) -> bool
where
    R: AbiCompatibleValidate<CType: Wide<Metadata = usize>> + ?Sized,
{
    let ctype = unsafe { R::CType::from_raw_parts(data.cast(), len) };
    unsafe { R::is_valid(ctype) }
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

#[cfg(feature = "alloc")]
#[inline(always)]
fn encode_box_owned<R: Owned + ?Sized>(
    value: Box<R>,
    store: &mut <R::Owned as EncodeMove>::Store,
) -> <Box<R> as ReprC>::CType
where
    Box<R>: Into<R::Owned>,
    R::Owned: EncodeMove,
    Box<R>: ReprC<CType: Sized>,
    <R::Owned as ReprC>::CType: OwnedBoxCarrier<<Box<R> as ReprC>::CType>,
{
    unsafe { value.into().soft_encode(store).into_box_carrier() }
}

#[cfg(feature = "alloc")]
#[inline(always)]
unsafe fn decode_box_owned<'d, 'itm: 'd, R: Owned + ?Sized>(
    source: <Box<R> as ReprC>::CType,
    store: &'itm mut <R::Owned as DecodeOwned<'d>>::Store,
) -> Option<Box<R>>
where
    R::Owned: DecodeOwned<'d> + Into<Box<R>>,
    Box<R>: ReprC<CType: Sized>,
    <R::Owned as ReprC>::CType: OwnedBoxCarrier<<Box<R> as ReprC>::CType>,
{
    let source = unsafe { <R::Owned as ReprC>::CType::from_box_carrier(source)? };
    unsafe { R::Owned::soft_decode(source, store) }.map(Into::into)
}

#[cfg(feature = "alloc")]
#[inline(always)]
unsafe fn decode_box_owned_unchecked<'d, 'itm: 'd, R: Owned + ?Sized>(
    source: <Box<R> as ReprC>::CType,
    store: &'itm mut <R::Owned as DecodeOwned<'d>>::Store,
) -> Box<R>
where
    R::Owned: DecodeOwned<'d> + Into<Box<R>>,
    Box<R>: ReprC<CType: Sized>,
    <R::Owned as ReprC>::CType: OwnedBoxCarrier<<Box<R> as ReprC>::CType>,
{
    let source = unsafe { <R::Owned as ReprC>::CType::from_box_carrier(source).unwrap_unchecked() };
    unsafe { R::Owned::soft_decode_unchecked(source, store) }.into()
}

#[cfg(feature = "alloc")]
#[inline(always)]
fn encode_vec_elements<R: EncodeMove>(
    value: Vec<R>,
    store: &mut Box<[R::Store]>,
) -> Vec<<R as ReprC>::CType> {
    *store = (0..value.len()).map(|_| Default::default()).collect();

    let mut encoded = Vec::with_capacity(value.capacity());
    encoded.extend(
        value
            .into_iter()
            .zip(store)
            .map(|(item, store)| item.soft_encode(store)),
    );
    encoded
}

#[cfg(feature = "alloc")]
#[inline(always)]
unsafe fn encode_vec_elements_unchecked<R: EncodeMoveUnchecked>(
    value: Vec<R>,
    store: &mut Box<[R::SoftStore]>,
) -> Vec<<R as ReprC>::CType> {
    *store = core::iter::repeat_with(Default::default)
        .take(value.len())
        .collect();

    let mut encoded = Vec::with_capacity(value.capacity());
    encoded.extend(
        value
            .into_iter()
            .zip(store.iter_mut())
            .map(|(item, store)| unsafe { item.soft_encode_unchecked(store) }),
    );
    encoded
}

#[cfg(feature = "alloc")]
#[inline(always)]
unsafe fn decode_vec_elements<'d, 'itm: 'd, R: DecodeOwned<'d>>(
    source: Vec<<R as ReprC>::CType>,
    store: &'itm mut Box<[R::Store]>,
) -> Option<Vec<R>> {
    let mut is_valid = true;

    *store = core::iter::repeat_with(Default::default)
        .take(source.len())
        .collect();

    let mut decoded = Vec::with_capacity(source.capacity());
    for (item, store) in source.into_iter().zip(store.iter_mut()) {
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
    source: Vec<<R as ReprC>::CType>,
    store: &'itm mut Box<[R::Store]>,
) -> Vec<R> {
    *store = core::iter::repeat_with(Default::default)
        .take(source.len())
        .collect();

    let mut decoded = Vec::with_capacity(source.capacity());
    decoded.extend(
        source
            .into_iter()
            .zip(store.iter_mut())
            .map(|(item, store)| unsafe { R::soft_decode_unchecked(item, store) }),
    );
    decoded
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

// Structural array fallbacks share element conversion; transmutation stays in the impls.
unsafe fn encode_unchecked_array_elements<R: EncodeMoveUnchecked, const N: usize>(
    value: [R; N],
    store: &mut ArrayStore<R::SoftStore, N>,
) -> [R::CType; N] {
    assert_arr_has_non_zero_len::<N>();

    let mut items = value.into_iter();
    let mut stores = store.0.iter_mut();

    core::array::from_fn(|_| {
        let item = items.next().unwrap();
        let store = stores.next().unwrap();

        unsafe { item.soft_encode_unchecked(store) }
    })
}

fn encode_array_elements<'itm, R: EncodeMove + 'itm, const N: usize>(
    value: [R; N],
    store: &'itm mut ArrayStore<R::Store, N>,
) -> [R::CType; N] {
    assert_arr_has_non_zero_len::<N>();

    let store = &mut store.0;

    let mut items = value.into_iter();
    let mut stores = store.iter_mut();

    core::array::from_fn(|_| {
        let item = items.next().unwrap();
        let store = stores.next().unwrap();

        item.soft_encode(store)
    })
}

unsafe fn decode_array_elements<'d, 'itm: 'd, R: DecodeOwned<'d>, const N: usize>(
    source: [R::CType; N],
    store: &'itm mut ArrayStore<R::Store, N>,
) -> Option<[R; N]> {
    assert_arr_has_non_zero_len::<N>();

    let mut stores = store.0.iter_mut();
    let decoded = source.map(|item| unsafe { R::soft_decode(item, stores.next().unwrap()) });

    if decoded.iter().any(Option::is_none) {
        return None;
    }

    Some(decoded.map(|item| unsafe { item.unwrap_unchecked() }))
}

unsafe fn decode_unchecked_array_elements<'d, 'itm: 'd, R: DecodeOwned<'d>, const N: usize>(
    source: [R::CType; N],
    store: &'itm mut ArrayStore<R::Store, N>,
) -> [R; N] {
    assert_arr_has_non_zero_len::<N>();

    let mut stores = store.0.iter_mut();
    source.map(|item| unsafe { R::soft_decode_unchecked(item, stores.next().unwrap()) })
}
