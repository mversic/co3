//! Referent-specific encoding for mutable-reference paths.
use core::{
    cell::{Cell, UnsafeCell},
    mem::ManuallyDrop,
};

use disjoint_impls::disjoint_impls;
use rust_spec::RustSpec;
#[cfg(feature = "alloc")]
use rust_spec::size::SizedKind;

use crate::{
    CType, ReprC,
    reference::{CSoftRefMut, CSoftRef},
    stored::{EncodeMove, EncodeMoveUnchecked, Store},
    transmute::AbiCompatibleValidate,
    wide::{CSoftWideRefMut, CSoftWideRef},
    wide::{Wide, WideHeader},
};
#[cfg(feature = "alloc")]
use crate::{
    boxed::CBox,
    reference::MutRefCarrier,
    sync::{RefMutDstEncodeStore, RefMutReferenceEncodeStore, RefMutWideEncodeStore},
    vec::CVec,
    wide::CWideBox,
};

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
        T: crate::transmute::AbiCompatible<CType: CType + Sized>
            + rust_spec::RustSpec<
                Layout = rust_spec::Stable,
                Drop: rust_spec::drop::DropKind,
            > + 'a,
    > EncodeMutUnchecked<'a> for [T]
    where
        &'a mut Self: ReprC<CType = crate::wide::CWideRefMut<'a, Self, true>>
            + EncodeMoveUnchecked<SoftStore = ()>,
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
                unsafe { EncodeMoveUnchecked::soft_encode_unchecked(self, store) };
            encoded
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, T: ReprC<CType: CType + Copy + Sized> + 'a> EncodeMutUnchecked<'a> for [T]
    where
        T: rust_spec::RustSpec<Layout = rust_spec::Unstable, Trap = rust_spec::layout::NonRobust>,
        &'a mut T: EncodeMoveUnchecked
            + ReprC<CType: MutRefCarrier<T, true>>,
        &'a mut Self: ReprC<CType = CSoftWideRefMut<Self, true>>,
    {
        type SoftStore = RefMutWideEncodeStore<
            'a,
            [T],
            alloc::boxed::Box<[<&'a mut T as EncodeMoveUnchecked>::SoftStore]>,
            alloc::boxed::Box<[CSoftRefMut<T, true>]>,
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
                        pointers.push(unsafe { CSoftRefMut::<T, true>::from_raw(pointer.as_ptr()) });
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
    unsafe impl<'a, T: ReprC<CType: CType + Copy + Sized> + 'a> EncodeMutUnchecked<'a> for [T]
    where
        T: rust_spec::RustSpec<Layout = rust_spec::Unstable, Trap = rust_spec::layout::Robust>,
        &'a mut T: EncodeMoveUnchecked
            + ReprC<CType: MutRefCarrier<T, true>>,
        &'a mut Self: ReprC<CType = CSoftWideRefMut<Self, true>>,
    {
        type SoftStore = RefMutWideEncodeStore<
            'a,
            [T],
            alloc::boxed::Box<[<&'a mut T as EncodeMoveUnchecked>::SoftStore]>,
            alloc::boxed::Box<[CSoftRefMut<T, true>]>,
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
                        pointers.push(unsafe { CSoftRefMut::<T, true>::from_raw(pointer.as_ptr()) });
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
    unsafe impl<'a, 'b: 'a, R: ReprC + RustSpec<Layout = rust_spec::Stable, Trap = rust_spec::layout::Robust, Size = rust_spec::size::Sized<K>>, K: SizedKind>
        EncodeMutUnchecked<'a> for &'b mut R
    where
        R: EncodeMutUnchecked<'a>,
        Self: RustSpec<Size: crate::Thin>,
        &'a mut Self: ReprC<CType = crate::reference::CRefMut<'a, Self, true>> + EncodeMoveUnchecked<SoftStore = ()>,
    {
        type SoftStore = ();

        unsafe fn encode_mut_unchecked<'itm>(
            &'a mut self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a mut Self as ReprC>::CType
        where
            'a: 'itm,
        {
            unsafe { self.soft_encode_unchecked(store) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, 'b: 'a, R: ReprC + RustSpec<Layout = rust_spec::Stable, Trap = rust_spec::layout::NonRobust, Size = rust_spec::size::Sized<K>>, K: SizedKind>
        EncodeMutUnchecked<'a> for &'b mut R
    where
        R: EncodeMutUnchecked<'a>,
        Self: RustSpec<Size: crate::Thin>,
        &'a mut Self: ReprC<CType = crate::reference::CRefMut<'a, Self, true>> + EncodeMoveUnchecked<SoftStore = ()>,
    {
        type SoftStore = ();

        unsafe fn encode_mut_unchecked<'itm>(
            &'a mut self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a mut Self as ReprC>::CType
        where
            'a: 'itm,
        {
            unsafe { self.soft_encode_unchecked(store) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, 'b: 'a, R: ReprC + RustSpec<Layout = rust_spec::Unstable, Size = rust_spec::size::Sized<K>>, K: SizedKind>
        EncodeMutUnchecked<'a> for &'b mut R
    where
        R: EncodeMutUnchecked<'a>,
        Self: RustSpec<Size: crate::Thin>,
        &'a mut Self: ReprC<CType = CSoftRefMut<Self, true>>,
    {
        type SoftStore = RefMutReferenceEncodeStore<CSoftRefMut<R, true>, R::SoftStore>;

        unsafe fn encode_mut_unchecked<'itm>(
            &'a mut self,
            store: &'itm mut Self::SoftStore,
        ) -> <&'a mut Self as ReprC>::CType
        where
            'a: 'itm,
        {
            let value = unsafe { R::encode_mut_unchecked(&mut **self, &mut store.inner) };
            let value = unsafe { CSoftRefMut::<R, true>::from_raw(value.as_ptr()) };
            store.original = Some(value);
            // Both inner carriers have the same raw pointer ABI. The outer carrier is soft.
            unsafe { CSoftRefMut::from_raw(core::ptr::from_mut(store.value.insert(value)).cast()) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, T: ReprC<CType: CType + Copy + Sized> + 'a, const N: usize> EncodeMutUnchecked<'a>
        for [T; N]
    where
        Self: RustSpec<Layout = rust_spec::Unstable>,
        &'a mut T: EncodeMoveUnchecked + ReprC<CType: MutRefCarrier<T, true>>,
        &'a mut Self: ReprC<CType = CSoftRefMut<Self, true>>,
    {
        type SoftStore = RefMutWideEncodeStore<
            'a,
            [T],
            alloc::boxed::Box<[<&'a mut T as EncodeMoveUnchecked>::SoftStore]>,
            alloc::boxed::Box<[CSoftRefMut<T, true>]>,
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
                        pointers.push(unsafe { CSoftRefMut::<T, true>::from_raw(pointer.as_ptr()) });
                    }
                    (values.into_boxed_slice(), pointers.into_boxed_slice())
                },
                |values, pointers| {
                    for (value, pointer) in values.iter().zip(pointers.iter()) {
                        unsafe { *pointer.as_ptr() = *value };
                    }
                },
            );
            unsafe { CSoftRefMut::from_raw(wide.as_ptr().cast::<[T::CType; N]>()) }
        }
    }

    unsafe impl<'a, T, const N: usize> EncodeMutUnchecked<'a> for [T; N]
    where
        Self: crate::transmute::AbiCompatible + RustSpec<Layout = rust_spec::Stable> + 'a,
        &'a mut Self: ReprC<CType = crate::reference::CRefMut<'a, Self, true>> + EncodeMoveUnchecked<SoftStore = ()>,
    {
        type SoftStore = ();
        unsafe fn encode_mut_unchecked<'itm>(&'a mut self, store: &'itm mut ()) -> <&'a mut Self as ReprC>::CType where 'a: 'itm {
            unsafe { EncodeMoveUnchecked::soft_encode_unchecked(self, store) }
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
        ) -> crate::encode_repr::Encoded<&'a mut Self>
        where
            &'a mut Self: ReprC<CType: crate::encode_repr::EncodeRepr>,
            'a: 'itm;
    }
    unsafe impl<
        'a,
        T: AbiCompatibleValidate<CType: CType + Sized>
            + rust_spec::RustSpec<
                Layout = rust_spec::Stable,
                Trap = rust_spec::layout::Robust,
                Drop: rust_spec::drop::DropKind,
            > + 'a,
    > EncodeMut<'a> for [T]
    where
        &'a mut Self: ReprC<CType = crate::wide::CWideRefMut<'a, Self, true>>
            + EncodeMoveUnchecked<SoftStore = ()>
            + EncodeMove<Store = ()>,
    {
        type Store = ();
        fn encode_mut<'itm>(&'a mut self, store: &'itm mut ()) -> crate::encode_repr::Encoded<&'a mut Self>
        where
            'a: 'itm,
        {
            let encoded = crate::encode_repr::encode_field(self, store);
            debug_assert!(
                unsafe {
                    <Self as AbiCompatibleValidate>::is_valid(
                        core::slice::from_raw_parts(encoded.as_ptr(), encoded.metadata()),
                    )
                },
                "encoding failed AbiCompatibleValidate::is_valid"
            );
            crate::encode_repr::project(encoded)
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, T: ReprC<CType: CType + Copy + Sized> + 'a> EncodeMut<'a> for [T]
    where
        T: rust_spec::RustSpec<Trap = rust_spec::layout::NonRobust>,
        Self: EncodeMutUnchecked<'a> + ReprC<CType = [<T as ReprC>::CType]>,
        &'a mut T: EncodeMove
            + ReprC<CType: MutRefCarrier<T, true>>,
        &'a mut Self: ReprC<CType: crate::encode_repr::EncodeRepr<Encoded = CSoftWideRefMut<Self, true>>>,
    {
        type Store = RefMutWideEncodeStore<
            'a,
            [T],
            alloc::boxed::Box<[<&'a mut T as EncodeMove>::Store]>,
            alloc::boxed::Box<[CSoftRefMut<T, true>]>,
        >;

        fn encode_mut<'itm>(
            &'a mut self,
            store: &'itm mut Self::Store,
        ) -> crate::encode_repr::Encoded<&'a mut Self>
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
                        let pointer = crate::encode_repr::encode_field(value, field_store);
                        values.push(unsafe { *pointer.as_ptr() });
                        pointers.push(unsafe { CSoftRefMut::<T, true>::from_raw(pointer.as_ptr()) });
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
    unsafe impl<'a, T: ReprC<CType: CType + Copy + Sized> + 'a> EncodeMut<'a> for [T]
    where
        T: rust_spec::RustSpec<Layout = rust_spec::Unstable, Trap = rust_spec::layout::Robust>,
        &'a mut T: EncodeMove
            + ReprC<CType: MutRefCarrier<T, true>>,
        &'a mut Self: ReprC<CType = CSoftWideRefMut<Self, true>>,
    {
        type Store = RefMutWideEncodeStore<
            'a,
            [T],
            alloc::boxed::Box<[<&'a mut T as EncodeMove>::Store]>,
            alloc::boxed::Box<[CSoftRefMut<T, true>]>,
        >;

        fn encode_mut<'itm>(
            &'a mut self,
            store: &'itm mut Self::Store,
        ) -> crate::encode_repr::Encoded<&'a mut Self>
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
                        let pointer = crate::encode_repr::encode_field(value, field_store);
                        values.push(unsafe { *pointer.as_ptr() });
                        pointers.push(unsafe { CSoftRefMut::<T, true>::from_raw(pointer.as_ptr()) });
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
    unsafe impl<'a, 'b: 'a, R: ReprC + RustSpec<Layout = rust_spec::Stable, Trap = rust_spec::layout::Robust, Size = rust_spec::size::Sized<K>>, K: SizedKind>
        EncodeMut<'a> for &'b mut R
    where
        R: EncodeMut<'a>,
        Self: RustSpec<Size: crate::Thin>,
        &'a mut Self: ReprC<CType = crate::reference::CRefMut<'a, Self, true>> + EncodeMoveUnchecked<SoftStore = ()>,
    {
        type Store = RefMutReferenceEncodeStore<CSoftRefMut<R, true>, R::Store>;

        fn encode_mut<'itm>(
            &'a mut self,
            store: &'itm mut Self::Store,
        ) -> crate::encode_repr::Encoded<&'a mut Self>
        where
            'a: 'itm,
        {
            let value = R::encode_mut(&mut **self, &mut store.inner);
            let value = unsafe { CSoftRefMut::<R, true>::from_raw(value.as_ptr()) };
            store.original = Some(value);
            // Both inner carriers have the same raw pointer ABI. The outer carrier is soft.
            unsafe { CSoftRefMut::from_raw(core::ptr::from_mut(store.value.insert(value)).cast()) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, 'b: 'a, R: ReprC + RustSpec<Layout = rust_spec::Stable, Trap = rust_spec::layout::NonRobust, Size = rust_spec::size::Sized<K>>, K: SizedKind>
        EncodeMut<'a> for &'b mut R
    where
        R: EncodeMut<'a>,
        Self: RustSpec<Size: crate::Thin>,
        &'a mut Self: ReprC<CType = crate::reference::CRefMut<'a, Self, true>> + EncodeMoveUnchecked<SoftStore = ()>,
    {
        type Store = RefMutReferenceEncodeStore<CSoftRefMut<R, true>, R::Store>;

        fn encode_mut<'itm>(
            &'a mut self,
            store: &'itm mut Self::Store,
        ) -> crate::encode_repr::Encoded<&'a mut Self>
        where
            'a: 'itm,
        {
            let value = R::encode_mut(&mut **self, &mut store.inner);
            let value = unsafe { CSoftRefMut::<R, true>::from_raw(value.as_ptr()) };
            store.original = Some(value);
            // Both inner carriers have the same raw pointer ABI. The outer carrier is soft.
            unsafe { CSoftRefMut::from_raw(core::ptr::from_mut(store.value.insert(value)).cast()) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, 'b: 'a, R: ReprC + RustSpec<Layout = rust_spec::Unstable, Size = rust_spec::size::Sized<K>>, K: SizedKind>
        EncodeMut<'a> for &'b mut R
    where
        R: EncodeMut<'a>,
        Self: RustSpec<Size: crate::Thin>,
        &'a mut Self: ReprC<CType = CSoftRefMut<Self, true>>,
    {
        type Store = RefMutReferenceEncodeStore<CSoftRefMut<R, true>, R::Store>;

        fn encode_mut<'itm>(
            &'a mut self,
            store: &'itm mut Self::Store,
        ) -> crate::encode_repr::Encoded<&'a mut Self>
        where
            'a: 'itm,
        {
            let value = R::encode_mut(&mut **self, &mut store.inner);
            let value = unsafe { CSoftRefMut::<R, true>::from_raw(value.as_ptr()) };
            store.original = Some(value);
            // Both inner carriers have the same raw pointer ABI. The outer carrier is soft.
            unsafe { CSoftRefMut::from_raw(core::ptr::from_mut(store.value.insert(value)).cast()) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, T: ReprC<CType: CType + Copy + Sized> + 'a, const N: usize> EncodeMut<'a> for [T; N]
    where
        Self: RustSpec<Layout = rust_spec::Unstable>,
        &'a mut T: EncodeMove + ReprC<CType: MutRefCarrier<T, true>>,
        &'a mut Self: ReprC<CType = CSoftRefMut<Self, true>>,
    {
        type Store = RefMutWideEncodeStore<
            'a,
            [T],
            alloc::boxed::Box<[<&'a mut T as EncodeMove>::Store]>,
            alloc::boxed::Box<[CSoftRefMut<T, true>]>,
        >;

        fn encode_mut<'itm>(
            &'a mut self,
            store: &'itm mut Self::Store,
        ) -> crate::encode_repr::Encoded<&'a mut Self>
        where
            'a: 'itm,
        {
            let wide = store.encode(
                |stores| {
                    *stores = core::iter::repeat_with(Default::default).take(N).collect();
                    let mut values = alloc::vec::Vec::with_capacity(N);
                    let mut pointers = alloc::vec::Vec::with_capacity(N);
                    for (value, field_store) in self.iter_mut().zip(stores.iter_mut()) {
                        let pointer = crate::encode_repr::encode_field(value, field_store);
                        values.push(unsafe { *pointer.as_ptr() });
                        pointers.push(unsafe { CSoftRefMut::<T, true>::from_raw(pointer.as_ptr()) });
                    }
                    (values.into_boxed_slice(), pointers.into_boxed_slice())
                },
                |values, pointers| {
                    for (value, pointer) in values.iter().zip(pointers.iter()) {
                        unsafe { *pointer.as_ptr() = *value };
                    }
                },
            );
            unsafe { CSoftRefMut::from_raw(wide.as_ptr().cast::<[T::CType; N]>()) }
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<'a, T: ReprC<CType: CType + Copy + Sized> + 'a, const N: usize> EncodeMut<'a> for [T; N]
    where
        Self: RustSpec<Layout = rust_spec::Stable, Trap = rust_spec::layout::NonRobust> + EncodeMutUnchecked<'a> + ReprC<CType = [<T as ReprC>::CType; N]>,
        &'a mut T: EncodeMove + ReprC<CType: MutRefCarrier<T, true>>,
        &'a mut Self: ReprC<CType = crate::reference::CRefMut<'a, Self, true>>,
    {
        type Store = RefMutWideEncodeStore<
            'a,
            [T],
            alloc::boxed::Box<[<&'a mut T as EncodeMove>::Store]>,
            alloc::boxed::Box<[CSoftRefMut<T, true>]>,
        >;

        fn encode_mut<'itm>(
            &'a mut self,
            store: &'itm mut Self::Store,
        ) -> crate::encode_repr::Encoded<&'a mut Self>
        where
            'a: 'itm,
        {
            let wide = store.encode(
                |stores| {
                    *stores = core::iter::repeat_with(Default::default).take(N).collect();
                    let mut values = alloc::vec::Vec::with_capacity(N);
                    let mut pointers = alloc::vec::Vec::with_capacity(N);
                    for (value, field_store) in self.iter_mut().zip(stores.iter_mut()) {
                        let pointer = crate::encode_repr::encode_field(value, field_store);
                        values.push(unsafe { *pointer.as_ptr() });
                        pointers.push(unsafe { CSoftRefMut::<T, true>::from_raw(pointer.as_ptr()) });
                    }
                    (values.into_boxed_slice(), pointers.into_boxed_slice())
                },
                |values, pointers| {
                    for (value, pointer) in values.iter().zip(pointers.iter()) {
                        unsafe { *pointer.as_ptr() = *value };
                    }
                },
            );
            unsafe { CSoftRefMut::from_raw(wide.as_ptr().cast::<[T::CType; N]>()) }
        }
    }
    unsafe impl<'a, T, const N: usize> EncodeMut<'a> for [T; N]
    where
        Self: crate::transmute::AbiCompatibleValidate + RustSpec<Layout = rust_spec::Stable, Trap = rust_spec::layout::Robust> + 'a,
        &'a mut Self: ReprC<CType = crate::reference::CRefMut<'a, Self, true>> + EncodeMove<Store = ()> + EncodeMoveUnchecked<SoftStore = ()>,
    {
        type Store = ();
        fn encode_mut<'itm>(&'a mut self, store: &'itm mut ()) -> crate::encode_repr::Encoded<&'a mut Self> where 'a: 'itm {
            EncodeMove::soft_encode(self, store)
        }
    }
}

// Each implementation owns its temporary value and decodes it exactly once on sync.
// Keeping this per-type avoids a stable-layout blanket impl overlapping downstream impls.
macro_rules! impl_direct_mut {
    ([$($params:tt)*] $ty:ty) => {
        unsafe impl<'_mut, $($params)*> $crate::stored::EncodeMutUnchecked<'_mut> for $ty
        where
            Self: $crate::transmute::AbiCompatible
                + $crate::rust_spec::RustSpec<Layout = $crate::rust_spec::Stable, Trap = $crate::rust_spec::layout::Robust> + '_mut,
            &'_mut mut Self: $crate::ReprC<CType: Sized>
                + $crate::transmute::AbiCompatible
                + $crate::stored::EncodeMoveUnchecked<SoftStore = ()>,
        {
            type SoftStore = ();
            unsafe fn encode_mut_unchecked<'itm>(&'_mut mut self, store: &'itm mut ()) -> <&'_mut mut Self as $crate::ReprC>::CType
            where '_mut: 'itm {
                let encoded = unsafe { $crate::stored::EncodeMoveUnchecked::soft_encode_unchecked(self, store) };
                encoded
            }
        }
        unsafe impl<'_mut, $($params)*> $crate::stored::EncodeMut<'_mut> for $ty
        where
            Self: $crate::transmute::AbiCompatibleValidate
                + $crate::rust_spec::RustSpec<Layout = $crate::rust_spec::Stable, Trap = $crate::rust_spec::layout::Robust> + '_mut,
            &'_mut mut Self: $crate::ReprC<CType: crate::encode_repr::EncodeRepr>
                + $crate::transmute::AbiCompatibleValidate
                + $crate::stored::EncodeMoveUnchecked<SoftStore = ()>
                + $crate::stored::EncodeMove<Store = ()>,
        {
            type Store = ();
            fn encode_mut<'itm>(&'_mut mut self, store: &'itm mut ()) -> $crate::encode_repr::Encoded<&'_mut mut Self>
            where '_mut: 'itm {
                let encoded = $crate::encode_repr::encode_field(self, store);
                debug_assert!(unsafe { <&'_mut mut Self as $crate::transmute::AbiCompatibleValidate>::is_valid(&encoded) }, "encoding failed AbiCompatibleValidate::is_valid");
                $crate::encode_repr::project(encoded)
            }
        }
    };
}

macro_rules! impl_copy_mut {
    ([] $ty:ty) => { $crate::mutable::impl_copy_mut!(@impl [for<'_dummy>] [] $ty); };
    ([$($params:tt)+] $ty:ty) => { $crate::mutable::impl_copy_mut!(@impl [] [$($params)+] $ty); };
    (@impl [$($spec:tt)*] [$($params:tt)*] $ty:ty) => {
        const _: () = {
            use $crate::stored::EncodeMutUnchecked;
            $crate::disjoint_impls! {
                #[disjoint_impls(remote)]
                pub unsafe trait EncodeMutUnchecked<'a>: $crate::ReprC + 'a {
                    type SoftStore: $crate::stored::Store + Default;
                    unsafe fn encode_mut_unchecked<'itm>(
                        &'a mut self, store: &'itm mut Self::SoftStore,
                    ) -> <&'a mut Self as $crate::ReprC>::CType
                    where &'a mut Self: $crate::ReprC<CType: Sized>, 'a: 'itm;
                }
                unsafe impl<'_mut, $($params)*> EncodeMutUnchecked<'_mut> for $ty
                where
                    $($spec)* Self: $crate::transmute::AbiCompatible
                        + $crate::rust_spec::RustSpec<Layout = $crate::rust_spec::Stable> + '_mut,
                    &'_mut mut Self: $crate::ReprC<CType = $crate::reference::CRefMut<'_mut, Self, true>>
                        + $crate::stored::EncodeMoveUnchecked<SoftStore = ()>,
                {
                    type SoftStore = ();
                    unsafe fn encode_mut_unchecked<'itm>(&'_mut mut self, store: &'itm mut ()) -> <&'_mut mut Self as $crate::ReprC>::CType
                    where '_mut: 'itm {
                        unsafe { $crate::stored::EncodeMoveUnchecked::soft_encode_unchecked(self, store) }
                    }
                }
                unsafe impl<'_mut, $($params)*> EncodeMutUnchecked<'_mut> for $ty
                where
                    $($spec)* Self: $crate::ReprC<CType: Sized> + $crate::stored::EncodeMoveUnchecked
                        + $crate::rust_spec::RustSpec<Layout = $crate::rust_spec::Unstable>
                        + $crate::stored::DecodeMove<'_mut, Store: $crate::stored::EmptyStore + '_mut> + Copy + '_mut,
                    &'_mut mut Self: $crate::ReprC<CType = $crate::reference::CSoftRefMut<Self, true>>,
                {
                    type SoftStore = $crate::sync::RefMutSizedEncodeStore<'_mut, Self, <Self as $crate::stored::EncodeMoveUnchecked>::SoftStore>;
                    unsafe fn encode_mut_unchecked<'itm>(&'_mut mut self, store: &'itm mut Self::SoftStore) -> <&'_mut mut Self as $crate::ReprC>::CType
                    where '_mut: 'itm {
                        store.encode(self, |original, fields| {
                            unsafe { $crate::stored::EncodeMoveUnchecked::soft_encode_unchecked(*original, fields) }
                        })
                    }
                }
            }
        };
        unsafe impl<'_mut, $($params)*> $crate::stored::EncodeMut<'_mut> for $ty
        where
            Self: $crate::stored::EncodeMove
                + $crate::stored::DecodeMove<'_mut, Store: $crate::stored::EmptyStore + '_mut>
                + $crate::stored::EncodeMutUnchecked<'_mut> + Copy + '_mut,
            &'_mut mut Self: $crate::ReprC<CType: $crate::encode_repr::EncodeRepr<Encoded = $crate::reference::CSoftRefMut<Self, true>>>,
        {
            type Store = $crate::sync::RefMutSizedEncodeStore<'_mut, Self, <Self as $crate::stored::EncodeMove>::Store>;
            fn encode_mut<'itm>(&'_mut mut self, store: &'itm mut Self::Store) -> $crate::encode_repr::Encoded<&'_mut mut Self>
            where '_mut: 'itm {
                store.encode(self, |original, fields| $crate::encode_repr::encode_field(*original, fields))
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
    &'a mut Self: ReprC<CType: Copy + crate::encode_repr::EncodeRepr>,
    &'a mut R: ReprC<CType: Copy + crate::encode_repr::EncodeRepr>,
{
    type Store = R::Store;
    fn encode_mut<'itm>(
        &'a mut self,
        store: &'itm mut Self::Store,
    ) -> crate::encode_repr::Encoded<&'a mut Self>
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
            &'a mut Self: ReprC<CType: Copy + crate::encode_repr::EncodeRepr>,
            &'a mut R: ReprC<CType: Copy + crate::encode_repr::EncodeRepr>,
        {
            type Store = R::Store;
            fn encode_mut<'itm>(
                &'a mut self,
                store: &'itm mut Self::Store,
            ) -> crate::encode_repr::Encoded<&'a mut Self>
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
impl_direct_mut!([T: ?Sized] core::marker::PhantomData<T>);
impl_direct_mut!([T] core::mem::MaybeUninit<T>);
impl_direct_mut!(['r, C: ReprC<CType: CType> + ?Sized] crate::reference::CRef<'r, C>);
impl_direct_mut!(['r, C: ReprC<CType: CType> + ?Sized, const EXCLUSIVE: bool] crate::reference::CRefMut<'r, C, EXCLUSIVE>);
impl_direct_mut!(['r, P: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized] crate::wide::CWideRef<'r, P>);
impl_direct_mut!(['r, P: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, const EXCLUSIVE: bool] crate::wide::CWideRefMut<'r, P, EXCLUSIVE>);
impl_direct_mut!([C: ReprC<CType: CType> + ?Sized] CSoftRef<C>);
impl_direct_mut!([C: ReprC<CType: CType> + ?Sized, const EXCLUSIVE: bool] CSoftRefMut<C, EXCLUSIVE>);
impl_direct_mut!([P: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized] CSoftWideRef<P>);
impl_direct_mut!([P: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized, const EXCLUSIVE: bool] CSoftWideRefMut<P, EXCLUSIVE>);
#[cfg(feature = "alloc")]
impl_direct_mut!([T: ReprC<CType: CType + Sized>] CVec<T>);
#[cfg(feature = "alloc")]
impl_direct_mut!([C: ReprC<CType: CType + Sized>] CBox<C>);
#[cfg(feature = "alloc")]
impl_direct_mut!([P: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized] CWideBox<P>);
macro_rules! impl_move_out_mut {
    ([] $ty:ty where $($bounds:tt)*) => { impl_move_out_mut!(@impl [for<'_dummy>] [] $ty where $($bounds)*); };
    ([$($params:tt)+] $ty:ty where $($bounds:tt)*) => { impl_move_out_mut!(@impl [] [$($params)+] $ty where $($bounds)*); };
    (@impl [$($spec:tt)*] [$($params:tt)*] $ty:ty where $($bounds:tt)*) => {
        const _: () = {
            use $crate::stored::EncodeMutUnchecked;
            $crate::disjoint_impls! {
                #[disjoint_impls(remote)]
                pub unsafe trait EncodeMutUnchecked<'a>: $crate::ReprC + 'a {
                    type SoftStore: $crate::stored::Store + Default;
                    unsafe fn encode_mut_unchecked<'itm>(
                        &'a mut self, store: &'itm mut Self::SoftStore,
                    ) -> <&'a mut Self as $crate::ReprC>::CType
                    where &'a mut Self: $crate::ReprC<CType: Sized>, 'a: 'itm;
                }
                unsafe impl<'_mut, $($params)*> EncodeMutUnchecked<'_mut> for $ty
                where
                    $($bounds)*
                    $($spec)* Self: $crate::transmute::AbiCompatible
                        + $crate::rust_spec::RustSpec<Layout = $crate::rust_spec::Stable> + '_mut,
                    &'_mut mut Self: $crate::ReprC<CType = $crate::reference::CRefMut<'_mut, Self, true>>
                        + $crate::stored::EncodeMoveUnchecked<SoftStore = ()>,
                {
                    type SoftStore = ();
                    unsafe fn encode_mut_unchecked<'itm>(&'_mut mut self, store: &'itm mut ()) -> <&'_mut mut Self as $crate::ReprC>::CType
                    where '_mut: 'itm {
                        unsafe { $crate::stored::EncodeMoveUnchecked::soft_encode_unchecked(self, store) }
                    }
                }
                unsafe impl<'_mut, $($params)*> EncodeMutUnchecked<'_mut> for $ty
                where
                    $($bounds)*
                    $($spec)* Self: $crate::ReprC<CType: Sized> + $crate::stored::EncodeMoveUnchecked
                        + $crate::rust_spec::RustSpec<Layout = $crate::rust_spec::Unstable>
                        + $crate::stored::DecodeMove<'_mut, Store: $crate::stored::EmptyStore + '_mut> + Default + '_mut,
                    &'_mut mut Self: $crate::ReprC<CType = $crate::reference::CSoftRefMut<Self, true>>,
                {
                    type SoftStore = $crate::sync::RefMutSizedEncodeStore<'_mut, Self, <Self as $crate::stored::EncodeMoveUnchecked>::SoftStore>;
                    unsafe fn encode_mut_unchecked<'itm>(&'_mut mut self, store: &'itm mut Self::SoftStore) -> <&'_mut mut Self as $crate::ReprC>::CType
                    where '_mut: 'itm {
                        store.encode(self, |original, fields| {
                            unsafe { $crate::stored::EncodeMoveUnchecked::soft_encode_unchecked(core::mem::take(original), fields) }
                        })
                    }
                }
            }
        };
        unsafe impl<'_mut, $($params)*> $crate::stored::EncodeMut<'_mut> for $ty
        where
            $($bounds)*
            Self: $crate::stored::EncodeMove
                + $crate::stored::DecodeMove<'_mut, Store: $crate::stored::EmptyStore + '_mut>
                + $crate::stored::EncodeMutUnchecked<'_mut> + Default + '_mut,
            &'_mut mut Self: $crate::ReprC<CType: $crate::encode_repr::EncodeRepr<Encoded = $crate::reference::CSoftRefMut<Self, true>>>,
        {
            type Store = $crate::sync::RefMutSizedEncodeStore<'_mut, Self, <Self as $crate::stored::EncodeMove>::Store>;
            fn encode_mut<'itm>(&'_mut mut self, store: &'itm mut Self::Store) -> $crate::encode_repr::Encoded<&'_mut mut Self>
            where '_mut: 'itm {
                store.encode(self, |original, fields| $crate::encode_repr::encode_field(core::mem::take(original), fields))
            }
        }
    };
}

impl_move_out_mut!([T: ReprC<CType: CType + Sized>] crate::option::COption<T> where Self: Default,);
impl_move_out_mut!([T] crate::option::Option<T> where);
// Keep the original vector and its elements alive: decoding an owned carrier
// before synchronizing element stores can restore references into freed storage.
#[cfg(feature = "alloc")]
macro_rules! impl_vec_mut {
    ($vec:path) => {
        const _: () = {
            use crate::stored::EncodeMutUnchecked;
            crate::disjoint_impls! {
                #[disjoint_impls(remote)]
                pub unsafe trait EncodeMutUnchecked<'a>: ReprC + 'a {
                    type SoftStore: Store + Default;
                    unsafe fn encode_mut_unchecked<'itm>(&'a mut self, store: &'itm mut Self::SoftStore) -> <&'a mut Self as ReprC>::CType
                    where &'a mut Self: ReprC<CType: Sized>, 'a: 'itm;
                }
        unsafe impl<'a, T: ReprC<CType: CType + Copy + Sized> + 'a> EncodeMutUnchecked<'a> for $vec
        where
            Self: RustSpec<Layout = rust_spec::Unstable>,
            &'a mut T: EncodeMoveUnchecked + ReprC<CType: MutRefCarrier<T, true>>,
            &'a mut Self: ReprC<CType = CSoftRefMut<Self, true>>,
        {
            type SoftStore = RefMutReferenceEncodeStore<
                CVec<T>,
                RefMutWideEncodeStore<
                    'a,
                    [T],
                    alloc::boxed::Box<[<&'a mut T as EncodeMoveUnchecked>::SoftStore]>,
                    alloc::boxed::Box<[CSoftRefMut<T, true>]>,
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
                            pointers.push(unsafe { CSoftRefMut::<T, true>::from_raw(pointer.as_ptr()) });
                        }
                        (values, pointers.into_boxed_slice())
                    },
                    |values, pointers| {
                        for (value, pointer) in values.iter().zip(pointers.iter()) {
                            unsafe { *pointer.as_ptr() = *value };
                        }
                    },
                );
                let header =
                    unsafe { CVec::from_raw_parts(wide.as_ptr(), wide.metadata(), capacity) };
                // Header replacement cannot change the original vector's allocation.
                store.original = Some(header);
                CSoftRefMut::from_mut(store.value.insert(header))
            }
        }
        unsafe impl<'a, T: ReprC<CType: CType + Copy + Sized> + 'a> EncodeMutUnchecked<'a> for $vec
        where
            Self: crate::transmute::AbiCompatible + RustSpec<Layout = rust_spec::Stable>,
            &'a mut Self: ReprC<CType = crate::reference::CRefMut<'a, Self, true>> + EncodeMoveUnchecked<SoftStore = ()>,
        {
            type SoftStore = ();
            unsafe fn encode_mut_unchecked<'itm>(&'a mut self, store: &'itm mut ()) -> <&'a mut Self as ReprC>::CType
            where 'a: 'itm {
                unsafe { self.soft_encode_unchecked(store) }
            }
        }
            }
        };
        unsafe impl<'a, T: ReprC<CType: CType + Copy + Sized> + 'a> EncodeMut<'a> for $vec
        where
            Self: EncodeMutUnchecked<'a> + ReprC<CType = CVec<T>>,
            &'a mut T: EncodeMove + ReprC<CType: MutRefCarrier<T, true>>,
            &'a mut Self: ReprC<CType: crate::encode_repr::EncodeRepr<Encoded = CSoftRefMut<Self, true>>>,
        {
            type Store = RefMutReferenceEncodeStore<
                CVec<T>,
                RefMutWideEncodeStore<
                    'a,
                    [T],
                    alloc::boxed::Box<[<&'a mut T as EncodeMove>::Store]>,
                    alloc::boxed::Box<[CSoftRefMut<T, true>]>,
                    alloc::vec::Vec<T::CType>,
                >,
            >;

            fn encode_mut<'itm>(
                &'a mut self,
                store: &'itm mut Self::Store,
            ) -> crate::encode_repr::Encoded<&'a mut Self>
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
                            let pointer = crate::encode_repr::encode_field(value, field_store);
                            // The carrier is Copy; no owned Rust element is read or moved.
                            values.push(unsafe { *pointer.as_ptr() });
                            pointers.push(unsafe { CSoftRefMut::<T, true>::from_raw(pointer.as_ptr()) });
                        }
                        (values, pointers.into_boxed_slice())
                    },
                    |values, pointers| {
                        for (value, pointer) in values.iter().zip(pointers.iter()) {
                            unsafe { *pointer.as_ptr() = *value };
                        }
                    },
                );
                let header =
                    unsafe { CVec::from_raw_parts(wide.as_ptr(), wide.metadata(), capacity) };
                // Header replacement cannot change the original vector's allocation.
                store.original = Some(header);
                CSoftRefMut::from_mut(store.value.insert(header))
            }
        }
    };
}

#[cfg(feature = "alloc")]
impl_vec_mut!(alloc::vec::Vec<T>);
#[cfg(feature = "alloc")]
impl_vec_mut!(crate::vec::Vec<T>);
#[cfg(feature = "alloc")]
impl_move_out_mut!([T] alloc::boxed::Box<[T]> where T: crate::ReprC<CType: CType + Sized>,);
#[cfg(feature = "alloc")]
impl_move_out_mut!([T: Default] alloc::boxed::Box<T> where T: crate::ReprC<CType: CType + Sized>,);
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
    type SoftStore = ();

    unsafe fn encode_mut_unchecked<'itm>(
        &'a mut self,
        store: &'itm mut Self::SoftStore,
    ) -> <&'a mut Self as ReprC>::CType
    where
        'a: 'itm,
    {
        // Stable unchecked encoding borrows the original bytes directly.
        unsafe { self.soft_encode_unchecked(store) }
    }
}

#[cfg(feature = "alloc")]
unsafe impl<'a> EncodeMut<'a> for str {
    type Store = RefMutDstEncodeStore<'a, str, ()>;

    fn encode_mut<'itm>(
        &'a mut self,
        store: &'itm mut Self::Store,
    ) -> crate::encode_repr::Encoded<&'a mut Self>
    where
        'a: 'itm,
    {
        let carrier = store.encode(self, |original, fields| {
            alloc::borrow::ToOwned::to_owned(&*original).soft_encode(fields)
        });
        unsafe { CSoftWideRefMut::from_raw_parts_mut(carrier.as_ptr(), carrier.len()) }
    }
}

pub(crate) use impl_copy_mut;
pub(crate) use impl_direct_mut;
