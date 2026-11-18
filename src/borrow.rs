#[cfg(feature = "alloc")]
pub use alloc::borrow::ToOwned;
#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};
use core::marker::PhantomData;

use disjoint_impls::disjoint_impls;
#[cfg(feature = "alloc")]
use rust_spec::size::NulTerminated;
use rust_spec::{
    Gt, RustSpec,
    layout::LayoutKind,
    mutability::{Exclusive, WithInterior, WithoutInterior},
    niche::{WithNiche, WithoutNiche},
    size::{Sized as RustSpecSized, SizedKind, Zero},
};
#[cfg(feature = "alloc")]
use rust_spec::{Stable, Unstable};

use crate::{
    CFnArg, CFnReturn, CType, ReprC,
    convert::{Decode, DecodeMove, EmptyStore, Encode, EncodeMove, EncodeMoveUnchecked, Store},
    niche::Niche,
    option::COptionSome,
    result::{CResult, CResultErr, CResultOk},
    transmute::{AbiCompatible, AbiCompatibleValidate},
};
#[cfg(feature = "alloc")]
use crate::{
    boxed::CBox,
    vec::CVec,
    wide::{CWideBox, Wide},
};

/// Encode a borrowed carrier without retaining conversion storage.
///
/// Keep the owner alive and unchanged while the carrier is in use.
pub fn encode<T: BorrowedEncode<Store: EmptyStore>>(
    value: T,
    owner: &mut T::Owner,
) -> CBorrowed<T> {
    CBorrowed(T::soft_encode(value, &mut Default::default(), owner))
}

