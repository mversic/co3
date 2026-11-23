#[cfg(feature = "repr-rust")]
use crate::CType;
#[cfg(feature = "repr-rust")]
use crate::wide::Wide;
#[cfg(all(feature = "alloc", feature = "repr-rust"))]
use crate::{
    Dst, boxed::OwnedBoxCarrier, ffi::NulTerminatedBuf, vec::abi::Vec as AbiVec,
    wide::abi::WideBox as AbiWideBox,
};
#[cfg(feature = "repr-rust")]
use crate::{
    convert::{
        SoftDecodeMut, SoftDecodeRef, SoftEncodeMutUnchecked, SoftEncodeRef, SoftEncodeRefUnchecked,
    },
    wide::{
        WideData, WideHeader,
        abi::{WideRefCarrier, WideRefMutCarrier},
    },
};
#[cfg(feature = "repr-rust")]
use crate::{option::Option as StableOption, result::Result as StableResult};
#[cfg(feature = "alloc")]
use alloc::{borrow::ToOwned, boxed::Box, vec::Vec};
#[cfg(all(feature = "alloc", feature = "repr-rust"))]
use core::ptr::NonNull;
use core::{
    cell::{Cell, UnsafeCell},
    marker::PhantomData,
    mem::ManuallyDrop,
};
#[cfg(all(feature = "alloc", feature = "repr-rust"))]
use rust_spec::size::NulTerminated;
#[cfg(feature = "alloc")]
use rust_spec::size::SizedKind;
#[cfg(feature = "repr-rust")]
use rust_spec::{
    Unspecified,
    size::{MetaSized, SliceLike},
};

use disjoint_impls::disjoint_impls;

use rust_spec::{
    Gt, RustSpec, Specified,
    layout::{NonRobust, Robust},
    mutability::{Exclusive, ExclusiveKind, WithInterior},
    size::{Sized as RustSpecSized, Zero},
};

use crate::{
    ReprC, Thin,
    abi::{InteriorTrap, transmute_owned},
    convert::{AbiCompatibleValidate, SoftEncodeMut},
    marker::{AbiCompatible, Native},
    niche::Niche,
    option::abi::OptionSome as AbiOptionSome,
    reference::abi::{RefCarrier, RefMutCarrier},
    result::abi::{ResultErr as AbiResultErr, ResultOk as AbiResultOk},
    sync::ArrayStore,
};
#[cfg(feature = "alloc")]
use crate::{boxed::abi::Box as AbiBox, vec::Vec as StableVec};

// TODO: Could the store just be synced on drop?
/// Holds temporary conversion state and applies deferred mutable write-back.
pub trait Store: Sized {
    /// Apply pending writeback, returning `None` if synchronization fails.
    ///
    /// Tuple and collection stores synchronize every child in order even after
    /// a child returns `None`. Successful writebacks are not rolled back.
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
        Self: RustSpec<Layout = Specified> + ReprC<CType: RefCarrier<Pointee = <R as ReprC>::CType>>,
        R: RustSpec<Layout = Specified, Size: Thin>,
    {
        type SoftStore = ();

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let ptr = core::ptr::from_ref(self);

            // TODO: Hack before `Thin` trait is available in stable:
            // https://doc.rust-lang.org/std/ptr/traitalias.Thin.html
            // https://github.com/rust-lang/rust/issues/81513
            let val = unsafe { core::mem::transmute_copy::<*const R, *const R::CType>(&ptr) };

            unsafe { Self::CType::from_raw(val) }
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: AbiCompatible + Wide<Metadata = usize> + ?Sized> EncodeMoveUnchecked for &R
    where
        Self: RustSpec<Layout = Unspecified>
            + ReprC<CType: WideRefCarrier<Pointee = <R as ReprC>::CType>>,
        R: RustSpec<Layout = Specified, Size = MetaSized<SliceLike>>,
        <R as ReprC>::CType: crate::CType + Wide<Header: WideHeader, Metadata = usize>,
        <<<R as ReprC>::CType as Wide>::Header as WideHeader>::Data: CType,
    {
        type SoftStore = ();

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let source = core::ptr::from_ref(self);

            let len = R::metadata(source);
            let data = R::as_ptr(source).cast();

            unsafe { Self::CType::from_raw_parts(data, len) }
        }
    }

    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, R: SoftEncodeRefUnchecked<'a> + ?Sized, __Interior: ExclusiveKind>
        EncodeMoveUnchecked for &'a R
    where
        Self: RustSpec<Layout = Unspecified> + ReprC,
        R: RustSpec<Layout = Unspecified, Mutability = Exclusive<__Interior>>,
        <Self as ReprC>::CType: Sized,
    {
        type SoftStore = R::SoftStore;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
        where
            Self: 'itm,
        {
            unsafe { self.soft_encode_ref_unchecked(store) }
        }
    }

