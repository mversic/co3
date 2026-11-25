//! A module for working with borrowed data.

#[doc(hidden)]
// TODO: Questionable export
#[cfg(feature = "alloc")]
pub use alloc::borrow::ToOwned;
#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};
use core::marker::PhantomData;

use disjoint_impls::disjoint_impls;
use rust_spec::{
    Gt, RustSpec,
    mutability::{Exclusive, WithInterior, WithoutInterior},
    size::{Sized as RustSpecSized, Zero},
};
#[cfg(feature = "alloc")]
use rust_spec::{
    Specified,
    layout::{NonRobust, Robust},
    size::SizedKind,
};
#[cfg(all(feature = "alloc", feature = "repr-rust"))]
use rust_spec::{Unspecified, size::NulTerminated};

#[cfg(feature = "alloc")]
use crate::{
    CType, SupportedLayout, boxed::abi::Box as AbiBox, marker::AbiCompatible, stored::zip_stores,
    vec::abi::Vec as AbiVec, wide::abi::WideBox as AbiWideBox,
};
use crate::{
    ReprC,
    convert::{
        DecodeMove, DecodeMoveUnchecked, EmptyStore, EncodeMove, EncodeMoveUnchecked, Store,
    },
    lang::{self, Language},
    marker::{Borrowed, Native},
    niche::Niche,
    option::abi::OptionSome as AbiOptionSome,
    result::abi::{ResultErr as AbiResultErr, ResultOk as AbiResultOk},
};
#[cfg(feature = "repr-rust")]
use crate::{option::Option as StableOption, result::Result as StableResult};

/// Encode a borrowed carrier without retaining conversion storage.
///
/// Keep the owner alive and unchanged while the carrier is in use.
///
/// # Safety
/// The requirements of [`crate::encode`] apply. Retain the owner until all
/// carrier access and conversion-store cleanup have finished.
pub unsafe fn encode<T: BorrowedEncode<L, Store: EmptyStore>, L: Language>(
    value: T,
    owner: &mut T::Owner,
) -> Borrowed<T::CType> {
    Borrowed(T::soft_encode(value, &mut Default::default(), owner))
}

/// Decode a borrowed carrier without retaining conversion storage.
///
/// # Safety
///
/// The source must satisfy the access and validity requirements of
/// [`BorrowedDecode::soft_decode`], including the lifetime and aliasing requirements
/// of any references in the result. The source must uphold the mapped native
/// validity and aliasing guarantees of `L`.
pub unsafe fn decode<'d, T: BorrowedDecode<'d, L, Store: EmptyStore + 'd>, L: Language>(
    source: Borrowed<T::CType>,
) -> Option<T> {
    let mut store = T::Store::default();
    // An empty store cannot back any references in the decoded result.
    let store = unsafe { core::mem::transmute::<&mut T::Store, &'d mut T::Store>(&mut store) };
    unsafe { <T as BorrowedDecode<'d, L>>::soft_decode(source.0, store) }
}

/// Perform the unchecked borrowed conversion.
///
/// # Safety
/// The source and foreign access must preserve all Rust validity and ownership invariants.
pub unsafe fn encode_unchecked<T: BorrowedEncodeUnchecked<SoftStore: EmptyStore>>(
    value: T,
    owner: &mut T::Owner,
) -> Borrowed<T::CType> {
    Borrowed(unsafe { T::soft_encode_unchecked(value, &mut Default::default(), owner) })
}
/// Perform the unchecked borrowed conversion.
///
/// # Safety
/// The source and foreign access must preserve all Rust validity and ownership invariants.
pub unsafe fn decode_unchecked<'d, T: BorrowedDecodeUnchecked<'d, SoftStore: EmptyStore + 'd>>(
    source: Borrowed<T::CType>,
) -> T {
    let mut store = T::SoftStore::default();
    // An empty store cannot back any references in the decoded result.
    let store =
        unsafe { core::mem::transmute::<&mut T::SoftStore, &'d mut T::SoftStore>(&mut store) };
    unsafe { <T as BorrowedDecodeUnchecked<'d>>::soft_decode_unchecked(source.0, store) }
}

disjoint_impls! {
    /// Encodes a value without transferring its owned resources across FFI.
    ///
    /// # Safety
    /// The carrier must obey borrowed access and remain valid while both owner and
    /// conversion storage are retained. Conversion storage must be released before
    /// its owner. An empty conversion store must not retain required backing storage.
    pub unsafe trait BorrowedEncodeUnchecked: ReprC<CType: Sized> + Sized {
        /// Retained ownership shared by checked and unchecked encoding.
        type Owner: Default;

        /// Conversion storage for the unchecked encoding path.
        type SoftStore: Store + Default;

        /// Encode using the unchecked path and its independent store.
        ///
        /// Prefer using [`crate::soft_encode_unchecked`] whenever possible
        ///
        /// # Safety
        ///
        /// See [`crate::soft_encode_unchecked`].
        unsafe fn soft_encode_unchecked<'itm>(
            self,
            store: &'itm mut Self::SoftStore,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm;
    }

    #[cfg(feature = "alloc")]
    unsafe impl<T: AbiCompatible + BorrowedEncodeUnchecked, S: SizedKind> BorrowedEncodeUnchecked
        for Box<T>
    where
        T: RustSpec<Layout = Specified, Size = RustSpecSized<S>> + ReprC,
        Self: ReprC<CType = AbiBox<<T as ReprC>::CType>>,
        <T as ReprC>::CType: Sized,
    {
        type Owner = (Option<Self>, Option<Box<T::CType>>, Option<Box<T::Owner>>);
        type SoftStore = ();

        unsafe fn soft_encode_unchecked<'itm>(
            self,
            store: &'itm mut Self::SoftStore,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            let _ = store;
            *owner = Default::default();
            let value = owner.0.insert(self);
            // The allocation stays owned here; only its address crosses FFI.
            unsafe { AbiBox::from_raw(core::ptr::from_mut(&mut **value).cast()) }
        }
    }

    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<T: BorrowedEncodeUnchecked, S: SizedKind> BorrowedEncodeUnchecked for Box<T>
    where
        T: RustSpec<Layout = Unspecified, Size = RustSpecSized<S>> + ReprC,
        Self: ReprC<CType = AbiBox<<T as ReprC>::CType>>,
    {
        type Owner = (Option<Box<T::CType>>, Box<T::Owner>);
        type SoftStore = Box<T::SoftStore>;

        unsafe fn soft_encode_unchecked<'itm>(
            self,
            store: &'itm mut Self::SoftStore,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            **store = Default::default();
            *owner = Default::default();
            let value = unsafe { T::soft_encode_unchecked(*self, store, &mut owner.1) };
            let value = owner.0.insert(Box::new(value));
            unsafe { AbiBox::from_raw(core::ptr::from_mut(&mut **value)) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<T: AbiCompatible + BorrowedEncodeUnchecked> BorrowedEncodeUnchecked for Box<[T]>
    where
        T: RustSpec<Layout = Specified> + ReprC,
        [T]: RustSpec<Layout = Specified>,
        Self: ReprC<CType = AbiWideBox<<[T] as crate::ReprC>::CType>>,
        <T as ReprC>::CType: Sized,
    {
        type Owner = (Option<Self>, Option<Box<[T::CType]>>, Box<[T::Owner]>);
        type SoftStore = ();

        unsafe fn soft_encode_unchecked<'itm>(
            self,
            store: &'itm mut Self::SoftStore,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            let _ = store;
            *owner = Default::default();
            let values = owner.0.insert(self);
            let data = core::ptr::NonNull::new(values.as_mut_ptr()).unwrap().cast();
            unsafe { AbiWideBox::from_raw_parts(data, values.len()) }
        }
    }

    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<T: BorrowedEncodeUnchecked> BorrowedEncodeUnchecked for Box<[T]>
    where
        T: RustSpec<Layout = Unspecified> + ReprC,
        [T]: RustSpec<Layout = Unspecified>,
        Self: ReprC<CType = AbiWideBox<<[T] as crate::ReprC>::CType>>,
    {
        type Owner = (Option<Box<[T::CType]>>, Box<[T::Owner]>);
        type SoftStore = Box<[T::SoftStore]>;

        unsafe fn soft_encode_unchecked<'itm>(
            self,
            store: &'itm mut Self::SoftStore,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            *store = Default::default();
            *owner = Default::default();
            let values = encode_borrowed_elements::<_, _, _, _, _>(
                self.into_iter(),
                store,
                &mut owner.1,
                |value, store, owner| unsafe { T::soft_encode_unchecked(value, store, owner) },
            );
            let values = owner.0.insert(values);
            let data = core::ptr::NonNull::new(values.as_mut_ptr()).unwrap();
            unsafe { AbiWideBox::from_raw_parts(data, values.len()) }
        }
    }

    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<T: crate::ffi::NulTerminatedBuf + ?Sized> BorrowedEncodeUnchecked for Box<T>
    where
        T: RustSpec<Layout = Unspecified, Size = NulTerminated> + ReprC,
        <T as crate::ffi::NulTerminatedBuf>::Header:
            AbiCompatible<CType = <T as ReprC>::CType> + RustSpec<Layout = Specified>,
        Self: ReprC<CType = AbiBox<<T as ReprC>::CType>>,
        <T as ReprC>::CType: Sized,
    {
        type Owner = Option<Self>;
        type SoftStore = ();

        unsafe fn soft_encode_unchecked<'itm>(
            self,
            store: &'itm mut Self::SoftStore,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            let _ = store;
            let value = owner.insert(self);
            // The borrowed ABI permits no writes through this carrier.
            unsafe { AbiBox::from_raw(T::as_ptr(value).cast_mut().cast()) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<T: AbiCompatible + BorrowedEncodeUnchecked> BorrowedEncodeUnchecked
        for crate::vec::Vec<T>
    where
        T: RustSpec<Layout = Specified> + ReprC,
        <T as ReprC>::CType: Sized,
    {
        type Owner = (Option<Self>, Vec<T::CType>, Box<[T::Owner]>);
        type SoftStore = ();

        unsafe fn soft_encode_unchecked<'itm>(
            self,
            store: &'itm mut Self::SoftStore,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            let _ = store;
            *owner = Default::default();
            let value = owner.0.insert(self);
            unsafe { AbiVec::from_raw_parts(value.as_mut_ptr().cast(), value.len(), value.capacity()) }
        }
    }

    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<T: BorrowedEncodeUnchecked> BorrowedEncodeUnchecked for crate::vec::Vec<T>
    where
        T: RustSpec<Layout = Unspecified> + ReprC,
    {
        type Owner = (Vec<T::CType>, Box<[T::Owner]>);
        type SoftStore = Box<[T::SoftStore]>;

        unsafe fn soft_encode_unchecked<'itm>(
            self,
            store: &'itm mut Self::SoftStore,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            *store = Default::default();
            *owner = Default::default();
            owner.0 = encode_borrowed_elements::<_, _, _, _, _>(
                self.into_iter(),
                store,
                &mut owner.1,
                |value, store, owner| unsafe { T::soft_encode_unchecked(value, store, owner) },
            );
            unsafe { AbiVec::from_raw_parts(owner.0.as_mut_ptr(), owner.0.len(), owner.0.capacity()) }
        }
    }

    #[cfg(feature = "repr-rust")]
    unsafe impl<R: BorrowedEncodeUnchecked> BorrowedEncodeUnchecked for Option<R>
    where
        Self: ReprC<CType = Native<crate::option::abi::Option<<R as ReprC>::CType>>>,
    {
        type Owner = <StableOption<R> as BorrowedEncodeUnchecked>::Owner;
        type SoftStore = <StableOption<R> as BorrowedEncodeUnchecked>::SoftStore;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(
            self,
            store: &'itm mut Self::SoftStore,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            Native({
                unsafe {
                    BorrowedEncodeUnchecked::soft_encode_unchecked(
                        StableOption::from(self),
                        store,
                        owner,
                    )
                }
            })
        }
    }
    unsafe impl<R: BorrowedEncodeUnchecked + Niche, N: rust_spec::layout::LayoutKind>
        BorrowedEncodeUnchecked for Option<R>
    where
        R: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<CType = Native<crate::option::abi::OptionSome<<R as ReprC>::CType>>>,
    {
        type Owner = Option<R::Owner>;
        type SoftStore = Option<R::SoftStore>;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(
            self,
            store: &'itm mut Self::SoftStore,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = unsafe { crate::option::encode_optional_payload!(self, Some, BorrowedEncodeUnchecked::soft_encode_unchecked, store, owner) };
            Native(AbiOptionSome(encoded.unwrap_or(R::NICHE)))
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<R: BorrowedEncodeUnchecked, E: BorrowedEncodeUnchecked> BorrowedEncodeUnchecked
        for Result<R, E>
    where
        Self:
            ReprC<CType = Native<crate::result::abi::Result<<R as ReprC>::CType, <E as ReprC>::CType>>>,
        <R as ReprC>::CType: Copy,
        <E as ReprC>::CType: Copy,
    {
        type Owner = <StableResult<R, E> as BorrowedEncodeUnchecked>::Owner;
        type SoftStore = <StableResult<R, E> as BorrowedEncodeUnchecked>::SoftStore;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(
            self,
            store: &'itm mut Self::SoftStore,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            Native({
                unsafe {
                    BorrowedEncodeUnchecked::soft_encode_unchecked(
                        StableResult::from(self),
                        store,
                        owner,
                    )
                }
            })
        }
    }
    unsafe impl<R: BorrowedEncodeUnchecked + Niche, E: ReprC, N: rust_spec::layout::LayoutKind>
        BorrowedEncodeUnchecked for Result<R, E>
    where
        R: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<
            CType = Native<crate::result::abi::ResultOk<<R as ReprC>::CType, <E as ReprC>::CType>>,
        >,
        R: RustSpec<Size = RustSpecSized<Gt<Zero>>>,
        E: RustSpec<Size = RustSpecSized<Zero>>,
        <E as ReprC>::CType: Sized,
    {
        type Owner = Option<R::Owner>;
        type SoftStore = Option<R::SoftStore>;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(
            self,
            store: &'itm mut Self::SoftStore,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = unsafe { crate::option::encode_optional_payload!(self, Ok, BorrowedEncodeUnchecked::soft_encode_unchecked, store, owner) };
            Native(AbiResultOk(encoded.unwrap_or(R::NICHE), PhantomData))
        }
    }
    unsafe impl<R: ReprC, E: BorrowedEncodeUnchecked + Niche, N: rust_spec::layout::LayoutKind>
        BorrowedEncodeUnchecked for Result<R, E>
    where
        E: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<
            CType = Native<crate::result::abi::ResultErr<<R as ReprC>::CType, <E as ReprC>::CType>>,
        >,
        R: RustSpec<Size = RustSpecSized<Zero>>,
        E: RustSpec<Size = RustSpecSized<Gt<Zero>>>,
        <R as ReprC>::CType: Sized,
    {
        type Owner = Option<E::Owner>;
        type SoftStore = Option<E::SoftStore>;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(
            self,
            store: &'itm mut Self::SoftStore,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = unsafe { crate::option::encode_optional_payload!(self, Err, BorrowedEncodeUnchecked::soft_encode_unchecked, store, owner) };
            Native(AbiResultErr(encoded.unwrap_or(E::NICHE), PhantomData))
        }
    }

    #[cfg(all(feature = "alloc", feature = "repr-rust"))]
    unsafe impl BorrowedEncodeUnchecked for Box<str>
    where
        str: RustSpec<Size = rust_spec::size::MetaSized<rust_spec::size::SliceLike>>,
        Self: ReprC<CType = AbiWideBox<crate::primitive::abi::str>>,
    {
        type Owner = Option<Self>;
        type SoftStore = ();

        unsafe fn soft_encode_unchecked<'itm>(
            self,
            store: &'itm mut Self::SoftStore,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            let _ = store;
            let value = owner.insert(self);
            let ptr = core::ptr::from_mut(&mut **value);
            let len = <str as crate::wide::Wide>::metadata(ptr);
            let data = core::ptr::NonNull::new(<str as crate::wide::Wide>::as_mut_ptr(ptr))
                .unwrap()
                .cast();
            unsafe { crate::wide::abi::WideBox::from_raw_parts(data, len) }
        }
    }
}

