//! Referent-specific encoding for mutable-reference paths.
use core::{
    cell::{Cell, UnsafeCell},
    mem::ManuallyDrop,
};

use disjoint_impls::disjoint_impls;
#[cfg(feature = "alloc")]
use rust_spec::{RustSpec, size::SizedKind};

#[cfg(feature = "alloc")]
use crate::stored::{EncodeMove, EncodeMoveUnchecked};
use crate::{ReprC, stored::Store};

disjoint_impls! {
    /// Encodes a mutable reference, retaining converted storage and writeback state.
    ///
    /// # Safety
    /// The returned carrier borrows `self` or the store. Both must remain valid for
    /// later access. Synchronization must validate changes before restoring Rust
    /// values. Encoding must leave `self` valid even if the store is never synced.
    /// If `SoftStore` implements `EmptyStore`, the carrier must not borrow from it.
    pub unsafe trait EncodeMutUnchecked<'a>: ReprC + 'a {
        type SoftStore: Store + Default;

        /// # Safety
        /// See [`crate::soft_encode_unchecked`].
        unsafe fn encode_mut_unchecked<'itm>(
            &'a mut self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a mut Self as ReprC>::CType
        where
            &'a mut Self: ReprC<CType: Sized>,
            'a: 'itm;
    }
    unsafe impl<
        'a,
        T: crate::transmute::AbiCompatibleValidate<CType: crate::CType + Sized>
            + rust_spec::RustSpec<
                Layout = rust_spec::Stable,
                Trap = rust_spec::layout::Robust,
                Drop: rust_spec::drop::DropKind,
            > + 'a,
    > EncodeMutUnchecked<'a> for [T]
    where
        &'a mut [T]: ReprC<CType = crate::slice::CWideRefMut<[<T as ReprC>::CType], true>>
            + crate::stored::EncodeMoveUnchecked<SoftStore = ()>,
    {
        type SoftStore = ();
        unsafe fn encode_mut_unchecked<'itm>(
            &'a mut self,
            store: &'itm mut (),
        ) -> <&'a mut Self as ReprC>::CType
        where
            'a: 'itm,
        {
            let encoded =
                unsafe { crate::stored::EncodeMoveUnchecked::soft_encode_unchecked(self, store) };
            debug_assert!(
                unsafe {
                    <[T] as crate::transmute::AbiCompatibleValidate>::is_valid(
                        core::slice::from_raw_parts(encoded.as_ptr(), encoded.metadata()),
                    )
                },
                "encoding failed AbiCompatibleValidate::is_valid"
            );
            encoded
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, T: ReprC<CType: crate::CType + Copy + Sized> + 'a> EncodeMutUnchecked<'a> for [T]
    where
        T: rust_spec::RustSpec<Trap = rust_spec::layout::NonRobust>,
        &'a mut T: crate::stored::EncodeMoveUnchecked
            + ReprC<CType = crate::reference::CRefMut<<T as ReprC>::CType, true>>,
        &'a mut [T]: ReprC<CType = crate::slice::CWideRefMut<[<T as ReprC>::CType], true>>,
    {
        type SoftStore = crate::sync::RefMutWideEncodeStore<
            'a,
            [T],
            alloc::boxed::Box<[<&'a mut T as crate::stored::EncodeMoveUnchecked>::SoftStore]>,
            alloc::boxed::Box<[crate::reference::CRefMut<<T as ReprC>::CType, true>]>,
        >;

        unsafe fn encode_mut_unchecked<'itm>(
            &'a mut self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a mut Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(
                |stores| {
                    *stores = core::iter::repeat_with(Default::default)
                        .take(self.len())
                        .collect();
                    let mut values = alloc::vec::Vec::with_capacity(self.len());
                    let mut pointers = alloc::vec::Vec::with_capacity(self.len());
                    for (value, field_store) in self.iter_mut().zip(stores.iter_mut()) {
                        let pointer = unsafe { value.soft_encode_unchecked(field_store) };
                        values.push(unsafe { *pointer.as_ptr() });
                        pointers.push(pointer);
                    }
                    (values.into_boxed_slice(), pointers.into_boxed_slice())
                },
                |values, pointers| {
                    for (value, pointer) in values.iter().zip(pointers.iter()) {
                        unsafe { *pointer.as_ptr() = *value };
                    }
                },
            )
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, T: ReprC<CType: crate::CType + Copy + Sized> + 'a> EncodeMutUnchecked<'a> for [T]
    where
        T: rust_spec::RustSpec<Layout = rust_spec::Unstable, Trap = rust_spec::layout::Robust>,
        &'a mut T: crate::stored::EncodeMoveUnchecked
            + ReprC<CType = crate::reference::CRefMut<<T as ReprC>::CType, true>>,
        &'a mut [T]: ReprC<CType = crate::slice::CWideRefMut<[<T as ReprC>::CType], true>>,
    {
        type SoftStore = crate::sync::RefMutWideEncodeStore<
            'a,
            [T],
            alloc::boxed::Box<[<&'a mut T as crate::stored::EncodeMoveUnchecked>::SoftStore]>,
            alloc::boxed::Box<[crate::reference::CRefMut<<T as ReprC>::CType, true>]>,
        >;

        unsafe fn encode_mut_unchecked<'itm>(
            &'a mut self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a mut Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(
                |stores| {
                    *stores = core::iter::repeat_with(Default::default)
                        .take(self.len())
                        .collect();
                    let mut values = alloc::vec::Vec::with_capacity(self.len());
                    let mut pointers = alloc::vec::Vec::with_capacity(self.len());
                    for (value, field_store) in self.iter_mut().zip(stores.iter_mut()) {
                        let pointer = unsafe { value.soft_encode_unchecked(field_store) };
                        values.push(unsafe { *pointer.as_ptr() });
                        pointers.push(pointer);
                    }
                    (values.into_boxed_slice(), pointers.into_boxed_slice())
                },
                |values, pointers| {
                    for (value, pointer) in values.iter().zip(pointers.iter()) {
                        unsafe { *pointer.as_ptr() = *value };
                    }
                },
            )
        }
    }
}

disjoint_impls! {
    /// Checked encoding of a mutable reference with deferred writeback.
    ///
    /// # Safety
    /// The returned carrier must satisfy the checked mutable-reference contract.
    /// It grants borrowed access, without transferring ownership to the caller.
    /// If `Store` implements `EmptyStore`, the carrier must not borrow from it.
    pub unsafe trait EncodeMut<'a>: EncodeMutUnchecked<'a> {
        type Store: Store + Default;

        fn encode_mut<'itm>(
            &'a mut self,
            store: &'itm mut Self::Store,
        ) -> <&'a mut Self as ReprC>::CType
        where
            &'a mut Self: ReprC<CType: Sized>,
            'a: 'itm;
    }
    unsafe impl<
        'a,
        T: crate::transmute::AbiCompatibleValidate<CType: crate::CType + Sized>
            + rust_spec::RustSpec<
                Layout = rust_spec::Stable,
                Trap = rust_spec::layout::Robust,
                Drop: rust_spec::drop::DropKind,
            > + 'a,
    > EncodeMut<'a> for [T]
    where
        &'a mut [T]: ReprC<CType = crate::slice::CWideRefMut<[<T as ReprC>::CType], true>>
            + crate::stored::EncodeMoveUnchecked<SoftStore = ()>
            + crate::stored::EncodeMove<Store = ()>,
    {
        type Store = ();
        fn encode_mut<'itm>(&'a mut self, store: &'itm mut ()) -> <&'a mut Self as ReprC>::CType
        where
            'a: 'itm,
        {
            let encoded = crate::stored::EncodeMove::soft_encode(self, store);
            debug_assert!(
                unsafe {
                    <[T] as crate::transmute::AbiCompatibleValidate>::is_valid(
                        core::slice::from_raw_parts(encoded.as_ptr(), encoded.metadata()),
                    )
                },
                "encoding failed AbiCompatibleValidate::is_valid"
            );
            encoded
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, T: ReprC<CType: crate::CType + Copy + Sized> + 'a> EncodeMut<'a> for [T]
    where
        T: rust_spec::RustSpec<Trap = rust_spec::layout::NonRobust>,
        &'a mut T: crate::stored::EncodeMove
            + ReprC<CType = crate::reference::CRefMut<<T as ReprC>::CType, true>>,
        &'a mut [T]: ReprC<CType = crate::slice::CWideRefMut<[<T as ReprC>::CType], true>>,
    {
        type Store = crate::sync::RefMutWideEncodeStore<
            'a,
            [T],
            alloc::boxed::Box<[<&'a mut T as crate::stored::EncodeMove>::Store]>,
            alloc::boxed::Box<[crate::reference::CRefMut<<T as ReprC>::CType, true>]>,
        >;

        fn encode_mut<'itm>(
            &'a mut self,
            store: &'itm mut Self::Store,
        ) -> <&'a mut Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(
                |stores| {
                    *stores = core::iter::repeat_with(Default::default)
                        .take(self.len())
                        .collect();
                    let mut values = alloc::vec::Vec::with_capacity(self.len());
                    let mut pointers = alloc::vec::Vec::with_capacity(self.len());
                    for (value, field_store) in self.iter_mut().zip(stores.iter_mut()) {
                        let pointer = value.soft_encode(field_store);
                        values.push(unsafe { *pointer.as_ptr() });
                        pointers.push(pointer);
                    }
                    (values.into_boxed_slice(), pointers.into_boxed_slice())
                },
                |values, pointers| {
                    for (value, pointer) in values.iter().zip(pointers.iter()) {
                        unsafe { *pointer.as_ptr() = *value };
                    }
                },
            )
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, T: ReprC<CType: crate::CType + Copy + Sized> + 'a> EncodeMut<'a> for [T]
    where
        T: rust_spec::RustSpec<Layout = rust_spec::Unstable, Trap = rust_spec::layout::Robust>,
        &'a mut T: crate::stored::EncodeMove
            + ReprC<CType = crate::reference::CRefMut<<T as ReprC>::CType, true>>,
        &'a mut [T]: ReprC<CType = crate::slice::CWideRefMut<[<T as ReprC>::CType], true>>,
    {
        type Store = crate::sync::RefMutWideEncodeStore<
            'a,
            [T],
            alloc::boxed::Box<[<&'a mut T as crate::stored::EncodeMove>::Store]>,
            alloc::boxed::Box<[crate::reference::CRefMut<<T as ReprC>::CType, true>]>,
        >;

        fn encode_mut<'itm>(
            &'a mut self,
            store: &'itm mut Self::Store,
        ) -> <&'a mut Self as ReprC>::CType
        where
            'a: 'itm,
        {
            store.encode(
                |stores| {
                    *stores = core::iter::repeat_with(Default::default)
                        .take(self.len())
                        .collect();
                    let mut values = alloc::vec::Vec::with_capacity(self.len());
                    let mut pointers = alloc::vec::Vec::with_capacity(self.len());
                    for (value, field_store) in self.iter_mut().zip(stores.iter_mut()) {
                        let pointer = value.soft_encode(field_store);
                        values.push(unsafe { *pointer.as_ptr() });
                        pointers.push(pointer);
                    }
                    (values.into_boxed_slice(), pointers.into_boxed_slice())
                },
                |values, pointers| {
                    for (value, pointer) in values.iter().zip(pointers.iter()) {
                        unsafe { *pointer.as_ptr() = *value };
                    }
                },
            )
        }
    }
}

// Each implementation owns its temporary value and decodes it exactly once on sync.
// Keeping this per-type avoids a stable-layout blanket impl overlapping downstream impls.
macro_rules! impl_direct_mut {
    ([$($params:tt)*] $ty:ty) => {
        unsafe impl<'_mut, $($params)*> $crate::stored::EncodeMutUnchecked<'_mut> for $ty
        where
            Self: $crate::transmute::AbiCompatibleValidate
                + $crate::rust_spec::RustSpec<Layout = $crate::rust_spec::Stable, Trap = $crate::rust_spec::layout::Robust> + '_mut,
            &'_mut mut Self: $crate::ReprC<CType: Sized>
                + $crate::transmute::AbiCompatibleValidate
                + $crate::stored::EncodeMoveUnchecked<SoftStore = ()>,
        {
            type SoftStore = ();
            unsafe fn encode_mut_unchecked<'itm>(&'_mut mut self, store: &'itm mut ()) -> <&'_mut mut Self as $crate::ReprC>::CType
            where '_mut: 'itm {
                let encoded = unsafe { $crate::stored::EncodeMoveUnchecked::soft_encode_unchecked(self, store) };
                debug_assert!(unsafe { <&'_mut mut Self as $crate::transmute::AbiCompatibleValidate>::is_valid(&encoded) }, "encoding failed AbiCompatibleValidate::is_valid");
                encoded
            }
        }
        unsafe impl<'_mut, $($params)*> $crate::stored::EncodeMut<'_mut> for $ty
        where
            Self: $crate::transmute::AbiCompatibleValidate
                + $crate::rust_spec::RustSpec<Layout = $crate::rust_spec::Stable, Trap = $crate::rust_spec::layout::Robust> + '_mut,
            &'_mut mut Self: $crate::ReprC<CType: Sized>
                + $crate::transmute::AbiCompatibleValidate
                + $crate::stored::EncodeMoveUnchecked<SoftStore = ()>
                + $crate::stored::EncodeMove<Store = ()>,
        {
            type Store = ();
            fn encode_mut<'itm>(&'_mut mut self, store: &'itm mut ()) -> <&'_mut mut Self as $crate::ReprC>::CType
            where '_mut: 'itm {
                let encoded = $crate::stored::EncodeMove::soft_encode(self, store);
                debug_assert!(unsafe { <&'_mut mut Self as $crate::transmute::AbiCompatibleValidate>::is_valid(&encoded) }, "encoding failed AbiCompatibleValidate::is_valid");
                encoded
            }
        }
    };
}

macro_rules! impl_copy_mut {
    ([$($params:tt)*] $ty:ty) => {
        unsafe impl<'_mut, $($params)*> $crate::stored::EncodeMutUnchecked<'_mut> for $ty
        where
            Self: $crate::ReprC<CType: Sized> + $crate::stored::EncodeMoveUnchecked
                + $crate::stored::DecodeMove<'_mut, Store: $crate::stored::EmptyStore + '_mut> + '_mut,
            &'_mut mut Self: $crate::ReprC<CType = $crate::reference::CRefMut<<Self as $crate::ReprC>::CType, true>>,
            Self: Copy,
        {
            type SoftStore = $crate::sync::RefMutSizedEncodeStore<'_mut, Self, <Self as $crate::stored::EncodeMoveUnchecked>::SoftStore>;
            unsafe fn encode_mut_unchecked<'itm>(&'_mut mut self, store: &'itm mut Self::SoftStore) -> <&'_mut mut Self as $crate::ReprC>::CType  where '_mut: 'itm {
                store.encode(self, |original, fields| {
                    let value = *original;
                    unsafe { $crate::stored::EncodeMoveUnchecked::soft_encode_unchecked(value, fields) }
                })
            }
        }
        unsafe impl<'_mut, $($params)*> $crate::stored::EncodeMut<'_mut> for $ty
        where
            Self: $crate::ReprC<CType: Sized> + $crate::stored::EncodeMove
                + $crate::stored::DecodeMove<'_mut, Store: $crate::stored::EmptyStore + '_mut> + '_mut,
            &'_mut mut Self: $crate::ReprC<CType = $crate::reference::CRefMut<<Self as $crate::ReprC>::CType, true>>,
            Self: $crate::stored::EncodeMutUnchecked<'_mut>, Self: Copy,
        {
            type Store = $crate::sync::RefMutSizedEncodeStore<'_mut, Self, <Self as $crate::stored::EncodeMove>::Store>;
            fn encode_mut<'itm>(&'_mut mut self, store: &'itm mut Self::Store) -> <&'_mut mut Self as $crate::ReprC>::CType  where '_mut: 'itm {
                store.encode(self, |original, fields| {
                    let value = *original;
                    $crate::stored::EncodeMove::soft_encode(value, fields)
                })
            }
        }
    };
}

unsafe impl<'a, R: EncodeMutUnchecked<'a> + ?Sized> EncodeMutUnchecked<'a> for ManuallyDrop<R>
where
    Self: ReprC + 'a,
    &'a mut Self: ReprC<CType: Copy + Sized>,
    &'a mut R: ReprC<CType: Copy + Sized>,
{
    type SoftStore = R::SoftStore;
    unsafe fn encode_mut_unchecked<'itm>(
        &'a mut self,
        store: &'itm mut Self::SoftStore,
    ) -> <&'a mut Self as ReprC>::CType
    where
        'a: 'itm,
    {
        let inner = unsafe { R::encode_mut_unchecked(&mut **self, store) };
        const {
            assert!(
                core::mem::size_of::<<&mut Self as ReprC>::CType>()
                    == core::mem::size_of::<<&mut R as ReprC>::CType>()
            );
        }
        // ManuallyDrop preserves the pointee layout and reference metadata.
        unsafe { core::mem::transmute_copy(&inner) }
    }
}

unsafe impl<'a, R: EncodeMut<'a> + ?Sized> EncodeMut<'a> for ManuallyDrop<R>
where
    Self: ReprC + 'a,
    &'a mut Self: ReprC<CType: Copy + Sized>,
    &'a mut R: ReprC<CType: Copy + Sized>,
{
    type Store = R::Store;
    fn encode_mut<'itm>(
        &'a mut self,
        store: &'itm mut Self::Store,
    ) -> <&'a mut Self as ReprC>::CType
    where
        'a: 'itm,
    {
        let inner = R::encode_mut(&mut **self, store);
        const {
            assert!(
                core::mem::size_of::<<&mut Self as ReprC>::CType>()
                    == core::mem::size_of::<<&mut R as ReprC>::CType>()
            );
        }
        unsafe { core::mem::transmute_copy(&inner) }
    }
}

macro_rules! impl_cell_mut {
    ($wrapper:ident, $field:expr) => {
        unsafe impl<'a, R: EncodeMutUnchecked<'a> + ?Sized> EncodeMutUnchecked<'a> for $wrapper<R>
        where
            Self: ReprC + 'a,
            &'a mut Self: ReprC<CType: Copy + Sized>,
            &'a mut R: ReprC<CType: Copy + Sized>,
        {
            type SoftStore = R::SoftStore;
            unsafe fn encode_mut_unchecked<'itm>(
                &'a mut self,
                store: &'itm mut Self::SoftStore,
            ) -> <&'a mut Self as ReprC>::CType
            where
                'a: 'itm,
            {
                let inner = unsafe { R::encode_mut_unchecked($field(self), store) };
                const {
                    assert!(
                        core::mem::size_of::<<&mut Self as ReprC>::CType>()
                            == core::mem::size_of::<<&mut R as ReprC>::CType>()
                    );
                }
                // Both cell wrappers preserve the pointee layout and reference metadata.
                unsafe { core::mem::transmute_copy(&inner) }
            }
        }
        unsafe impl<'a, R: EncodeMut<'a> + ?Sized> EncodeMut<'a> for $wrapper<R>
        where
            Self: ReprC + 'a,
            &'a mut Self: ReprC<CType: Copy + Sized>,
            &'a mut R: ReprC<CType: Copy + Sized>,
        {
            type Store = R::Store;
            fn encode_mut<'itm>(
                &'a mut self,
                store: &'itm mut Self::Store,
            ) -> <&'a mut Self as ReprC>::CType
            where
                'a: 'itm,
            {
                let inner = R::encode_mut($field(self), store);
                const {
                    assert!(
                        core::mem::size_of::<<&mut Self as ReprC>::CType>()
                            == core::mem::size_of::<<&mut R as ReprC>::CType>()
                    );
                }
                unsafe { core::mem::transmute_copy(&inner) }
            }
        }
    };
}
impl_cell_mut!(Cell, Cell::get_mut);
impl_cell_mut!(UnsafeCell, UnsafeCell::get_mut);

impl_copy_mut!(['r, T: ?Sized] &'r T);
impl_copy_mut!([R: Copy] Option<R>);
impl_copy_mut!([T: Copy, E: Copy] Result<T, E>);
impl_copy_mut!([T: ReprC<CType: Copy>, E: ReprC<CType: Copy>] crate::result::CResult<T, E>);
impl_copy_mut!([T: Copy, E: Copy] crate::result::Result<T, E>);
// Arrays may contain exclusive references, so encode each element through its
// mutable reference and retain all element stores until synchronization.
#[cfg(feature = "alloc")]
unsafe impl<'a, T: ReprC<CType: crate::CType + Copy + Sized> + 'a, const N: usize>
    EncodeMutUnchecked<'a> for [T; N]
where
    &'a mut T: EncodeMoveUnchecked + ReprC<CType = crate::reference::CRefMut<T::CType, true>>,
    &'a mut [T; N]: ReprC<CType = crate::reference::CRefMut<[T::CType; N], true>>,
{
    type SoftStore = crate::sync::RefMutWideEncodeStore<
        'a,
        [T],
        alloc::boxed::Box<[<&'a mut T as EncodeMoveUnchecked>::SoftStore]>,
        alloc::boxed::Box<[crate::reference::CRefMut<T::CType, true>]>,
    >;

    unsafe fn encode_mut_unchecked<'itm>(
        &'a mut self,
        store: &'itm mut Self::SoftStore,
    ) -> <&'a mut Self as ReprC>::CType
    where
        'a: 'itm,
    {
        let wide = store.encode(
            |stores| {
                *stores = core::iter::repeat_with(Default::default).take(N).collect();
                let mut values = alloc::vec::Vec::with_capacity(N);
                let mut pointers = alloc::vec::Vec::with_capacity(N);
                for (value, field_store) in self.iter_mut().zip(stores.iter_mut()) {
                    let pointer = unsafe { value.soft_encode_unchecked(field_store) };
                    values.push(unsafe { *pointer.as_ptr() });
                    pointers.push(pointer);
                }
                (values.into_boxed_slice(), pointers.into_boxed_slice())
            },
            |values, pointers| {
                for (value, pointer) in values.iter().zip(pointers.iter()) {
                    unsafe { *pointer.as_ptr() = *value };
                }
            },
        );
        unsafe { crate::reference::CRefMut::from_raw(wide.as_ptr().cast::<[T::CType; N]>()) }
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'a, T: ReprC<CType: crate::CType + Copy + Sized> + 'a, const N: usize> EncodeMut<'a>
    for [T; N]
where
    &'a mut T: EncodeMove + ReprC<CType = crate::reference::CRefMut<T::CType, true>>,
    &'a mut [T; N]: ReprC<CType = crate::reference::CRefMut<[T::CType; N], true>>,
{
    type Store = crate::sync::RefMutWideEncodeStore<
        'a,
        [T],
        alloc::boxed::Box<[<&'a mut T as EncodeMove>::Store]>,
        alloc::boxed::Box<[crate::reference::CRefMut<T::CType, true>]>,
    >;

    fn encode_mut<'itm>(
        &'a mut self,
        store: &'itm mut Self::Store,
    ) -> <&'a mut Self as ReprC>::CType
    where
        'a: 'itm,
    {
        let wide = store.encode(
            |stores| {
                *stores = core::iter::repeat_with(Default::default).take(N).collect();
                let mut values = alloc::vec::Vec::with_capacity(N);
                let mut pointers = alloc::vec::Vec::with_capacity(N);
                for (value, field_store) in self.iter_mut().zip(stores.iter_mut()) {
                    let pointer = value.soft_encode(field_store);
                    values.push(unsafe { *pointer.as_ptr() });
                    pointers.push(pointer);
                }
                (values.into_boxed_slice(), pointers.into_boxed_slice())
            },
            |values, pointers| {
                for (value, pointer) in values.iter().zip(pointers.iter()) {
                    unsafe { *pointer.as_ptr() = *value };
                }
            },
        );
        unsafe { crate::reference::CRefMut::from_raw(wide.as_ptr().cast::<[T::CType; N]>()) }
    }
}

impl_direct_mut!([T: ?Sized] core::marker::PhantomData<T>);
impl_direct_mut!([T] core::mem::MaybeUninit<T>);
impl_direct_mut!([C: crate::CType + ?Sized] crate::reference::CRef<C>);
impl_direct_mut!([C: crate::CType + ?Sized, const EXCLUSIVE: bool] crate::reference::CRefMut<C, EXCLUSIVE>);
impl_direct_mut!([P: crate::wide::Wide<Header: crate::wide::WideHeader<Data: crate::CType>, Metadata: crate::CType> + ?Sized] crate::slice::CWideRef<P>);
impl_direct_mut!([P: crate::wide::Wide<Header: crate::wide::WideHeader<Data: crate::CType>, Metadata: crate::CType> + ?Sized, const EXCLUSIVE: bool] crate::slice::CWideRefMut<P, EXCLUSIVE>);
#[cfg(feature = "alloc")]
impl_direct_mut!([T: ReprC<CType: crate::CType + Sized>] crate::vec::CVec<T>);
#[cfg(feature = "alloc")]
impl_direct_mut!([C: crate::CType] crate::boxed::CBox<C>);
#[cfg(feature = "alloc")]
#[cfg(feature = "alloc")]
impl_direct_mut!([P: crate::wide::Wide<Header: crate::wide::WideHeader<Data: crate::CType>, Metadata: crate::CType> + ?Sized] crate::boxed::CWideBox<P>);
#[cfg(feature = "alloc")]
macro_rules! impl_move_out_mut {
    ([$($params:tt)*] $ty:ty where $($bounds:tt)*) => {
        unsafe impl<'_mut, $($params)*> $crate::stored::EncodeMutUnchecked<'_mut> for $ty
        where
            Self: $crate::ReprC<CType: Sized> + $crate::stored::EncodeMoveUnchecked
                + $crate::stored::DecodeMove<'_mut, Store: $crate::stored::EmptyStore + '_mut> + '_mut,
            &'_mut mut Self: $crate::ReprC<CType = $crate::reference::CRefMut<<Self as $crate::ReprC>::CType, true>>,
            $($bounds)*
        {
            type SoftStore = $crate::sync::RefMutSizedEncodeStore<'_mut, Self, <Self as $crate::stored::EncodeMoveUnchecked>::SoftStore>;
            unsafe fn encode_mut_unchecked<'itm>(&'_mut mut self, store: &'itm mut Self::SoftStore) -> <&'_mut mut Self as $crate::ReprC>::CType  where '_mut: 'itm {
                store.encode(self, |original, fields| {
                    let value = core::mem::take(original);
                    unsafe { $crate::stored::EncodeMoveUnchecked::soft_encode_unchecked(value, fields) }
                })
            }
        }
        unsafe impl<'_mut, $($params)*> $crate::stored::EncodeMut<'_mut> for $ty
        where
            Self: $crate::ReprC<CType: Sized> + $crate::stored::EncodeMove
                + $crate::stored::DecodeMove<'_mut, Store: $crate::stored::EmptyStore + '_mut> + '_mut,
            &'_mut mut Self: $crate::ReprC<CType = $crate::reference::CRefMut<<Self as $crate::ReprC>::CType, true>>,
            Self: $crate::stored::EncodeMutUnchecked<'_mut>, $($bounds)*
        {
            type Store = $crate::sync::RefMutSizedEncodeStore<'_mut, Self, <Self as $crate::stored::EncodeMove>::Store>;
            fn encode_mut<'itm>(&'_mut mut self, store: &'itm mut Self::Store) -> <&'_mut mut Self as $crate::ReprC>::CType  where '_mut: 'itm {
                store.encode(self, |original, fields| {
                    let value = core::mem::take(original);
                    $crate::stored::EncodeMove::soft_encode(value, fields)
                })
            }
        }
    };
}
impl_move_out_mut!([T: ReprC<CType: crate::CType + Sized>] crate::option::COption<T> where Self: Default);
impl_move_out_mut!([T] crate::option::Option<T> where);
// Keep the original vector and its elements alive: decoding an owned carrier
// before synchronizing element stores can restore references into freed storage.
#[cfg(feature = "alloc")]
macro_rules! impl_vec_mut {
    ($vec:path) => {
        unsafe impl<'a, T: ReprC<CType: crate::CType + Copy + Sized> + 'a> EncodeMutUnchecked<'a>
            for $vec
        where
            &'a mut T:
                EncodeMoveUnchecked + ReprC<CType = crate::reference::CRefMut<T::CType, true>>,
            &'a mut Self: ReprC<CType = crate::reference::CRefMut<crate::vec::CVec<T>, true>>,
        {
            type SoftStore = crate::sync::RefMutReferenceEncodeStore<
                crate::vec::CVec<T>,
                crate::sync::RefMutWideEncodeStore<
                    'a,
                    [T],
                    alloc::boxed::Box<[<&'a mut T as EncodeMoveUnchecked>::SoftStore]>,
                    alloc::boxed::Box<[crate::reference::CRefMut<T::CType, true>]>,
                    alloc::vec::Vec<T::CType>,
                >,
            >;

            unsafe fn encode_mut_unchecked<'itm>(
                &'a mut self,
                store: &'itm mut Self::SoftStore,
            ) -> <&'a mut Self as ReprC>::CType
            where
                'a: 'itm,
            {
                let capacity = self.capacity();
                store.value = None;
                store.original = None;
                let wide = store.inner.encode(
                    |stores| {
                        *stores = core::iter::repeat_with(Default::default)
                            .take(self.len())
                            .collect();
                        let mut values = alloc::vec::Vec::with_capacity(capacity);
                        let mut pointers = alloc::vec::Vec::with_capacity(self.len());
                        for (value, field_store) in self.iter_mut().zip(stores.iter_mut()) {
                            let pointer = unsafe { value.soft_encode_unchecked(field_store) };
                            // The carrier is Copy; no owned Rust element is read or moved.
                            values.push(unsafe { *pointer.as_ptr() });
                            pointers.push(pointer);
                        }
                        (values, pointers.into_boxed_slice())
                    },
                    |values, pointers| {
                        for (value, pointer) in values.iter().zip(pointers.iter()) {
                            unsafe { *pointer.as_ptr() = *value };
                        }
                    },
                );
                let header = unsafe {
                    crate::vec::CVec::from_raw_parts(wide.as_ptr(), wide.metadata(), capacity)
                };
                // Header replacement cannot change the original vector's allocation.
                store.original = Some(header);
                crate::reference::CRefMut::from_mut(store.value.insert(header))
            }
        }
        unsafe impl<'a, T: ReprC<CType: crate::CType + Copy + Sized> + 'a> EncodeMut<'a> for $vec
        where
            &'a mut T: EncodeMove + ReprC<CType = crate::reference::CRefMut<T::CType, true>>,
            &'a mut Self: ReprC<CType = crate::reference::CRefMut<crate::vec::CVec<T>, true>>,
        {
            type Store = crate::sync::RefMutReferenceEncodeStore<
                crate::vec::CVec<T>,
                crate::sync::RefMutWideEncodeStore<
                    'a,
                    [T],
                    alloc::boxed::Box<[<&'a mut T as EncodeMove>::Store]>,
                    alloc::boxed::Box<[crate::reference::CRefMut<T::CType, true>]>,
                    alloc::vec::Vec<T::CType>,
                >,
            >;

            fn encode_mut<'itm>(
                &'a mut self,
                store: &'itm mut Self::Store,
            ) -> <&'a mut Self as ReprC>::CType
            where
                'a: 'itm,
            {
                let capacity = self.capacity();
                store.value = None;
                store.original = None;
                let wide = store.inner.encode(
                    |stores| {
                        *stores = core::iter::repeat_with(Default::default)
                            .take(self.len())
                            .collect();
                        let mut values = alloc::vec::Vec::with_capacity(capacity);
                        let mut pointers = alloc::vec::Vec::with_capacity(self.len());
                        for (value, field_store) in self.iter_mut().zip(stores.iter_mut()) {
                            let pointer = value.soft_encode(field_store);
                            // The carrier is Copy; no owned Rust element is read or moved.
                            values.push(unsafe { *pointer.as_ptr() });
                            pointers.push(pointer);
                        }
                        (values, pointers.into_boxed_slice())
                    },
                    |values, pointers| {
                        for (value, pointer) in values.iter().zip(pointers.iter()) {
                            unsafe { *pointer.as_ptr() = *value };
                        }
                    },
                );
                let header = unsafe {
                    crate::vec::CVec::from_raw_parts(wide.as_ptr(), wide.metadata(), capacity)
                };
                // Header replacement cannot change the original vector's allocation.
                store.original = Some(header);
                crate::reference::CRefMut::from_mut(store.value.insert(header))
            }
        }
    };
}

#[cfg(feature = "alloc")]
impl_vec_mut!(alloc::vec::Vec<T>);
#[cfg(feature = "alloc")]
impl_vec_mut!(crate::vec::Vec<T>);
#[cfg(feature = "alloc")]
impl_move_out_mut!([T] alloc::boxed::Box<[T]> where T: crate::ReprC<CType: crate::CType + Sized>);
#[cfg(feature = "alloc")]
impl_move_out_mut!([T: Default] alloc::boxed::Box<T> where T: crate::ReprC<CType: crate::CType + Sized>);
#[cfg(feature = "alloc")]
impl_move_out_mut!([] alloc::string::String where);
#[cfg(feature = "alloc")]
impl_move_out_mut!([] alloc::ffi::CString where);
#[cfg(feature = "alloc")]
impl_move_out_mut!([] alloc::boxed::Box<str> where);
#[cfg(feature = "alloc")]
impl_move_out_mut!([] alloc::boxed::Box<core::ffi::CStr> where);

#[cfg(feature = "alloc")]
unsafe impl<'a> EncodeMutUnchecked<'a> for str {
    type SoftStore = crate::sync::RefMutDstEncodeStore<'a, str, ()>;

    unsafe fn encode_mut_unchecked<'itm>(
        &'a mut self,
        store: &'itm mut Self::SoftStore,
    ) -> <&'a mut Self as ReprC>::CType
    where
        'a: 'itm,
    {
        let carrier = store.encode(self, |original, fields| unsafe {
            alloc::borrow::ToOwned::to_owned(&*original).soft_encode_unchecked(fields)
        });
        unsafe {
            crate::slice::CWideRefMut::from_raw_parts_mut(carrier.as_ptr(), carrier.metadata())
        }
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'a> EncodeMut<'a> for str {
    type Store = crate::sync::RefMutDstEncodeStore<'a, str, ()>;

    fn encode_mut<'itm>(
        &'a mut self,
        store: &'itm mut Self::Store,
    ) -> <&'a mut Self as ReprC>::CType
    where
        'a: 'itm,
    {
        let carrier = store.encode(self, |original, fields| {
            alloc::borrow::ToOwned::to_owned(&*original).soft_encode(fields)
        });
        unsafe {
            crate::slice::CWideRefMut::from_raw_parts_mut(carrier.as_ptr(), carrier.metadata())
        }
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'a, 'b: 'a, R: ReprC + RustSpec<Size = rust_spec::size::Sized<K>>, K: SizedKind>
    EncodeMutUnchecked<'a> for &'b mut R
where
    R: EncodeMutUnchecked<'a>,
    Self: RustSpec<Size: crate::Thin>,
{
    type SoftStore = crate::sync::RefMutReferenceEncodeStore<
        crate::reference::CRefMut<R::CType, true>,
        R::SoftStore,
    >;

    unsafe fn encode_mut_unchecked<'itm>(
        &'a mut self,
        store: &'itm mut Self::SoftStore,
    ) -> <&'a mut Self as ReprC>::CType
    where
        'a: 'itm,
    {
        let value = unsafe { R::encode_mut_unchecked(&mut **self, &mut store.inner) };
        store.original = Some(value);
        crate::reference::CRefMut::from_mut(store.value.insert(value))
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'a, 'b: 'a, R: ReprC + RustSpec<Size = rust_spec::size::Sized<K>>, K: SizedKind>
    EncodeMut<'a> for &'b mut R
where
    R: EncodeMut<'a>,
    Self: RustSpec<Size: crate::Thin>,
{
    type Store = crate::sync::RefMutReferenceEncodeStore<
        crate::reference::CRefMut<R::CType, true>,
        R::Store,
    >;

    fn encode_mut<'itm>(
        &'a mut self,
        store: &'itm mut Self::Store,
    ) -> <&'a mut Self as ReprC>::CType
    where
        'a: 'itm,
    {
        let value = R::encode_mut(&mut **self, &mut store.inner);
        store.original = Some(value);
        crate::reference::CRefMut::from_mut(store.value.insert(value))
    }
}

pub(crate) use impl_copy_mut;
pub(crate) use impl_direct_mut;