    unsafe impl<R: AbiCompatible + ?Sized> EncodeMoveUnchecked for &mut R
    where
        Self: RustSpec<Layout = Specified> + ReprC<CType: RefMutCarrier<Pointee = <R as ReprC>::CType>>,
        R: RustSpec<Layout = Specified, Size: Thin>,
    {
        type SoftStore = ();

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let ptr = core::ptr::from_mut(self);

            // TODO: Hack before `Thin` trait is available in stable:
            // https://doc.rust-lang.org/std/ptr/traitalias.Thin.html
            // https://github.com/rust-lang/rust/issues/81513
            let val = unsafe { core::mem::transmute_copy::<*mut R, *mut R::CType>(&ptr) };

            unsafe { Self::CType::from_raw(val) }
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: AbiCompatible + Wide<Metadata = usize> + ?Sized> EncodeMoveUnchecked for &mut R
    where
        Self: RustSpec<Layout = Unspecified>
            + ReprC<CType: WideRefMutCarrier<Pointee = <R as ReprC>::CType>>,
        R: RustSpec<Layout = Specified, Size = MetaSized<SliceLike>>,
        <R as ReprC>::CType: crate::CType + Wide<Header: WideHeader, Metadata = usize>,
        <<<R as ReprC>::CType as Wide>::Header as WideHeader>::Data: CType,
    {
        type SoftStore = ();

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let len = R::metadata(self);
            let ptr = R::as_mut_ptr(self).cast();
            unsafe { Self::CType::from_raw_parts_mut(ptr, len) }
        }
    }

    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, R: SoftEncodeMutUnchecked<'a> + ?Sized> EncodeMoveUnchecked for &'a mut R
    where
        Self: RustSpec<Layout = Unspecified> + ReprC,
        R: RustSpec<Layout = Unspecified>,
        <Self as ReprC>::CType: Sized,
    {
        type SoftStore = R::SoftStore;
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
        where
            Self: 'itm,
        {
            unsafe { self.soft_encode_mut_unchecked(store) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: AbiCompatible, S: SizedKind> EncodeMoveUnchecked for Box<R>
    where
        Self: RustSpec<Layout = Specified> + ReprC<CType = AbiBox<<R as ReprC>::CType>>,
        R: ReprC,
        R: RustSpec<Layout = Specified, Size = RustSpecSized<S>>,
        <R as ReprC>::CType: Sized,
    {
        type SoftStore = ();

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let ptr = Box::into_non_null(self).cast();
            unsafe { Self::CType::from_non_null(ptr) }
        }
    }
    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: AbiCompatible + Wide<Metadata = usize> + ?Sized> EncodeMoveUnchecked for Box<R>
    where
        Self: RustSpec<Layout = Unspecified> + ReprC<CType = AbiWideBox<<R as ReprC>::CType>>,
        R: RustSpec<Layout = Specified, Size = MetaSized<SliceLike>>,
        <R as ReprC>::CType: Wide<Header: WideHeader, Metadata = usize>,
        <<<R as ReprC>::CType as Wide>::Header as WideHeader>::Data: CType,
    {
        type SoftStore = ();

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let len = R::metadata(core::ptr::from_ref(&*self));
            let data = R::into_non_null(self).cast();
            unsafe { Self::CType::from_raw_parts(data, len) }
        }
    }

    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: NulTerminatedBuf<Header: ReprC + CType> + ReprC + ?Sized> EncodeMoveUnchecked
        for Box<R>
    where
        Self: RustSpec<Layout = Unspecified> + ReprC<CType = AbiBox<<R as ReprC>::CType>>,
        R: ReprC,
        R: RustSpec<Layout = Unspecified, Size = NulTerminated>,
        <R as NulTerminatedBuf>::Header: RustSpec<Layout = Specified>,
        <<R as NulTerminatedBuf>::Header as ReprC>::CType: Sized,
        <R as ReprC>::CType: Sized,
    {
        type SoftStore = ();

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let ptr = R::into_non_null(self).cast();
            unsafe { Self::CType::from_non_null(ptr) }
        }
    }
    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: EncodeMoveUnchecked, S: SizedKind> EncodeMoveUnchecked for Box<R>
    where
        Self: RustSpec<Layout = Unspecified> + ReprC<CType = AbiBox<<R as ReprC>::CType>>,
        R: ReprC,
        R: RustSpec<Layout = Unspecified, Size = RustSpecSized<S>>,
    {
        type SoftStore = Box<R::SoftStore>;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
        where
            Self: 'itm,
        {
            let source = Box::new(unsafe { (*self).soft_encode_unchecked(store) });
            unsafe { Self::CType::from_non_null(Box::into_non_null(source)) }
        }
    }
    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: Owned<Owned: EncodeMoveUnchecked> + ?Sized> EncodeMoveUnchecked for Box<R>
    where
        Self: RustSpec<Layout = Unspecified> + Into<<R as Owned>::Owned>,
        R: RustSpec<Layout = Unspecified, Size: Dst>,
        Self: ReprC,
        <<R as Owned>::Owned as ReprC>::CType: crate::CType + OwnedBoxCarrier<<Self as ReprC>::CType>,
        <Self as ReprC>::CType: Sized,
    {
        type SoftStore = <R::Owned as EncodeMoveUnchecked>::SoftStore;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
        where
            Self: 'itm,
        {
            unsafe { self.into().soft_encode_unchecked(store).into_box_carrier() }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: AbiCompatible> EncodeMoveUnchecked for StableVec<R>
    where
        R: RustSpec<Layout = Specified>,
        R: ReprC,
        <R as ReprC>::CType: Sized,
    {
        type SoftStore = ();
        unsafe fn soft_encode_unchecked<'itm>(self, _store: &'itm mut Self::SoftStore) -> Self::CType
        where
            Self: 'itm,
        {
            unsafe { transmute_owned(self) }
        }
    }
    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: EncodeMoveUnchecked> EncodeMoveUnchecked for StableVec<R>
    where
        R: RustSpec<Layout = Unspecified>,
        R: ReprC,
    {
        type SoftStore = Box<[R::SoftStore]>;
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
        where
            Self: 'itm,
        {
            AbiVec::from_vec(unsafe { encode_vec_elements_unchecked(self.into(), store) })
        }
    }

    unsafe impl<R: EncodeMoveUnchecked + AbiCompatible, const N: usize> EncodeMoveUnchecked for [R; N]
    where
        R: RustSpec<Layout = Specified, Mutability: InteriorTrap<Trap = Robust>>,
    {
        type SoftStore = ();
        unsafe fn soft_encode_unchecked<'itm>(self, _: &'itm mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            // SAFETY: The owned wrapper/array preserves its ABI-compatible payload layout.
            unsafe { transmute_owned(self) }
        }
    }
    unsafe impl<R: EncodeMoveUnchecked, const N: usize> EncodeMoveUnchecked for [R; N]
    where
        R: RustSpec<Layout = Specified, Mutability: InteriorTrap<Trap = NonRobust>>,
    {
        type SoftStore = ArrayStore<R::SoftStore, N>;

        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
        where
            Self: 'itm,
        {
            unsafe { encode_unchecked_array_elements(self, store) }
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: EncodeMoveUnchecked, const N: usize> EncodeMoveUnchecked for [R; N]
    where
        R: RustSpec<Layout = Unspecified>,
    {
        type SoftStore = ArrayStore<R::SoftStore, N>;

        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
        where
            Self: 'itm,
        {
            unsafe { encode_unchecked_array_elements(self, store) }
        }
    }

    unsafe impl<R: EncodeMoveUnchecked + AbiCompatible> EncodeMoveUnchecked for UnsafeCell<R>
    where
        R: RustSpec<Layout = Specified, Mutability: InteriorTrap<Trap = Robust>>,
    {
        type SoftStore = ();
        unsafe fn soft_encode_unchecked<'itm>(self, _: &'itm mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            // SAFETY: The owned wrapper/array preserves its ABI-compatible payload layout.
            unsafe { transmute_owned(self) }
        }
    }
    unsafe impl<R: EncodeMoveUnchecked> EncodeMoveUnchecked for UnsafeCell<R>
    where
        R: RustSpec<Layout = Specified, Mutability: InteriorTrap<Trap = NonRobust>>,
    {
        type SoftStore = R::SoftStore;

        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
        where
            Self: 'itm,
        {
            core::cell::UnsafeCell::new(unsafe { self.into_inner().soft_encode_unchecked(store) })
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: EncodeMoveUnchecked> EncodeMoveUnchecked for UnsafeCell<R>
    where
        R: RustSpec<Layout = Unspecified>,
    {
        type SoftStore = R::SoftStore;

        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
        where
            Self: 'itm,
        {
            core::cell::UnsafeCell::new(unsafe { self.into_inner().soft_encode_unchecked(store) })
        }
    }

    unsafe impl<R: EncodeMoveUnchecked + AbiCompatible> EncodeMoveUnchecked for Cell<R>
    where
        R: RustSpec<Layout = Specified, Mutability: InteriorTrap<Trap = Robust>>,
    {
        type SoftStore = ();
        unsafe fn soft_encode_unchecked<'itm>(self, _: &'itm mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            // SAFETY: The owned wrapper/array preserves its ABI-compatible payload layout.
            unsafe { transmute_owned(self) }
        }
    }
    unsafe impl<R: EncodeMoveUnchecked> EncodeMoveUnchecked for Cell<R>
    where
        R: RustSpec<Layout = Specified, Mutability: InteriorTrap<Trap = NonRobust>>,
    {
        type SoftStore = R::SoftStore;

        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
        where
            Self: 'itm,
        {
            core::cell::Cell::new(unsafe { self.into_inner().soft_encode_unchecked(store) })
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: EncodeMoveUnchecked> EncodeMoveUnchecked for Cell<R>
    where
        R: RustSpec<Layout = Unspecified>,
    {
        type SoftStore = R::SoftStore;

        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
        where
            Self: 'itm,
        {
            core::cell::Cell::new(unsafe { self.into_inner().soft_encode_unchecked(store) })
        }
    }

    unsafe impl<R> EncodeMoveUnchecked for ManuallyDrop<R>
    where
        R: RustSpec<Layout = Specified>,
        Self: AbiCompatible,
        <Self as ReprC>::CType: Sized,
    {
        type SoftStore = ();
        unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            unsafe { core::mem::transmute_copy(&self) }
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: EncodeMoveUnchecked> EncodeMoveUnchecked for ManuallyDrop<R>
    where
        R: RustSpec<Layout = Unspecified>,
    {
        type SoftStore = R::SoftStore;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
        where
            Self: 'itm,
        {
            core::mem::ManuallyDrop::new(unsafe { Self::into_inner(self).soft_encode_unchecked(store) })
        }
    }

    unsafe impl<R: EncodeMoveUnchecked + Niche, N: rust_spec::layout::LayoutKind> EncodeMoveUnchecked
        for Option<R>
    where
        R: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<CType = Native<crate::option::abi::OptionSome<<R as ReprC>::CType>>>,
    {
        type SoftStore = Option<R::SoftStore>;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
        where
            Self: 'itm,
        {
            Native({
                match self {
                    Some(value) => AbiOptionSome(unsafe {
                        value.soft_encode_unchecked(store.insert(Default::default()))
                    }),
                    None => {
                        *store = None;
                        AbiOptionSome(R::NICHE)
                    }
                }
            })
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: EncodeMoveUnchecked> EncodeMoveUnchecked for Option<R>
    where
        Self: ReprC<CType = Native<crate::option::abi::Option<<R as ReprC>::CType>>>,
    {
        type SoftStore = <StableOption<R> as EncodeMoveUnchecked>::SoftStore;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
        where
            Self: 'itm,
        {
            Native({
                unsafe { EncodeMoveUnchecked::soft_encode_unchecked(StableOption::from(self), store) }
            })
        }
    }

    unsafe impl<R: EncodeMoveUnchecked + Niche, E: ReprC, N: rust_spec::layout::LayoutKind>
        EncodeMoveUnchecked for Result<R, E>
    where
        R: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<
            CType = Native<crate::result::abi::ResultOk<<R as ReprC>::CType, <E as ReprC>::CType>>,
        >,
        R: RustSpec<Size = RustSpecSized<Gt<Zero>>>,
        E: RustSpec<Size = RustSpecSized<Zero>>,
        <E as ReprC>::CType: Sized,
    {
        type SoftStore = Option<R::SoftStore>;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
        where
            Self: 'itm,
        {
            Native({
                match self {
                    Ok(ok) => AbiResultOk(
                        unsafe { ok.soft_encode_unchecked(store.insert(Default::default())) },
                        PhantomData,
                    ),
                    Err(_) => {
                        *store = None;
                        AbiResultOk(R::NICHE, PhantomData)
                    }
                }
            })
        }
    }
    unsafe impl<R: ReprC, E: EncodeMoveUnchecked + Niche, N: rust_spec::layout::LayoutKind>
        EncodeMoveUnchecked for Result<R, E>
    where
        E: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<
            CType = Native<crate::result::abi::ResultErr<<R as ReprC>::CType, <E as ReprC>::CType>>,
        >,
        R: RustSpec<Size = RustSpecSized<Zero>>,
        E: RustSpec<Size = RustSpecSized<Gt<Zero>>>,
        <R as ReprC>::CType: Sized,
    {
        type SoftStore = Option<E::SoftStore>;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
        where
            Self: 'itm,
        {
            Native({
                match self {
                    Ok(_) => {
                        *store = None;
                        AbiResultErr(E::NICHE, PhantomData)
                    }
                    Err(err) => AbiResultErr(
                        unsafe { err.soft_encode_unchecked(store.insert(Default::default())) },
                        PhantomData,
                    ),
                }
            })
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: EncodeMoveUnchecked, E: EncodeMoveUnchecked> EncodeMoveUnchecked for Result<R, E>
    where
        Self:
            ReprC<CType = Native<crate::result::abi::Result<<R as ReprC>::CType, <E as ReprC>::CType>>>,
        <R as ReprC>::CType: Copy,
        <E as ReprC>::CType: Copy,
    {
        type SoftStore = <StableResult<R, E> as EncodeMoveUnchecked>::SoftStore;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
        where
            Self: 'itm,
        {
            Native({
                unsafe { EncodeMoveUnchecked::soft_encode_unchecked(StableResult::from(self), store) }
            })
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

        /// Convert [`Self`] into its checked carrier, [`ReprC::CType`].
        ///
        /// Prefer using [`crate::soft_encode`] whenever possible
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm;
    }
    unsafe impl<R: AbiCompatibleValidate + ?Sized> EncodeMove for &R
    where
        Self: RustSpec<Layout = Specified> + ReprC<CType: RefCarrier<Pointee = <R as ReprC>::CType>>,
        R: RustSpec<Layout = Specified, Size: Thin>,
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
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: AbiCompatibleValidate + Wide<Metadata = usize> + ?Sized> EncodeMove for &R
    where
        Self: RustSpec<Layout = Unspecified>
            + ReprC<CType: WideRefCarrier<Pointee = <R as ReprC>::CType>>,
        R: RustSpec<Layout = Specified, Size = MetaSized<SliceLike>>,
        <R as ReprC>::CType: crate::CType + Wide<Header: WideHeader<Data: CType>, Metadata = usize>,
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

    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, R: SoftEncodeRef<'a> + ?Sized, __Interior: ExclusiveKind> EncodeMove for &'a R
    where
        Self: RustSpec<Layout = Unspecified> + ReprC<CType: Sized>,
        R: RustSpec<Layout = Unspecified, Mutability = Exclusive<__Interior>>,
    {
        type Store = R::Store;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            self.soft_encode_ref(store)
        }
    }

    unsafe impl<R: AbiCompatibleValidate + ?Sized> EncodeMove for &mut R
    where
        Self: RustSpec<Layout = Specified> + ReprC<CType: RefMutCarrier<Pointee = <R as ReprC>::CType>>,
        R: RustSpec<Layout = Specified, Size: Thin, Trap = Robust>,
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
    unsafe impl<'a, R: AbiCompatibleValidate + SoftEncodeMut<'a>> EncodeMove for &'a mut R
    where
        Self: RustSpec<Layout = Specified> + ReprC<CType: RefMutCarrier<Pointee = <R as ReprC>::CType>>,
        R: RustSpec<Layout = Specified, Size: Thin, Trap = NonRobust>,
    {
        type Store = R::Store;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = self.soft_encode_mut(store);
            debug_assert_encoded_valid::<Self>(&encoded);
            encoded
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: AbiCompatibleValidate + Wide<Metadata = usize> + ?Sized> EncodeMove for &mut R
    where
        Self: RustSpec<Layout = Unspecified>
            + ReprC<CType: WideRefMutCarrier<Pointee = <R as ReprC>::CType>>,
        R: RustSpec<Layout = Specified, Size = MetaSized<SliceLike>, Trap = Robust>,
        <R as ReprC>::CType: crate::CType + Wide<Header: WideHeader<Data: CType>, Metadata = usize>,
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
    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, R: AbiCompatibleValidate + SoftEncodeMut<'a> + Wide<Metadata = usize> + ?Sized>
        EncodeMove for &'a mut R
    where
        Self: RustSpec<Layout = Unspecified>
            + ReprC<CType: WideRefMutCarrier<Pointee = <R as ReprC>::CType>>,
        R: RustSpec<Layout = Specified, Size = MetaSized<SliceLike>, Trap = NonRobust>,
        <R as ReprC>::CType: crate::CType + Wide<Header: WideHeader<Data: CType>, Metadata = usize>,
    {
        type Store = R::Store;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = self.soft_encode_mut(store);
            // TODO:
            //debug_assert_wide_pointee_valid::<R>(encoded.data(), encoded.metadata());
            encoded
        }
    }

    #[cfg(feature = "repr-rust")]
    unsafe impl<'a, R: SoftEncodeMut<'a> + ?Sized> EncodeMove for &'a mut R
    where
        Self: RustSpec<Layout = Unspecified> + ReprC,
        R: RustSpec<Layout = Unspecified>,
        <Self as ReprC>::CType: Sized,
    {
        type Store = R::Store;
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            self.soft_encode_mut(store)
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: AbiCompatibleValidate + EncodeMove, S: SizedKind> EncodeMove for Box<R>
    where
        Self: RustSpec<Layout = Specified>,
        R: RustSpec<Layout = Specified, Size = RustSpecSized<S>, Trap = Robust>,
        Self: AbiCompatibleValidate + ReprC<CType = AbiBox<<R as ReprC>::CType>>,
        R: ReprC,
        <R as ReprC>::CType: Sized,
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
        Self: RustSpec<Layout = Specified>,
        R: RustSpec<Layout = Specified, Size = RustSpecSized<S>, Trap = NonRobust>,
        Self: AbiCompatibleValidate + ReprC<CType = AbiBox<<R as ReprC>::CType>>,
        R: ReprC,
        <R as ReprC>::CType: Sized,
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

            let source = Box::new(crate::stored::EncodeMove::soft_encode(*self, store));
            unsafe { Self::CType::from_non_null(Box::into_non_null(source)) }
        }
    }
    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: AbiCompatibleValidate + Wide<Metadata = usize> + ?Sized> EncodeMove for Box<R>
    where
        Self: RustSpec<Layout = Unspecified>,
        R: RustSpec<Layout = Specified, Size = MetaSized<SliceLike>, Trap = Robust>,
        Self: ReprC<CType = AbiWideBox<<R as ReprC>::CType>>,
        <R as ReprC>::CType: Wide<Header: WideHeader<Data: CType>, Metadata = usize>,
    {
        type Store = ();

        #[inline(always)]
        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = unsafe { self.soft_encode_unchecked(&mut ()) };
            debug_assert_wide_pointee_valid::<R>(encoded.as_ptr(), encoded.metadata());
            encoded
        }
    }
    // TODO: This path requires special care
    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: AbiCompatibleValidate + Wide<Metadata = usize> + Owned + ?Sized> EncodeMove
        for Box<R>
    where
        Self: RustSpec<Layout = Unspecified> + Into<<R as Owned>::Owned>,
        R: RustSpec<Layout = Specified, Size = MetaSized<SliceLike>, Trap = NonRobust>,
        <R as ReprC>::CType: Wide<Header: WideHeader<Data: CType>, Metadata = usize>,
        <R as Owned>::Owned: EncodeMove,
        <<R as Owned>::Owned as ReprC>::CType: crate::CType + OwnedBoxCarrier<<Self as ReprC>::CType>,
        Self: ReprC<CType = AbiWideBox<<R as ReprC>::CType>>,
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
                debug_assert_wide_pointee_valid::<R>(encoded.as_ptr(), encoded.metadata());
                return encoded;
            }

            encode_box_owned::<R>(self, store)
        }
    }

    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: EncodeMove, S: SizedKind> EncodeMove for Box<R>
    where
        Self: RustSpec<Layout = Unspecified>,
        R: RustSpec<Layout = Unspecified, Size = RustSpecSized<S>>,
        Self: ReprC<CType = AbiBox<<R as ReprC>::CType>>,
        R: ReprC,
    {
        type Store = Box<R::Store>;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            let source = Box::new(crate::stored::EncodeMove::soft_encode(*self, store));
            unsafe { Self::CType::from_non_null(Box::into_non_null(source)) }
        }
    }
    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: NulTerminatedBuf<Header: ReprC + CType> + ReprC + ?Sized> EncodeMove for Box<R>
    where
        Self: RustSpec<Layout = Unspecified>,
        R: RustSpec<Layout = Unspecified, Size = NulTerminated>,
        <R as NulTerminatedBuf>::Header: RustSpec<Layout = Specified>,
        <<R as NulTerminatedBuf>::Header as ReprC>::CType: Sized,
        Self: ReprC<CType = AbiBox<<R as ReprC>::CType>>,
        R: ReprC,
        <R as ReprC>::CType: Sized,
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
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: Owned<Owned: EncodeMove> + ?Sized> EncodeMove for Box<R>
    where
        Self: RustSpec<Layout = Unspecified> + Into<<R as Owned>::Owned>,
        R: RustSpec<Layout = Unspecified, Size: Dst>,
        Self: ReprC,
        <<R as Owned>::Owned as ReprC>::CType: crate::CType + OwnedBoxCarrier<<Self as ReprC>::CType>,
        <Self as ReprC>::CType: Sized,
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

    unsafe impl<R: EncodeMove + AbiCompatibleValidate> EncodeMove for UnsafeCell<R>
    where
        R: RustSpec<Layout = Specified, Trap = Robust>,
        Self: EncodeMoveUnchecked + ReprC<CType = core::cell::UnsafeCell<<R as ReprC>::CType>>,
    {
        type Store = ();
        fn soft_encode<'itm>(self, _: &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            // SAFETY: Ownership is transferred and robust payloads require no validity conversion.
            unsafe { transmute_owned(self) }
        }
    }
    unsafe impl<R: EncodeMove> EncodeMove for UnsafeCell<R>
    where
        R: RustSpec<Layout = Specified, Trap = NonRobust>,
        Self: EncodeMoveUnchecked + ReprC<CType = core::cell::UnsafeCell<<R as ReprC>::CType>>,
    {
        type Store = R::Store;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            core::cell::UnsafeCell::new(crate::stored::EncodeMove::soft_encode(
                self.into_inner(),
                store,
            ))
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: EncodeMove> EncodeMove for UnsafeCell<R>
    where
        R: RustSpec<Layout = Unspecified>,
    {
        type Store = R::Store;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            core::cell::UnsafeCell::new(crate::stored::EncodeMove::soft_encode(
                self.into_inner(),
                store,
            ))
        }
    }

    unsafe impl<R> EncodeMove for Cell<R>
    where
        R: EncodeMove + AbiCompatibleValidate + RustSpec<Layout = Specified, Trap = Robust>,
        Self: EncodeMoveUnchecked + ReprC<CType = core::cell::Cell<<R as ReprC>::CType>>,
    {
        type Store = ();
        fn soft_encode<'itm>(self, _: &mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            // SAFETY: Ownership is transferred and robust payloads require no validity conversion.
            unsafe { transmute_owned(self) }
        }
    }
    unsafe impl<R: EncodeMove> EncodeMove for Cell<R>
    where
        R: RustSpec<Layout = Specified, Trap = NonRobust>,
        Self: EncodeMoveUnchecked + ReprC<CType = core::cell::Cell<<R as ReprC>::CType>>,
    {
        type Store = R::Store;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            core::cell::Cell::new(crate::stored::EncodeMove::soft_encode(
                self.into_inner(),
                store,
            ))
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: EncodeMove> EncodeMove for Cell<R>
    where
        R: RustSpec<Layout = Unspecified>,
    {
        type Store = R::Store;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            core::cell::Cell::new(crate::stored::EncodeMove::soft_encode(
                self.into_inner(),
                store,
            ))
        }
    }

    unsafe impl<R: EncodeMove + Niche, N: rust_spec::layout::LayoutKind> EncodeMove for Option<R>
    where
        R: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<CType = Native<crate::option::abi::OptionSome<<R as ReprC>::CType>>>,
    {
        type Store = Option<R::Store>;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            Native({
                if let Some(value) = self {
                    let encoded = EncodeMove::soft_encode(value, store.insert(Default::default()));
                    debug_assert!(!R::is_niche(&encoded), "encoding produced NICHE value");
                    return Native(AbiOptionSome(encoded));
                }

                *store = None;
                AbiOptionSome(R::NICHE)
            })
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: EncodeMove> EncodeMove for Option<R>
    where
        Self: ReprC<CType = Native<crate::option::abi::Option<<R as ReprC>::CType>>>,
    {
        type Store = <StableOption<R> as EncodeMove>::Store;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            Native(StableOption::from(self).soft_encode(store))
        }
    }

    unsafe impl<R: EncodeMove + Niche, E: ReprC, N: rust_spec::layout::LayoutKind> EncodeMove
        for Result<R, E>
    where
        R: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<
            CType = Native<crate::result::abi::ResultOk<<R as ReprC>::CType, <E as ReprC>::CType>>,
        >,
        R: RustSpec<Size = RustSpecSized<Gt<Zero>>>,
        E: RustSpec<Size = RustSpecSized<Zero>>,
        <E as ReprC>::CType: Sized,
    {
        type Store = Option<R::Store>;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            Native({
                match self {
                    Ok(ok) => AbiResultOk(
                        EncodeMove::soft_encode(ok, store.insert(Default::default())),
                        PhantomData,
                    ),
                    Err(_) => AbiResultOk(R::NICHE, PhantomData),
                }
            })
        }
    }
    unsafe impl<R: ReprC, E: EncodeMove + Niche, N: rust_spec::layout::LayoutKind> EncodeMove
        for Result<R, E>
    where
        E: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<
            CType = Native<crate::result::abi::ResultErr<<R as ReprC>::CType, <E as ReprC>::CType>>,
        >,
        R: RustSpec<Size = RustSpecSized<Zero>>,
        E: RustSpec<Size = RustSpecSized<Gt<Zero>>>,
        <R as ReprC>::CType: Sized,
    {
        type Store = Option<E::Store>;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            Native({
                match self {
                    Ok(_) => AbiResultErr(E::NICHE, PhantomData),
                    Err(err) => AbiResultErr(
                        EncodeMove::soft_encode(err, store.insert(Default::default())),
                        PhantomData,
                    ),
                }
            })
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: EncodeMove, E: EncodeMove> EncodeMove for Result<R, E>
    where
        Self:
            ReprC<CType = Native<crate::result::abi::Result<<R as ReprC>::CType, <E as ReprC>::CType>>>,
        <R as ReprC>::CType: Copy,
        <E as ReprC>::CType: Copy,
    {
        type Store = <StableResult<R, E> as EncodeMove>::Store;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            Native({ StableResult::from(self).soft_encode(store) })
        }
    }
}