disjoint_impls! {
    /// Checked borrowed encoding.
    ///
    /// # Safety
    /// The carrier must satisfy the checked borrowed-access contract and the unchecked
    /// trait's storage requirements.
    pub unsafe trait BorrowedEncode<L: Language = lang::C>: BorrowedEncodeUnchecked {
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
        /// Use [`encode`] when no conversion storage is required.
        fn soft_encode<'itm>(
            self,
            store: &'itm mut Self::Store,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm;
    }
    #[cfg(feature = "alloc")]
    unsafe impl<L: Language, T: AbiCompatible + BorrowedEncode<L>, S: SizedKind> BorrowedEncode<L>
        for Box<T>
    where
        T: RustSpec<Layout = Specified, Size = RustSpecSized<S>, Trap = Robust> + ReprC,
        Self: ReprC<CType = AbiBox<<T as ReprC>::CType>>,
        <T as ReprC>::CType: Sized,
    {
        type Store = ();

        fn soft_encode<'itm>(
            self,
            store: &'itm mut (),
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            // The original allocation is already a valid carrier and the owner retains it.
            unsafe { BorrowedEncodeUnchecked::soft_encode_unchecked(self, store, owner) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<L: Language, T: AbiCompatible + BorrowedEncode<L>, S: SizedKind> BorrowedEncode<L>
        for Box<T>
    where
        T: RustSpec<Layout = Specified, Size = RustSpecSized<S>, Trap = NonRobust> + ReprC,
        Self: ReprC<CType = AbiBox<<T as ReprC>::CType>>,
        <T as ReprC>::CType: Sized,
    {
        type Store = Box<T::Store>;

        fn soft_encode<'itm>(
            self,
            store: &'itm mut Self::Store,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            **store = Default::default();
            *owner = Default::default();
            // Move the pointee once, keeping its allocation for the ABI-compatible result.
            // On panic the uninitialized box frees the allocation without dropping T again.
            let ptr = Box::into_raw(self);
            let mut allocation =
                unsafe { Box::from_raw(ptr.cast::<core::mem::MaybeUninit<T::CType>>()) };
            let value = T::soft_encode(
                unsafe { allocation.as_mut_ptr().cast::<T>().read() },
                store,
                owner.2.insert(Box::default()),
            );
            allocation.write(value);
            let value = owner.1.insert(unsafe { allocation.assume_init() });
            unsafe { AbiBox::from_raw(core::ptr::from_mut(&mut **value)) }
        }
    }

    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<L: Language, T: BorrowedEncode<L>, S: SizedKind> BorrowedEncode<L> for Box<T>
    where
        T: RustSpec<Layout = Unspecified, Size = RustSpecSized<S>> + ReprC,
        Self: ReprC<CType = AbiBox<<T as ReprC>::CType>>,
    {
        type Store = Box<T::Store>;

        fn soft_encode<'itm>(
            self,
            store: &'itm mut Self::Store,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            **store = Default::default();
            *owner = Default::default();
            let value = T::soft_encode(*self, store, &mut owner.1);
            let value = owner.0.insert(Box::new(value));
            unsafe { AbiBox::from_raw(core::ptr::from_mut(&mut **value)) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<L: Language, T: AbiCompatible + BorrowedEncode<L>> BorrowedEncode<L> for Box<[T]>
    where
        T: RustSpec<Layout = Specified> + ReprC,
        [T]: RustSpec<Layout = Specified, Trap = Robust>,
        Self: ReprC<CType = AbiWideBox<<[T] as crate::ReprC>::CType>>,
        <T as ReprC>::CType: Sized,
    {
        type Store = ();

        fn soft_encode<'itm>(
            self,
            store: &'itm mut (),
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            unsafe { BorrowedEncodeUnchecked::soft_encode_unchecked(self, store, owner) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<L: Language, T: AbiCompatible + BorrowedEncode<L>> BorrowedEncode<L> for Box<[T]>
    where
        T: RustSpec<Layout = Specified> + ReprC,
        [T]: RustSpec<Layout = Specified, Trap = NonRobust>,
        Self: ReprC<CType = AbiWideBox<<[T] as crate::ReprC>::CType>>,
        <T as ReprC>::CType: Sized,
    {
        type Store = Box<[T::Store]>;

        fn soft_encode<'itm>(
            self,
            store: &'itm mut Self::Store,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            *store = Default::default();
            *owner = Default::default();
            let values = encode_borrowed_elements::<_, _, _, _, _>(
                self.into_iter(),
                store,
                &mut owner.2,
                |value, store, owner| T::soft_encode(value, store, owner),
            );
            let values = owner.1.insert(values);
            let data = core::ptr::NonNull::new(values.as_mut_ptr()).unwrap();
            unsafe { AbiWideBox::from_raw_parts(data, values.len()) }
        }
    }

    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<L: Language, T: BorrowedEncode<L>> BorrowedEncode<L> for Box<[T]>
    where
        T: RustSpec<Layout = Unspecified> + ReprC,
        [T]: RustSpec<Layout = Unspecified>,
        Self: ReprC<CType = AbiWideBox<<[T] as crate::ReprC>::CType>>,
    {
        type Store = Box<[T::Store]>;

        fn soft_encode<'itm>(
            self,
            store: &'itm mut Self::Store,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            *store = Default::default();
            *owner = Default::default();
            let values = encode_borrowed_elements::<_, _, _, _, _>(
                self.into_iter(),
                store,
                &mut owner.1,
                |value, store, owner| T::soft_encode(value, store, owner),
            );
            let values = owner.0.insert(values);
            let data = core::ptr::NonNull::new(values.as_mut_ptr()).unwrap();
            unsafe { AbiWideBox::from_raw_parts(data, values.len()) }
        }
    }

    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<L: Language, T: crate::ffi::NulTerminatedBuf + ?Sized> BorrowedEncode<L> for Box<T>
    where
        T: RustSpec<Layout = Unspecified, Size = NulTerminated> + ReprC,
        <T as crate::ffi::NulTerminatedBuf>::Header:
            AbiCompatible<CType = <T as ReprC>::CType> + RustSpec<Layout = Specified>,
        Self: ReprC<CType = AbiBox<<T as ReprC>::CType>>,
        <T as ReprC>::CType: Sized,
    {
        type Store = ();

        fn soft_encode<'itm>(
            self,
            store: &'itm mut Self::Store,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            // The owned input already satisfies the NUL-terminated buffer contract.
            unsafe { BorrowedEncodeUnchecked::soft_encode_unchecked(self, store, owner) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<L: Language, T: AbiCompatible + BorrowedEncode<L>> BorrowedEncode<L>
        for crate::vec::Vec<T>
    where
        T: RustSpec<Layout = Specified> + ReprC,
        <T as ReprC>::CType: Sized,
    {
        type Store = Box<[T::Store]>;

        fn soft_encode<'itm>(
            self,
            store: &'itm mut Self::Store,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            *store = Default::default();
            *owner = Default::default();
            owner.1 = encode_borrowed_elements::<_, _, _, _, _>(
                self.into_iter(),
                store,
                &mut owner.2,
                |value, store, owner| T::soft_encode(value, store, owner),
            );
            unsafe { AbiVec::from_raw_parts(owner.1.as_mut_ptr(), owner.1.len(), owner.1.capacity()) }
        }
    }

    #[cfg(feature = "alloc")]
    #[cfg(feature = "repr-rust")]
    unsafe impl<L: Language, T: BorrowedEncode<L>> BorrowedEncode<L> for crate::vec::Vec<T>
    where
        T: RustSpec<Layout = Unspecified> + ReprC,
    {
        type Store = Box<[T::Store]>;

        fn soft_encode<'itm>(
            self,
            store: &'itm mut Self::Store,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            *store = Default::default();
            *owner = Default::default();
            owner.0 = encode_borrowed_elements::<_, _, _, _, _>(
                self.into_iter(),
                store,
                &mut owner.1,
                |value, store, owner| T::soft_encode(value, store, owner),
            );
            unsafe { AbiVec::from_raw_parts(owner.0.as_mut_ptr(), owner.0.len(), owner.0.capacity()) }
        }
    }

    unsafe impl<'a, L: Language, T: ?Sized> BorrowedEncode<L> for &'a T
    where
        T: RustSpec<Mutability = Exclusive<WithoutInterior>>,
        Self: EncodeMove<L>,
    {
        type Store = <Self as EncodeMove<L>>::Store;

        fn soft_encode<'itm>(
            self,
            store: &'itm mut Self::Store,
            _owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            EncodeMove::<L>::soft_encode(self, store)
        }
    }

    unsafe impl<'a, L: Language, T: ?Sized, I> BorrowedEncode<L> for &'a T
    where
        T: RustSpec<Mutability = Exclusive<WithInterior<I>>>,
        Self: EncodeMoveUnchecked,
    {
        type Store = <Self as EncodeMoveUnchecked>::SoftStore;

        fn soft_encode<'itm>(
            self,
            store: &'itm mut Self::Store,
            _owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            unsafe { EncodeMoveUnchecked::soft_encode_unchecked(self, store) }
        }
    }

    unsafe impl<'a, L: Language, T: ?Sized, I> BorrowedEncode<L> for &'a T
    where
        T: RustSpec<Mutability = WithInterior<I>>,
        Self: EncodeMoveUnchecked,
    {
        type Store = <Self as EncodeMoveUnchecked>::SoftStore;

        fn soft_encode<'itm>(
            self,
            store: &'itm mut Self::Store,
            _owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            unsafe { EncodeMoveUnchecked::soft_encode_unchecked(self, store) }
        }
    }

    #[cfg(feature = "repr-rust")]
    unsafe impl<L: Language, R: BorrowedEncode<L>> BorrowedEncode<L> for Option<R>
    where
        Self: ReprC<CType = Native<crate::option::abi::Option<<R as ReprC>::CType>>>,
        Self: ReprC<CType = Native<crate::option::abi::Option<<R as ReprC>::CType>>>,
    {
        type Store = <StableOption<R> as BorrowedEncode<L>>::Store;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &mut Self::Store, owner: &'itm mut Self::Owner) -> Self::CType
        where
            Self: 'itm,
        {
            Native({ StableOption::from(self).soft_encode(store, owner) })
        }
    }
    unsafe impl<L: Language, R: BorrowedEncode<L> + Niche, N: rust_spec::layout::LayoutKind>
        BorrowedEncode<L> for Option<R>
    where
        R: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<CType = Native<crate::option::abi::OptionSome<<R as ReprC>::CType>>>,
    {
        type Store = Option<R::Store>;

        #[inline(always)]
        fn soft_encode<'itm>(
            self,
            store: &'itm mut Self::Store,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = crate::option::encode_optional_payload!(self, Some, BorrowedEncode::<L>::soft_encode, store, owner);
            Native(AbiOptionSome(crate::niche::checked_option_payload::<R>(encoded)))
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<L: Language, R: BorrowedEncode<L>, E: BorrowedEncode<L>> BorrowedEncode<L>
        for Result<R, E>
    where
        Self:
            ReprC<CType = Native<crate::result::abi::Result<<R as ReprC>::CType, <E as ReprC>::CType>>>,
        <R as ReprC>::CType: Copy,
        <E as ReprC>::CType: Copy,
    {
        type Store = <StableResult<R, E> as BorrowedEncode<L>>::Store;

        #[inline(always)]
        fn soft_encode<'itm>(
            self,
            store: &'itm mut Self::Store,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            Native({ StableResult::from(self).soft_encode(store, owner) })
        }
    }
    unsafe impl<L: Language, R: BorrowedEncode<L> + Niche, E: ReprC, N: rust_spec::layout::LayoutKind>
        BorrowedEncode<L> for Result<R, E>
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
        fn soft_encode<'itm>(
            self,
            store: &'itm mut Self::Store,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = crate::option::encode_optional_payload!(
                self, Ok, BorrowedEncode::<L>::soft_encode, store, owner
            );
            Native(AbiResultOk(encoded.unwrap_or(R::NICHE), PhantomData))
        }
    }
    unsafe impl<L: Language, R: ReprC, E: BorrowedEncode<L> + Niche, N: rust_spec::layout::LayoutKind>
        BorrowedEncode<L> for Result<R, E>
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
        fn soft_encode<'itm>(
            self,
            store: &'itm mut Self::Store,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            let encoded = crate::option::encode_optional_payload!(
                self, Err, BorrowedEncode::<L>::soft_encode, store, owner
            );
            Native(AbiResultErr(encoded.unwrap_or(E::NICHE), PhantomData))
        }
    }

    #[cfg(all(feature = "alloc", feature = "repr-rust"))]
    unsafe impl<L: Language> BorrowedEncode<L> for Box<str>
    where
        str: RustSpec<Size = rust_spec::size::MetaSized<rust_spec::size::SliceLike>>,
        Self: ReprC<CType = AbiWideBox<crate::primitive::abi::str>>,
    {
        type Store = ();

        fn soft_encode<'itm>(
            self,
            store: &'itm mut Self::Store,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            // The owned str already contains valid UTF-8.
            unsafe { BorrowedEncodeUnchecked::soft_encode_unchecked(self, store, owner) }
        }
    }
}