/// Decode a borrowed carrier without retaining conversion storage.
///
/// # Safety
///
/// The source must satisfy the access and validity requirements of
/// [`BorrowedDecode::soft_decode`], including the lifetime and aliasing requirements
/// of any references in the result.
pub unsafe fn decode<'d, T: BorrowedDecode<'d, Store: EmptyStore + 'd>>(
    source: CBorrowed<T>,
) -> Option<T> {
    let mut store = T::Store::default();
    // An empty store cannot back any references in the decoded result.
    let store = unsafe { core::mem::transmute::<&mut T::Store, &'d mut T::Store>(&mut store) };
    unsafe { T::soft_decode(source.0, store) }
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
        /// See safety section in [`crate::soft_encode_unchecked`].
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
        T: RustSpec<Layout = Stable, Size = RustSpecSized<S>> + ReprC<CType: CType + Sized>,
        Self: ReprC<CType = CBox<T>>,
    {
        type Owner = (Option<Self>, Option<Box<T::CType>>, Box<T::Owner>);
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
            unsafe { CBox::from_raw(core::ptr::from_mut(&mut **value).cast()) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<T: BorrowedEncodeUnchecked, S: SizedKind> BorrowedEncodeUnchecked for Box<T>
    where
        T: RustSpec<Layout = Unstable, Size = RustSpecSized<S>> + ReprC<CType: CType + Sized>,
        Self: ReprC<CType = CBox<T>>,
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
            unsafe { CBox::from_raw(core::ptr::from_mut(&mut **value)) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl BorrowedEncodeUnchecked for Box<str> {
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
            let len = <str as Wide>::metadata(ptr);
            let data = core::ptr::NonNull::new(<str as Wide>::as_mut_ptr(ptr))
                .unwrap()
                .cast();
            unsafe { CWideBox::from_raw_parts(data, len) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<T: AbiCompatible + BorrowedEncodeUnchecked> BorrowedEncodeUnchecked for Box<[T]>
    where
        T: RustSpec<Layout = Stable> + ReprC<CType: CType + Sized>,
        [T]: RustSpec<Layout = Stable>,
        Self: ReprC<CType = CWideBox<[T]>>,
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
            unsafe { CWideBox::from_raw_parts(data, values.len()) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<T: BorrowedEncodeUnchecked> BorrowedEncodeUnchecked for Box<[T]>
    where
        T: RustSpec<Layout = Unstable> + ReprC<CType: CType + Sized>,
        [T]: RustSpec<Layout = Unstable>,
        Self: ReprC<CType = CWideBox<[T]>>,
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
            let values = encode_borrowed_elements(
                self.into_iter(),
                store,
                &mut owner.1,
                |value, store, owner| unsafe { T::soft_encode_unchecked(value, store, owner) },
            );
            let values = owner.0.insert(values);
            let data = core::ptr::NonNull::new(values.as_mut_ptr()).unwrap();
            unsafe { CWideBox::from_raw_parts(data, values.len()) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<T: crate::ffi::NulTerminatedBuf + ?Sized> BorrowedEncodeUnchecked for Box<T>
    where
        T: RustSpec<Layout = Unstable, Size = NulTerminated> + ReprC<CType: CType + Sized>,
        <T as crate::ffi::NulTerminatedBuf>::Header:
            AbiCompatible<CType = <T as ReprC>::CType> + RustSpec<Layout = Stable>,
        Self: ReprC<CType = CBox<T>>,
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
            unsafe { CBox::from_raw(T::as_ptr(value).cast_mut().cast()) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<T: AbiCompatible + BorrowedEncodeUnchecked> BorrowedEncodeUnchecked
        for crate::vec::Vec<T>
    where
        T: RustSpec<Layout = Stable> + ReprC<CType: CType + Sized>,
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
            unsafe { CVec::from_raw_parts(value.as_mut_ptr().cast(), value.len(), value.capacity()) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<T: BorrowedEncodeUnchecked> BorrowedEncodeUnchecked for crate::vec::Vec<T>
    where
        T: RustSpec<Layout = Unstable> + ReprC<CType: CType + Sized>,
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
            owner.0 = encode_borrowed_elements(
                self.into_iter(),
                store,
                &mut owner.1,
                |value, store, owner| unsafe { T::soft_encode_unchecked(value, store, owner) },
            );
            unsafe { CVec::from_raw_parts(owner.0.as_mut_ptr(), owner.0.len(), owner.0.capacity()) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<T> BorrowedEncodeUnchecked for Vec<T>
    where
        T: ReprC<CType: CType + Sized>,
        crate::vec::Vec<T>: BorrowedEncodeUnchecked + ReprC<CType = CVec<T, true>>,
    {
        type Owner = <crate::vec::Vec<T> as BorrowedEncodeUnchecked>::Owner;
        type SoftStore = <crate::vec::Vec<T> as BorrowedEncodeUnchecked>::SoftStore;

        unsafe fn soft_encode_unchecked<'itm>(
            self,
            store: &'itm mut Self::SoftStore,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            unsafe {
                <crate::vec::Vec<T> as BorrowedEncodeUnchecked>::soft_encode_unchecked(
                    self.into(),
                    store,
                    owner,
                )
            }
            .with_source()
        }
    }

    unsafe impl<R: BorrowedEncodeUnchecked> BorrowedEncodeUnchecked for Option<R>
    where
        R: RustSpec<Niche = WithoutNiche>,
    {
        type Owner = <crate::option::Option<R> as BorrowedEncodeUnchecked>::Owner;
        type SoftStore = <crate::option::Option<R> as BorrowedEncodeUnchecked>::SoftStore;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(
            self,
            store: &'itm mut Self::SoftStore,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            unsafe {
                BorrowedEncodeUnchecked::soft_encode_unchecked(crate::option::Option::from(self), store, owner)
                    .with_source()
            }
        }
    }
    unsafe impl<R: BorrowedEncodeUnchecked + Niche, N: LayoutKind> BorrowedEncodeUnchecked for Option<R>
    where
        R: RustSpec<Niche = WithNiche<N>>,
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
            match self {
                Some(value) => COptionSome(unsafe {
                    value.soft_encode_unchecked(store.insert(Default::default()), owner.insert(Default::default()))
                }),
                None => {
                    *store = None;
                    *owner = None;
                    COptionSome(R::NICHE)
                }
            }
        }
    }
    unsafe impl<
        R: BorrowedEncodeUnchecked<CType: Copy>,
        E: BorrowedEncodeUnchecked<CType: Copy>,
        K: SizedKind,
    > BorrowedEncodeUnchecked for Result<R, E>
    where
        Self: ReprC<CType = CResult<R, E>>,
        R: RustSpec<Size = RustSpecSized<K>>,
        E: RustSpec<Size = RustSpecSized<K>>,
    {
        type Owner = <crate::result::Result<R, E> as BorrowedEncodeUnchecked>::Owner;
        type SoftStore = <crate::result::Result<R, E> as BorrowedEncodeUnchecked>::SoftStore;

        #[inline(always)]
        unsafe fn soft_encode_unchecked<'itm>(
            self,
            store: &'itm mut Self::SoftStore,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            unsafe {
                BorrowedEncodeUnchecked::soft_encode_unchecked(crate::result::Result::from(self), store, owner)
                    .with_source()
            }
        }
    }
    unsafe impl<R: BorrowedEncodeUnchecked + Niche, E: ReprC, N: LayoutKind> BorrowedEncodeUnchecked
        for Result<R, E>
    where
        Self: ReprC<CType = CResultOk<R, E>>,
        R: RustSpec<Size = RustSpecSized<Gt<Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = RustSpecSized<Zero>>,
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
            match self {
                Ok(ok) => CResultOk(
                    unsafe { ok.soft_encode_unchecked(store.insert(Default::default()), owner.insert(Default::default())) },
                    PhantomData,
                ),
                Err(_) => {
                    *store = None;
                    *owner = None;
                    CResultOk(R::NICHE, PhantomData)
                }
            }
        }
    }
    unsafe impl<R: ReprC, E: BorrowedEncodeUnchecked + Niche, N: LayoutKind> BorrowedEncodeUnchecked
        for Result<R, E>
    where
        Self: ReprC<CType = CResultErr<R, E>>,
        R: RustSpec<Size = RustSpecSized<Zero>>,
        E: RustSpec<Size = RustSpecSized<Gt<Zero>>, Niche = WithNiche<N>>,
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
            match self {
                Ok(_) => {
                    *store = None;
                    *owner = None;
                    CResultErr(E::NICHE, PhantomData)
                }
                Err(err) => CResultErr(
                    unsafe { err.soft_encode_unchecked(store.insert(Default::default()), owner.insert(Default::default())) },
                    PhantomData,
                ),
            }
        }
    }
}
disjoint_impls! {
    /// Checked borrowed encoding.
    ///
    /// # Safety
    /// The carrier must satisfy the checked borrowed-access contract and the unchecked
    /// trait's storage requirements.
    pub unsafe trait BorrowedEncode: BorrowedEncodeUnchecked {
        /// Auxiliary storage used during conversion. If storage is not used, set the type to `()`.
        ///
        /// Use cases include:
        /// - Keeping the result of the conversion of references to stored types
        /// - Storing mutable references that need to be updated in [`Store::sync`]
        ///
        /// Conceptually, serves a role similar to the "context" captured by a closure.
        type Store: Store + Default;

        /// Convert [`Self`] into its checked carrier, [`Self::CType`].
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
    unsafe impl<T: AbiCompatible + BorrowedEncode, S: SizedKind> BorrowedEncode for Box<T>
    where
        T: RustSpec<Layout = Stable, Size = RustSpecSized<S>> + ReprC<CType: CType + Sized>,
        Self: ReprC<CType = CBox<T>>,
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
                &mut owner.2,
            );
            allocation.write(value);
            let value = owner.1.insert(unsafe { allocation.assume_init() });
            unsafe { CBox::from_raw(core::ptr::from_mut(&mut **value)) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<T: BorrowedEncode, S: SizedKind> BorrowedEncode for Box<T>
    where
        T: RustSpec<Layout = Unstable, Size = RustSpecSized<S>> + ReprC<CType: CType + Sized>,
        Self: ReprC<CType = CBox<T>>,
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
            unsafe { CBox::from_raw(core::ptr::from_mut(&mut **value)) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl BorrowedEncode for Box<str> {
        type Store = ();

        fn soft_encode<'itm>(
            self,
            store: &'itm mut Self::Store,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            let _ = store;
            let value = owner.insert(self);
            let ptr = core::ptr::from_mut(&mut **value);
            let len = <str as Wide>::metadata(ptr);
            let data = core::ptr::NonNull::new(<str as Wide>::as_mut_ptr(ptr))
                .unwrap()
                .cast();
            unsafe { CWideBox::from_raw_parts(data, len) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<T: AbiCompatible + BorrowedEncode> BorrowedEncode for Box<[T]>
    where
        T: RustSpec<Layout = Stable> + ReprC<CType: CType + Sized>,
        [T]: RustSpec<Layout = Stable>,
        Self: ReprC<CType = CWideBox<[T]>>,
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
            let values = encode_borrowed_elements(
                self.into_iter(),
                store,
                &mut owner.2,
                |value, store, owner| T::soft_encode(value, store, owner),
            );
            let values = owner.1.insert(values);
            let data = core::ptr::NonNull::new(values.as_mut_ptr()).unwrap();
            unsafe { CWideBox::from_raw_parts(data, values.len()) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<T: BorrowedEncode> BorrowedEncode for Box<[T]>
    where
        T: RustSpec<Layout = Unstable> + ReprC<CType: CType + Sized>,
        [T]: RustSpec<Layout = Unstable>,
        Self: ReprC<CType = CWideBox<[T]>>,
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
            let values = encode_borrowed_elements(
                self.into_iter(),
                store,
                &mut owner.1,
                |value, store, owner| T::soft_encode(value, store, owner),
            );
            let values = owner.0.insert(values);
            let data = core::ptr::NonNull::new(values.as_mut_ptr()).unwrap();
            unsafe { CWideBox::from_raw_parts(data, values.len()) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<T: crate::ffi::NulTerminatedBuf + ?Sized> BorrowedEncode for Box<T>
    where
        T: RustSpec<Layout = Unstable, Size = NulTerminated> + ReprC<CType: CType + Sized>,
        <T as crate::ffi::NulTerminatedBuf>::Header:
            AbiCompatible<CType = <T as ReprC>::CType> + RustSpec<Layout = Stable>,
        Self: ReprC<CType = CBox<T>>,
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
            let _ = store;
            let value = owner.insert(self);
            // The borrowed ABI permits no writes through this carrier.
            unsafe { CBox::from_raw(T::as_ptr(value).cast_mut().cast()) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<T: AbiCompatible + BorrowedEncode> BorrowedEncode for crate::vec::Vec<T>
    where
        T: RustSpec<Layout = Stable> + ReprC<CType: CType + Sized>,
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
            owner.1 = encode_borrowed_elements(
                self.into_iter(),
                store,
                &mut owner.2,
                |value, store, owner| T::soft_encode(value, store, owner),
            );
            unsafe { CVec::from_raw_parts(owner.1.as_mut_ptr(), owner.1.len(), owner.1.capacity()) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<T: BorrowedEncode> BorrowedEncode for crate::vec::Vec<T>
    where
        T: RustSpec<Layout = Unstable> + ReprC<CType: CType + Sized>,
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
            owner.0 = encode_borrowed_elements(
                self.into_iter(),
                store,
                &mut owner.1,
                |value, store, owner| T::soft_encode(value, store, owner),
            );
            unsafe { CVec::from_raw_parts(owner.0.as_mut_ptr(), owner.0.len(), owner.0.capacity()) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<T> BorrowedEncode for Vec<T>
    where
        T: ReprC<CType: CType + Sized>,
        crate::vec::Vec<T>: BorrowedEncode + ReprC<CType = CVec<T, true>>,
    {
        type Store = <crate::vec::Vec<T> as BorrowedEncode>::Store;

        fn soft_encode<'itm>(
            self,
            store: &'itm mut Self::Store,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            <crate::vec::Vec<T> as BorrowedEncode>::soft_encode(self.into(), store, owner).with_source()
        }
    }

    unsafe impl<'a, T: ?Sized> BorrowedEncode for &'a T
    where
        T: RustSpec<Mutability = Exclusive<WithoutInterior>>,
        Self: EncodeMove,
    {
        type Store = <Self as EncodeMove>::Store;

        fn soft_encode<'itm>(
            self,
            store: &'itm mut Self::Store,
            _owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            EncodeMove::soft_encode(self, store)
        }
    }

    unsafe impl<'a, T: ?Sized, I> BorrowedEncode for &'a T
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

    unsafe impl<'a, T: ?Sized, I> BorrowedEncode for &'a T
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

    unsafe impl<R: BorrowedEncode> BorrowedEncode for Option<R>
    where
        R: RustSpec<Niche = WithoutNiche>,
        Self: ReprC<CType = crate::option::COption<R>>,
    {
        type Store = <crate::option::Option<R> as BorrowedEncode>::Store;

        #[inline(always)]
        fn soft_encode<'itm>(self, store: &mut Self::Store, owner: &'itm mut Self::Owner) -> Self::CType
        where
            Self: 'itm,
        {
            crate::option::Option::from(self)
                .soft_encode(store, owner)
                .with_source()
        }
    }
    unsafe impl<R: BorrowedEncode + Niche, N: LayoutKind> BorrowedEncode for Option<R>
    where
        R: RustSpec<Niche = WithNiche<N>>,
        Self: ReprC<CType = COptionSome<R>>,
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
            if let Some(value) = self {
                let encoded = BorrowedEncode::soft_encode(value, store.insert(Default::default()), owner.insert(Default::default()));
                debug_assert!(!R::is_niche(&encoded), "encoding produced NICHE value");
                return COptionSome(encoded);
            }

            *store = None;
                    *owner = None;
            COptionSome(R::NICHE)
        }
    }
    unsafe impl<R: BorrowedEncode<CType: Copy>, E: BorrowedEncode<CType: Copy>, K: SizedKind>
        BorrowedEncode for Result<R, E>
    where
        Self: ReprC<CType = CResult<R, E>>,
        R: RustSpec<Size = RustSpecSized<K>>,
        E: RustSpec<Size = RustSpecSized<K>>,
    {
        type Store = <crate::result::Result<R, E> as BorrowedEncode>::Store;

        #[inline(always)]
        fn soft_encode<'itm>(
            self,
            store: &'itm mut Self::Store,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            crate::result::Result::from(self)
                .soft_encode(store, owner)
                .with_source()
        }
    }
    unsafe impl<R: BorrowedEncode + Niche, E: ReprC, N: LayoutKind> BorrowedEncode for Result<R, E>
    where
        Self: ReprC<CType = CResultOk<R, E>>,
        R: RustSpec<Size = RustSpecSized<Gt<Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = RustSpecSized<Zero>>,
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
            match self {
                Ok(ok) => CResultOk(
                    BorrowedEncode::soft_encode(ok, store.insert(Default::default()), owner.insert(Default::default())),
                    PhantomData,
                ),
                Err(_) => CResultOk(R::NICHE, PhantomData),
            }
        }
    }
    unsafe impl<R: ReprC, E: BorrowedEncode + Niche, N: LayoutKind> BorrowedEncode for Result<R, E>
    where
        Self: ReprC<CType = CResultErr<R, E>>,
        R: RustSpec<Size = RustSpecSized<Zero>>,
        E: RustSpec<Size = RustSpecSized<Gt<Zero>>, Niche = WithNiche<N>>,
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
            match self {
                Ok(_) => CResultErr(E::NICHE, PhantomData),
                Err(err) => CResultErr(
                    BorrowedEncode::soft_encode(err, store.insert(Default::default()), owner.insert(Default::default())),
                    PhantomData,
                ),
            }
        }
    }
}
disjoint_impls! {
    /// Decodes a borrowed carrier into the original Rust type.
    ///
    /// # Safety
    /// Decoding must preserve the source's ownership and reference access requirements.
    /// As with [`DecodeMove`], an empty store must not back references in the result.
    pub unsafe trait BorrowedDecode<'d>: ReprC<CType: Sized> + Sized {
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
            unsafe { Self::soft_decode(source, store) }.expect("unchecked decoding produces a value")
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'d, T: BorrowedDecode<'d>, S: SizedKind> BorrowedDecode<'d> for Box<T>
    where
        T: RustSpec<Size = RustSpecSized<S>> + ReprC<CType: CType + Copy + Sized>,
        Self: ReprC<CType = CBox<T>>,
    {
        type Store = Box<T::Store>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            // Copy only the lowered payload; the foreign allocation remains borrowed.
            let source = unsafe { source.as_ptr().as_ref()? };
            unsafe { T::soft_decode(*source, store) }.map(Box::new)
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            Box::new(unsafe { T::soft_decode_unchecked(source.as_ptr().read(), store) })
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, T: BorrowedDecode<'d>> BorrowedDecode<'d> for Box<[T]>
    where
        T: ReprC<CType: CType + Copy + Sized>,
        Self: ReprC<CType = CWideBox<[T]>>,
    {
        type Store = Box<[T::Store]>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            let fields = unsafe { borrowed_decode_slice(source.as_ptr(), source.metadata())? };
            unsafe { decode_borrowed_elements::<T, _>(fields, store) }
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let fields = unsafe { core::slice::from_raw_parts(source.as_ptr(), source.metadata()) };
            unsafe { decode_borrowed_elements_unchecked::<T, _>(fields, store) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d> BorrowedDecode<'d> for Box<str> {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, _: &'itm mut Self::Store) -> Option<Self> {
            let bytes = unsafe { borrowed_decode_slice(source.as_ptr(), source.metadata())? };
            Some(Box::from(core::str::from_utf8(bytes).ok()?))
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            _: &'itm mut Self::Store,
        ) -> Self {
            let bytes = unsafe { core::slice::from_raw_parts(source.as_ptr(), source.metadata()) };
            Box::from(unsafe { core::str::from_utf8_unchecked(bytes) })
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, T: crate::ffi::NulTerminatedBuf + ToOwned + ?Sized> BorrowedDecode<'d> for Box<T>
    where
        T: RustSpec<Size = NulTerminated> + ReprC<CType: CType + Copy + Sized>,
        <T as crate::ffi::NulTerminatedBuf>::Header: AbiCompatible<CType = <T as ReprC>::CType>,
        Self: ReprC<CType = CBox<T>> + From<<T as ToOwned>::Owned>,
    {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, _: &'itm mut Self::Store) -> Option<Self> {
            let ptr = core::ptr::NonNull::new(source.as_ptr())?;
            let value = unsafe { T::from_raw(ptr.as_ptr().cast()) };
            Some(value.to_owned().into())
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            _: &'itm mut Self::Store,
        ) -> Self {
            let value = unsafe { T::from_raw(source.as_ptr().cast()) };
            value.to_owned().into()
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, T: BorrowedDecode<'d>> BorrowedDecode<'d> for Vec<T>
    where
        T: ReprC<CType: CType + Copy + Sized>,
        Self: ReprC<CType = CVec<T, false>>,
    {
        type Store = Box<[T::Store]>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            if source.len() > source.capacity() {
                return None;
            }
            let fields = unsafe { borrowed_decode_slice(source.as_ptr(), source.len())? };
            unsafe { decode_borrowed_elements::<T, _>(fields, store) }
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let fields = unsafe { core::slice::from_raw_parts(source.as_ptr(), source.len()) };
            unsafe { decode_borrowed_elements_unchecked::<T, _>(fields, store) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d, T: BorrowedDecode<'d>> BorrowedDecode<'d> for crate::vec::Vec<T>
    where
        T: ReprC<CType: CType + Copy + Sized>,
        Self: ReprC<CType = CVec<T, true>>,
    {
        type Store = Box<[T::Store]>;

        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            if source.len() > source.capacity() {
                return None;
            }
            let fields = unsafe { borrowed_decode_slice(source.as_ptr(), source.len())? };
            unsafe { decode_borrowed_elements::<T, _>(fields, store) }
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            let fields = unsafe { core::slice::from_raw_parts(source.as_ptr(), source.len()) };
            unsafe { decode_borrowed_elements_unchecked::<T, _>(fields, store) }
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d> BorrowedDecode<'d> for alloc::string::String {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, _: &'itm mut Self::Store) -> Option<Self> {
            if source.len() > source.capacity() {
                return None;
            }
            let bytes = unsafe { borrowed_decode_slice(source.as_ptr(), source.len())? };
            Some(core::str::from_utf8(bytes).ok()?.to_owned())
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            _: &'itm mut Self::Store,
        ) -> Self {
            let bytes = unsafe { core::slice::from_raw_parts(source.as_ptr(), source.len()) };
            unsafe { core::str::from_utf8_unchecked(bytes) }.to_owned()
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<'d> BorrowedDecode<'d> for alloc::ffi::CString {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: Self::CType, _: &'itm mut Self::Store) -> Option<Self> {
            let ptr = core::ptr::NonNull::new(source.as_ptr())?;
            Some(unsafe { core::ffi::CStr::from_ptr(ptr.as_ptr().cast()) }.to_owned())
        }

        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            _: &'itm mut Self::Store,
        ) -> Self {
            unsafe { core::ffi::CStr::from_ptr(source.as_ptr().cast()) }.to_owned()
        }
    }

    unsafe impl<'d, R: BorrowedDecode<'d>> BorrowedDecode<'d> for Option<R>
    where
        R: RustSpec<Niche = WithoutNiche>,
    {
        type Store = <crate::option::Option<R> as BorrowedDecode<'d>>::Store;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe {
                <crate::option::Option<R> as BorrowedDecode<'d>>::soft_decode(
                    source.with_source(),
                    store,
                )
            }
            .map(Into::into)
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            unsafe {
                <crate::option::Option<R> as BorrowedDecode<'d>>::soft_decode_unchecked(
                    source.with_source(),
                    store,
                )
            }
            .into()
        }
    }
    unsafe impl<'d, R: BorrowedDecode<'d> + Niche, N: LayoutKind> BorrowedDecode<'d> for Option<R>
    where
        R: RustSpec<Niche = WithNiche<N>>,
    {
        type Store = Option<<R as BorrowedDecode<'d>>::Store>;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
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
            if R::is_niche(&source.0) {
                *store = None;
                return None;
            }
            let source = source.0;
            Some(unsafe { R::soft_decode_unchecked(source, store.insert(Default::default())) })
        }
    }
    unsafe impl<
        'd,
        R: BorrowedDecode<'d, CType: Copy>,
        E: BorrowedDecode<'d, CType: Copy>,
        K: SizedKind,
    > BorrowedDecode<'d> for Result<R, E>
    where
        Self: ReprC<CType = CResult<R, E>>,
        R: RustSpec<Size = RustSpecSized<K>>,
        E: RustSpec<Size = RustSpecSized<K>>,
    {
        type Store = <crate::result::Result<R, E> as BorrowedDecode<'d>>::Store;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
            unsafe {
                <crate::result::Result<R, E> as BorrowedDecode<'d>>::soft_decode(
                    source.with_source(),
                    store,
                )
            }
            .map(Into::into)
        }

        #[inline(always)]
        unsafe fn soft_decode_unchecked<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Self {
            unsafe {
                <crate::result::Result<R, E> as BorrowedDecode<'d>>::soft_decode_unchecked(
                    source.with_source(),
                    store,
                )
            }
            .into()
        }
    }
    unsafe impl<'d, R: BorrowedDecode<'d> + Niche, E: Default + ReprC, N: LayoutKind> BorrowedDecode<'d>
        for Result<R, E>
    where
        Self: ReprC<CType = CResultOk<R, E>>,
        R: RustSpec<Size = RustSpecSized<Gt<Zero>>, Niche = WithNiche<N>>,
        E: RustSpec<Size = RustSpecSized<Zero>>,
    {
        type Store = R::Store;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
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
            if R::is_niche(&source.0) {
                return Err(E::default());
            }
            let source = source.0;
            Ok(unsafe { R::soft_decode_unchecked(source, store) })
        }
    }
    unsafe impl<'d, R: Default + ReprC, E: BorrowedDecode<'d> + Niche, N: LayoutKind> BorrowedDecode<'d>
        for Result<R, E>
    where
        Self: ReprC<CType = CResultErr<R, E>>,
        R: RustSpec<Size = RustSpecSized<Zero>>,
        E: RustSpec<Size = RustSpecSized<Gt<Zero>>, Niche = WithNiche<N>>,
    {
        type Store = E::Store;

        #[inline(always)]
        unsafe fn soft_decode<'itm: 'd>(
            source: Self::CType,
            store: &'itm mut Self::Store,
        ) -> Option<Self> {
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
            if E::is_niche(&source.0) {
                return Ok(R::default());
            }
            let source = source.0;
            Err(unsafe { E::soft_decode_unchecked(source, store) })
        }
    }
}