disjoint_impls! {
    /// Perform the conversion from a [`ReprC::CType`] into [`Self`].
    ///
    /// # Safety
    ///
    /// If [`Self::Store`] implements [`EmptyStore`], [`DecodeMove::soft_decode`] must not
    /// return references into the store. [`crate::soft_decode`] correctness relies on it.
    pub unsafe trait DecodeMove<'d>: ReprC<CType: Sized> + Sized {
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
            unsafe { Self::soft_decode(source, store) }
                .expect("called `Option::unwrap()` on a `None` value")
        }
    }
    unsafe impl<'d, 'r: 'd, R: AbiCompatibleValidate + ?Sized> DecodeMove<'d> for &'r R
    where
        Self: RustSpec<Layout = Specified> + ReprC<CType: RefCarrier<Pointee = <R as ReprC>::CType>>,
        R: RustSpec<Layout = Specified, Size: Thin>,
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
    #[cfg(feature = "repr-rust")]
    unsafe impl<
        'd,
        'r: 'd,
        R: AbiCompatibleValidate<CType: crate::CType + Wide<Metadata = usize>> + ?Sized,
    > DecodeMove<'d> for &'r R
    where
        Self: RustSpec<Layout = Unspecified>,
        R: RustSpec<Layout = Specified, Size = MetaSized<SliceLike>>,
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
    unsafe impl<
        'd,
        'r: 'd,
        R: AbiCompatibleValidate + RustSpec<Layout = Specified, Size: Thin> + ?Sized,
    > DecodeMove<'d> for &'r mut R
    where
        Self: RustSpec<Layout = Specified> + ReprC<CType: RefMutCarrier<Pointee = <R as ReprC>::CType>>,
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
    #[cfg(feature = "repr-rust")]
    unsafe impl<
        'd,
        'r: 'd,
        R: AbiCompatibleValidate<CType: crate::CType + Wide<Metadata = usize>> + ?Sized,
    > DecodeMove<'d> for &'r mut R
    where
        Self: RustSpec<Layout = Unspecified>
            + ReprC<CType: WideRefMutCarrier<Pointee = <R as ReprC>::CType>>,
        R: RustSpec<Layout = Specified, Size = MetaSized<SliceLike>>,
        WideData<<R as ReprC>::CType>: CType,
        R: Wide<Metadata = usize>,
    {
        type Store = ();

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut ()) -> Option<Self> {
            if source.data().is_null() {
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

    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, R: SoftDecodeRef<'d> + ?Sized> DecodeMove<'d> for &'d R
    where
        R: RustSpec<Layout = Unspecified>,
        Self: RustSpec<Layout = Unspecified> + ReprC,
        <Self as ReprC>::CType: Sized,
    {
        type Store = <R as SoftDecodeRef<'d>>::Store;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { R::soft_decode_ref(source, store) }
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            unsafe { R::soft_decode_ref_unchecked(source, store) }
        }
    }

    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, R: SoftDecodeMut<'d> + ?Sized> DecodeMove<'d> for &'d mut R
    where
        R: RustSpec<Layout = Unspecified>,
        Self: RustSpec<Layout = Unspecified> + ReprC,
        <Self as ReprC>::CType: Sized,
    {
        type Store = <R as SoftDecodeMut<'d>>::Store;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe { R::soft_decode_mut(source, store) }
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            unsafe { R::soft_decode_mut_unchecked(source, store) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, R: AbiCompatibleValidate, S: SizedKind> DecodeMove<'d> for Box<R>
    where
        Self: RustSpec<Layout = Specified>,
        R: RustSpec<Layout = Specified, Size = RustSpecSized<S>>,
        Self: AbiCompatibleValidate + ReprC<CType = AbiBox<<R as ReprC>::CType>>,
        R: ReprC,
        <R as ReprC>::CType: Sized,
    {
        type Store = ();

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if !unsafe { Self::is_valid(&source) } {
                // Reclaim the owned carrier without constructing the rejected Rust value.
                // SAFETY: Non-null sources own an allocation of R::CType; null owns nothing.
                drop(unsafe { source.into_box() });
                return None;
            }

            Some(unsafe { Self::soft_decode_unchecked(source, &mut ()) })
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            unsafe { Box::from_raw(source.as_ptr().cast()) }
        }
    }
    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, R: NulTerminatedBuf<Header: ReprC + CType> + ReprC + ?Sized> DecodeMove<'d>
        for Box<R>
    where
        Self: RustSpec<Layout = Unspecified> + ReprC<CType = AbiBox<<R as ReprC>::CType>>,
        R: ReprC,
        R: RustSpec<Layout = Unspecified, Size = NulTerminated>,
        <R as NulTerminatedBuf>::Header: RustSpec<Layout = Specified> + Sized,
        <<R as NulTerminatedBuf>::Header as ReprC>::CType: Sized,
        <R as ReprC>::CType: Sized,
    {
        type Store = ();

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut ()) -> Option<Self> {
            NonNull::new(source.as_ptr())?;
            Some(unsafe { Self::soft_decode_unchecked(source, store) })
        }
        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
            let ptr = unsafe { NonNull::new_unchecked(source.as_ptr()) };
            unsafe { R::from_non_null(ptr.cast()) }
        }
    }
    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, R: AbiCompatibleValidate + ?Sized> DecodeMove<'d> for Box<R>
    where
        Self: RustSpec<Layout = Unspecified>,
        R: RustSpec<Layout = Specified, Size = MetaSized<SliceLike>>,
        WideData<R>: ReprC<CType = WideData<<R as ReprC>::CType>>,
        Self: ReprC<CType = AbiWideBox<<R as ReprC>::CType>>,
        R: Wide<Metadata = usize>,
        <R as ReprC>::CType: Wide<Metadata = usize>,
    {
        type Store = ();

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
            if source.as_ptr().is_null() {
                return None;
            }

            let len = source.metadata();
            let data = source.as_ptr();

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
            let data = source.as_ptr().cast();

            unsafe { R::from_non_null(NonNull::new_unchecked(data), len) }
        }
    }
    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, R: DecodeMove<'d>, S: SizedKind> DecodeMove<'d> for Box<R>
    where
        Self: RustSpec<Layout = Unspecified> + ReprC<CType = AbiBox<<R as ReprC>::CType>>,
        R: ReprC,
        R: RustSpec<Layout = Unspecified, Size = RustSpecSized<S>>,
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
    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, R: Owned + ?Sized> DecodeMove<'d> for Box<R>
    where
        Self: RustSpec<Layout = Unspecified> + ReprC,
        <<R as Owned>::Owned as ReprC>::CType: crate::CType + OwnedBoxCarrier<<Self as ReprC>::CType>,
        R: RustSpec<Layout = Unspecified, Size: Dst>,
        <R as Owned>::Owned: DecodeMove<'d> + Into<Self>,
        <Self as ReprC>::CType: Sized,
    {
        type Store = <R::Owned as DecodeMove<'d>>::Store;

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
    unsafe impl<'d, R: AbiCompatibleValidate> DecodeMove<'d> for StableVec<R>
    where
        R: RustSpec<Layout = Specified, Mutability: rust_spec::mutability::PointerMutability>,
        R: ReprC,
        <R as ReprC>::CType: Sized,
    {
        type Store = ();
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, _: &'itm mut ()) -> Option<Self> {
            if !unsafe { <Self as crate::convert::AbiCompatibleValidate>::is_valid(&source) } {
                // Reclaim the carrier allocation without constructing invalid R values.
                drop(unsafe { source.into_vec() });
                return None;
            }
            Some(unsafe { transmute_owned(source) })
        }
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, _: &'itm mut ()) -> Self {
            unsafe { transmute_owned(source) }
        }
    }
    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, R: DecodeMove<'d>> DecodeMove<'d> for StableVec<R>
    where
        R: RustSpec<Layout = Unspecified>,
        R: ReprC,
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

    unsafe impl<'d, R: DecodeMove<'d> + AbiCompatibleValidate, const N: usize> DecodeMove<'d> for [R; N]
    where
        R: RustSpec<Layout = Specified, Trap = Robust>,
    {
        type Store = ();
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, _: &'itm mut ()) -> Option<Self> {
            // SAFETY: Robust, ABI-compatible payloads have no invalid bit patterns to inspect.
            Some(unsafe { transmute_owned(source) })
        }
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, _: &'itm mut ()) -> Self {
            unsafe { transmute_owned(source) }
        }
    }
    unsafe impl<'d, R: DecodeMove<'d>, const N: usize> DecodeMove<'d> for [R; N]
    where
        R: RustSpec<Layout = Specified, Trap = NonRobust>,
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
    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, R: DecodeMove<'d>, const N: usize> DecodeMove<'d> for [R; N]
    where
        R: RustSpec<Layout = Unspecified>,
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

    unsafe impl<'d, R: DecodeMove<'d> + AbiCompatibleValidate> DecodeMove<'d> for UnsafeCell<R>
    where
        R: RustSpec<Layout = Specified, Trap = Robust>,
    {
        type Store = ();
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, _: &'itm mut ()) -> Option<Self> {
            // SAFETY: Robust, ABI-compatible payloads have no invalid bit patterns to inspect.
            Some(unsafe { transmute_owned(source) })
        }
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, _: &'itm mut ()) -> Self {
            unsafe { transmute_owned(source) }
        }
    }
    unsafe impl<'d, R: DecodeMove<'d>> DecodeMove<'d> for UnsafeCell<R>
    where
        R: RustSpec<Layout = Specified, Trap = NonRobust>,
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
    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, R: DecodeMove<'d>> DecodeMove<'d> for UnsafeCell<R>
    where
        R: RustSpec<Layout = Unspecified>,
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

    unsafe impl<'d, R: DecodeMove<'d> + AbiCompatibleValidate> DecodeMove<'d> for Cell<R>
    where
        R: RustSpec<Layout = Specified, Trap = Robust>,
    {
        type Store = ();
        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, _: &'itm mut ()) -> Option<Self> {
            // SAFETY: Robust, ABI-compatible payloads have no invalid bit patterns to inspect.
            Some(unsafe { transmute_owned(source) })
        }
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, _: &'itm mut ()) -> Self {
            unsafe { transmute_owned(source) }
        }
    }
    unsafe impl<'d, R: DecodeMove<'d>> DecodeMove<'d> for Cell<R>
    where
        R: RustSpec<Layout = Specified, Trap = NonRobust>,
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
    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, R: DecodeMove<'d>> DecodeMove<'d> for Cell<R>
    where
        R: RustSpec<Layout = Unspecified>,
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

    unsafe impl<'d, R, __Interior: ExclusiveKind> DecodeMove<'d> for ManuallyDrop<R>
    where
        R: RustSpec<Layout = Specified, Mutability = Exclusive<__Interior>>,
        Self: AbiCompatibleValidate,
        <Self as ReprC>::CType: Sized,
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
    unsafe impl<'d, R, __Interior> DecodeMove<'d> for ManuallyDrop<R>
    where
        R: RustSpec<Layout = Specified, Trap = Robust, Mutability = WithInterior<__Interior>>,
        Self: AbiCompatibleValidate,
        <Self as ReprC>::CType: Sized,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &'itm mut ()) -> Option<Self> {
            // SAFETY: Robust, ABI-compatible values need no validation or payload reads.
            Some(unsafe { transmute_owned(source) })
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &'itm mut ()) -> Self {
            // SAFETY: Ownership is transferred using the ABI-compatible wrapper layout.
            unsafe { transmute_owned(source) }
        }
    }
    unsafe impl<'d, R: DecodeMove<'d>, __Trap> DecodeMove<'d> for ManuallyDrop<R>
    where
        R: RustSpec<Layout = Specified, Trap = NonRobust, Mutability = WithInterior<__Trap>>,
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
    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, R: DecodeMove<'d>> DecodeMove<'d> for ManuallyDrop<R>
    where
        R: RustSpec<Layout = Unspecified>,
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

    unsafe impl<'d, R: DecodeMove<'d> + Niche, N: rust_spec::layout::LayoutKind> DecodeMove<'d>
        for Option<R>
    where
        R: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<CType = Native<crate::option::abi::OptionSome<<R as ReprC>::CType>>>,
    {
        type Store = Option<<R as DecodeMove<'d>>::Store>;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = source.0;

            if R::is_niche(&source.0) {
                *store = None;
                return Some(None);
            }
            let source = source.0;
            unsafe { R::soft_decode(source, store.insert(Default::default())) }.map(Some)
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let source = source.0;

            if R::is_niche(&source.0) {
                *store = None;
                return None;
            }
            let source = source.0;
            Some(unsafe { R::soft_decode_unchecked(source, store.insert(Default::default())) })
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, R: DecodeMove<'d>> DecodeMove<'d> for Option<R>
    where
        Self: ReprC<CType = Native<crate::option::abi::Option<<R as ReprC>::CType>>>,
    {
        type Store = <StableOption<R> as DecodeMove<'d>>::Store;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = source.0;

            unsafe { <StableOption<R> as DecodeMove<'d>>::soft_decode(source, store) }.map(Into::into)
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let source = source.0;

            unsafe { <StableOption<R> as DecodeMove<'d>>::soft_decode_unchecked(source, store) }.into()
        }
    }

    unsafe impl<'d, R: DecodeMove<'d> + Niche, E: Default + ReprC, N: rust_spec::layout::LayoutKind>
        DecodeMove<'d> for Result<R, E>
    where
        R: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<
            CType = Native<crate::result::abi::ResultOk<<R as ReprC>::CType, <E as ReprC>::CType>>,
        >,
        R: RustSpec<Size = RustSpecSized<Gt<Zero>>>,
        E: RustSpec<Size = RustSpecSized<Zero>>,
        <E as ReprC>::CType: Sized,
    {
        type Store = R::Store;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = source.0;

            if R::is_niche(&source.0) {
                return Some(Err(E::default()));
            }
            let source = source.0;
            unsafe { R::soft_decode(source, store) }.map(Ok)
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let source = source.0;

            if R::is_niche(&source.0) {
                return Err(E::default());
            }
            let source = source.0;
            Ok(unsafe { R::soft_decode_unchecked(source, store) })
        }
    }
    unsafe impl<'d, R: Default + ReprC, E: DecodeMove<'d> + Niche, N: rust_spec::layout::LayoutKind>
        DecodeMove<'d> for Result<R, E>
    where
        E: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<
            CType = Native<crate::result::abi::ResultErr<<R as ReprC>::CType, <E as ReprC>::CType>>,
        >,
        R: RustSpec<Size = RustSpecSized<Zero>>,
        E: RustSpec<Size = RustSpecSized<Gt<Zero>>>,
        <R as ReprC>::CType: Sized,
    {
        type Store = E::Store;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = source.0;

            if E::is_niche(&source.0) {
                return Some(Ok(R::default()));
            }
            let source = source.0;
            unsafe { E::soft_decode(source, store) }.map(Err)
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let source = source.0;

            if E::is_niche(&source.0) {
                return Ok(R::default());
            }
            let source = source.0;
            Err(unsafe { E::soft_decode_unchecked(source, store) })
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, R: DecodeMove<'d>, E: DecodeMove<'d>> DecodeMove<'d> for Result<R, E>
    where
        Self:
            ReprC<CType = Native<crate::result::abi::Result<<R as ReprC>::CType, <E as ReprC>::CType>>>,
        <R as ReprC>::CType: Copy,
        <E as ReprC>::CType: Copy,
    {
        type Store = <StableResult<R, E> as DecodeMove<'d>>::Store;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = source.0;

            unsafe { <StableResult<R, E> as DecodeMove<'d>>::soft_decode(source, store) }
                .map(Into::into)
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let source = source.0;

            unsafe { <StableResult<R, E> as DecodeMove<'d>>::soft_decode_unchecked(source, store) }
                .into()
        }
    }
}

unsafe impl<R: EncodeMove, const N: usize> EncodeMove for [R; N]
where
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

/// Convert `T` into [`T::CType`].
///
/// Prefer using [`crate::encode`] whenever possible.
#[doc(hidden)]
#[inline(always)]
pub fn encode_move<T: EncodeMove<Store: EmptyStore>>(item: T) -> T::CType {
    let mut store = Default::default();
    T::soft_encode(item, &mut store)
}

/// Perform the conversion from [`T::CType`](ReprC::CType) into `T`.
///
/// This function allows you to encode/decode unstable owned types
///
/// Prefer using [`crate::decode`] whenever possible.
///
/// # Safety
///
/// See the safety section in [`crate::decode`].
#[doc(hidden)]
#[inline(always)]
pub unsafe fn decode_move<'d, T: DecodeMove<'d, Store: EmptyStore + 'd>>(
    source: T::CType,
) -> Option<T> {
    unsafe { with_empty_decode_store::<T, _>(|store| T::soft_decode(source, store)) }
}

#[doc(hidden)]
#[inline(always)]
pub(crate) unsafe fn encode_move_unchecked<T: EncodeMoveUnchecked<SoftStore: EmptyStore>>(
    item: T,
) -> T::CType {
    let mut store = Default::default();
    unsafe { T::soft_encode_unchecked(item, &mut store) }
}

#[doc(hidden)]
#[inline(always)]
pub(crate) unsafe fn decode_move_unchecked<'d, T: DecodeMove<'d, Store: EmptyStore + 'd>>(
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
#[cfg(feature = "repr-rust")]
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
#[cfg(feature = "repr-rust")]
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
/// `DecodeMove` contract guarantees this for both decode methods when the
/// store implements `EmptyStore`.
#[inline(always)]
unsafe fn with_empty_decode_store<'d, T, R>(f: impl FnOnce(&'d mut T::Store) -> R) -> R
where
    T: DecodeMove<'d, Store: EmptyStore + 'd>,
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
#[inline(always)]
#[cfg(feature = "repr-rust")]
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
    unsafe { crate::stored::EncodeMove::soft_encode(value.into(), store).into_box_carrier() }
}