disjoint_impls! {
    /// Decodes a carrier whose Rust validity is guaranteed by the caller.
    ///
    /// # Safety
    /// Implementations must preserve ownership, lifetimes and aliasing. Shared
    /// interior-mutable contents must never be inspected. An empty conversion
    /// store must not provide backing storage for the returned value.
    pub unsafe trait BorrowedDecodeUnchecked<'d>: ReprC<CType: Sized> + Sized {
        /// Storage retained for the unchecked conversion.
        type SoftStore: Store + Default;

        /// Decode without validating the source value.
        ///
        /// # Safety
        /// The source must satisfy all Rust validity, access and ownership
        /// requirements, including those of any references in the result.
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::SoftStore,
        ) -> Self;
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, T: BorrowedDecodeUnchecked<'d>, S: SizedKind> BorrowedDecodeUnchecked<'d> for Box<T>
    where
        Self: ReprC<CType = AbiBox<<T as ReprC>::CType>>,
        T: RustSpec<Size = RustSpecSized<S>> + SupportedLayout + ReprC,
        <T as ReprC>::CType: CType + Copy,
    {
        type SoftStore = Box<T::SoftStore>;

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::SoftStore,
        ) -> Self {
            Box::new(unsafe {
                <T as crate::borrow::BorrowedDecodeUnchecked<'d>>::soft_decode_unchecked(
                    source.as_ptr().read(),
                    store,
                )
            })
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, T: BorrowedDecodeUnchecked<'d>> BorrowedDecodeUnchecked<'d> for Box<[T]>
    where
        Self: ReprC<CType = AbiWideBox<<[T] as crate::ReprC>::CType>>,
        <T as ReprC>::CType: CType + Copy,
    {
        type SoftStore = Box<[T::SoftStore]>;

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::SoftStore,
        ) -> Self {
            let fields = unsafe { core::slice::from_raw_parts(source.as_ptr(), source.metadata()) };
            unsafe { decode_borrowed_elements_unchecked::<T, _>(fields, store) }
        }
    }

    #[cfg(all(feature = "alloc", feature = "repr-rust"))]
    unsafe impl<'d, T: crate::ffi::NulTerminatedBuf + ToOwned + ?Sized> BorrowedDecodeUnchecked<'d>
        for Box<T>
    where
        T: RustSpec<Layout = Unspecified, Size = NulTerminated> + ReprC,
        <T as crate::ffi::NulTerminatedBuf>::Header:
            AbiCompatible<CType = <T as ReprC>::CType> + RustSpec<Layout = Specified>,
        Self: ReprC<CType = AbiBox<<T as ReprC>::CType>> + From<<T as ToOwned>::Owned>,
        <T as ReprC>::CType: CType + Copy,
    {
        type SoftStore = ();

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            _: &'itm mut Self::SoftStore,
        ) -> Self {
            let value = unsafe { T::from_raw(source.as_ptr().cast()) };
            value.to_owned().into()
        }
    }

    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, R: BorrowedDecodeUnchecked<'d>> BorrowedDecodeUnchecked<'d> for Option<R>
    where
        Self: ReprC<CType = Native<crate::option::abi::Option<<R as ReprC>::CType>>>,
    {
        type SoftStore = <StableOption<R> as BorrowedDecodeUnchecked<'d>>::SoftStore;

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::SoftStore,
        ) -> Self {
            let source = source.0;

            unsafe {
                <StableOption<R> as BorrowedDecodeUnchecked<'d>>::soft_decode_unchecked(source, store)
            }
            .into()
        }
    }
    unsafe impl<'d, R: BorrowedDecodeUnchecked<'d> + Niche, N: rust_spec::layout::LayoutKind>
        BorrowedDecodeUnchecked<'d> for Option<R>
    where
        R: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<CType = Native<crate::option::abi::OptionSome<<R as ReprC>::CType>>>,
    {
        type SoftStore = Option<<R as BorrowedDecodeUnchecked<'d>>::SoftStore>;

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::SoftStore,
        ) -> Self {
            let source = source.0;

            if R::is_niche(&source.0) {
                *store = None;
                return None;
            }
            let source = source.0;
            Some(unsafe {
                <R as crate::borrow::BorrowedDecodeUnchecked<'d>>::soft_decode_unchecked(
                    source,
                    store.insert(Default::default()),
                )
            })
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, R: BorrowedDecodeUnchecked<'d>, E: BorrowedDecodeUnchecked<'d>>
        BorrowedDecodeUnchecked<'d> for Result<R, E>
    where
        Self:
            ReprC<CType = Native<crate::result::abi::Result<<R as ReprC>::CType, <E as ReprC>::CType>>>,
        <R as ReprC>::CType: Copy,
        <E as ReprC>::CType: Copy,
    {
        type SoftStore = <StableResult<R, E> as BorrowedDecodeUnchecked<'d>>::SoftStore;

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::SoftStore,
        ) -> Self {
            let source = source.0;

            unsafe {
                <StableResult<R, E> as BorrowedDecodeUnchecked<'d>>::soft_decode_unchecked(
                    source, store,
                )
            }
            .into()
        }
    }
    unsafe impl<
        'd,
        R: BorrowedDecodeUnchecked<'d> + Niche,
        E: Default + ReprC,
        N: rust_spec::layout::LayoutKind,
    > BorrowedDecodeUnchecked<'d> for Result<R, E>
    where
        R: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<
            CType = Native<crate::result::abi::ResultOk<<R as ReprC>::CType, <E as ReprC>::CType>>,
        >,
        R: RustSpec<Size = RustSpecSized<Gt<Zero>>>,
        E: RustSpec<Size = RustSpecSized<Zero>>,
        <E as ReprC>::CType: Sized,
    {
        type SoftStore = R::SoftStore;

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::SoftStore,
        ) -> Self {
            let source = source.0;

            if R::is_niche(&source.0) {
                return Err(E::default());
            }
            let source = source.0;
            Ok(unsafe {
                <R as crate::borrow::BorrowedDecodeUnchecked<'d>>::soft_decode_unchecked(source, store)
            })
        }
    }
    unsafe impl<
        'd,
        R: Default + ReprC,
        E: BorrowedDecodeUnchecked<'d> + Niche,
        N: rust_spec::layout::LayoutKind,
    > BorrowedDecodeUnchecked<'d> for Result<R, E>
    where
        E: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<
            CType = Native<crate::result::abi::ResultErr<<R as ReprC>::CType, <E as ReprC>::CType>>,
        >,
        R: RustSpec<Size = RustSpecSized<Zero>>,
        E: RustSpec<Size = RustSpecSized<Gt<Zero>>>,
        <R as ReprC>::CType: Sized,
    {
        type SoftStore = E::SoftStore;

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::SoftStore,
        ) -> Self {
            let source = source.0;

            if E::is_niche(&source.0) {
                return Ok(R::default());
            }
            let source = source.0;
            Err(unsafe {
                <E as crate::borrow::BorrowedDecodeUnchecked<'d>>::soft_decode_unchecked(source, store)
            })
        }
    }

    #[cfg(all(feature = "alloc", feature = "repr-rust"))]
    unsafe impl<'d> BorrowedDecodeUnchecked<'d> for Box<str>
    where
        str: RustSpec<Size = rust_spec::size::MetaSized<rust_spec::size::SliceLike>>,
        Self: ReprC<CType = AbiWideBox<crate::primitive::abi::str>>,
    {
        type SoftStore = ();

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            _: &'itm mut Self::SoftStore,
        ) -> Self {
            let bytes = unsafe { core::slice::from_raw_parts(source.as_ptr(), source.metadata()) };
            Box::from(unsafe { core::str::from_utf8_unchecked(bytes) })
        }
    }
}
disjoint_impls! {
    /// Decodes a borrowed carrier under the source language's validity rules.
    ///
    /// # Safety
    /// Implementations must validate before constructing a Rust value, preserve
    /// the source's ownership, and keep any borrowed backing storage live for
    /// the returned value. Shared interior-mutable contents must never be read.
    pub unsafe trait BorrowedDecode<'d, L: Language = lang::C>:
        ReprC<CType: Sized> + Sized
    {
        type Store: Store + Default;

        /// Perform the conversion from an [`ReprC::CType`] into [`Self`].
        ///
        /// Use [`decode`] when no conversion storage is required.
        ///
        /// # Safety
        ///
        /// See the safety section in [`crate::soft_decode`].
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self>;
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, L: Language, T: BorrowedDecode<'d, L>, S: SizedKind> BorrowedDecode<'d, L>
        for Box<T>
    where
        Self: ReprC<CType = AbiBox<<T as ReprC>::CType>>,
        T: RustSpec<Size = RustSpecSized<S>> + SupportedLayout + ReprC,
        <T as ReprC>::CType: CType + Copy,
    {
        type Store = Box<T::Store>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            // Both layouts need an independently decoded pointee. A Copy carrier can
            // still represent a nested owner such as Box<U>.
            let source = unsafe { source.as_ptr().as_ref()? };
            unsafe { <T as crate::borrow::BorrowedDecode<'d, L>>::soft_decode(*source, store) }
                .map(Box::new)
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, L: Language, T: BorrowedDecode<'d, L>> BorrowedDecode<'d, L> for Box<[T]>
    where
        Self: ReprC<CType = AbiWideBox<<[T] as crate::ReprC>::CType>>,
        <T as ReprC>::CType: CType + Copy,
    {
        type Store = Box<[T::Store]>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let fields = unsafe { borrowed_decode_slice(source.as_ptr(), source.metadata())? };
            unsafe { decode_borrowed_elements::<T, _, L>(fields, store) }
        }
    }
    #[cfg(all(feature = "alloc", feature = "repr-rust"))]
    unsafe impl<'d, L: Language, T: crate::ffi::NulTerminatedBuf + ToOwned + ?Sized>
        BorrowedDecode<'d, L> for Box<T>
    where
        T: RustSpec<Layout = Unspecified, Size = NulTerminated> + ReprC,
        <T as crate::ffi::NulTerminatedBuf>::Header:
            AbiCompatible<CType = <T as ReprC>::CType> + RustSpec<Layout = Specified>,
        Self: ReprC<CType = AbiBox<<T as ReprC>::CType>> + From<<T as ToOwned>::Owned>,
        <T as ReprC>::CType: CType + Copy,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
            core::ptr::NonNull::new(source.as_ptr())?;
            // The caller guarantees a readable, terminated buffer behind a non-null pointer.
            Some(unsafe { <Self as BorrowedDecodeUnchecked<'d>>::soft_decode_unchecked(source, store) })
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, L: Language, R: BorrowedDecode<'d, L>> BorrowedDecode<'d, L> for Option<R>
    where
        Self: ReprC<CType = Native<crate::option::abi::Option<<R as ReprC>::CType>>>,
    {
        type Store = <StableOption<R> as BorrowedDecode<'d, L>>::Store;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = source.0;

            unsafe { <StableOption<R> as BorrowedDecode<'d, L>>::soft_decode(source, store) }
                .map(Into::into)
        }
    }
    unsafe impl<'d, L: Language, R: BorrowedDecode<'d, L> + Niche, N: rust_spec::layout::LayoutKind>
        BorrowedDecode<'d, L> for Option<R>
    where
        R: RustSpec<Niche = rust_spec::niche::WithNiche<N>>,
        Self: ReprC<CType = Native<crate::option::abi::OptionSome<<R as ReprC>::CType>>>,
    {
        type Store = Option<<R as BorrowedDecode<'d, L>>::Store>;

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
            unsafe {
                <R as crate::borrow::BorrowedDecode<'d, L>>::soft_decode(
                    source,
                    store.insert(Default::default()),
                )
            }
            .map(Some)
        }
    }
    #[cfg(feature = "repr-rust")]
    unsafe impl<'d, L: Language, R: BorrowedDecode<'d, L>, E: BorrowedDecode<'d, L>>
        BorrowedDecode<'d, L> for Result<R, E>
    where
        Self:
            ReprC<CType = Native<crate::result::abi::Result<<R as ReprC>::CType, <E as ReprC>::CType>>>,
        <R as ReprC>::CType: Copy,
        <E as ReprC>::CType: Copy,
    {
        type Store = <StableResult<R, E> as BorrowedDecode<'d, L>>::Store;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let source = source.0;

            unsafe { <StableResult<R, E> as BorrowedDecode<'d, L>>::soft_decode(source, store) }
                .map(Into::into)
        }
    }
    unsafe impl<
        'd,
        L: Language,
        R: BorrowedDecode<'d, L> + Niche,
        E: Default + ReprC,
        N: rust_spec::layout::LayoutKind,
    > BorrowedDecode<'d, L> for Result<R, E>
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
            unsafe { <R as crate::borrow::BorrowedDecode<'d, L>>::soft_decode(source, store) }.map(Ok)
        }
    }
    unsafe impl<
        'd,
        L: Language,
        R: Default + ReprC,
        E: BorrowedDecode<'d, L> + Niche,
        N: rust_spec::layout::LayoutKind,
    > BorrowedDecode<'d, L> for Result<R, E>
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
            unsafe { <E as crate::borrow::BorrowedDecode<'d, L>>::soft_decode(source, store) }.map(Err)
        }
    }
    #[cfg(all(feature = "alloc", feature = "repr-rust"))]
    unsafe impl<'d, L: Language> BorrowedDecode<'d, L> for Box<str>
    where
        str: RustSpec<Size = rust_spec::size::MetaSized<rust_spec::size::SliceLike>>,
        Self: ReprC<CType = AbiWideBox<crate::primitive::abi::str>>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, _: &'itm mut Self::Store) -> Option<Self> {
            let bytes = unsafe { borrowed_decode_slice(source.as_ptr(), source.metadata())? };
            Some(Box::from(unsafe {
                crate::lang::checks::decode_utf8::<L>(bytes)
            }?))
        }
    }
}