#[cfg(feature = "alloc")]
unsafe impl BorrowedEncodeUnchecked for alloc::string::String {
    type Owner = <Vec<u8> as BorrowedEncodeUnchecked>::Owner;
    type SoftStore = <Vec<u8> as BorrowedEncodeUnchecked>::SoftStore;

    unsafe fn soft_encode_unchecked<'itm>(
        self,
        store: &'itm mut Self::SoftStore,
        owner: &'itm mut Self::Owner,
    ) -> Self::CType
    where
        Self: 'itm,
    {
        let carrier = unsafe {
            <Vec<u8> as BorrowedEncodeUnchecked>::soft_encode_unchecked(
                self.into_bytes(),
                store,
                owner,
            )
        };
        crate::utf8::Utf8String(carrier)
    }
}

#[cfg(feature = "alloc")]
unsafe impl BorrowedEncode for alloc::string::String {
    type Store = <Vec<u8> as BorrowedEncode>::Store;

    fn soft_encode<'itm>(
        self,
        store: &'itm mut Self::Store,
        owner: &'itm mut Self::Owner,
    ) -> Self::CType
    where
        Self: 'itm,
    {
        let carrier = <Vec<u8> as BorrowedEncode>::soft_encode(self.into_bytes(), store, owner);
        crate::utf8::Utf8String(carrier)
    }
}