#[cfg(feature = "alloc")]
#[inline(always)]
#[cfg(feature = "repr-rust")]
unsafe fn decode_box_owned<'d, 'itm: 'd, R: Owned + ?Sized>(
    source: <Box<R> as ReprC>::CType,
    store: &'itm mut <R::Owned as DecodeMove<'d>>::Store,
) -> Option<Box<R>>
where
    R::Owned: DecodeMove<'d> + Into<Box<R>>,
    Box<R>: ReprC<CType: Sized>,
    <R::Owned as ReprC>::CType: OwnedBoxCarrier<<Box<R> as ReprC>::CType>,
{
    let source = unsafe { <R::Owned as ReprC>::CType::from_box_carrier(source)? };
    unsafe { R::Owned::soft_decode(source, store) }.map(Into::into)
}

#[cfg(feature = "alloc")]
#[inline(always)]
#[cfg(feature = "repr-rust")]
unsafe fn decode_box_owned_unchecked<'d, 'itm: 'd, R: Owned + ?Sized>(
    source: <Box<R> as ReprC>::CType,
    store: &'itm mut <R::Owned as DecodeMove<'d>>::Store,
) -> Box<R>
where
    R::Owned: DecodeMove<'d> + Into<Box<R>>,
    Box<R>: ReprC<CType: Sized>,
    <R::Owned as ReprC>::CType: OwnedBoxCarrier<<Box<R> as ReprC>::CType>,
{
    let source = unsafe { <R::Owned as ReprC>::CType::from_box_carrier(source).unwrap_unchecked() };
    unsafe { R::Owned::soft_decode_unchecked(source, store) }.into()
}