/// Implements borrowed conversion by delegating to ordinary value conversion.
macro_rules! __impl_borrowed_identity {
    (@decode [$($speculative:tt)*] [$($lifetimes:tt)*] [$lifetime:lifetime, $($rest:tt)*] $ty:ty $(where $($bounds:tt)*)?) => {
        $crate::borrow::__impl_borrowed_identity!(@decode [$($speculative)*] [$($lifetimes)* $lifetime,] [$($rest)*] $ty $(where $($bounds)*)?);
    };
    (@decode [$($speculative:tt)*] [$($lifetimes:tt)*] [$lifetime:lifetime] $ty:ty $(where $($bounds:tt)*)?) => {
        $crate::borrow::__impl_borrowed_identity!(@decode [$($speculative)*] [$($lifetimes)* $lifetime,] [] $ty $(where $($bounds)*)?);
    };
    (@decode [$($speculative:tt)*] [$($lifetimes:tt)*] [$($generics:tt)*] $ty:ty $(where $($bounds:tt)*)?) => {
        unsafe impl<'_co3_d, $($lifetimes)*  $($generics)*> $crate::borrow::BorrowedDecodeUnchecked<'_co3_d> for $ty
        where
            $($speculative)* Self: $crate::convert::DecodeMoveUnchecked<'_co3_d>,
            $($($bounds)*)?
        {
            type SoftStore = <Self as $crate::convert::DecodeMoveUnchecked<'_co3_d>>::SoftStore;

            unsafe fn soft_decode_unchecked<'itm: '_co3_d>(
                source: Self::CType,
                store: &'itm mut Self::SoftStore,
            ) -> Self {
                unsafe { $crate::convert::DecodeMoveUnchecked::<'_co3_d>::soft_decode_unchecked(source, store) }
            }
        }
        unsafe impl<'_co3_d, $($lifetimes)* L: $crate::lang::Language, $($generics)*> $crate::borrow::BorrowedDecode<'_co3_d, L> for $ty
        where
            $($speculative)* Self: $crate::convert::DecodeMove<'_co3_d, L>,
            $($($bounds)*)?
        {
            type Store = <Self as $crate::convert::DecodeMove<'_co3_d, L>>::Store;

            unsafe fn soft_decode<'itm: '_co3_d>(
                source: Self::CType,
                store: &'itm mut Self::Store,
            ) -> core::option::Option<Self> {
                unsafe { $crate::convert::DecodeMove::<'_co3_d, L>::soft_decode(source, store) }
            }

        }
    };

    ([] $ty:ty $(where $($bounds:tt)*)?) => {
        $crate::borrow::__impl_borrowed_identity!(@impl [for<'_co3>] [] $ty $(where $($bounds)*)?);
    };
    ([$($generics:tt)+] $ty:ty $(where $($bounds:tt)*)?) => {
        $crate::borrow::__impl_borrowed_identity!(@impl [] [$($generics)+] $ty $(where $($bounds)*)?);
    };
    (@impl [$($speculative:tt)*] [$($generics:tt)*] $ty:ty $(where $($bounds:tt)*)?) => {
        $crate::borrow::__impl_borrowed_identity!(@encode [$($speculative)*] [] [$($generics)*] $ty $(where $($bounds)*)?);
        $crate::borrow::__impl_borrowed_identity!(@decode [$($speculative)*] [] [$($generics)*] $ty $(where $($bounds)*)?);
    };
    (@encode [$($speculative:tt)*] [$($lifetimes:tt)*] [$lifetime:lifetime, $($rest:tt)*] $ty:ty $(where $($bounds:tt)*)?) => {
        $crate::borrow::__impl_borrowed_identity!(@encode [$($speculative)*] [$($lifetimes)* $lifetime,] [$($rest)*] $ty $(where $($bounds)*)?);
    };
    (@encode [$($speculative:tt)*] [$($lifetimes:tt)*] [$lifetime:lifetime] $ty:ty $(where $($bounds:tt)*)?) => {
        $crate::borrow::__impl_borrowed_identity!(@encode [$($speculative)*] [$($lifetimes)* $lifetime,] [] $ty $(where $($bounds)*)?);
    };
    (@encode [$($speculative:tt)*] [$($lifetimes:tt)*] [$($generics:tt)*] $ty:ty $(where $($bounds:tt)*)?) => {
        unsafe impl<$($lifetimes)* $($generics)*> $crate::borrow::BorrowedEncodeUnchecked for $ty
        where
            $($speculative)* Self: $crate::convert::EncodeMoveUnchecked,
            $($($bounds)*)?
        {
            type Owner = ();
            type SoftStore = <Self as $crate::convert::EncodeMoveUnchecked>::SoftStore;

            unsafe fn soft_encode_unchecked<'itm>(
                self,
                store: &'itm mut Self::SoftStore, _owner: &'itm mut Self::Owner) -> Self::CType
            where
                Self: 'itm,
            {
                unsafe { $crate::convert::EncodeMoveUnchecked::soft_encode_unchecked(self, store) }
            }
        }
        unsafe impl<$($lifetimes)* L: $crate::lang::Language, $($generics)*> $crate::borrow::BorrowedEncode<L> for $ty
        where
            $($speculative)* Self: $crate::convert::EncodeMove<L>,
            $($($bounds)*)?
        {
            type Store = <Self as $crate::convert::EncodeMove<L>>::Store;

            fn soft_encode<'itm>(
                self,
                store: &'itm mut Self::Store,
                _owner: &'itm mut Self::Owner,
            ) -> Self::CType
            where
                Self: 'itm,
            {
                $crate::convert::EncodeMove::<L>::soft_encode(self, store)
            }
        }

    };
}

// Borrowed payloads must be Copy: CType alone does not permit duplicating ownership
// or inspecting shared interior-mutable memory.
#[cfg(feature = "alloc")]
pub(crate) unsafe fn borrowed_decode_slice<'a, C: Copy>(
    data: *const C,
    len: usize,
) -> Option<&'a [C]> {
    if data.is_null() || len.checked_mul(core::mem::size_of::<C>())? > isize::MAX as usize {
        return None;
    }
    // Alignment, initialized storage and lifetime remain the unsafe caller's obligations.
    Some(unsafe { core::slice::from_raw_parts(data, len) })
}