#[cfg(feature = "alloc")]
unsafe impl BorrowedEncodeUnchecked for alloc::ffi::CString {
    type Owner = <Box<core::ffi::CStr> as BorrowedEncodeUnchecked>::Owner;
    type SoftStore = <Box<core::ffi::CStr> as BorrowedEncodeUnchecked>::SoftStore;

    unsafe fn soft_encode_unchecked<'itm>(
        self,
        store: &'itm mut Self::SoftStore,
        owner: &'itm mut Self::Owner,
    ) -> Self::CType
    where
        Self: 'itm,
    {
        unsafe {
            <Box<core::ffi::CStr> as BorrowedEncodeUnchecked>::soft_encode_unchecked(
                self.into_boxed_c_str(),
                store,
                owner,
            )
        }
    }
}

#[cfg(feature = "alloc")]
unsafe impl BorrowedEncode for alloc::ffi::CString {
    type Store = <Box<core::ffi::CStr> as BorrowedEncode>::Store;

    fn soft_encode<'itm>(
        self,
        store: &'itm mut Self::Store,
        owner: &'itm mut Self::Owner,
    ) -> Self::CType
    where
        Self: 'itm,
    {
        <Box<core::ffi::CStr> as BorrowedEncode>::soft_encode(self.into_boxed_c_str(), store, owner)
    }
}