// Shared traversal preserves each caller's conversion and access contracts.
pub(crate) fn map_array<'a, T, U, S, const N: usize>(
    values: [T; N],
    stores: &'a mut [S; N],
    mut convert: impl FnMut(T, &'a mut S) -> U,
) -> [U; N] {
    let mut stores = stores.iter_mut();
    values.map(|value| convert(value, stores.next().unwrap()))
}

pub(crate) fn try_map_array<'a, T, U, S, const N: usize>(
    values: [T; N],
    stores: &'a mut [S; N],
    convert: impl FnMut(T, &'a mut S) -> Option<U>,
) -> Option<[U; N]> {
    let converted = map_array(values, stores, convert);
    if converted.iter().any(Option::is_none) {
        return None;
    }
    Some(converted.map(Option::unwrap))
}

#[cfg(feature = "alloc")]
fn zip_stores<I: ExactSizeIterator, S: Default>(
    values: I,
    stores: &mut Box<[S]>,
) -> impl ExactSizeIterator<Item = (I::Item, &mut S)> {
    *stores = core::iter::repeat_with(S::default)
        .take(values.len())
        .collect();
    values.zip(stores.iter_mut())
}

#[cfg(feature = "alloc")]
pub(crate) fn map_vec<'a, I: ExactSizeIterator, U, S: Default>(
    values: I,
    capacity: usize,
    stores: &'a mut Box<[S]>,
    mut convert: impl FnMut(I::Item, &'a mut S) -> U,
) -> Vec<U> {
    let values = zip_stores(values, stores);
    let mut converted = Vec::with_capacity(capacity);
    converted.extend(values.map(|(value, store)| convert(value, store)));
    converted
}