#[cfg(feature = "alloc")]
pub(crate) unsafe fn decode_borrowed_elements<'d, 'itm: 'd, T, B, L: Language>(
    fields: &[T::CType],
    stores: &'itm mut Box<[T::Store]>,
) -> Option<B>
where
    T: BorrowedDecode<'d, L, CType: Copy>,
    B: FromIterator<T>,
{
    zip_stores(fields.iter().copied(), stores)
        .map(|(field, store)| unsafe { <T as BorrowedDecode<'d, L>>::soft_decode(field, store) })
        .collect()
}

#[cfg(feature = "alloc")]
pub(crate) unsafe fn decode_borrowed_elements_unchecked<'d, 'itm: 'd, T, B>(
    fields: &[T::CType],
    stores: &'itm mut Box<[T::SoftStore]>,
) -> B
where
    T: BorrowedDecodeUnchecked<'d, CType: Copy>,
    B: FromIterator<T>,
{
    zip_stores(fields.iter().copied(), stores)
        .map(|(field, store)| unsafe {
            <T as BorrowedDecodeUnchecked<'d>>::soft_decode_unchecked(field, store)
        })
        .collect()
}

// Element owners are allocated before encoding and never moved afterwards.
// The caller retains lowered values separately and releases stores before owners.
#[cfg(feature = "alloc")]
fn encode_borrowed_elements<'a, T: 'a, C, S: Default, O: Default, B: FromIterator<C>>(
    values: impl ExactSizeIterator<Item = T>,
    stores: &'a mut Box<[S]>,
    owners: &'a mut Box<[O]>,
    mut encode: impl FnMut(T, &'a mut S, &'a mut O) -> C,
) -> B {
    let fields = zip_stores(values, stores);
    zip_stores(fields, owners)
        .map(|((value, store), owner)| encode(value, store, owner))
        .collect()
}

/// Retains the original value until reference conversion and synchronization finish.
/// This store must not implement `EmptyStore`: the encoded carrier can point
/// into `value`, which is dropped when the store is dropped.
pub struct BorrowedEncodeStore<T, S> {
    value: Option<T>,
    reference: S,
}
impl<T, S: Default> Default for BorrowedEncodeStore<T, S> {
    fn default() -> Self {
        Self {
            value: None,
            reference: S::default(),
        }
    }
}
impl<T, S: Store> Store for BorrowedEncodeStore<T, S> {
    fn sync(self) -> Option<()> {
        let Self { value, reference } = self;
        let result = reference.sync();
        drop(value);
        result
    }
}
impl<T: ReprC, S: Store + Default> BorrowedEncodeStore<T, S>
where
    <T as ReprC>::CType: Copy,
{
    /// Retain an owner and encode its borrowed payload with the supplied field store.
    #[doc(hidden)]
    pub fn encode_with(
        &mut self,
        value: T,
        encode: impl FnOnce(&mut T, &mut S) -> T::CType,
    ) -> T::CType {
        encode(self.value.insert(value), &mut self.reference)
    }
}

macro_rules! reference_borrowed {
    ($($mutability:tt)?) => {
        unsafe impl<'a, T: ?Sized> BorrowedEncodeUnchecked for &'a $($mutability)? T
        where
            Self: EncodeMoveUnchecked,
        {
            type Owner = ();
            type SoftStore = <Self as EncodeMoveUnchecked>::SoftStore;

            unsafe fn soft_encode_unchecked<'itm>(
                self,
                store: &'itm mut Self::SoftStore, _owner: &'itm mut Self::Owner) -> Self::CType
            where
                Self: 'itm,
            {
                unsafe { EncodeMoveUnchecked::soft_encode_unchecked(self, store) }
            }
        }
        unsafe impl<'d, 'a,  T: ?Sized> BorrowedDecodeUnchecked<'d> for &'a $($mutability)? T
        where
            Self: DecodeMoveUnchecked<'d>,
        {
            type SoftStore = <Self as DecodeMoveUnchecked<'d>>::SoftStore;

            unsafe fn soft_decode_unchecked<'itm: 'd>(
                source: Self::CType,
                store: &'itm mut Self::SoftStore,
            ) -> Self {
                unsafe { DecodeMoveUnchecked::<'d>::soft_decode_unchecked(source, store) }
            }
        }
        unsafe impl<'d, 'a, L: Language, T: ?Sized> BorrowedDecode<'d, L> for &'a $($mutability)? T
        where
            Self: DecodeMove<'d, L>,
        {
            type Store = <Self as DecodeMove<'d, L>>::Store;

            unsafe fn soft_decode<'itm: 'd>(
                source: Self::CType,
                store: &'itm mut Self::Store,
            ) -> core::option::Option<Self> {
                unsafe { DecodeMove::<'d, L>::soft_decode(source, store) }
            }

        }
    };
}
reference_borrowed!();
reference_borrowed!(mut);

unsafe impl<L: Language, T: ?Sized> BorrowedEncode<L> for &mut T
where
    Self: EncodeMove<L>,
{
    type Store = <Self as EncodeMove<L>>::Store;
    fn soft_encode<'itm>(
        self,
        store: &'itm mut Self::Store,
        _owner: &'itm mut Self::Owner,
    ) -> Self::CType
    where
        Self: 'itm,
    {
        EncodeMove::<L>::soft_encode(self, store)
    }
}

// Owned cells can be opened; borrowed shared cells still use the reference codecs above.
macro_rules! cell_borrowed {
    ($cell:ident) => {
        unsafe impl<T: BorrowedEncodeUnchecked> BorrowedEncodeUnchecked for core::cell::$cell<T> {
            type Owner = T::Owner;
            type SoftStore = T::SoftStore;
            unsafe fn soft_encode_unchecked<'itm>(
                self,
                store: &'itm mut Self::SoftStore,
                owner: &'itm mut Self::Owner,
            ) -> Self::CType
            where
                Self: 'itm,
            {
                core::cell::$cell::new(unsafe {
                    T::soft_encode_unchecked(self.into_inner(), store, owner)
                })
            }
        }
        unsafe impl<L: Language, T: BorrowedEncode<L>> BorrowedEncode<L> for core::cell::$cell<T> {
            type Store = T::Store;
            fn soft_encode<'itm>(
                self,
                store: &'itm mut Self::Store,
                owner: &'itm mut Self::Owner,
            ) -> Self::CType
            where
                Self: 'itm,
            {
                core::cell::$cell::new(T::soft_encode(self.into_inner(), store, owner))
            }
        }
        unsafe impl<'d, T: BorrowedDecodeUnchecked<'d>> BorrowedDecodeUnchecked<'d>
            for core::cell::$cell<T>
        {
            type SoftStore = T::SoftStore;

            unsafe fn soft_decode_unchecked<'itm: 'd>(
                source: Self::CType,
                store: &'itm mut Self::SoftStore,
            ) -> Self {
                Self::new(unsafe {
                    <T as crate::borrow::BorrowedDecodeUnchecked<'d>>::soft_decode_unchecked(
                        source.into_inner(),
                        store,
                    )
                })
            }
        }
        unsafe impl<'d, L: Language, T: BorrowedDecode<'d, L>> BorrowedDecode<'d, L>
            for core::cell::$cell<T>
        {
            type Store = T::Store;
            unsafe fn soft_decode<'itm: 'd>(
                source: Self::CType,
                store: &'itm mut Self::Store,
            ) -> core::option::Option<Self> {
                unsafe {
                    <T as crate::borrow::BorrowedDecode<'d, L>>::soft_decode(
                        source.into_inner(),
                        store,
                    )
                }
                .map(Self::new)
            }
        }
    };
}
cell_borrowed!(Cell);
cell_borrowed!(UnsafeCell);