/// Implements borrowed conversion by delegating to ordinary value conversion.
macro_rules! __impl_borrowed_identity {
    ([] $ty:ty $(where $($bounds:tt)*)?) => {
        $crate::__impl_borrowed_identity!(@impl [for<'_co3>] [] $ty $(where $($bounds)*)?);
    };
    ([$($generics:tt)+] $ty:ty $(where $($bounds:tt)*)?) => {
        $crate::__impl_borrowed_identity!(@impl [] [$($generics)+] $ty $(where $($bounds)*)?);
    };
    (@impl [$($speculative:tt)*] [$($generics:tt)*] $ty:ty $(where $($bounds:tt)*)?) => {
        unsafe impl<$($generics)*> $crate::borrow::BorrowedEncodeUnchecked for $ty
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
        unsafe impl<$($generics)*> $crate::borrow::BorrowedEncode for $ty
        where
            $($speculative)* Self: $crate::convert::EncodeMove,
            $($($bounds)*)?
        {
            type Store = <Self as $crate::convert::EncodeMove>::Store;

            fn soft_encode<'itm>(
                self,
                store: &'itm mut Self::Store,
                _owner: &'itm mut Self::Owner,
            ) -> Self::CType
            where
                Self: 'itm,
            {
                $crate::convert::EncodeMove::soft_encode(self, store)
            }
        }
        unsafe impl<'_co3_d, $($generics)*> $crate::borrow::BorrowedDecode<'_co3_d> for $ty
        where
            $($speculative)* Self: $crate::convert::DecodeMove<'_co3_d>,
            $($($bounds)*)?
        {
            type Store = <Self as $crate::convert::DecodeMove<'_co3_d>>::Store;

            unsafe fn soft_decode<'itm: '_co3_d>(
                source: Self::CType,
                store: &'itm mut Self::Store,
            ) -> core::option::Option<Self> {
                unsafe { $crate::convert::DecodeMove::soft_decode(source, store) }
            }
            unsafe fn soft_decode_unchecked<'itm: '_co3_d>(
                source: Self::CType,
                store: &'itm mut Self::Store,
            ) -> Self {
                unsafe { $crate::convert::DecodeMove::soft_decode_unchecked(source, store) }
            }
        }
    };
}

// Borrowed payloads must be Copy: CType alone does not permit duplicating ownership
// or inspecting shared interior-mutable memory.
#[cfg(feature = "alloc")]
unsafe fn borrowed_decode_slice<'a, C: Copy>(data: *const C, len: usize) -> Option<&'a [C]> {
    if data.is_null() || len.checked_mul(core::mem::size_of::<C>())? > isize::MAX as usize {
        return None;
    }
    // Alignment, initialized storage and lifetime remain the unsafe caller's obligations.
    Some(unsafe { core::slice::from_raw_parts(data, len) })
}

#[cfg(feature = "alloc")]
unsafe fn decode_borrowed_elements<'d, 'itm: 'd, T, B>(
    fields: &[T::CType],
    stores: &'itm mut Box<[T::Store]>,
) -> Option<B>
where
    T: BorrowedDecode<'d, CType: Copy>,
    B: FromIterator<T>,
{
    *stores = core::iter::repeat_with(Default::default)
        .take(fields.len())
        .collect();
    fields
        .iter()
        .zip(stores.iter_mut())
        .map(|(field, store)| unsafe { T::soft_decode(*field, store) })
        .collect()
}