/// Attempt every element, retaining successful values until all conversions
/// finish. A failed conversion drops those values and leaves every store alive.
#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
fn try_map_vec<'a, I: ExactSizeIterator, U, S: Default>(
    values: I,
    capacity: usize,
    stores: &'a mut Box<[S]>,
    mut convert: impl FnMut(I::Item, &'a mut S) -> Option<U>,
) -> Option<Vec<U>> {
    let values = zip_stores(values, stores);
    let mut converted = Vec::with_capacity(capacity);
    let mut valid = true;
    for (value, store) in values {
        if let Some(value) = convert(value, store) {
            converted.push(value);
        } else {
            valid = false;
        }
    }
    valid.then_some(converted)
}

#[cfg(feature = "alloc")]
#[inline(always)]
#[cfg(feature = "repr-rust")]
unsafe fn encode_vec_elements_unchecked<R: EncodeMoveUnchecked>(
    value: Vec<R>,
    store: &mut Box<[R::SoftStore]>,
) -> Vec<R::CType> {
    let capacity = value.capacity();
    map_vec(value.into_iter(), capacity, store, |item, store| unsafe {
        item.soft_encode_unchecked(store)
    })
}

#[cfg(feature = "alloc")]
#[inline(always)]
#[cfg(feature = "repr-rust")]
unsafe fn decode_vec_elements<'d, 'itm: 'd, R: DecodeMove<'d>>(
    source: Vec<R::CType>,
    store: &'itm mut Box<[R::Store]>,
) -> Option<Vec<R>> {
    let capacity = source.capacity();
    try_map_vec(source.into_iter(), capacity, store, |item, store| unsafe {
        R::soft_decode(item, store)
    })
}

#[cfg(feature = "alloc")]
#[inline(always)]
#[cfg(feature = "repr-rust")]
unsafe fn decode_vec_elements_unchecked<'d, 'itm: 'd, R: DecodeMove<'d>>(
    source: Vec<R::CType>,
    store: &'itm mut Box<[R::Store]>,
) -> Vec<R> {
    let capacity = source.capacity();
    map_vec(source.into_iter(), capacity, store, |item, store| unsafe {
        R::soft_decode_unchecked(item, store)
    })
}

/// Copy an exclusively borrowed field representation into an aggregate's temporary value.
/// Its backing storage remains borrowed and must be kept alive by the aggregate store.
#[doc(hidden)]
pub(crate) fn encode_ref_field<'a, R: ReprC<CType: Copy> + 'a>(
    value: &'a R,
    store: &mut <&'a R as EncodeMove>::Store,
) -> R::CType
where
    &'a R: EncodeMove<CType: RefCarrier<Pointee = <R as ReprC>::CType>>,
{
    let encoded = crate::stored::EncodeMove::soft_encode(value, store);
    // SoftRef guarantees shared access to initialized storage, and Copy permits
    // duplicating the field representation without moving out of that storage.
    unsafe { encoded.as_ptr().read() }
}

/// Unchecked counterpart of `encode_ref_field`.
///
/// # Safety
/// The caller must uphold the unchecked encoding contract for the field.
#[doc(hidden)]
pub(crate) unsafe fn encode_ref_field_unchecked<'a, R: ReprC<CType: Copy> + 'a>(
    value: &'a R,
    store: &mut <&'a R as EncodeMoveUnchecked>::SoftStore,
) -> R::CType
where
    &'a R: EncodeMoveUnchecked<CType: RefCarrier<Pointee = <R as ReprC>::CType>>,
{
    let encoded = unsafe { value.soft_encode_unchecked(store) };
    unsafe { encoded.as_ptr().read() }
}

// Structural array fallbacks share element conversion; transmutation stays in the impls.
unsafe fn encode_unchecked_array_elements<R: EncodeMoveUnchecked, const N: usize>(
    value: [R; N],
    store: &mut ArrayStore<R::SoftStore, N>,
) -> [R::CType; N] {
    map_array(value, &mut store.0, |item, store| unsafe {
        item.soft_encode_unchecked(store)
    })
}

fn encode_array_elements<'itm, R: EncodeMove + 'itm, const N: usize>(
    value: [R; N],
    store: &'itm mut ArrayStore<R::Store, N>,
) -> [R::CType; N] {
    map_array(value, &mut store.0, crate::stored::EncodeMove::soft_encode)
}

unsafe fn decode_array_elements<'d, 'itm: 'd, R: DecodeMove<'d>, const N: usize>(
    source: [R::CType; N],
    store: &'itm mut ArrayStore<R::Store, N>,
) -> Option<[R; N]> {
    try_map_array(source, &mut store.0, |item, store| unsafe {
        R::soft_decode(item, store)
    })
}