unsafe impl<T: BorrowedEncodeUnchecked, const N: usize> BorrowedEncodeUnchecked for [T; N] {
    type Owner = crate::sync::ArrayStore<T::Owner, N>;
    type SoftStore = crate::sync::ArrayStore<T::SoftStore, N>;
    unsafe fn soft_encode_unchecked<'itm>(
        self,
        store: &'itm mut Self::SoftStore,
        owner: &'itm mut Self::Owner,
    ) -> Self::CType
    where
        Self: 'itm,
    {
        let mut owners = owner.0.iter_mut();
        crate::stored::map_array(self, &mut store.0, |value, store| unsafe {
            T::soft_encode_unchecked(value, store, owners.next().unwrap())
        })
    }
}
unsafe impl<L: Language, T: BorrowedEncode<L>, const N: usize> BorrowedEncode<L> for [T; N] {
    type Store = crate::sync::ArrayStore<T::Store, N>;
    fn soft_encode<'itm>(
        self,
        store: &'itm mut Self::Store,
        owner: &'itm mut Self::Owner,
    ) -> Self::CType
    where
        Self: 'itm,
    {
        let mut owners = owner.0.iter_mut();
        crate::stored::map_array(self, &mut store.0, |value, store| {
            T::soft_encode(value, store, owners.next().unwrap())
        })
    }
}
unsafe impl<'d, T: BorrowedDecodeUnchecked<'d>, const N: usize> BorrowedDecodeUnchecked<'d>
    for [T; N]
{
    type SoftStore = crate::sync::ArrayStore<T::SoftStore, N>;

    unsafe fn soft_decode_unchecked<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::SoftStore,
    ) -> Self {
        crate::stored::map_array(source, &mut store.0, |value, store| unsafe {
            <T as crate::borrow::BorrowedDecodeUnchecked<'d>>::soft_decode_unchecked(value, store)
        })
    }
}
unsafe impl<'d, L: Language, T: BorrowedDecode<'d, L>, const N: usize> BorrowedDecode<'d, L>
    for [T; N]
{
    type Store = crate::sync::ArrayStore<T::Store, N>;
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> core::option::Option<Self> {
        crate::stored::try_map_array(source, &mut store.0, |value, store| unsafe {
            <T as crate::borrow::BorrowedDecode<'d, L>>::soft_decode(value, store)
        })
    }
}

pub(crate) use __impl_borrowed_identity;

#[cfg(all(test, feature = "alloc", feature = "derive"))]
mod tests {
    use alloc::{boxed::Box, vec::Vec};
    use core::{
        cell::Cell,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use crate::{ReprC, Store, lang, marker::Native, rust_spec::RustSpec};

    // Deliberately not Clone: borrowing converts each field on the original type.
    #[derive(RustSpec, ReprC)]
    struct Args<'a> {
        owner: Box<u32>,
        target: &'a mut u32,
    }

    #[test]
    fn strings_borrow_through_their_owned_buffers_without_soft() {
        use alloc::{ffi::CString, string::String};

        use crate::{
            borrow::{BorrowedEncodeUnchecked, decode, decode_unchecked, encode},
            marker::Borrowed,
        };

        for text in ["", "héllo"] {
            let mut owner = Default::default();
            let carrier = unsafe { encode::<_, lang::C>(String::from(text), &mut owner) }.0;
            let bytes = unsafe { core::slice::from_raw_parts(carrier.0.as_ptr(), carrier.0.len()) };
            assert_eq!(bytes, text.as_bytes());
            let decoded = unsafe { decode::<String, lang::C>(Borrowed(carrier)) }.unwrap();
            drop(owner);
            assert_eq!(decoded, text);

            let mut owner = Default::default();
            let mut store: () = Default::default();
            let carrier = unsafe {
                BorrowedEncodeUnchecked::soft_encode_unchecked(
                    String::from(text),
                    &mut store,
                    &mut owner,
                )
            };
            let bytes = unsafe { core::slice::from_raw_parts(carrier.0.as_ptr(), carrier.0.len()) };
            assert_eq!(bytes, text.as_bytes());
            let decoded = unsafe { decode_unchecked::<String>(Borrowed(carrier)) };
            store.sync().unwrap();
            drop(owner);
            assert_eq!(decoded, text);

            let value = CString::new(text).unwrap();
            let original = value.as_ptr();
            let mut owner = Default::default();
            let carrier = unsafe { encode::<_, lang::C>(value, &mut owner) }.0;
            assert_eq!(
                carrier.as_ptr().cast::<core::ffi::c_char>().cast_const(),
                original
            );
            assert_eq!(
                unsafe { core::ffi::CStr::from_ptr(carrier.as_ptr().cast()) }.to_bytes(),
                text.as_bytes()
            );
            drop(owner);

            let value = CString::new(text).unwrap();
            let original = value.as_ptr();
            let mut owner = Default::default();
            let mut store: () = Default::default();
            let carrier = unsafe {
                BorrowedEncodeUnchecked::soft_encode_unchecked(value, &mut store, &mut owner)
            };
            assert_eq!(
                carrier.as_ptr().cast::<core::ffi::c_char>().cast_const(),
                original
            );
            assert_eq!(
                unsafe { core::ffi::CStr::from_ptr(carrier.as_ptr().cast()) }.to_bytes(),
                text.as_bytes()
            );
            store.sync().unwrap();
            drop(owner);
        }
    }

    #[test]
    fn aggregate_borrows_fields_without_cloning_the_aggregate() {
        let mut target = 9;
        let owner = Box::new(7);
        let original = core::ptr::from_ref(&*owner);
        let mut encode_owner = Default::default();
        let mut encode_store = Default::default();
        let encoded = crate::borrow::BorrowedEncode::<lang::C>::soft_encode(
            Args {
                owner,
                target: &mut target,
            },
            &mut encode_store,
            &mut encode_owner,
        );
        let carrier = encoded;
        assert_eq!(carrier.owner.as_ptr().cast_const(), original);
        assert_eq!(unsafe { *carrier.target.as_ptr() }, 9);
        let mut decode_store = Default::default();
        let decoded = unsafe {
            <Args<'_> as crate::borrow::BorrowedDecode>::soft_decode(carrier, &mut decode_store)
        }
        .unwrap();
        assert_eq!(*decoded.owner, 7);
        assert_ne!(core::ptr::from_ref(&*decoded.owner), original);
        assert_eq!(*decoded.target, 9);
        drop(decoded);
        decode_store.sync().unwrap();
        encode_store.sync().unwrap();
        drop(encode_owner);
    }

    // The validator deliberately checks that it receives the original Box field.
    #[expect(clippy::borrowed_box)]
    fn nonzero_box(value: &Box<u32>) -> bool {
        **value != 0
    }

    #[derive(RustSpec, ReprC)]
    #[rust_spec(non_robust)]
    #[repr_c(is_valid = |value| nonzero_box(value))]
    struct NonZeroBox(Box<u32>);

    #[test]
    fn borrowed_validation_receives_original_fields() {
        let mut encode_owner = Default::default();
        let mut encode_store = Default::default();
        let carrier = crate::borrow::BorrowedEncode::<lang::C>::soft_encode(
            NonZeroBox(Box::new(5)),
            &mut encode_store,
            &mut encode_owner,
        );
        let mut decode_store = Default::default();
        let decoded = unsafe {
            <NonZeroBox as crate::borrow::BorrowedDecode>::soft_decode(carrier, &mut decode_store)
        }
        .unwrap();
        assert_eq!(*decoded.0, 5);
        decode_store.sync().unwrap();
        encode_store.sync().unwrap();
        drop(encode_owner);

        let owner = Box::new(0_u32);
        let carrier = unsafe {
            <NonZeroBox as crate::ReprC>::CType::from_fields(crate::boxed::abi::Box::from_box(
                owner,
            ))
        };
        let pointer = carrier.0;
        let mut decode_store = Default::default();
        assert!(
            unsafe {
                <NonZeroBox as crate::borrow::BorrowedDecode>::soft_decode(
                    carrier,
                    &mut decode_store,
                )
            }
            .is_none()
        );
        decode_store.sync().unwrap();
        // Borrowed decoding did not consume the foreign allocation, even on rejection.
        let owner = unsafe { pointer.into_box() }.unwrap();
        assert_eq!(*owner, 0);
    }