#[cfg(feature = "alloc")]
unsafe fn decode_borrowed_elements_unchecked<'d, 'itm: 'd, T, B>(
    fields: &[T::CType],
    stores: &'itm mut Box<[T::Store]>,
) -> B
where
    T: BorrowedDecode<'d, CType: Copy>,
    B: FromIterator<T>,
{
    *stores = core::iter::repeat_with(Default::default)
        .take(fields.len())
        .collect();
    fields
        .iter()
        .zip(stores.iter_mut())
        .map(|(field, store)| unsafe { T::soft_decode_unchecked(*field, store) })
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
    let len = values.len();
    *stores = core::iter::repeat_with(S::default).take(len).collect();
    *owners = core::iter::repeat_with(O::default).take(len).collect();
    values
        .zip(stores.iter_mut())
        .zip(owners.iter_mut())
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
impl<T: ReprC<CType: Copy>, S: Store + Default> BorrowedEncodeStore<T, S> {
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
        unsafe impl<'d, 'a, T: ?Sized> BorrowedDecode<'d> for &'a $($mutability)? T
        where
            Self: DecodeMove<'d>,
        {
            type Store = <Self as DecodeMove<'d>>::Store;

            unsafe fn soft_decode<'itm: 'd>(
                source: Self::CType,
                store: &'itm mut Self::Store,
            ) -> core::option::Option<Self> {
                unsafe { DecodeMove::soft_decode(source, store) }
            }
            unsafe fn soft_decode_unchecked<'itm: 'd>(
                source: Self::CType,
                store: &'itm mut Self::Store,
            ) -> Self {
                unsafe { DecodeMove::soft_decode_unchecked(source, store) }
            }
        }
    };
}
reference_borrowed!();
reference_borrowed!(mut);

unsafe impl<T: ?Sized> BorrowedEncode for &mut T
where
    Self: EncodeMove,
{
    type Store = <Self as EncodeMove>::Store;
    fn soft_encode<'itm>(
        self,
        store: &'itm mut Self::Store,
        _owner: &'itm mut Self::Owner,
    ) -> Self::CType
    where
        Self: 'itm,
    {
        EncodeMove::soft_encode(self, store)
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
        unsafe impl<T: BorrowedEncode> BorrowedEncode for core::cell::$cell<T> {
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
        unsafe impl<'d, T: BorrowedDecode<'d>> BorrowedDecode<'d> for core::cell::$cell<T> {
            type Store = T::Store;
            unsafe fn soft_decode<'itm: 'd>(
                source: Self::CType,
                store: &'itm mut Self::Store,
            ) -> core::option::Option<Self> {
                unsafe { T::soft_decode(source.into_inner(), store) }.map(Self::new)
            }
            unsafe fn soft_decode_unchecked<'itm: 'd>(
                source: Self::CType,
                store: &'itm mut Self::Store,
            ) -> Self {
                Self::new(unsafe { T::soft_decode_unchecked(source.into_inner(), store) })
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
        crate::elements::map_array(self, &mut store.0, |value, store| unsafe {
            T::soft_encode_unchecked(value, store, owners.next().unwrap())
        })
    }
}
unsafe impl<T: BorrowedEncode, const N: usize> BorrowedEncode for [T; N] {
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
        crate::elements::map_array(self, &mut store.0, |value, store| {
            T::soft_encode(value, store, owners.next().unwrap())
        })
    }
}
unsafe impl<'d, T: BorrowedDecode<'d>, const N: usize> BorrowedDecode<'d> for [T; N] {
    type Store = crate::sync::ArrayStore<T::Store, N>;
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> core::option::Option<Self> {
        crate::elements::try_map_array(source, &mut store.0, |value, store| unsafe {
            T::soft_decode(value, store)
        })
    }
    unsafe fn soft_decode_unchecked<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Self {
        crate::elements::map_array(source, &mut store.0, |value, store| unsafe {
            T::soft_decode_unchecked(value, store)
        })
    }
}