unsafe fn decode_unchecked_array_elements<'d, 'itm: 'd, R: DecodeMove<'d>, const N: usize>(
    source: [R::CType; N],
    store: &'itm mut ArrayStore<R::Store, N>,
) -> [R; N] {
    map_array(source, &mut store.0, |item, store| unsafe {
        R::soft_decode_unchecked(item, store)
    })
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    #[test]
    fn failure_finishes_conversion_before_dropping_successful_values() {
        use core::cell::Cell;

        struct CountDrop<'a>(&'a Cell<usize>);
        impl Drop for CountDrop<'_> {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }

        let drops = Cell::new(0);
        let mut stores = alloc::boxed::Box::default();
        let result = super::try_map_vec(0..3, 8, &mut stores, |index, store| {
            assert_eq!(drops.get(), 0);
            *store = Some(index);
            (index != 1).then(|| CountDrop(&drops))
        });
        assert!(result.is_none());
        assert_eq!(&*stores, &[Some(0), Some(1), Some(2)]);
        assert_eq!(drops.get(), 2);
    }

    #[test]
    fn constructs_lifetime_bearing_shared_carriers() {
        use core::cell::UnsafeCell;

        use super::{RefCarrier, WideRefCarrier};
        use crate::{reference::abi::Ref, wide::abi::WideRef as AbiWideRef};

        let value = 7_u8;
        let carrier: Ref<'_, u8> = unsafe { RefCarrier::from_raw(&value) };
        assert_eq!(unsafe { carrier.into_ref() }, Some(&value));
        let cell = core::cell::UnsafeCell::<u8>::new(5_u8);
        let carrier: Ref<'_, <UnsafeCell<u8> as crate::ReprC>::CType> =
            unsafe { RefCarrier::from_raw(&cell) };
        assert_eq!(carrier.as_ptr(), core::ptr::from_ref(&cell).cast_mut());
        let values = [1_u8, 2, 3];
        let carrier: AbiWideRef<'_, [u8]> =
            unsafe { WideRefCarrier::from_raw_parts(values.as_ptr(), values.len()) };
        assert_eq!(unsafe { carrier.into_ref() }, Some(&values[..]));
        let cells = [
            core::cell::UnsafeCell::<u8>::new(1_u8),
            core::cell::UnsafeCell::new(2),
        ];
        let carrier: AbiWideRef<'_, [UnsafeCell<u8>]> =
            unsafe { WideRefCarrier::from_raw_parts(cells.as_ptr(), cells.len()) };
        assert_eq!(carrier.as_ptr(), cells.as_ptr().cast_mut());
        assert_eq!(carrier.metadata(), cells.len());
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    use alloc::{boxed::Box, vec::Vec};
    #[cfg(all(feature = "alloc", feature = "derive"))]
    use core::{
        cell::UnsafeCell,
        mem::{ManuallyDrop, MaybeUninit},
    };

    use co3::convert::{Decode, Encode};
    #[cfg(all(feature = "alloc", feature = "derive"))]
    use co3::{
        convert::{EmptyStore, EncodeMove, EncodeMoveUnchecked, Store},
        marker::AbiCompatible,
    };
    #[cfg(all(feature = "alloc", feature = "derive"))]
    use static_assertions::{assert_impl_all, assert_not_impl_any};

    // FIXME: Restore these fixtures with the disabled fast-path tests below.
    /*
    #[cfg(feature = "alloc")]
    use core::sync::atomic::{AtomicUsize, Ordering};

    #[cfg(feature = "alloc")]
    static RAW_PROBE_CALLS: AtomicUsize = AtomicUsize::new(0);

    #[cfg(feature = "alloc")]
    #[derive(co3::rust_spec::RustSpec)]
    #[repr(transparent)]
    struct RawProbe(u8);

    #[cfg(feature = "alloc")]
    impl co3::ReprC for RawProbe
    {
        type CType = u8;
    }

    #[cfg(feature = "alloc")]
    unsafe impl co3::convert::EncodeMove for RawProbe
    {
        type Store = ();

        fn soft_encode<'itm>(self, (): &'itm mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            RAW_PROBE_CALLS.fetch_add(1, Ordering::SeqCst);
            self.0
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl EncodeMoveUnchecked for RawProbe
    {
        type SoftStore = ();

        unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType where Self: 'itm {
            self.0
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl co3::convert::AbiCompatibleValidate for RawProbe {
        unsafe fn is_valid(_: &Self::CType) -> bool {
            true
        }
    }
    unsafe impl AbiCompatible for RawProbe {}

    #[cfg(feature = "alloc")]
    impl co3::convert::EncodeUnchecked for RawProbe
    {}

    #[cfg(feature = "alloc")]
    impl Encode for RawProbe
    {}
    */

    #[cfg(all(feature = "alloc", feature = "derive"))]
    struct UncheckedOnly(u8);

    #[cfg(all(feature = "alloc", feature = "derive"))]
    impl co3::ReprC for UncheckedOnly {
        type CType = u8;
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    unsafe impl co3::convert::EncodeMove for UncheckedOnly {
        type Store = ();

        fn soft_encode<'itm>(self, (): &'itm mut ()) -> Self::CType
        where
            Self: 'itm,
        {
            self.0.wrapping_add(1)
        }
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    unsafe impl co3::convert::EncodeMoveUnchecked for UncheckedOnly {
        type SoftStore = ();

        unsafe fn soft_encode_unchecked<'itm>(
            self,
            _store: &'itm mut <Self as co3::convert::EncodeMoveUnchecked>::SoftStore,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            co3::encode(self)
        }
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    impl Encode for UncheckedOnly {}
    #[cfg(all(feature = "alloc", feature = "derive"))]
    impl co3::convert::EncodeUnchecked for UncheckedOnly {}

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[test]
    fn direct_encode_does_not_require_checked_transmute() {
        assert_not_impl_any!(UncheckedOnly: AbiCompatible);
        assert_eq!(unsafe { co3::encode_unchecked(UncheckedOnly(7)) }, 8);
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[derive(Default)]
    struct DirectState {
        calls: usize,
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    impl Store for DirectState {
        fn sync(self) -> Option<()> {
            Some(())
        }
    }

    // Deliberately unstable: exercise field conversion and its auxiliary store.
    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[derive(co3::rust_spec::RustSpec)]
    struct StoredDirect(u8);

    #[cfg(all(feature = "alloc", feature = "derive"))]
    crate::borrow::__impl_borrowed_identity!([] StoredDirect);

    #[cfg(all(feature = "alloc", feature = "derive"))]
    impl co3::ReprC for StoredDirect {
        type CType = u8;
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    unsafe impl EncodeMove for StoredDirect {
        type Store = ();

        fn soft_encode<'itm>(self, _: &mut Self::Store) -> Self::CType
        where
            Self: 'itm,
        {
            self.0
        }
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    unsafe impl EncodeMoveUnchecked for StoredDirect {
        type SoftStore = DirectState;

        unsafe fn soft_encode_unchecked<'itm>(
            self,
            store: &'itm mut <Self as EncodeMoveUnchecked>::SoftStore,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            store.calls += 1;
            self.0
        }
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    impl Encode for StoredDirect {}
    #[cfg(all(feature = "alloc", feature = "derive"))]
    impl co3::convert::EncodeUnchecked for StoredDirect {}

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[derive(co3::rust_spec::RustSpec, co3::ReprC)]
    #[repr(C)]
    struct StoredDirectRecord {
        value: StoredDirect,
        tag: u8,
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[test]
    fn direct_store_is_independent_and_propagates_through_derives() {
        let mut store = Default::default();
        let encoded = unsafe {
            co3::soft_encode_unchecked(
                StoredDirectRecord {
                    value: StoredDirect(7),
                    tag: 3,
                },
                &mut store,
            )
        };
        assert_eq!(encoded.value, 7);
        assert_eq!(encoded.tag, 3);
        assert_eq!(store.0.calls, 1);
        store.sync().unwrap();

        let mut option_store = Default::default();
        let encoded =
            unsafe { co3::soft_encode_unchecked(Some(StoredDirect(8)), &mut option_store) };
        assert_eq!(encoded.into_option(), Some(Some(8)));
        assert_eq!(option_store.as_ref().unwrap().calls, 1);
        let encoded =
            unsafe { co3::soft_encode_unchecked(None::<StoredDirect>, &mut option_store) };
        assert_eq!(encoded.into_option(), Some(None));
        assert!(option_store.is_none());

        let mut tuple_store = Default::default();
        let encoded =
            unsafe { co3::soft_encode_unchecked((StoredDirect(9), 4_u8), &mut tuple_store) };
        assert_eq!(encoded.0.0, 9);
        assert_eq!(tuple_store.0.calls, 1);

        let mut array_store = Default::default();
        let encoded = unsafe { co3::soft_encode_unchecked([StoredDirect(10)], &mut array_store) };
        assert_eq!(encoded, [10]);
        array_store.sync().unwrap();

        fn empty<T: EmptyStore>() {}
        empty::<<&mut bool as EncodeMoveUnchecked>::SoftStore>();
        empty::<<Option<&mut bool> as EncodeMoveUnchecked>::SoftStore>();
        empty::<<TransparentMut<'static> as EncodeMoveUnchecked>::SoftStore>();
        assert_not_impl_any!(<TransparentMut<'static> as EncodeMove>::Store: EmptyStore);
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[test]
    fn unstable_mutable_option_has_an_explicit_encoder() {
        assert_impl_all!(&mut Option<u8>: EncodeMoveUnchecked, EncodeMove);
    }

    // FIXME: Generic Box encoding cannot select its `EmptyStore` branch with `impls::impls!`;
    // checked Vec encoding also needs an allocation-reuse path.
    /*
    #[cfg(feature = "alloc")]
    #[test]
    fn stable_wrappers_use_fast_paths_without_inner_encoder_calls() {
        RAW_PROBE_CALLS.store(0, Ordering::SeqCst);

        let boxed = Box::new(RawProbe(7));
        let ptr = (&*boxed as *const RawProbe).cast_mut().cast::<u8>();
        let mut store = Default::default();
        let encoded = co3::soft_encode(boxed, &mut store);
        assert_eq!(encoded.as_ptr(), ptr);
        assert_eq!(unsafe { *encoded.as_ptr() }, 7);
        drop(unsafe { encoded.into_box().unwrap() });
        store.sync().unwrap();

        let mut store = Default::default();
        let encoded = co3::soft_encode(alloc::vec![RawProbe(1), RawProbe(2)], &mut store);
        assert_eq!(unsafe { *encoded.as_ptr() }, 1);
        drop(unsafe { encoded.into_box().unwrap() });
        store.sync().unwrap();

        let mut store = Default::default();
        let encoded = co3::soft_encode(ManuallyDrop::new(RawProbe(3)), &mut store);
        assert_eq!(encoded, 3);
        store.sync().unwrap();

        assert_eq!(RAW_PROBE_CALLS.load(Ordering::SeqCst), 0);
    }
    */

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[test]
    fn shared_interior_and_mutable_non_robust_refs_have_direct_paths() {
        assert_impl_all!(&UnsafeCell<u8>: EncodeMoveUnchecked);
        assert_impl_all!(&[UnsafeCell<u8>]: EncodeMoveUnchecked);
        assert_impl_all!(&mut bool: EncodeMoveUnchecked);
        assert_impl_all!(TransparentMut<'static>: EncodeMoveUnchecked);
        assert_impl_all!(&mut u8: EncodeMoveUnchecked);
        assert_impl_all!(&mut UnsafeCell<u8>: EncodeMoveUnchecked);
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[test]
    fn unchecked_encode_exposes_the_original_mutable_bool() {
        let mut value = false;
        let original = core::ptr::from_mut(&mut value).cast::<u8>();
        let encoded = unsafe { co3::encode_unchecked(&mut value) };
        assert_eq!(encoded.as_ptr().cast::<u8>(), original);
        unsafe { *encoded.as_ptr() = co3::primitive::abi::bool::TRUE };
        assert!(value);

        let encoded = unsafe { co3::encode_unchecked(TransparentMut(&mut value)) };
        assert_eq!(encoded.0.as_ptr().cast::<u8>(), original);
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[cfg(feature = "alloc")]
    #[test]
    fn unchecked_encode_reuses_a_boxed_mutable_bool_reference() {
        let mut value = false;
        let boxed = Box::new(&mut value);
        let original = core::ptr::from_ref(&*boxed).cast_mut().cast::<u8>();
        let encoded = unsafe { co3::encode_unchecked(boxed) };
        assert_eq!(encoded.as_ptr().cast::<u8>(), original);
        unsafe { *(*encoded.as_ptr()).as_ptr() = co3::primitive::abi::bool::TRUE };
        drop(unsafe { encoded.into_box().unwrap() });
        assert!(value);
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[cfg(feature = "alloc")]
    #[test]
    fn unchecked_encode_reuses_a_boxed_slice_of_mutable_bool_references() {
        let mut value = false;
        let boxed = alloc::vec![&mut value].into_boxed_slice();
        let original = boxed.as_ptr().cast_mut().cast::<u8>();
        let encoded = unsafe { co3::encode_unchecked(boxed) };
        assert_eq!(encoded.as_ptr().cast::<u8>(), original);
        unsafe { *(*encoded.as_ptr()).as_ptr() = co3::primitive::abi::bool::TRUE };
        drop(unsafe { encoded.into_box().unwrap() });
        assert!(value);
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[test]
    fn unchecked_encode_propagates_through_wrappers() {
        let mut value = false;
        let original = core::ptr::from_mut(&mut value).cast::<u8>();

        let optional = unsafe { co3::encode_unchecked(Some(&mut value)) };
        assert_eq!(optional.0.0.as_ptr().cast::<u8>(), original);

        let result = unsafe { co3::encode_unchecked(Ok::<&mut bool, ()>(&mut value)) };
        assert_eq!(result.0.0.as_ptr().cast::<u8>(), original);

        let record = unsafe {
            co3::encode_unchecked(RecordMut {
                value: &mut value,
                tag: 7,
            })
        };
        assert_eq!(record.value.as_ptr().cast::<u8>(), original);
        unsafe { *record.value.as_ptr() = co3::primitive::abi::bool::TRUE };
        assert!(value);

        let tuple = unsafe { co3::encode_unchecked((&mut value, 7_u8)) };
        assert_eq!(tuple.0.0.as_ptr().cast::<u8>(), original);

        let array = unsafe { co3::encode_unchecked([&mut value]) };
        assert_eq!(array[0].as_ptr().cast::<u8>(), original);
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[derive(co3::rust_spec::RustSpec, co3::ReprC)]
    #[repr(transparent)]
    struct TransparentMut<'a>(&'a mut bool);

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[derive(co3::rust_spec::RustSpec, co3::ReprC)]
    #[repr(C)]
    struct RecordMut<'a> {
        value: &'a mut bool,
        tag: u8,
    }

    // FIXME: Restore these fixtures with the disabled fast-path test below.
    /*
    #[derive(co3::rust_spec::RustSpec, co3::ReprC)]
    #[repr(C)]
    struct RawRecord {
        value: u8,
        tag: u16,
    }

    #[derive(co3::rust_spec::RustSpec, co3::ReprC)]
    #[repr(transparent)]
    struct RawTransparent(u8);

    #[derive(co3::rust_spec::RustSpec, co3::ReprC)]
    struct RustRecord(u8, u16);

    #[derive(co3::rust_spec::RustSpec, co3::ReprC)]
    #[repr(C, u8)]
    enum RawVariant {
        Value(u8),
        Empty,
    }
    */

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[derive(co3::rust_spec::RustSpec, co3::ReprC)]
    enum VariantMut<'a> {
        Value(&'a mut bool),
        Empty,
    }

    // FIXME: Generic Box encoding cannot select its `EmptyStore` branch with `impls::impls!`;
    // checked Vec encoding also needs an allocation-reuse path.
    /*
    #[cfg(feature = "alloc")]
    #[test]
    fn derived_representations_preserve_stable_fast_paths() {
        assert_impl_all!(RawRecord: EncodeMoveUnchecked);
        assert_impl_all!(RawTransparent: EncodeMoveUnchecked);
        assert_impl_all!(RawVariant: EncodeMoveUnchecked);
        assert_impl_all!(RustRecord: EncodeMoveUnchecked);
        assert_impl_all!(RecordMut<'static>: EncodeMoveUnchecked);
        assert_impl_all!(VariantMut<'static>: EncodeMoveUnchecked);

        let value = Box::new(RawRecord { value: 3, tag: 5 });
        let ptr = (&*value as *const RawRecord).cast_mut().cast();
        let mut store = Default::default();
        let encoded = co3::soft_encode(value, &mut store);
        assert_eq!(encoded.as_ptr(), ptr);
        drop(unsafe { encoded.into_box().unwrap() });
        store.sync().unwrap();

        let values = alloc::vec![RawRecord { value: 7, tag: 9 }];
        let ptr = values.as_ptr().cast_mut().cast();
        let mut store = Default::default();
        let encoded = co3::soft_encode(values, &mut store);
        assert_eq!(encoded.as_ptr(), ptr);
        drop(unsafe { encoded.into_box().unwrap() });
        store.sync().unwrap();

        let value = Box::new(RawVariant::Value(11));
        let ptr = (&*value as *const RawVariant).cast_mut().cast();
        let mut store = Default::default();
        let encoded = co3::soft_encode(value, &mut store);
        assert_eq!(encoded.as_ptr(), ptr);
        let carrier = unsafe { encoded.into_box().unwrap() };
        let decoded: RawVariant = unsafe { co3::decode(*carrier) }.unwrap();
        assert!(matches!(decoded, RawVariant::Value(11)));
        store.sync().unwrap();
    }
    */

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[test]
    fn maybe_uninit_rejects_encoding_an_inner_value_with_a_store() {
        assert_impl_all!(MaybeUninit<&mut bool>: AbiCompatible, Decode<'static>);
        assert_not_impl_any!(MaybeUninit<&mut bool>: Encode);
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[test]
    fn manually_drop_mut_bool_syncs_its_store() {
        let mut value = false;
        let mut store = Default::default();
        let encoded = co3::soft_encode(ManuallyDrop::new(&mut value), &mut store);
        unsafe { *encoded.as_ptr() = co3::primitive::abi::bool::TRUE };
        store.sync().unwrap();
        assert!(value);
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[cfg(feature = "alloc")]
    #[test]
    fn manually_drop_boxed_mut_bool_syncs_its_store() {
        let mut value = false;
        let mut store = Default::default();
        let encoded = co3::soft_encode(ManuallyDrop::new(Box::new(&mut value)), &mut store);
        unsafe { *(*encoded.as_ptr()).as_ptr() = co3::primitive::abi::bool::TRUE };
        drop(unsafe {
            core::mem::ManuallyDrop::into_inner(encoded)
                .into_box()
                .unwrap()
        });
        store.sync().unwrap();
        assert!(value);
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[cfg(feature = "alloc")]
    #[test]
    fn vec_mut_bool_syncs_element_stores() {
        let mut values = [false, false];
        let mut store = Default::default();
        let encoded = co3::soft_encode(values.iter_mut().collect::<Vec<_>>(), &mut store);
        unsafe { *(*encoded.as_ptr()).as_ptr() = co3::primitive::abi::bool::TRUE };
        drop(unsafe { encoded.into_vec().unwrap() });
        store.sync().unwrap();
        assert_eq!(values, [true, false]);
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[test]
    fn optional_mut_bool_syncs_only_present_values() {
        let mut value = false;
        let mut store = Default::default();
        let encoded = co3::soft_encode(Some(&mut value), &mut store);
        unsafe { *encoded.0.0.as_ptr() = co3::primitive::abi::bool::TRUE };
        store.sync().unwrap();
        assert!(value);

        let mut store = Default::default();
        let _: <Option<&mut bool> as co3::ReprC>::CType =
            co3::soft_encode(None::<&mut bool>, &mut store);
        store.sync().unwrap();
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[cfg(feature = "alloc")]
    #[test]
    fn vec_of_optional_mut_bool_syncs_present_elements() {
        let mut value = false;
        let mut store = Default::default();
        let encoded = co3::soft_encode(alloc::vec![Some(&mut value), None], &mut store);
        unsafe { *(*encoded.as_ptr()).0.0.as_ptr() = co3::primitive::abi::bool::TRUE };
        drop(unsafe { encoded.into_vec().unwrap() });
        store.sync().unwrap();
        assert!(value);
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[test]
    fn repr_c_option_without_payload_skips_inner_store() {
        let mut store = Default::default();
        let _: <co3::option::Option<&mut bool> as co3::ReprC>::CType =
            co3::soft_encode(co3::option::Option::<&mut bool>::None, &mut store);
        store.sync().unwrap();
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[test]
    fn niche_result_without_mut_ref_skips_inner_store() {
        let mut store = Default::default();
        let _: <Result<&mut bool, ()> as co3::ReprC>::CType =
            co3::soft_encode(Err::<&mut bool, ()>(()), &mut store);
        store.sync().unwrap();

        let mut store = Default::default();
        let _: <Result<(), &mut bool> as co3::ReprC>::CType =
            co3::soft_encode(Ok::<(), &mut bool>(()), &mut store);
        store.sync().unwrap();
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[test]
    fn derived_transparent_mut_bool_syncs_its_store() {
        assert_impl_all!(TransparentMut<'static>: AbiCompatible);
        let mut value = false;
        let mut store = Default::default();
        let encoded = co3::soft_encode(TransparentMut(&mut value), &mut store);
        unsafe { *encoded.0.as_ptr() = co3::primitive::abi::bool::TRUE };
        store.sync().unwrap();
        assert!(value);
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[test]
    fn optional_derived_transparent_syncs_only_present_values() {
        let mut value = false;
        let mut store = Default::default();
        let encoded = co3::soft_encode(Some(TransparentMut(&mut value)), &mut store);
        unsafe { *encoded.0.0.0.as_ptr() = co3::primitive::abi::bool::TRUE };
        store.sync().unwrap();
        assert!(value);

        let mut store = Default::default();
        let _encoded = co3::soft_encode(None::<TransparentMut<'static>>, &mut store);
        store.sync().unwrap();
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[test]
    fn derived_record_mut_bool_syncs_its_field_store() {
        assert_impl_all!(RecordMut<'static>: AbiCompatible);
        let mut value = false;
        let mut store = Default::default();
        let encoded = co3::soft_encode(
            RecordMut {
                value: &mut value,
                tag: 7,
            },
            &mut store,
        );
        unsafe { *encoded.value.as_ptr() = co3::primitive::abi::bool::TRUE };
        assert_eq!(encoded.tag, 7);
        store.sync().unwrap();
        assert!(value);
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[cfg(feature = "alloc")]
    #[test]
    fn vec_of_derived_records_syncs_field_stores() {
        let mut value = false;
        let mut store = Default::default();
        let encoded = co3::soft_encode(
            alloc::vec![RecordMut {
                value: &mut value,
                tag: 7,
            }],
            &mut store,
        );
        unsafe { *(*encoded.as_ptr()).value.as_ptr() = co3::primitive::abi::bool::TRUE };
        drop(unsafe { encoded.into_vec().unwrap() });
        store.sync().unwrap();
        assert!(value);
    }

    #[cfg(all(feature = "alloc", feature = "derive"))]
    #[test]
    fn derived_enum_syncs_its_active_variant_store() {
        let mut value = false;
        let mut store = Default::default();
        let _encoded = co3::soft_encode(VariantMut::Value(&mut value), &mut store);
        store.sync().unwrap();

        let mut store = Default::default();
        let _encoded = co3::soft_encode(VariantMut::Empty, &mut store);
        store.sync().unwrap();
    }
}