    #[derive(RustSpec, ReprC)]
    enum Choice<'a> {
        Owner(Box<u32>),
        Reference(&'a mut u32),
    }

    #[test]
    fn enum_preserves_the_active_borrowed_payload() {
        let mut encode_owner = Default::default();
        let mut encode_store = Default::default();
        let encoded = crate::borrow::BorrowedEncode::<lang::C>::soft_encode(
            Choice::Owner(Box::new(12)),
            &mut encode_store,
            &mut encode_owner,
        );
        let mut decode_store = Default::default();
        let decoded = unsafe {
            <Choice<'_> as crate::borrow::BorrowedDecode>::soft_decode(encoded, &mut decode_store)
        }
        .unwrap();
        let Choice::Owner(value) = decoded else {
            panic!("wrong variant")
        };
        assert_eq!(*value, 12);
        drop(value);
        decode_store.sync().unwrap();
        encode_store.sync().unwrap();
        drop(encode_owner);
    }

    #[test]
    fn borrowed_vectors_decode_directly_without_soft() {
        use crate::{borrow::decode, marker::Borrowed};
        let foreign = unsafe { crate::encode::<_, lang::C>(alloc::vec![3_u8, 5, 8]) };
        let decoded = unsafe { decode::<alloc::vec::Vec<u8>, lang::C>(Borrowed(foreign)) }.unwrap();
        assert_eq!(decoded, [3, 5, 8]);
        assert_ne!(decoded.as_ptr(), foreign.as_ptr().cast_const());
        drop(decoded);
        assert_eq!(
            unsafe { crate::decode::<alloc::vec::Vec<u8>, lang::C>(foreign) }.unwrap(),
            [3, 5, 8]
        );

        let foreign = unsafe { crate::encode::<_, lang::C>(alloc::vec![(3_u8,), (5,)]) };
        let decoded =
            unsafe { decode::<alloc::vec::Vec<(u8,)>, lang::C>(Borrowed(foreign)) }.unwrap();
        assert_eq!(decoded, [(3,), (5,)]);
        drop(decoded);
        assert_eq!(
            unsafe { crate::decode::<alloc::vec::Vec<(u8,)>, lang::C>(foreign) }.unwrap(),
            [(3,), (5,)]
        );
    }

    #[test]
    fn borrowed_container_decode_rejects_bad_headers_and_values() {
        use crate::{borrow::decode, marker::Borrowed, vec::abi::Vec as AbiVec};
        assert!(
            unsafe { decode::<alloc::vec::Vec<u8>, lang::C>(Borrowed(Native(AbiVec::null()))) }
                .is_none()
        );
        assert!(
            unsafe { decode::<Box<u8>, lang::C>(Borrowed(crate::boxed::abi::Box::null())) }
                .is_none()
        );
        let mut foreign = alloc::vec![
            crate::primitive::abi::bool(1),
            crate::primitive::abi::bool(2)
        ];
        let header = unsafe {
            AbiVec::<<bool as crate::ReprC>::CType>::from_raw_parts(
                foreign.as_mut_ptr(),
                foreign.len(),
                foreign.capacity(),
            )
        };
        assert!(
            unsafe { decode::<alloc::vec::Vec<bool>, lang::C>(Borrowed(Native(header))) }.is_none()
        );
        assert_eq!(foreign[1].0, 2);
        let invalid = unsafe {
            AbiVec::<<bool as crate::ReprC>::CType>::from_raw_parts(
                foreign.as_mut_ptr(),
                foreign.capacity() + 1,
                foreign.capacity(),
            )
        };
        assert!(
            unsafe { decode::<alloc::vec::Vec<bool>, lang::C>(Borrowed(Native(invalid))) }
                .is_none()
        );
    }