#[cfg(all(test, feature = "alloc", feature = "derive"))]
mod tests {
    use alloc::{boxed::Box, vec::Vec};
    use core::{
        cell::Cell,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use crate::{ReprC, Store, rust_spec::RustSpec};

    // Deliberately not Clone: borrowing converts each field on the original type.
    #[derive(RustSpec, ReprC)]
    struct Args<'a> {
        owner: Box<u32>,
        target: &'a mut u32,
    }

    #[test]
    fn strings_borrow_through_their_owned_buffers_without_soft() {
        use alloc::{ffi::CString, string::String};

        use crate::borrow::{BorrowedEncodeUnchecked, encode};

        for text in ["", "héllo"] {
            let mut owner = Default::default();
            let carrier = encode(String::from(text), &mut owner).0;
            let bytes = unsafe { core::slice::from_raw_parts(carrier.0.as_ptr(), carrier.0.len()) };
            assert_eq!(bytes, text.as_bytes());
            drop(owner);

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
            store.sync().unwrap();
            drop(owner);

            let value = CString::new(text).unwrap();
            let original = value.as_ptr();
            let mut owner = Default::default();
            let carrier = encode(value, &mut owner).0;
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
        let encoded = crate::borrow::BorrowedEncode::soft_encode(
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
    #[allow(clippy::borrowed_box)]
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
        let carrier = crate::borrow::BorrowedEncode::soft_encode(
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
            <NonZeroBox as crate::ReprC>::CType::from_fields(crate::boxed::CBox::from_box(owner))
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
        let encoded = crate::borrow::BorrowedEncode::soft_encode(
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
        use crate::borrow::{CBorrowed, decode};
        let foreign = crate::encode(alloc::vec![3_u8, 5, 8]);
        let decoded = unsafe { decode(CBorrowed::<alloc::vec::Vec<u8>>(foreign)) }.unwrap();
        assert_eq!(decoded, [3, 5, 8]);
        assert_ne!(decoded.as_ptr(), foreign.as_ptr().cast_const());
        drop(decoded);
        assert_eq!(
            unsafe { crate::decode::<alloc::vec::Vec<u8>>(foreign) }.unwrap(),
            [3, 5, 8]
        );

        let foreign = crate::encode(alloc::vec![(3_u8,), (5,)]);
        let decoded = unsafe { decode(CBorrowed::<alloc::vec::Vec<(u8,)>>(foreign)) }.unwrap();
        assert_eq!(decoded, [(3,), (5,)]);
        drop(decoded);
        assert_eq!(
            unsafe { crate::decode::<alloc::vec::Vec<(u8,)>>(foreign) }.unwrap(),
            [(3,), (5,)]
        );
    }

    #[test]
    fn borrowed_container_decode_rejects_bad_headers_and_values() {
        use crate::{
            borrow::{CBorrowed, decode},
            primitives::Bool,
            vec::CVec,
        };
        assert!(unsafe { decode(CBorrowed::<alloc::vec::Vec<u8>>(CVec::null())) }.is_none());
        assert!(unsafe { decode(CBorrowed::<Box<u8>>(crate::boxed::CBox::null())) }.is_none());
        let mut foreign = alloc::vec![Bool(1), Bool(2)];
        let header = unsafe {
            CVec::<bool>::from_raw_parts(foreign.as_mut_ptr(), foreign.len(), foreign.capacity())
        };
        assert!(unsafe { decode(CBorrowed::<alloc::vec::Vec<bool>>(header)) }.is_none());
        assert_eq!(foreign[1].0, 2);
        let invalid = unsafe {
            CVec::<bool>::from_raw_parts(
                foreign.as_mut_ptr(),
                foreign.capacity() + 1,
                foreign.capacity(),
            )
        };
        assert!(unsafe { decode(CBorrowed::<alloc::vec::Vec<bool>>(invalid)) }.is_none());
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
        use crate::borrow::BorrowedDecode;
        let foreign = crate::encode(alloc::vec![(7_u8,)]);
        let mut store = Default::default();
        let decoded =
            unsafe { alloc::vec::Vec::<(u8,)>::soft_decode_unchecked(foreign, &mut store) };
        assert_eq!(decoded, [(7,)]);
        drop(decoded);
        store.sync().unwrap();
        drop(unsafe { crate::decode::<alloc::vec::Vec<(u8,)>>(foreign) }.unwrap());
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
        let source = crate::convert::encode_move(StringOwner(value));
        assert_eq!(DROPS.load(Ordering::SeqCst), 0);
        let decoded = unsafe { crate::convert::decode_move::<StringOwner>(source) }.unwrap();
        assert_eq!(decoded.0, "transferred");
        assert_eq!(decoded.0.as_ptr(), original);
        drop(decoded);
        assert_eq!(DROPS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn arrays_and_owned_cells_retain_borrowed_owners() {
        let mut encode_owner = Default::default();
        let mut encode_store = Default::default();
        let encoded = crate::borrow::BorrowedEncode::soft_encode(
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
            let _ = crate::borrow::encode(value, &mut owner);
        }

        encode(Box::new(7_u8));
        encode(Box::new((7_u8,)));
        encode(alloc::vec![7_u8]);
        encode(alloc::vec![(7_u8,)]);
        encode(alloc::vec![true, false]);
        encode(crate::vec::Vec::from(alloc::vec![(7_u8,)]));
        encode(alloc::vec![7_u8].into_boxed_slice());
        encode(alloc::vec![(7_u8,)].into_boxed_slice());
        encode(alloc::string::String::from("hello").into_boxed_str());
        encode(
            alloc::ffi::CString::new("hello")
                .unwrap()
                .into_boxed_c_str(),
        );
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
            let encoded = BorrowedEncode::soft_encode(Box::new(&mut value), &mut store, &mut owner);
            unsafe {
                *(*encoded.as_ptr()).as_ptr() = crate::primitives::Bool(byte);
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
            let encoded =
                BorrowedEncode::soft_encode(alloc::vec![&mut value], &mut store, &mut owner);
            unsafe {
                *(*encoded.as_ptr()).as_ptr() = crate::primitives::Bool(byte);
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
        let encoded = BorrowedEncode::soft_encode(
            alloc::vec![&mut value].into_boxed_slice(),
            &mut store,
            &mut owner,
        );
        unsafe {
            *(*encoded.as_ptr()).as_ptr() = crate::primitives::Bool(1);
        }
        store.sync().unwrap();
        drop(owner);
        assert!(value);
    }

    #[test]
    fn imports_encode_owned_bytes_and_tuples_without_soft() {
        #[unsafe(export_name = "co3_unit_owned_bytes")]
        extern "C" fn bytes(value: crate::borrow::CBorrowed<alloc::vec::Vec<u8>>) -> u8 {
            assert_eq!(value.0.len(), 1);
            unsafe { *value.0.as_ptr() }
        }
        #[unsafe(export_name = "co3_unit_owned_tuples")]
        extern "C" fn tuples(value: crate::borrow::CBorrowed<alloc::vec::Vec<(u8,)>>) -> u8 {
            assert_eq!(value.0.len(), 1);
            unsafe { (*value.0.as_ptr()).0 }
        }
        #[unsafe(export_name = "co3_unit_owned_box_tuple")]
        extern "C" fn boxed(value: crate::borrow::CBorrowed<Box<(u8,)>>) -> u8 {
            unsafe { (*value.0.as_ptr()).0 }
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
            RustSpec<Mutability = co3::rust_spec::mutability::WithInterior<co3::rust_spec::layout::Robust>>,
        );
        static_assertions::assert_impl_all!(co3::borrow::CBorrowed<InteriorMutableField>:
            RustSpec<
                Layout = rust_spec::Stable,
                Size = <InteriorMutableField as RustSpec>::Size,
                __Alignment = <InteriorMutableField as RustSpec>::__Alignment,
                Trap = <InteriorMutableField as RustSpec>::Trap,
                Niche = <InteriorMutableField as RustSpec>::Niche,
                Drop = co3::rust_spec::drop::NoDrop,
                Mutability = co3::rust_spec::mutability::WithInterior<co3::rust_spec::layout::Robust>,
                __IndirectTrap = <InteriorMutableField as RustSpec>::__IndirectTrap,
            >,
        );
    }

    #[test]
    fn identity_carriers_and_wrappers_implement_borrowed_codecs() {
        use core::{mem::ManuallyDrop, num::NonZero};

        use static_assertions::{assert_impl_all, assert_not_impl_any};

        use super::{BorrowedDecode, BorrowedEncode, BorrowedEncodeUnchecked};
        use crate::reference::CSoftRefMut;

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
        borrowed!(Source<u32>, Source<()>, SourceZst, Sparse, <Sparse as co3::ReprC>::CType,
            Option<NonZero<u8>>, Option<extern "C" fn()>,
            Result<NonZero<u8>, ()>, Result<(), NonZero<u8>>,
            Cell<u8>, ManuallyDrop<u8>, CSoftRefMut<u8, true>);
        assert_not_impl_any!(Bytes: BorrowedEncode);
        assert_not_impl_any!(crate::borrow::CBorrowed<()>: crate::CFnArg, crate::CFnReturn);
    }
}

/// Carrier retaining the original Rust type parameter.
#[repr(transparent)]
#[derive(RustSpec)]
#[doc(hidden)]
pub struct CBorrowed<T: ReprC<CType: Sized>>(pub T::CType);

impl<T: ReprC<CType: Sized>> ReprC for CBorrowed<T> {
    type CType = Self;
}
unsafe impl<T: ReprC<CType: Sized>> CType for CBorrowed<T> {}
unsafe impl<T: ReprC<CType: Sized>> AbiCompatibleValidate for CBorrowed<T> {
    unsafe fn is_valid(_: &Self) -> bool {
        true
    }
}
unsafe impl<T: ReprC<CType: Sized>> AbiCompatible for CBorrowed<T> {}
unsafe impl<T: ReprC<CType: Sized>> CFnArg for CBorrowed<T> where T::CType: CFnArg {}
unsafe impl<T: ReprC<CType: Sized>> CFnReturn for CBorrowed<T>
where
    T::CType: CFnReturn,
    Self: RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
{
}
impl<T: ReprC<CType: Sized>> CBorrowed<T> {
    pub const fn new(value: T::CType) -> Self {
        Self(value)
    }
    pub fn into_inner(self) -> T::CType {
        self.0
    }
}
unsafe impl<T: ReprC<CType: Sized>> EncodeMoveUnchecked for CBorrowed<T> {
    type SoftStore = ();
    unsafe fn soft_encode_unchecked<'a>(self, _: &'a mut ()) -> Self
    where
        Self: 'a,
    {
        self
    }
}
unsafe impl<T: ReprC<CType: Sized>> EncodeMove for CBorrowed<T> {
    type Store = ();
    fn soft_encode<'a>(self, _: &'a mut ()) -> Self
    where
        Self: 'a,
    {
        self
    }
}
unsafe impl<'d, T: ReprC<CType: Sized>> DecodeMove<'d> for CBorrowed<T> {
    type Store = ();
    unsafe fn soft_decode<'a: 'd>(value: Self, _: &'a mut ()) -> Option<Self> {
        Some(value)
    }
}
impl<T: ReprC<CType: Sized>> Encode for CBorrowed<T> {}
impl<T: ReprC<CType: Sized>> crate::convert::EncodeUnchecked for CBorrowed<T> {}
impl<'d, T: ReprC<CType: Sized>> Decode<'d> for CBorrowed<T> {}
crate::__impl_borrowed_identity!([T: ReprC<CType: Sized>] CBorrowed<T>);
impl<T: ReprC<CType: Copy>> Copy for CBorrowed<T> {}
impl<T: ReprC<CType: Copy>> Clone for CBorrowed<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: ReprC<CType: core::fmt::Debug + Sized>> core::fmt::Debug for CBorrowed<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("CBorrowed").field(&self.0).finish()
    }
}
impl<T: ReprC<CType: PartialEq + Sized>> PartialEq for CBorrowed<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<T: ReprC<CType: Eq + Sized>> Eq for CBorrowed<T> {}

pub(crate) use __impl_borrowed_identity;