    #[test]
    fn borrowed_container_decode_does_not_read_shared_cells() {
        use crate::borrow::BorrowedDecode;
        static_assertions::assert_not_impl_any!(alloc::vec::Vec<Cell<u8>>: BorrowedDecode<'static>);
        static_assertions::assert_not_impl_any!(Box<Cell<u8>>: BorrowedDecode<'static>);
        static_assertions::assert_not_impl_any!(Box<[Cell<u8>]>: BorrowedDecode<'static>);
    }

    #[test]
    fn borrowed_unchecked_containers_reconstruct_owned_values() {
        let foreign = unsafe { crate::encode::<_, lang::C>(alloc::vec![(7_u8,)]) };
        let mut store = Default::default();
        let decoded = unsafe {
            <alloc::vec::Vec<(u8,)> as crate::borrow::BorrowedDecodeUnchecked<'_>>::soft_decode_unchecked(
                foreign, &mut store,
            )
        };
        assert_eq!(decoded, [(7,)]);
        drop(decoded);
        store.sync().unwrap();
        drop(unsafe { crate::decode::<alloc::vec::Vec<(u8,)>, lang::C>(foreign) }.unwrap());
    }

    #[test]
    fn unstable_custom_drop_supports_move_but_not_reference_decoding() {
        static DROPS: AtomicUsize = AtomicUsize::new(0);

        #[derive(Clone, RustSpec, ReprC)]
        #[rust_spec(custom_drop)]
        #[repr(transparent)]
        struct StringOwner(alloc::string::String);

        impl Drop for StringOwner {
            fn drop(&mut self) {
                DROPS.fetch_add(1, Ordering::SeqCst);
            }
        }

        #[derive(Clone, RustSpec, ReprC)]
        #[rust_spec(custom_drop)]
        struct Aggregate(Vec<(u8,)>, u8);
        impl Drop for Aggregate {
            fn drop(&mut self) {}
        }

        #[derive(Clone, RustSpec, ReprC)]
        #[rust_spec(custom_drop)]
        enum Choice {
            Owner(Vec<(u8,)>),
            Empty,
        }
        impl Drop for Choice {
            fn drop(&mut self) {}
        }

        #[derive(Clone, RustSpec, ReprC)]
        #[rust_spec(custom_drop)]
        #[repr(transparent)]
        struct GenericOwner<T>(T);
        impl<T> Drop for GenericOwner<T> {
            fn drop(&mut self) {}
        }

        #[derive(RustSpec, ReprC)]
        #[rust_spec(custom_drop)]
        #[repr(transparent)]
        struct NonCloneOwner(u8);
        impl Drop for NonCloneOwner {
            fn drop(&mut self) {}
        }

        static_assertions::assert_not_impl_any!(GenericOwner<u8>: crate::borrow::BorrowedDecode<'static>, crate::borrow::BorrowedEncode, crate::borrow::BorrowedEncodeUnchecked);
        static_assertions::assert_not_impl_any!(GenericOwner<bool>:
            crate::convert::SoftDecodeRef<'static>, crate::convert::SoftDecodeMut<'static>);
        static_assertions::assert_not_impl_any!(GenericOwner<u8>:
            crate::convert::SoftDecodeRef<'static>, crate::convert::SoftDecodeMut<'static>);
        static_assertions::assert_not_impl_any!(GenericOwner<bool>: crate::borrow::BorrowedDecode<'static>, crate::borrow::BorrowedEncode, crate::borrow::BorrowedEncodeUnchecked);
        static_assertions::assert_impl_all!(&'static GenericOwner<bool>: crate::convert::DecodeMove<'static>);
        static_assertions::assert_impl_all!(&'static mut GenericOwner<u8>: crate::convert::DecodeMove<'static>);

        static_assertions::assert_not_impl_any!(GenericOwner<alloc::string::String>:
            crate::convert::SoftDecodeRef<'static>, crate::convert::SoftDecodeMut<'static>,
            crate::borrow::BorrowedDecode<'static>);
        static_assertions::assert_not_impl_any!(NonCloneOwner: crate::borrow::BorrowedDecode<'static>);

        static_assertions::assert_not_impl_any!(StringOwner:
            crate::convert::SoftDecodeRef<'static>, crate::convert::SoftDecodeMut<'static>,
            crate::borrow::BorrowedDecode<'static>);
        static_assertions::assert_not_impl_any!(Aggregate:
            crate::convert::SoftDecodeRef<'static>, crate::convert::SoftDecodeMut<'static>,
            crate::borrow::BorrowedDecode<'static>);
        static_assertions::assert_not_impl_any!(Choice:
            crate::convert::SoftDecodeRef<'static>, crate::convert::SoftDecodeMut<'static>,
            crate::borrow::BorrowedDecode<'static>);
        static_assertions::assert_not_impl_any!(&'static StringOwner: crate::convert::DecodeMove<'static>);
        static_assertions::assert_not_impl_any!(&'static mut StringOwner: crate::convert::DecodeMove<'static>);
        static_assertions::assert_impl_all!(StringOwner: crate::convert::DecodeMove<'static>);
        static_assertions::assert_not_impl_any!(StringOwner: crate::borrow::BorrowedEncode, crate::borrow::BorrowedEncodeUnchecked);
        static_assertions::assert_impl_all!(Aggregate: crate::convert::DecodeMove<'static>);
        static_assertions::assert_impl_all!(Choice: crate::convert::DecodeMove<'static>);

        let value = alloc::string::String::from("transferred");
        let original = value.as_ptr();
        let source = unsafe { crate::convert::encode_move::<_, lang::C>(StringOwner(value)) };
        assert_eq!(DROPS.load(Ordering::SeqCst), 0);
        let decoded =
            unsafe { crate::convert::decode_move::<StringOwner, lang::C>(source) }.unwrap();
        assert_eq!(decoded.0, "transferred");
        assert_eq!(decoded.0.as_ptr(), original);
        drop(decoded);
        assert_eq!(DROPS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn arrays_and_owned_cells_retain_borrowed_owners() {
        let mut encode_owner = Default::default();
        let mut encode_store = Default::default();
        let encoded = crate::borrow::BorrowedEncode::<lang::C>::soft_encode(
            [Cell::new(Box::new(3_u32))],
            &mut encode_store,
            &mut encode_owner,
        );
        let mut decode_store = Default::default();
        let decoded = unsafe {
            <[Cell<Box<u32>>; 1] as crate::borrow::BorrowedDecode>::soft_decode(
                encoded,
                &mut decode_store,
            )
        }
        .unwrap();
        let [cell] = decoded;
        assert_eq!(*cell.into_inner(), 3);
        decode_store.sync().unwrap();
        encode_store.sync().unwrap();
        drop(encode_owner);
    }

    #[test]
    fn owned_containers_with_empty_element_stores_encode_without_soft() {
        use crate::borrow::BorrowedEncode;

        fn encode<T: BorrowedEncode<Store: crate::convert::EmptyStore>>(value: T) {
            let mut owner = Default::default();
            let _ = unsafe { crate::borrow::encode::<_, lang::C>(value, &mut owner) };
        }

        encode::<_>(Box::new(7_u8));
        encode::<_>(Box::new((7_u8,)));
        encode::<_>(alloc::vec![7_u8]);
        encode::<_>(alloc::vec![(7_u8,)]);
        encode::<_>(alloc::vec![true, false]);
        encode::<_>(crate::vec::Vec::from(alloc::vec![(7_u8,)]));
        encode::<_>(alloc::vec![7_u8].into_boxed_slice());
        encode::<_>(alloc::vec![(7_u8,)].into_boxed_slice());
        encode::<_>(alloc::string::String::from("hello").into_boxed_str());
        encode::<_>(
            alloc::ffi::CString::new("hello")
                .unwrap()
                .into_boxed_c_str(),
        );
    }

    #[test]
    fn niche_result_reuse_clears_borrowed_store_and_owner() {
        use crate::borrow::BorrowedEncode;

        let mut owner = Default::default();
        let mut store = Default::default();
        BorrowedEncode::<lang::C>::soft_encode(
            Err::<(), _>(Box::new(7_u32)),
            &mut store,
            &mut owner,
        );
        assert!(store.is_some());
        assert!(owner.is_some());

        BorrowedEncode::<lang::C>::soft_encode(Ok::<(), Box<u32>>(()), &mut store, &mut owner);
        assert!(store.is_none());
        assert!(owner.is_none());
    }

    #[test]
    fn robust_boxes_borrow_the_original_allocation_with_a_unit_store() {
        use crate::borrow::BorrowedEncode;

        let value = Box::new(7_u32);
        let original = core::ptr::from_ref(&*value);
        let mut owner = Default::default();
        let carrier = BorrowedEncode::<lang::C>::soft_encode(value, &mut (), &mut owner);
        assert_eq!(carrier.as_ptr().cast_const(), original);
        assert_eq!(unsafe { *carrier.as_ptr() }, 7);

        for values in [alloc::vec![], alloc::vec![3_u32, 5]] {
            let values = values.into_boxed_slice();
            let original = values.as_ptr();
            let len = values.len();
            let mut owner = Default::default();
            let carrier = BorrowedEncode::<lang::C>::soft_encode(values, &mut (), &mut owner);
            assert_eq!(carrier.as_ptr().cast_const(), original);
            assert_eq!(carrier.metadata(), len);
        }
    }

    #[test]
    fn borrowing_nested_boxes_decodes_independent_allocations() {
        use crate::{
            borrow::{BorrowedEncode, decode, decode_unchecked},
            marker::Borrowed,
        };

        // Option<Box<u32>> has a robust representation and still owns an allocation.
        let inner = Box::new(7_u32);
        let original = core::ptr::from_ref(&*inner);
        let mut owner = Default::default();
        let value = Box::new(Some(inner));
        let carrier = Borrowed(BorrowedEncode::<lang::C>::soft_encode(
            value,
            &mut (),
            &mut owner,
        ));
        let checked = unsafe { decode::<Box<Option<Box<u32>>>, lang::C>(carrier) }.unwrap();
        let unchecked = unsafe { decode_unchecked::<Box<Option<Box<u32>>>>(carrier) };
        let mut checked = (*checked).unwrap();
        let unchecked = (*unchecked).unwrap();
        assert_ne!(core::ptr::from_ref(&*checked), original);
        assert_ne!(core::ptr::from_ref(&*unchecked), original);
        drop(owner);
        *checked = 9;
        assert_eq!(*unchecked, 7);
    }

    #[test]
    fn mutable_bool_containers_require_soft_encoding() {
        use crate::{
            borrow::{BorrowedEncode, BorrowedEncodeUnchecked},
            convert::{EmptyStore, EncodeMove, EncodeMoveUnchecked},
        };
        static_assertions::assert_type_eq_all!(
            <&'static mut bool as BorrowedEncode>::Store,
            <&'static mut bool as EncodeMove>::Store
        );
        static_assertions::assert_type_eq_all!(
            <&'static mut bool as BorrowedEncodeUnchecked>::SoftStore,
            <&'static mut bool as EncodeMoveUnchecked>::SoftStore
        );
        static_assertions::assert_not_impl_any!(
            <Box<&'static mut bool> as BorrowedEncode>::Store: EmptyStore
        );
        static_assertions::assert_not_impl_any!(
            <alloc::vec::Vec<&'static mut bool> as BorrowedEncode>::Store: EmptyStore
        );
        static_assertions::assert_not_impl_any!(
            <Box<[&'static mut bool]> as BorrowedEncode>::Store: EmptyStore
        );
    }

    #[test]
    fn boxed_mutable_bool_keeps_checked_writeback_store() {
        use crate::borrow::BorrowedEncode;

        for byte in [1, 2] {
            let mut value = false;
            let mut owner = Default::default();
            let mut store = Default::default();
            let encoded = BorrowedEncode::<lang::C>::soft_encode(
                Box::new(&mut value),
                &mut store,
                &mut owner,
            );
            unsafe {
                *(*encoded.as_ptr()).as_ptr() = crate::primitive::abi::bool(byte);
            }
            assert_eq!(store.sync(), (byte == 1).then_some(()));
            drop(owner);
            assert_eq!(value, byte == 1);
        }
    }

    #[test]
    fn vector_mutable_bool_keeps_checked_writeback_store() {
        use crate::borrow::BorrowedEncode;

        for byte in [1, 2] {
            let mut value = false;
            let mut owner = Default::default();
            let mut store = Default::default();
            let encoded = BorrowedEncode::<lang::C>::soft_encode(
                alloc::vec![&mut value],
                &mut store,
                &mut owner,
            );
            unsafe {
                *(*encoded.as_ptr()).as_ptr() = crate::primitive::abi::bool(byte);
            }
            assert_eq!(store.sync(), (byte == 1).then_some(()));
            drop(owner);
            assert_eq!(value, byte == 1);
        }
    }

    #[test]
    fn boxed_slice_mutable_bool_keeps_checked_writeback_store() {
        use crate::borrow::BorrowedEncode;

        let mut value = false;
        let mut owner = Default::default();
        let mut store = Default::default();
        let encoded = BorrowedEncode::<lang::C>::soft_encode(
            alloc::vec![&mut value].into_boxed_slice(),
            &mut store,
            &mut owner,
        );
        unsafe {
            *(*encoded.as_ptr()).as_ptr() = crate::primitive::abi::bool(1);
        }
        store.sync().unwrap();
        drop(owner);
        assert!(value);
    }

    #[test]
    fn imports_encode_owned_bytes_and_tuples_without_soft() {
        #[unsafe(export_name = "co3_unit_owned_bytes")]
        extern "C" fn bytes(
            value: crate::marker::Borrowed<<alloc::vec::Vec<u8> as crate::ReprC>::CType>,
        ) -> u8 {
            assert_eq!(value.0.len(), 1);
            unsafe { *value.0.as_ptr() }
        }
        #[unsafe(export_name = "co3_unit_owned_tuples")]
        extern "C" fn tuples(
            value: crate::marker::Borrowed<<alloc::vec::Vec<(u8,)> as crate::ReprC>::CType>,
        ) -> u8 {
            assert_eq!(value.0.len(), 1);
            unsafe { (*value.0.as_ptr()).0.0 }
        }
        #[unsafe(export_name = "co3_unit_owned_box_tuple")]
        extern "C" fn boxed(
            value: crate::marker::Borrowed<<Box<(u8,)> as crate::ReprC>::CType>,
        ) -> u8 {
            unsafe { (*value.0.as_ptr()).0.0 }
        }
        crate::ffi! {
            #![unsafe(extern("C"))]
            #[symbol_name = "co3_unit_owned_bytes"]
            fn imported_bytes(value: alloc::vec::Vec<u8>) -> u8;
            #[symbol_name = "co3_unit_owned_tuples"]
            fn imported_tuples(value: alloc::vec::Vec<(u8,)>) -> u8;
            #[symbol_name = "co3_unit_owned_box_tuple"]
            fn imported_boxed(value: Box<(u8,)>) -> u8;
        }
        assert_eq!(imported_bytes(alloc::vec![7]), 7);
        assert_eq!(imported_tuples(alloc::vec![(11,)]), 11);
        assert_eq!(imported_boxed(Box::new((13,))), 13);
    }

    #[derive(RustSpec, ReprC)]
    struct InteriorMutableField(Cell<u8>);

    #[test]
    fn borrowed_wrapper_keeps_owner_axes() {
        static_assertions::assert_impl_all!(InteriorMutableField:
            RustSpec<Mutability = crate::rust_spec::mutability::WithInterior<crate::rust_spec::layout::Robust>>,
        );
        static_assertions::assert_impl_all!(crate::marker::Borrowed< <InteriorMutableField as crate::ReprC>::CType >:
            RustSpec<
                Layout = rust_spec::Specified,
                Size = <InteriorMutableField as RustSpec>::Size,
                __Alignment = <InteriorMutableField as RustSpec>::__Alignment,
                Trap = <InteriorMutableField as RustSpec>::Trap,
                Niche = <InteriorMutableField as RustSpec>::Niche,
                Drop = crate::rust_spec::drop::NoDrop,
                Mutability = crate::rust_spec::mutability::WithInterior<crate::rust_spec::layout::Robust>,
                __IndirectTrap = <InteriorMutableField as RustSpec>::__IndirectTrap,
            >,
        );
    }

    #[test]
    fn identity_carriers_and_wrappers_implement_borrowed_codecs() {
        use core::{mem::ManuallyDrop, num::NonZero};

        use static_assertions::{assert_impl_all, assert_not_impl_any};

        use super::{BorrowedDecode, BorrowedEncode, BorrowedEncodeUnchecked};
        use crate::reference::abi::SoftRefMut;

        #[derive(RustSpec, ReprC)]
        #[repr(transparent)]
        struct Source<T>(T);
        #[derive(RustSpec, ReprC)]
        #[repr(C)]
        struct SourceZst;
        #[derive(RustSpec, ReprC)]
        #[repr(u8)]
        enum Sparse {
            A(u32) = 1,
            B,
            C = 5,
        }
        #[derive(RustSpec, ReprC)]
        #[repr_c(identity)]
        #[repr(transparent)]
        struct Bytes([u8]);

        macro_rules! borrowed {
            ($($ty:ty),* $(,)?) => { $(
                assert_impl_all!($ty: BorrowedEncode, BorrowedEncodeUnchecked, BorrowedDecode<'static>);
            )* };
        }
        borrowed!(Source<u32>, Source<()>, SourceZst, Sparse, <Sparse as crate::ReprC>::CType,
            Option<NonZero<u8>>, Option<extern "C" fn()>,
            Result<NonZero<u8>, ()>, Result<(), NonZero<u8>>,
            Cell<u8>, ManuallyDrop<u8>, SoftRefMut<u8>);
        assert_not_impl_any!(Bytes: BorrowedEncode);
    }
}
