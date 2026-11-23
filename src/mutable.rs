//! Referent-specific encoding for mutable-reference paths.
#[cfg(feature = "repr-rust")]
use crate::convert::AbiCompatibleValidate;
#[cfg(all(feature = "alloc", feature = "repr-rust"))]
use crate::sync::RefMutDstEncodeStore;
#[cfg(all(feature = "alloc", feature = "repr-rust"))]
use alloc::string::String;
#[cfg(feature = "alloc")]
use alloc::vec::Vec;
use core::{
    cell::{Cell, UnsafeCell},
    mem::ManuallyDrop,
};
use rust_spec::RustSpec;
#[cfg(all(feature = "alloc", feature = "repr-rust"))]
use rust_spec::Unspecified;

use disjoint_impls::disjoint_impls;
#[cfg(feature = "alloc")]
use rust_spec::size::SizedKind;

use crate::{
    CType, ReprC,
    convert::{EncodeMove, EncodeMoveUnchecked, Store},
    option::Option as StableOption,
    reference::abi::{SoftRef, SoftRefMut},
    wide::{
        Wide, WideHeader,
        abi::{SoftWideRef as AbiSoftWideRef, SoftWideRefMut as AbiSoftWideRefMut},
    },
};
#[cfg(feature = "alloc")]
use crate::{
    boxed::abi::Box as AbiBox,
    reference::abi::RefMutCarrier,
    string::String as StableString,
    sync::{RefMutReferenceEncodeStore, RefMutWideEncodeStore},
    vec::{Vec as StableVec, abi::Vec as AbiVec},
    wide::abi::WideBox as AbiWideBox,
};

// Each implementation owns its temporary value and decodes it exactly once on sync.
// Keeping this per-type avoids a stable-layout blanket impl overlapping downstream impls.

#[inline(always)]
unsafe fn encode_direct_mut_unchecked<'a: 'itm, 'itm, T: ?Sized>(
    value: &'a mut T,
    store: &'itm mut (),
) -> <&'a mut T as ReprC>::CType
where
    &'a mut T: EncodeMoveUnchecked<SoftStore = ()>,
{
    unsafe { EncodeMoveUnchecked::soft_encode_unchecked(value, store) }
}

#[inline(always)]
fn encode_direct_mut<'a: 'itm, 'itm, T: ?Sized>(
    value: &'a mut T,
    store: &'itm mut (),
) -> <&'a mut T as ReprC>::CType
where
    &'a mut T: EncodeMove<Store = ()>,
{
    EncodeMove::soft_encode(value, store)
}

#[inline(always)]
unsafe fn encode_copy_mut_unchecked<'a: 'itm, 'itm, T>(
    value: &'a mut T,
    store: &'itm mut crate::sync::RefMutSizedEncodeStore<
        'a,
        T,
        <T as EncodeMoveUnchecked>::SoftStore,
    >,
) -> SoftRefMut<T::CType>
where
    T: EncodeMoveUnchecked
        + Copy
        + crate::convert::DecodeMove<'a, Store: crate::convert::EmptyStore + 'a>,
{
    store.encode(value, |original, fields| unsafe {
        EncodeMoveUnchecked::soft_encode_unchecked(*original, fields)
    })
}

#[inline(always)]
fn encode_copy_mut<'a: 'itm, 'itm, T>(
    value: &'a mut T,
    store: &'itm mut crate::sync::RefMutSizedEncodeStore<'a, T, <T as EncodeMove>::Store>,
) -> SoftRefMut<T::CType>
where
    T: EncodeMove + Copy + crate::convert::DecodeMove<'a, Store: crate::convert::EmptyStore + 'a>,
{
    store.encode(value, |original, fields| {
        EncodeMove::soft_encode(*original, fields)
    })
}

#[inline(always)]
unsafe fn encode_take_mut_unchecked<'a: 'itm, 'itm, T>(
    value: &'a mut T,
    store: &'itm mut crate::sync::RefMutSizedEncodeStore<
        'a,
        T,
        <T as EncodeMoveUnchecked>::SoftStore,
    >,
) -> SoftRefMut<T::CType>
where
    T: EncodeMoveUnchecked
        + Default
        + crate::convert::DecodeMove<'a, Store: crate::convert::EmptyStore + 'a>,
{
    store.encode(value, |original, fields| unsafe {
        EncodeMoveUnchecked::soft_encode_unchecked(core::mem::take(original), fields)
    })
}

#[inline(always)]
fn encode_take_mut<'a: 'itm, 'itm, T>(
    value: &'a mut T,
    store: &'itm mut crate::sync::RefMutSizedEncodeStore<'a, T, <T as EncodeMove>::Store>,
) -> SoftRefMut<T::CType>
where
    T: EncodeMove
        + Default
        + crate::convert::DecodeMove<'a, Store: crate::convert::EmptyStore + 'a>,
{
    store.encode(value, |original, fields| {
        EncodeMove::soft_encode(core::mem::take(original), fields)
    })
}

// All mutable dispatch specializations are emitted with their trait definitions.

#[cfg(feature = "alloc")]
type MutableVecStore<'a, T, S, C> = RefMutReferenceEncodeStore<
    C,
    RefMutWideEncodeStore<
        'a,
        [T],
        alloc::boxed::Box<[S]>,
        alloc::boxed::Box<[SoftRefMut<<T as ReprC>::CType>]>,
        alloc::vec::Vec<<T as ReprC>::CType>,
    >,
>;

#[cfg(feature = "alloc")]
fn encode_vec_mut<'a, T, V, S, P>(
    items: impl ExactSizeIterator<Item = &'a mut T>,
    capacity: usize,
    store: &mut MutableVecStore<'a, T, S, V::CType>,
    mut encode: impl FnMut(&'a mut T, &mut S) -> P,
) -> SoftRefMut<V::CType>
where
    T: ReprC + 'a,
    V: ReprC<CType: Copy + PartialEq + From<AbiVec<<T as ReprC>::CType>>>,
    S: Store + Default,
    P: RefMutCarrier<Pointee = <T as ReprC>::CType>,
    <T as ReprC>::CType: Copy,
{
    store.value = None;
    store.original = None;
    let wide = store.inner.encode(
        |stores| {
            *stores = core::iter::repeat_with(Default::default)
                .take(items.len())
                .collect();
            let mut values = alloc::vec::Vec::with_capacity(capacity);
            let mut pointers = alloc::vec::Vec::with_capacity(items.len());
            for (value, field_store) in items.zip(stores.iter_mut()) {
                let pointer = encode(value, field_store);
                // Only the Copy C payload is read; ownership of the Rust element stays in the vector.
                values.push(unsafe { *pointer.as_ptr() });
                pointers.push(unsafe { SoftRefMut::<T::CType>::from_raw(pointer.as_ptr()) });
            }
            (values, pointers.into_boxed_slice())
        },
        |values, pointers| {
            for (value, pointer) in values.iter().zip(pointers.iter()) {
                unsafe { *RefMutCarrier::as_ptr(pointer) = *value };
            }
        },
    );
    let header = unsafe { AbiVec::from_raw_parts(wide.as_ptr(), wide.metadata(), capacity) };
    // Header replacement cannot change the original vector's allocation.
    let header = V::CType::from(header);
    store.original = Some(header);
    SoftRefMut::from_mut(store.value.insert(header))
}
macro_rules! define_mutable_encoders {
    ( copy [$($(#[$copy_attr:meta])* [$($copy_params:tt)*] [$($copy_spec:tt)*] $copy_ty:ty;)*]
        owned [$($(#[$owned_attr:meta])* [$($owned_params:tt)*] [$($owned_spec:tt)*] [$($owned_bounds:tt)*] $owned_ty:ty;)*]
        vectors [$($vec:path;)*]
        unchecked {$($unchecked:tt)*}
        checked {$($checked:tt)*}
        [$($(#[$fn_attr:meta])* [($($arg:ident),*)] $fn_type:ty;)*]) => {
        disjoint_impls! {
            /// Encodes a mutable reference, retaining converted storage and writeback state.
            ///
            /// # Safety
            /// The returned carrier borrows `self` or the store. Both must remain valid for
            /// later access. Synchronization must validate changes before restoring Rust
            /// values. Encoding must leave `self` valid even if the store is never synced.
            /// If `SoftStore` implements `EmptyStore`, the carrier must not borrow from it.
            pub unsafe trait SoftEncodeMutUnchecked<'a>: ReprC + 'a {
                type SoftStore: Store + Default;

                /// # Safety
                /// See [`crate::soft_encode_unchecked`].
                unsafe fn soft_encode_mut_unchecked<'itm>(
                    &'a mut self,
                    store: &'itm mut Self::SoftStore,
                ) -> <&'a mut Self as ReprC>::CType
                where
                    &'a mut Self: ReprC<CType: Sized>,
                    'a: 'itm;
            }
            $($unchecked)*
            $(
                $(#[$copy_attr])*
                unsafe impl<'_mut, $($copy_params)*> SoftEncodeMutUnchecked<'_mut> for $copy_ty
                where
                    $($copy_spec)* Self: $crate::marker::AbiCompatible
                        + $crate::rust_spec::RustSpec<Layout = $crate::rust_spec::Specified>
                        + '_mut,
                    &'_mut mut Self: $crate::ReprC
                        + $crate::convert::EncodeMoveUnchecked<SoftStore = ()>,
                    <&'_mut mut Self as $crate::ReprC>::CType: Sized,
                {
                    type SoftStore = ();
                    unsafe fn soft_encode_mut_unchecked<'itm>(
                        &'_mut mut self,
                        store: &'itm mut (),
                    ) -> <&'_mut mut Self as $crate::ReprC>::CType
                    where
                        '_mut: 'itm,
                    {
                        unsafe { encode_direct_mut_unchecked(self, store) }
                    }
                }
                $(#[$copy_attr])*
                unsafe impl<'_mut, $($copy_params)*> SoftEncodeMutUnchecked<'_mut> for $copy_ty
                where
                    $($copy_spec)* Self: $crate::ReprC
                        + $crate::convert::EncodeMoveUnchecked
                        + $crate::rust_spec::RustSpec<Layout = $crate::Unspecified>
                        + $crate::convert::DecodeMove<'_mut, Store: $crate::convert::EmptyStore + '_mut>
                        + Copy
                        + '_mut,
                    &'_mut mut Self:
                        $crate::ReprC<CType = $crate::reference::abi::SoftRefMut<<Self as $crate::ReprC>::CType>>,
                    $($copy_spec)* <Self as $crate::ReprC>::CType: Sized,
                {
                    type SoftStore = $crate::sync::RefMutSizedEncodeStore<
                        '_mut,
                        Self,
                        <Self as $crate::convert::EncodeMoveUnchecked>::SoftStore,
                    >;
                    unsafe fn soft_encode_mut_unchecked<'itm>(
                        &'_mut mut self,
                        store: &'itm mut Self::SoftStore,
                    ) -> <&'_mut mut Self as $crate::ReprC>::CType
                    where
                        '_mut: 'itm,
                    {
                        unsafe { encode_copy_mut_unchecked(self, store) }
                    }
                }
            )*
            $(
                $(#[$owned_attr])*
                unsafe impl<'_mut, $($owned_params)*> SoftEncodeMutUnchecked<'_mut> for $owned_ty
                where
                    $($owned_bounds)*
                    $($owned_spec)* Self: $crate::marker::AbiCompatible
                        + $crate::rust_spec::RustSpec<Layout = $crate::rust_spec::Specified>
                        + '_mut,
                    &'_mut mut Self: $crate::ReprC
                        + $crate::convert::EncodeMoveUnchecked<SoftStore = ()>,
                    <&'_mut mut Self as $crate::ReprC>::CType: Sized,
                {
                    type SoftStore = ();
                    unsafe fn soft_encode_mut_unchecked<'itm>(
                        &'_mut mut self,
                        store: &'itm mut (),
                    ) -> <&'_mut mut Self as $crate::ReprC>::CType
                    where
                        '_mut: 'itm,
                    {
                        unsafe { encode_direct_mut_unchecked(self, store) }
                    }
                }
                $(#[$owned_attr])*
                unsafe impl<'_mut, $($owned_params)*> SoftEncodeMutUnchecked<'_mut> for $owned_ty
                where
                    $($owned_bounds)*
                    $($owned_spec)* Self: $crate::ReprC
                        + $crate::convert::EncodeMoveUnchecked
                        + $crate::rust_spec::RustSpec<Layout = $crate::Unspecified>
                        + $crate::convert::DecodeMove<'_mut, Store: $crate::convert::EmptyStore + '_mut>
                        + Default
                        + '_mut,
                    &'_mut mut Self:
                        $crate::ReprC<CType = $crate::reference::abi::SoftRefMut<<Self as $crate::ReprC>::CType>>,
                    $($owned_spec)* <Self as $crate::ReprC>::CType: Sized,
                {
                    type SoftStore = $crate::sync::RefMutSizedEncodeStore<
                        '_mut,
                        Self,
                        <Self as $crate::convert::EncodeMoveUnchecked>::SoftStore,
                    >;
                    unsafe fn soft_encode_mut_unchecked<'itm>(
                        &'_mut mut self,
                        store: &'itm mut Self::SoftStore,
                    ) -> <&'_mut mut Self as $crate::ReprC>::CType
                    where
                        '_mut: 'itm,
                    {
                        unsafe { encode_take_mut_unchecked(self, store) }
                    }
                }
            )*
            $(
                $(#[$fn_attr])*
                unsafe impl<'_mut, $($arg,)* R> SoftEncodeMutUnchecked<'_mut> for $fn_type
                where
                    Self: $crate::marker::AbiCompatible
                        + $crate::rust_spec::RustSpec<Layout = $crate::rust_spec::Specified>
                        + '_mut,
                    &'_mut mut Self: $crate::ReprC
                        + $crate::convert::EncodeMoveUnchecked<SoftStore = ()>,
                    <&'_mut mut Self as $crate::ReprC>::CType: Sized,
                {
                    type SoftStore = ();
                    unsafe fn soft_encode_mut_unchecked<'itm>(
                        &'_mut mut self,
                        store: &'itm mut (),
                    ) -> <&'_mut mut Self as $crate::ReprC>::CType
                    where
                        '_mut: 'itm,
                    {
                        unsafe { encode_direct_mut_unchecked(self, store) }
                    }
                }
                $(#[$fn_attr])*
                unsafe impl<'_mut, $($arg,)* R> SoftEncodeMutUnchecked<'_mut> for $fn_type
                where
                    Self: $crate::ReprC
                        + $crate::convert::EncodeMoveUnchecked
                        + $crate::rust_spec::RustSpec<Layout = $crate::Unspecified>
                        + $crate::convert::DecodeMove<'_mut, Store: $crate::convert::EmptyStore + '_mut>
                        + Copy
                        + '_mut,
                    &'_mut mut Self:
                        $crate::ReprC<CType = $crate::reference::abi::SoftRefMut<<Self as $crate::ReprC>::CType>>,
                    <Self as $crate::ReprC>::CType: Sized,
                {
                    type SoftStore = $crate::sync::RefMutSizedEncodeStore<
                        '_mut,
                        Self,
                        <Self as $crate::convert::EncodeMoveUnchecked>::SoftStore,
                    >;
                    unsafe fn soft_encode_mut_unchecked<'itm>(
                        &'_mut mut self,
                        store: &'itm mut Self::SoftStore,
                    ) -> <&'_mut mut Self as $crate::ReprC>::CType
                    where
                        '_mut: 'itm,
                    {
                        unsafe { encode_copy_mut_unchecked(self, store) }
                    }
                }
            )*
            $(
                #[cfg(feature = "alloc")]
                #[cfg(feature = "repr-rust")]
                unsafe impl<'a, T: ReprC + 'a> SoftEncodeMutUnchecked<'a>
                    for $vec
                where
                    Self: RustSpec<Layout = Unspecified> + ReprC<CType: CType + Copy + PartialEq + From<AbiVec<<T as ReprC>::CType>>>,
                    &'a mut T: EncodeMoveUnchecked + ReprC<CType: RefMutCarrier<Pointee = <T as ReprC>::CType>>,
                    &'a mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
                    <T as ReprC>::CType: CType + Copy,
                {
                    type SoftStore = RefMutReferenceEncodeStore<
                        <Self as ReprC>::CType,
                        RefMutWideEncodeStore<
                            'a,
                            [T],
                            alloc::boxed::Box<[<&'a mut T as EncodeMoveUnchecked>::SoftStore]>,
                            alloc::boxed::Box<[SoftRefMut<T::CType>]>,
                            alloc::vec::Vec<T::CType>,
                        >,
                    >;

                    unsafe fn soft_encode_mut_unchecked<'itm>(
                        &'a mut self,
                        store: &'itm mut Self::SoftStore,
                    ) -> <&'a mut Self as ReprC>::CType
                    where
                        'a: 'itm,
                    {
                        let capacity = self.capacity();
                        encode_vec_mut::<T, Self, _, _>(self.iter_mut(), capacity, store, |value, fields| unsafe {
                            EncodeMoveUnchecked::soft_encode_unchecked(value, fields)
                        })
                    }
                }
                #[cfg(feature = "alloc")]
                unsafe impl<'a, T: ReprC + 'a> SoftEncodeMutUnchecked<'a>
                    for $vec
                where
                    Self: crate::marker::AbiCompatible + RustSpec<Layout = rust_spec::Specified>,
                    &'a mut Self: ReprC + EncodeMoveUnchecked<SoftStore = ()>,
                    <T as ReprC>::CType: CType + Copy,
                    <&'a mut Self as ReprC>::CType: Sized,
                {
                    type SoftStore = ();
                    unsafe fn soft_encode_mut_unchecked<'itm>(
                        &'a mut self,
                        store: &'itm mut (),
                    ) -> <&'a mut Self as ReprC>::CType
                    where
                        'a: 'itm,
                    {
                        unsafe { encode_direct_mut_unchecked(self, store) }
                    }
                }
            )*
        }
        disjoint_impls! {
            /// Checked encoding of a mutable reference with deferred writeback.
            ///
            /// # Safety
            /// The returned carrier must satisfy the checked mutable-reference contract.
            /// It grants borrowed access, without transferring ownership to the caller.
            /// If `Store` implements `EmptyStore`, the carrier must not borrow from it.
            pub unsafe trait SoftEncodeMut<'a>: SoftEncodeMutUnchecked<'a> {
                type Store: Store + Default;

                fn soft_encode_mut<'itm>(
                    &'a mut self,
                    store: &'itm mut Self::Store,
                ) -> <&'a mut Self as crate::ReprC>::CType
                where
                    &'a mut Self: ReprC<CType: Sized>,
                    'a: 'itm;
            }
            $($checked)*
            $(
                $(#[$copy_attr])*
                unsafe impl<'_mut, $($copy_params)*> SoftEncodeMut<'_mut> for $copy_ty
                where
                    $($copy_spec)* Self: $crate::rust_spec::RustSpec<
                            Layout = $crate::rust_spec::Specified,
                            Trap = $crate::rust_spec::layout::Robust,
                        > + $crate::convert::SoftEncodeMutUnchecked<'_mut>,
                    &'_mut mut Self: $crate::ReprC<CType = $crate::reference::abi::RefMut<'_mut, <Self as $crate::ReprC>::CType>>
                        + $crate::convert::EncodeMove<Store = ()>,
                {
                    type Store = ();
                    fn soft_encode_mut<'itm>(
                        &'_mut mut self,
                        store: &'itm mut (),
                    ) -> <&'_mut mut Self as $crate::ReprC>::CType
                    where
                        '_mut: 'itm,
                    {
                        encode_direct_mut(self, store)
                    }
                }
                $(#[$copy_attr])*
                unsafe impl<'_mut, $($copy_params)*> SoftEncodeMut<'_mut> for $copy_ty
                where
                    Self: $crate::convert::EncodeMove
                        + $crate::convert::DecodeMove<'_mut, Store: $crate::convert::EmptyStore + '_mut>
                        + $crate::convert::SoftEncodeMutUnchecked<'_mut>
                        + Copy
                        + '_mut,
                    &'_mut mut Self:
                        $crate::ReprC<CType = $crate::reference::abi::SoftRefMut<<Self as $crate::ReprC>::CType>>,
                {
                    type Store = $crate::sync::RefMutSizedEncodeStore<
                        '_mut,
                        Self,
                        <Self as $crate::convert::EncodeMove>::Store,
                    >;
                    fn soft_encode_mut<'itm>(
                        &'_mut mut self,
                        store: &'itm mut Self::Store,
                    ) -> <&'_mut mut Self as $crate::ReprC>::CType
                    where
                        '_mut: 'itm,
                    {
                        encode_copy_mut(self, store)
                    }
                }
            )*
            $(
                $(#[$owned_attr])*
                unsafe impl<'_mut, $($owned_params)*> SoftEncodeMut<'_mut> for $owned_ty
                where
                    $($owned_bounds)*
                    $($owned_spec)* Self: $crate::rust_spec::RustSpec<
                            Layout = $crate::rust_spec::Specified,
                            Trap = $crate::rust_spec::layout::Robust,
                        > + $crate::convert::SoftEncodeMutUnchecked<'_mut>,
                    &'_mut mut Self: $crate::ReprC<CType = $crate::reference::abi::RefMut<'_mut, <Self as $crate::ReprC>::CType>>
                        + $crate::convert::EncodeMove<Store = ()>,
                {
                    type Store = ();
                    fn soft_encode_mut<'itm>(
                        &'_mut mut self,
                        store: &'itm mut (),
                    ) -> <&'_mut mut Self as $crate::ReprC>::CType
                    where
                        '_mut: 'itm,
                    {
                        encode_direct_mut(self, store)
                    }
                }
                $(#[$owned_attr])*
                unsafe impl<'_mut, $($owned_params)*> SoftEncodeMut<'_mut> for $owned_ty
                where
                    $($owned_bounds)*
                    Self: $crate::convert::EncodeMove
                        + $crate::convert::DecodeMove<'_mut, Store: $crate::convert::EmptyStore + '_mut>
                        + $crate::convert::SoftEncodeMutUnchecked<'_mut>
                        + Default
                        + '_mut,
                    &'_mut mut Self:
                        $crate::ReprC<CType = $crate::reference::abi::SoftRefMut<<Self as $crate::ReprC>::CType>>,
                {
                    type Store = $crate::sync::RefMutSizedEncodeStore<
                        '_mut,
                        Self,
                        <Self as $crate::convert::EncodeMove>::Store,
                    >;
                    fn soft_encode_mut<'itm>(
                        &'_mut mut self,
                        store: &'itm mut Self::Store,
                    ) -> <&'_mut mut Self as $crate::ReprC>::CType
                    where
                        '_mut: 'itm,
                    {
                        encode_take_mut(self, store)
                    }
                }
            )*
            $(
                $(#[$fn_attr])*
                unsafe impl<'_mut, $($arg,)* R> SoftEncodeMut<'_mut> for $fn_type
                where
                    Self: $crate::rust_spec::RustSpec<
                            Layout = $crate::rust_spec::Specified,
                            Trap = $crate::rust_spec::layout::Robust,
                        > + $crate::convert::SoftEncodeMutUnchecked<'_mut>,
                    &'_mut mut Self: $crate::ReprC<CType = $crate::reference::abi::RefMut<'_mut, <Self as $crate::ReprC>::CType>>
                        + $crate::convert::EncodeMove<Store = ()>,
                {
                    type Store = ();
                    fn soft_encode_mut<'itm>(
                        &'_mut mut self,
                        store: &'itm mut (),
                    ) -> <&'_mut mut Self as $crate::ReprC>::CType
                    where
                        '_mut: 'itm,
                    {
                        encode_direct_mut(self, store)
                    }
                }
                $(#[$fn_attr])*
                unsafe impl<'_mut, $($arg,)* R> SoftEncodeMut<'_mut> for $fn_type
                where
                    Self: $crate::convert::EncodeMove
                        + $crate::convert::DecodeMove<'_mut, Store: $crate::convert::EmptyStore + '_mut>
                        + $crate::convert::SoftEncodeMutUnchecked<'_mut>
                        + Copy
                        + '_mut,
                    &'_mut mut Self:
                        $crate::ReprC<CType = $crate::reference::abi::SoftRefMut<<Self as $crate::ReprC>::CType>>,
                {
                    type Store = $crate::sync::RefMutSizedEncodeStore<
                        '_mut,
                        Self,
                        <Self as $crate::convert::EncodeMove>::Store,
                    >;
                    fn soft_encode_mut<'itm>(
                        &'_mut mut self,
                        store: &'itm mut Self::Store,
                    ) -> <&'_mut mut Self as $crate::ReprC>::CType
                    where
                        '_mut: 'itm,
                    {
                        encode_copy_mut(self, store)
                    }
                }
            )*
            $(#[cfg(feature = "alloc")]
                unsafe impl<'a, T: ReprC + 'a> SoftEncodeMut<'a> for $vec
                where
                    Self: crate::convert::SoftEncodeMutUnchecked<'a> + ReprC<CType: CType + Copy + PartialEq + From<AbiVec<<T as ReprC>::CType>>>,
                    &'a mut T: EncodeMove + ReprC<CType: RefMutCarrier<Pointee = <T as ReprC>::CType>>,
                    &'a mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
                    <T as ReprC>::CType: CType + Copy,
                {
                    type Store = RefMutReferenceEncodeStore<
                        <Self as ReprC>::CType,
                        RefMutWideEncodeStore<
                            'a,
                            [T],
                            alloc::boxed::Box<[<&'a mut T as EncodeMove>::Store]>,
                            alloc::boxed::Box<[SoftRefMut<T::CType>]>,
                            alloc::vec::Vec<T::CType>,
                        >,
                    >;

                    fn soft_encode_mut<'itm>(
                        &'a mut self,
                        store: &'itm mut Self::Store,
                    ) -> <&'a mut Self as crate::ReprC>::CType
                    where
                        'a: 'itm,
                    {
                        let capacity = self.capacity();
                        encode_vec_mut::<T, Self, _, _>(self.iter_mut(), capacity, store, |value, fields| {
                            EncodeMove::soft_encode(value, fields)
                        })
                    }
                })*
        }
    };
}
crate::primitive::with_fn_types! { define_mutable_encoders, {
    copy [
        ['r, T: ?Sized] [] &'r T;
        [R: Copy] [] Option<R>;
        [T: Copy, E: Copy] [] Result<T, E>;
        [A: Copy] [] (A,);
        [A: Copy, B: Copy] [] (A, B,);
        [A: Copy, B: Copy, C: Copy] [] (A, B, C,);
        [A: Copy, B: Copy, C: Copy, D: Copy] [] (A, B, C, D,);
        [A: Copy, B: Copy, C: Copy, D: Copy, E: Copy] [] (A, B, C, D, E,);
        [A: Copy, B: Copy, C: Copy, D: Copy, E: Copy, F: Copy] [] (A, B, C, D, E, F,);
        [A: Copy, B: Copy, C: Copy, D: Copy, E: Copy, F: Copy, G: Copy] [] (A, B, C, D, E, F, G,);
        [A: Copy, B: Copy, C: Copy, D: Copy, E: Copy, F: Copy, G: Copy, H: Copy] [] (A, B, C, D, E, F, G, H,);
        [A: Copy, B: Copy, C: Copy, D: Copy, E: Copy, F: Copy, G: Copy, H: Copy, I: Copy] [] (A, B, C, D, E, F, G, H, I,);
        [A: Copy, B: Copy, C: Copy, D: Copy, E: Copy, F: Copy, G: Copy, H: Copy, I: Copy, J: Copy] [] (A, B, C, D, E, F, G, H, I, J,);
        [A: Copy, B: Copy, C: Copy, D: Copy, E: Copy, F: Copy, G: Copy, H: Copy, I: Copy, J: Copy, K: Copy] [] (A, B, C, D, E, F, G, H, I, J, K,);
        [A: Copy, B: Copy, C: Copy, D: Copy, E: Copy, F: Copy, G: Copy, H: Copy, I: Copy, J: Copy, K: Copy, L: Copy] [] (A, B, C, D, E, F, G, H, I, J, K, L,);
        [T: CType + Copy + RustSpec, E: CType + Copy + RustSpec] [] crate::result::abi::Result<T, E>;
        [T: Copy, E: Copy] [] crate::result::Result<T, E>;
        [] [for<'_dummy>] bool;
        [] [for<'_dummy>] char;
        [] [for<'_dummy>] core::cmp::Ordering;
        [] [for<'_dummy>] core::num::NonZero<u8>;
        [] [for<'_dummy>] core::num::NonZero<i8>;
        [] [for<'_dummy>] core::num::NonZero<u16>;
        [] [for<'_dummy>] core::num::NonZero<i16>;
        [] [for<'_dummy>] core::num::NonZero<u32>;
        [] [for<'_dummy>] core::num::NonZero<i32>;
        [] [for<'_dummy>] core::num::NonZero<u64>;
        [] [for<'_dummy>] core::num::NonZero<i64>;
        [] [for<'_dummy>] core::num::NonZero<u128>;
        [] [for<'_dummy>] core::num::NonZero<i128>;
        [] [for<'_dummy>] core::num::NonZero<usize>;
        [] [for<'_dummy>] core::num::NonZero<isize>;
    ]
    owned [
        #[cfg(feature = "alloc")]
        [T] [] [T: ReprC<CType: Sized>, [T]: RustSpec<Size = rust_spec::size::MetaSized<rust_spec::size::SliceLike>>,] alloc::boxed::Box<[T]>;
        #[cfg(feature = "alloc")]
        [T: Default, K: rust_spec::size::SizedKind] [] [T: ReprC<CType: Sized> + RustSpec<Size = rust_spec::size::Sized<K>>,] alloc::boxed::Box<T>;
        #[cfg(feature = "alloc")]
        #[cfg(feature = "repr-rust")]
        [] [for<'_dummy>] [str: RustSpec<Size = rust_spec::size::MetaSized<rust_spec::size::SliceLike>>,] alloc::boxed::Box<str>;
        #[cfg(feature = "alloc")]
        #[cfg(feature = "repr-rust")]
        [] [for<'_dummy>] [core::ffi::CStr: RustSpec<Size = rust_spec::size::NulTerminated>,] alloc::boxed::Box<core::ffi::CStr>;
        [T: CType] [] [ Self: Default,] crate::option::abi::Option<T>;
        [T] [] [] StableOption<T>;

        #[cfg(feature = "alloc")]
        #[cfg(feature = "repr-rust")]
        [] [for<'_dummy>] [] String;
        #[cfg(feature = "alloc")]
        [] [for<'_dummy>] [] StableString;
        #[cfg(feature = "alloc")]
        #[cfg(feature = "repr-rust")]
        [] [for<'_dummy>] [] alloc::ffi::CString;
    ]
    vectors [Vec<T>; StableVec<T>;]
    unchecked {
        #[cfg(feature = "repr-rust")]
        unsafe impl<
            'a,
            T: crate::marker::AbiCompatible<CType: CType + Sized>
                + rust_spec::RustSpec<Layout = rust_spec::Specified, Drop: rust_spec::drop::DropKind>
                + 'a,
        > SoftEncodeMutUnchecked<'a> for [T]
        where
            &'a mut Self: ReprC + EncodeMoveUnchecked<SoftStore = ()>,
            <&'a mut Self as ReprC>::CType: Sized,
        {
            type SoftStore = ();
            unsafe fn soft_encode_mut_unchecked<'itm>(
                &'a mut self,
                store: &'itm mut (),
            ) -> <&'a mut Self as ReprC>::CType
            where
                'a: 'itm,
            {
                unsafe { EncodeMoveUnchecked::soft_encode_unchecked(self, store) }
            }
        }
        #[cfg(feature = "alloc")]
        #[cfg(feature = "repr-rust")]
        unsafe impl<'a, T: ReprC + 'a> SoftEncodeMutUnchecked<'a> for [T]
        where
            T: rust_spec::RustSpec<Layout = Unspecified, Trap = rust_spec::layout::NonRobust>,
            &'a mut T: EncodeMoveUnchecked + ReprC<CType: RefMutCarrier<Pointee = <T as ReprC>::CType>>,
            &'a mut Self: ReprC<CType = AbiSoftWideRefMut<<Self as ReprC>::CType>>,
            <T as ReprC>::CType: CType + Copy,
        {
            type SoftStore = RefMutWideEncodeStore<
                'a,
                [T],
                alloc::boxed::Box<[<&'a mut T as EncodeMoveUnchecked>::SoftStore]>,
                alloc::boxed::Box<[SoftRefMut<T::CType>]>,
            >;

            unsafe fn soft_encode_mut_unchecked<'itm>(
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
                            pointers.push(unsafe { SoftRefMut::<T::CType>::from_raw(pointer.as_ptr()) });
                        }
                        (values.into_boxed_slice(), pointers.into_boxed_slice())
                    },
                    |values, pointers| {
                        for (value, pointer) in values.iter().zip(pointers.iter()) {
                            unsafe { *RefMutCarrier::as_ptr(pointer) = *value };
                        }
                    },
                )
            }
        }
        #[cfg(feature = "alloc")]
        #[cfg(feature = "repr-rust")]
        unsafe impl<'a, T: ReprC + 'a> SoftEncodeMutUnchecked<'a> for [T]
        where
            T: rust_spec::RustSpec<Layout = Unspecified, Trap = rust_spec::layout::Robust>,
            &'a mut T: EncodeMoveUnchecked + ReprC<CType: RefMutCarrier<Pointee = <T as ReprC>::CType>>,
            &'a mut Self: ReprC<CType = AbiSoftWideRefMut<<Self as ReprC>::CType>>,
            <T as ReprC>::CType: CType + Copy,
        {
            type SoftStore = RefMutWideEncodeStore<
                'a,
                [T],
                alloc::boxed::Box<[<&'a mut T as EncodeMoveUnchecked>::SoftStore]>,
                alloc::boxed::Box<[SoftRefMut<T::CType>]>,
            >;

            unsafe fn soft_encode_mut_unchecked<'itm>(
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
                            pointers.push(unsafe { SoftRefMut::<T::CType>::from_raw(pointer.as_ptr()) });
                        }
                        (values.into_boxed_slice(), pointers.into_boxed_slice())
                    },
                    |values, pointers| {
                        for (value, pointer) in values.iter().zip(pointers.iter()) {
                            unsafe { *RefMutCarrier::as_ptr(pointer) = *value };
                        }
                    },
                )
            }
        }

        #[cfg(feature = "alloc")]
        unsafe impl<
            'a,
            'b: 'a,
            R: ReprC
                + RustSpec<
                    Layout = rust_spec::Specified,
                    Trap = rust_spec::layout::Robust,
                    Size = rust_spec::size::Sized<K>,
                >,
            K: SizedKind,
        > SoftEncodeMutUnchecked<'a> for &'b mut R
        where
            Self: RustSpec<Size: crate::Thin>,
            &'a mut Self: ReprC<CType = SoftRefMut<crate::reference::abi::RefMut<'b, <R as ReprC>::CType>>> + EncodeMoveUnchecked<SoftStore = ()>,
        {
            type SoftStore = ();

            unsafe fn soft_encode_mut_unchecked<'itm>(
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
        unsafe impl<
            'a,
            'b: 'a,
            R: ReprC
                + RustSpec<
                    Layout = rust_spec::Specified,
                    Trap = rust_spec::layout::NonRobust,
                    Size = rust_spec::size::Sized<K>,
                >,
            K: SizedKind,
        > SoftEncodeMutUnchecked<'a> for &'b mut R
        where
            Self: RustSpec<Size: crate::Thin>,
            &'a mut Self: ReprC<CType = SoftRefMut<SoftRefMut<<R as ReprC>::CType>>> + EncodeMoveUnchecked<SoftStore = ()>,
        {
            type SoftStore = ();

            unsafe fn soft_encode_mut_unchecked<'itm>(
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
        #[cfg(feature = "repr-rust")]
        unsafe impl<
            'a,
            'b: 'a,
            R: ReprC + RustSpec<Layout = Unspecified, Size = rust_spec::size::Sized<K>>,
            K: SizedKind,
        > SoftEncodeMutUnchecked<'a> for &'b mut R
        where
            R: SoftEncodeMutUnchecked<'a>,
            Self: RustSpec<Size: crate::Thin>,
            &'a mut Self: ReprC<CType = SoftRefMut<SoftRefMut<<R as ReprC>::CType>>>,
        {
            type SoftStore = RefMutReferenceEncodeStore<SoftRefMut<R::CType>, R::SoftStore>;

            unsafe fn soft_encode_mut_unchecked<'itm>(
                &'a mut self,
                store: &'itm mut Self::SoftStore,
            ) -> <&'a mut Self as ReprC>::CType
            where
                'a: 'itm,
            {
                let value = unsafe { R::soft_encode_mut_unchecked(&mut **self, &mut store.inner) };
                let value = unsafe { SoftRefMut::<R::CType>::from_raw(RefMutCarrier::as_ptr(&value)) };
                store.original = Some(value);
                // Both inner carriers have the same raw pointer ABI. The outer carrier is soft.
                unsafe { SoftRefMut::from_raw(core::ptr::from_mut(store.value.insert(value)).cast()) }
            }
        }
        #[cfg(feature = "alloc")]
        #[cfg(feature = "repr-rust")]
        unsafe impl<'a, T: ReprC + 'a, const N: usize>
            SoftEncodeMutUnchecked<'a> for [T; N]
        where
            Self: RustSpec<Layout = Unspecified>,
            &'a mut T: EncodeMoveUnchecked + ReprC<CType: RefMutCarrier<Pointee = <T as ReprC>::CType>>,
            &'a mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
            <T as ReprC>::CType: CType + Copy,
        {
            type SoftStore = RefMutWideEncodeStore<
                'a,
                [T],
                alloc::boxed::Box<[<&'a mut T as EncodeMoveUnchecked>::SoftStore]>,
                alloc::boxed::Box<[SoftRefMut<T::CType>]>,
            >;

            unsafe fn soft_encode_mut_unchecked<'itm>(
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
                            pointers.push(unsafe { SoftRefMut::<T::CType>::from_raw(pointer.as_ptr()) });
                        }
                        (values.into_boxed_slice(), pointers.into_boxed_slice())
                    },
                    |values, pointers| {
                        for (value, pointer) in values.iter().zip(pointers.iter()) {
                            unsafe { *RefMutCarrier::as_ptr(pointer) = *value };
                        }
                    },
                );
                unsafe { SoftRefMut::from_raw(wide.as_ptr().cast::<[<T as ReprC>::CType; N]>()) }
            }
        }

        unsafe impl<'a, T, const N: usize> SoftEncodeMutUnchecked<'a> for [T; N]
        where
            Self: crate::marker::AbiCompatible + RustSpec<Layout = rust_spec::Specified> + 'a,
            &'a mut Self: ReprC + EncodeMoveUnchecked<SoftStore = ()>,
            <&'a mut Self as ReprC>::CType: Sized,
        {
            type SoftStore = ();
            unsafe fn soft_encode_mut_unchecked<'itm>(
                &'a mut self,
                store: &'itm mut (),
            ) -> <&'a mut Self as ReprC>::CType
            where
                'a: 'itm,
            {
                unsafe { EncodeMoveUnchecked::soft_encode_unchecked(self, store) }
            }
        }
    }
    checked {
        #[cfg(feature = "repr-rust")]
        unsafe impl<
            'a,
            T: AbiCompatibleValidate
                + rust_spec::RustSpec<
                    Layout = rust_spec::Specified,
                    Trap = rust_spec::layout::Robust,
                    Drop: rust_spec::drop::DropKind,
                > + 'a,
        > SoftEncodeMut<'a> for [T]
        where
            &'a mut Self: ReprC<CType = crate::wide::abi::WideRefMut<'a, <Self as ReprC>::CType>>
                + EncodeMoveUnchecked<SoftStore = ()>
                + EncodeMove<Store = ()>,
            <T as ReprC>::CType: Sized,
        {
            type Store = ();
            fn soft_encode_mut<'itm>(
                &'a mut self,
                store: &'itm mut (),
            ) -> <&'a mut Self as crate::ReprC>::CType
            where
                'a: 'itm,
            {
                let encoded = crate::convert::EncodeMove::soft_encode(self, store);
                debug_assert!(
                    unsafe {
                        <Self>::is_valid(core::slice::from_raw_parts(
                            encoded.as_ptr(),
                            encoded.metadata(),
                        ))
                    },
                    "encoding failed AbiCompatibleValidate::is_valid"
                );
                encoded
            }
        }
        #[cfg(feature = "alloc")]
        #[cfg(feature = "repr-rust")]
        unsafe impl<'a, T: ReprC + 'a> SoftEncodeMut<'a> for [T]
        where
            T: rust_spec::RustSpec<Trap = rust_spec::layout::NonRobust>,
            Self: crate::convert::SoftEncodeMutUnchecked<'a> + ReprC<CType = [<T as ReprC>::CType]>,
            &'a mut T: EncodeMove + ReprC<CType: RefMutCarrier<Pointee = <T as ReprC>::CType>>,
            &'a mut Self: ReprC<CType = AbiSoftWideRefMut<<Self as ReprC>::CType>>,
            <T as ReprC>::CType: CType + Copy,
        {
            type Store = RefMutWideEncodeStore<
                'a,
                [T],
                alloc::boxed::Box<[<&'a mut T as EncodeMove>::Store]>,
                alloc::boxed::Box<[SoftRefMut<T::CType>]>,
            >;

            fn soft_encode_mut<'itm>(
                &'a mut self,
                store: &'itm mut Self::Store,
            ) -> <&'a mut Self as crate::ReprC>::CType
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
                            let pointer = crate::convert::EncodeMove::soft_encode(value, field_store);
                            values.push(unsafe { *pointer.as_ptr() });
                            pointers.push(unsafe { SoftRefMut::<T::CType>::from_raw(pointer.as_ptr()) });
                        }
                        (values.into_boxed_slice(), pointers.into_boxed_slice())
                    },
                    |values, pointers| {
                        for (value, pointer) in values.iter().zip(pointers.iter()) {
                            unsafe { *RefMutCarrier::as_ptr(pointer) = *value };
                        }
                    },
                )
            }
        }
        #[cfg(feature = "alloc")]
        #[cfg(feature = "repr-rust")]
        unsafe impl<'a, T: ReprC + 'a> SoftEncodeMut<'a> for [T]
        where
            T: rust_spec::RustSpec<Layout = Unspecified, Trap = rust_spec::layout::Robust>,
            &'a mut T: EncodeMove + ReprC<CType: RefMutCarrier<Pointee = <T as ReprC>::CType>>,
            &'a mut Self: ReprC<CType = AbiSoftWideRefMut<<Self as ReprC>::CType>>,
            <T as ReprC>::CType: CType + Copy,
        {
            type Store = RefMutWideEncodeStore<
                'a,
                [T],
                alloc::boxed::Box<[<&'a mut T as EncodeMove>::Store]>,
                alloc::boxed::Box<[SoftRefMut<T::CType>]>,
            >;

            fn soft_encode_mut<'itm>(
                &'a mut self,
                store: &'itm mut Self::Store,
            ) -> <&'a mut Self as crate::ReprC>::CType
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
                            let pointer = crate::convert::EncodeMove::soft_encode(value, field_store);
                            values.push(unsafe { *pointer.as_ptr() });
                            pointers.push(unsafe { SoftRefMut::<T::CType>::from_raw(pointer.as_ptr()) });
                        }
                        (values.into_boxed_slice(), pointers.into_boxed_slice())
                    },
                    |values, pointers| {
                        for (value, pointer) in values.iter().zip(pointers.iter()) {
                            unsafe { *RefMutCarrier::as_ptr(pointer) = *value };
                        }
                    },
                )
            }
        }

        #[cfg(feature = "alloc")]
        unsafe impl<
            'a,
            'b: 'a,
            R: ReprC
                + RustSpec<
                    Layout = rust_spec::Specified,
                    Trap = rust_spec::layout::Robust,
                    Size = rust_spec::size::Sized<K>,
                >,
            K: SizedKind,
        > SoftEncodeMut<'a> for &'b mut R
        where
            R: SoftEncodeMut<'a>,
            Self: RustSpec<Size: crate::Thin>,
            &'a mut Self: ReprC<CType = SoftRefMut<crate::reference::abi::RefMut<'b, <R as ReprC>::CType>>> + EncodeMoveUnchecked<SoftStore = ()>,
        {
            type Store = RefMutReferenceEncodeStore<SoftRefMut<R::CType>, R::Store>;

            fn soft_encode_mut<'itm>(
                &'a mut self,
                store: &'itm mut Self::Store,
            ) -> <&'a mut Self as crate::ReprC>::CType
            where
                'a: 'itm,
            {
                let value = R::soft_encode_mut(&mut **self, &mut store.inner);
                let value = unsafe { SoftRefMut::<R::CType>::from_raw(RefMutCarrier::as_ptr(&value)) };
                store.original = Some(value);
                // Both inner carriers have the same raw pointer ABI. The outer carrier is soft.
                unsafe { SoftRefMut::from_raw(core::ptr::from_mut(store.value.insert(value)).cast()) }
            }
        }
        #[cfg(feature = "alloc")]
        unsafe impl<
            'a,
            'b: 'a,
            R: ReprC
                + RustSpec<
                    Layout = rust_spec::Specified,
                    Trap = rust_spec::layout::NonRobust,
                    Size = rust_spec::size::Sized<K>,
                >,
            K: SizedKind,
        > SoftEncodeMut<'a> for &'b mut R
        where
            R: SoftEncodeMut<'a>,
            Self: RustSpec<Size: crate::Thin>,
            &'a mut Self: ReprC<CType = SoftRefMut<SoftRefMut<<R as ReprC>::CType>>> + EncodeMoveUnchecked<SoftStore = ()>,
        {
            type Store = RefMutReferenceEncodeStore<SoftRefMut<R::CType>, R::Store>;

            fn soft_encode_mut<'itm>(
                &'a mut self,
                store: &'itm mut Self::Store,
            ) -> <&'a mut Self as crate::ReprC>::CType
            where
                'a: 'itm,
            {
                let value = R::soft_encode_mut(&mut **self, &mut store.inner);
                let value = unsafe { SoftRefMut::<R::CType>::from_raw(RefMutCarrier::as_ptr(&value)) };
                store.original = Some(value);
                // Both inner carriers have the same raw pointer ABI. The outer carrier is soft.
                unsafe { SoftRefMut::from_raw(core::ptr::from_mut(store.value.insert(value)).cast()) }
            }
        }
        #[cfg(feature = "alloc")]
        #[cfg(feature = "repr-rust")]
        unsafe impl<
            'a,
            'b: 'a,
            R: ReprC + RustSpec<Layout = Unspecified, Size = rust_spec::size::Sized<K>>,
            K: SizedKind,
        > SoftEncodeMut<'a> for &'b mut R
        where
            R: SoftEncodeMut<'a>,
            Self: RustSpec<Size: crate::Thin>,
            &'a mut Self: ReprC<CType = SoftRefMut<SoftRefMut<<R as ReprC>::CType>>>,
        {
            type Store = RefMutReferenceEncodeStore<SoftRefMut<R::CType>, R::Store>;

            fn soft_encode_mut<'itm>(
                &'a mut self,
                store: &'itm mut Self::Store,
            ) -> <&'a mut Self as crate::ReprC>::CType
            where
                'a: 'itm,
            {
                let value = R::soft_encode_mut(&mut **self, &mut store.inner);
                let value = unsafe { SoftRefMut::<R::CType>::from_raw(RefMutCarrier::as_ptr(&value)) };
                store.original = Some(value);
                // Both inner carriers have the same raw pointer ABI. The outer carrier is soft.
                unsafe { SoftRefMut::from_raw(core::ptr::from_mut(store.value.insert(value)).cast()) }
            }
        }
        #[cfg(feature = "alloc")]
        #[cfg(feature = "repr-rust")]
        unsafe impl<'a, T: ReprC + 'a, const N: usize> SoftEncodeMut<'a>
            for [T; N]
        where
            Self: RustSpec<Layout = Unspecified>,
            &'a mut T: EncodeMove + ReprC<CType: RefMutCarrier<Pointee = <T as ReprC>::CType>>,
            &'a mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
            <T as ReprC>::CType: CType + Copy,
        {
            type Store = RefMutWideEncodeStore<
                'a,
                [T],
                alloc::boxed::Box<[<&'a mut T as EncodeMove>::Store]>,
                alloc::boxed::Box<[SoftRefMut<T::CType>]>,
            >;

            fn soft_encode_mut<'itm>(
                &'a mut self,
                store: &'itm mut Self::Store,
            ) -> <&'a mut Self as crate::ReprC>::CType
            where
                'a: 'itm,
            {
                let wide = store.encode(
                    |stores| {
                        *stores = core::iter::repeat_with(Default::default).take(N).collect();
                        let mut values = alloc::vec::Vec::with_capacity(N);
                        let mut pointers = alloc::vec::Vec::with_capacity(N);
                        for (value, field_store) in self.iter_mut().zip(stores.iter_mut()) {
                            let pointer = crate::convert::EncodeMove::soft_encode(value, field_store);
                            values.push(unsafe { *pointer.as_ptr() });
                            pointers.push(unsafe { SoftRefMut::<T::CType>::from_raw(pointer.as_ptr()) });
                        }
                        (values.into_boxed_slice(), pointers.into_boxed_slice())
                    },
                    |values, pointers| {
                        for (value, pointer) in values.iter().zip(pointers.iter()) {
                            unsafe { *RefMutCarrier::as_ptr(pointer) = *value };
                        }
                    },
                );
                unsafe { SoftRefMut::from_raw(wide.as_ptr().cast::<[<T as ReprC>::CType; N]>()) }
            }
        }
        #[cfg(feature = "alloc")]
        unsafe impl<'a, T: ReprC + 'a, const N: usize> SoftEncodeMut<'a>
            for [T; N]
        where
            Self: RustSpec<Layout = rust_spec::Specified, Trap = rust_spec::layout::NonRobust>
                + SoftEncodeMutUnchecked<'a>
                + ReprC<CType = [<T as ReprC>::CType; N]>,
            &'a mut T: EncodeMove + ReprC<CType: RefMutCarrier<Pointee = <T as ReprC>::CType>>,
            &'a mut Self: ReprC<CType = SoftRefMut<<Self as ReprC>::CType>>,
            <T as ReprC>::CType: CType + Copy,
        {
            type Store = RefMutWideEncodeStore<
                'a,
                [T],
                alloc::boxed::Box<[<&'a mut T as EncodeMove>::Store]>,
                alloc::boxed::Box<[SoftRefMut<T::CType>]>,
            >;

            fn soft_encode_mut<'itm>(
                &'a mut self,
                store: &'itm mut Self::Store,
            ) -> <&'a mut Self as crate::ReprC>::CType
            where
                'a: 'itm,
            {
                let wide = store.encode(
                    |stores| {
                        *stores = core::iter::repeat_with(Default::default).take(N).collect();
                        let mut values = alloc::vec::Vec::with_capacity(N);
                        let mut pointers = alloc::vec::Vec::with_capacity(N);
                        for (value, field_store) in self.iter_mut().zip(stores.iter_mut()) {
                            let pointer = crate::convert::EncodeMove::soft_encode(value, field_store);
                            values.push(unsafe { *pointer.as_ptr() });
                            pointers.push(unsafe { SoftRefMut::<T::CType>::from_raw(pointer.as_ptr()) });
                        }
                        (values.into_boxed_slice(), pointers.into_boxed_slice())
                    },
                    |values, pointers| {
                        for (value, pointer) in values.iter().zip(pointers.iter()) {
                            unsafe { *RefMutCarrier::as_ptr(pointer) = *value };
                        }
                    },
                );
                unsafe { SoftRefMut::from_raw(wide.as_ptr().cast::<[<T as ReprC>::CType; N]>()) }
            }
        }
        unsafe impl<'a, T, const N: usize> SoftEncodeMut<'a> for [T; N]
        where
            Self: crate::convert::AbiCompatibleValidate
                + RustSpec<Layout = rust_spec::Specified, Trap = rust_spec::layout::Robust>
                + 'a,
            &'a mut Self: ReprC<CType = crate::reference::abi::RefMut<'a, <Self as ReprC>::CType>>
                + EncodeMove<Store = ()>
                + EncodeMoveUnchecked<SoftStore = ()>,
        {
            type Store = ();
            fn soft_encode_mut<'itm>(
                &'a mut self,
                store: &'itm mut (),
            ) -> <&'a mut Self as crate::ReprC>::CType
            where
                'a: 'itm,
            {
                EncodeMove::soft_encode(self, store)
            }
        }
    }
} }

macro_rules! impl_direct_mut {
    ([$($params:tt)*] $ty:ty) => {
        unsafe impl<'_mut, $($params)*> $crate::convert::SoftEncodeMutUnchecked<'_mut>
            for $ty
        where
            Self: $crate::marker::AbiCompatible
                + $crate::rust_spec::RustSpec<
                    Layout = $crate::rust_spec::Specified,
                    Trap = $crate::rust_spec::layout::Robust,
                > + '_mut,
            &'_mut mut Self: $crate::ReprC
                + $crate::marker::AbiCompatible
                + $crate::convert::EncodeMoveUnchecked<SoftStore = ()>,
            <&'_mut mut Self as $crate::ReprC>::CType: Sized,
        {
            type SoftStore = ();
            unsafe fn soft_encode_mut_unchecked<'itm>(
                &'_mut mut self,
                store: &'itm mut (),
            ) -> <&'_mut mut Self as $crate::ReprC>::CType
            where
                '_mut: 'itm,
            {
                let encoded = unsafe {
                    $crate::convert::EncodeMoveUnchecked::soft_encode_unchecked(self, store)
                };
                encoded
            }
        }
        unsafe impl<'_mut, $($params)*> $crate::convert::SoftEncodeMut<'_mut> for $ty
        where
            Self: $crate::convert::AbiCompatibleValidate
                + $crate::rust_spec::RustSpec<
                    Layout = $crate::rust_spec::Specified,
                    Trap = $crate::rust_spec::layout::Robust,
                > + '_mut,
            &'_mut mut Self: $crate::ReprC
                + $crate::convert::AbiCompatibleValidate
                + $crate::convert::EncodeMoveUnchecked<SoftStore = ()>
                + $crate::convert::EncodeMove<Store = ()>,
            <&'_mut mut Self as $crate::ReprC>::CType: Sized,
        {
            type Store = ();
            fn soft_encode_mut<'itm>(
                &'_mut mut self,
                store: &'itm mut (),
            ) -> <&'_mut mut Self as $crate::ReprC>::CType
            where
                '_mut: 'itm,
            {
                let encoded = $crate::convert::EncodeMove::soft_encode(self, store);
                debug_assert!(
                    unsafe {
                        <&'_mut mut Self as $crate::convert::AbiCompatibleValidate>::is_valid(
                            &encoded,
                        )
                    },
                    "encoding failed AbiCompatibleValidate::is_valid"
                );
                encoded
            }
        }
    };
}

unsafe impl<'a, R: ReprC + 'a + ?Sized> SoftEncodeMutUnchecked<'a> for ManuallyDrop<R>
where
    Self: ReprC + 'a,
    &'a mut Self: ReprC,
    &'a mut R: EncodeMoveUnchecked + ReprC,
    <&'a mut Self as ReprC>::CType: Copy,
    <&'a mut R as ReprC>::CType: Copy,
{
    type SoftStore = <&'a mut R as EncodeMoveUnchecked>::SoftStore;
    unsafe fn soft_encode_mut_unchecked<'itm>(
        &'a mut self,
        store: &'itm mut Self::SoftStore,
    ) -> <&'a mut Self as ReprC>::CType
    where
        'a: 'itm,
    {
        let inner = unsafe { EncodeMoveUnchecked::soft_encode_unchecked(&mut **self, store) };
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

unsafe impl<'a, R: ReprC + 'a + ?Sized> SoftEncodeMut<'a> for ManuallyDrop<R>
where
    Self: ReprC + 'a,
    &'a mut Self: ReprC,
    &'a mut R: EncodeMove + ReprC,
    <&'a mut Self as ReprC>::CType: Copy,
    <&'a mut R as ReprC>::CType: Copy,
{
    type Store = <&'a mut R as EncodeMove>::Store;
    fn soft_encode_mut<'itm>(
        &'a mut self,
        store: &'itm mut Self::Store,
    ) -> <&'a mut Self as crate::ReprC>::CType
    where
        'a: 'itm,
    {
        let inner = EncodeMove::soft_encode(&mut **self, store);
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
        unsafe impl<'a, R: ReprC + 'a + ?Sized> SoftEncodeMutUnchecked<'a> for $wrapper<R>
        where
            Self: ReprC + 'a,
            &'a mut Self: ReprC,
            &'a mut R: EncodeMoveUnchecked + ReprC,
            <&'a mut Self as ReprC>::CType: Copy,
            <&'a mut R as ReprC>::CType: Copy,
        {
            type SoftStore = <&'a mut R as EncodeMoveUnchecked>::SoftStore;
            unsafe fn soft_encode_mut_unchecked<'itm>(
                &'a mut self,
                store: &'itm mut Self::SoftStore,
            ) -> <&'a mut Self as ReprC>::CType
            where
                'a: 'itm,
            {
                let inner =
                    unsafe { EncodeMoveUnchecked::soft_encode_unchecked($field(self), store) };
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
        unsafe impl<'a, R: ReprC + 'a + ?Sized> SoftEncodeMut<'a> for $wrapper<R>
        where
            Self: ReprC + 'a,
            &'a mut Self: ReprC,
            &'a mut R: EncodeMove + ReprC,
            <&'a mut Self as ReprC>::CType: Copy,
            <&'a mut R as ReprC>::CType: Copy,
        {
            type Store = <&'a mut R as EncodeMove>::Store;
            fn soft_encode_mut<'itm>(
                &'a mut self,
                store: &'itm mut Self::Store,
            ) -> <&'a mut Self as crate::ReprC>::CType
            where
                'a: 'itm,
            {
                let inner = EncodeMove::soft_encode($field(self), store);
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

impl_direct_mut!([T] core::mem::MaybeUninit<T>);
impl_direct_mut!(['r, C: CType + ?Sized] crate::reference::abi::Ref<'r, C>);
impl_direct_mut!(['r, C: CType + ?Sized] crate::reference::abi::RefMut<'r, C>);
impl_direct_mut!(['r, P: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized] crate::wide::abi::WideRef<'r, P>);
impl_direct_mut!(['r, P: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized] crate::wide::abi::WideRefMut<'r, P>);
impl_direct_mut!([C: CType + ?Sized] SoftRef<C>);
impl_direct_mut!([C: CType + ?Sized] SoftRefMut<C>);
impl_direct_mut!([P: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized] AbiSoftWideRef<P>);
impl_direct_mut!([P: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized] AbiSoftWideRefMut<P>);
#[cfg(feature = "alloc")]
impl_direct_mut!([T: CType] AbiVec<T>);
#[cfg(feature = "alloc")]
impl_direct_mut!([C: CType] AbiBox<C>);
#[cfg(feature = "alloc")]
impl_direct_mut!([P: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized] AbiWideBox<P>);

// Keep the original vector and its elements alive: decoding an owned carrier
// before synchronizing element stores can restore references into freed storage.

#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
unsafe impl<'a> SoftEncodeMutUnchecked<'a> for str {
    type SoftStore = ();

    unsafe fn soft_encode_mut_unchecked<'itm>(
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
#[cfg(feature = "repr-rust")]
unsafe impl<'a> SoftEncodeMut<'a> for str {
    type Store = RefMutDstEncodeStore<'a, str, ()>;

    fn soft_encode_mut<'itm>(
        &'a mut self,
        store: &'itm mut Self::Store,
    ) -> <&'a mut Self as crate::ReprC>::CType
    where
        'a: 'itm,
    {
        let carrier = store.encode(self, |original, fields| {
            alloc::borrow::ToOwned::to_owned(&*original).soft_encode(fields)
        });
        unsafe { AbiSoftWideRefMut::from_raw_parts_mut(carrier.as_ptr(), carrier.len()) }
    }
}

pub(crate) use impl_direct_mut;

#[cfg(all(test, feature = "alloc", feature = "derive"))]
mod tests {
    use alloc::vec::Vec;

    use co3::{
        ReprC,
        convert::{SoftEncodeMut, SoftEncodeMutUnchecked, Store},
        rust_spec::RustSpec,
    };

    // Explicit reference encoders do not require an owned decoder or value helper.
    #[derive(RustSpec)]
    struct Explicit(u32);
    impl ReprC for Explicit {
        type CType = u32;
    }
    unsafe impl<'a> SoftEncodeMutUnchecked<'a> for Explicit {
        type SoftStore = ();
        unsafe fn soft_encode_mut_unchecked<'itm>(
            &'a mut self,
            _: &'itm mut (),
        ) -> <&'a mut Self as ReprC>::CType
        where
            'a: 'itm,
        {
            co3::reference::abi::SoftRefMut::from_mut(&mut self.0)
        }
    }
    unsafe impl<'a> SoftEncodeMut<'a> for Explicit {
        type Store = ();
        fn soft_encode_mut<'itm>(
            &'a mut self,
            _: &'itm mut (),
        ) -> <&'a mut Self as co3::ReprC>::CType
        where
            'a: 'itm,
        {
            co3::reference::abi::SoftRefMut::from_mut(&mut self.0)
        }
    }

    #[test]
    fn transparent_mutable_encoders_dispatch_stable_inner() {
        use core::{
            cell::{Cell, UnsafeCell},
            mem::ManuallyDrop,
        };

        use co3::convert::{SoftEncodeMut, SoftEncodeMutUnchecked};

        let mut manual = ManuallyDrop::new(7_u32);
        let carrier = SoftEncodeMut::soft_encode_mut(&mut manual, &mut ());
        unsafe { *carrier.as_ptr().cast::<u32>() = 11 };
        assert_eq!(*manual, 11);
        let carrier =
            unsafe { SoftEncodeMutUnchecked::soft_encode_mut_unchecked(&mut manual, &mut ()) };
        unsafe { *carrier.as_ptr().cast::<u32>() = 13 };
        assert_eq!(*manual, 13);

        let mut cell = Cell::new(7_u32);
        let carrier = SoftEncodeMut::soft_encode_mut(&mut cell, &mut ());
        unsafe { *carrier.as_ptr().cast::<u32>() = 11 };
        assert_eq!(cell.get(), 11);
        let carrier =
            unsafe { SoftEncodeMutUnchecked::soft_encode_mut_unchecked(&mut cell, &mut ()) };
        unsafe { *carrier.as_ptr().cast::<u32>() = 13 };
        assert_eq!(cell.get(), 13);

        let mut cell = UnsafeCell::new(7_u32);
        let carrier = SoftEncodeMut::soft_encode_mut(&mut cell, &mut ());
        unsafe { *carrier.as_ptr().cast::<u32>() = 11 };
        assert_eq!(*cell.get_mut(), 11);
        let carrier =
            unsafe { SoftEncodeMutUnchecked::soft_encode_mut_unchecked(&mut cell, &mut ()) };
        unsafe { *carrier.as_ptr().cast::<u32>() = 13 };
        assert_eq!(*cell.get_mut(), 13);
    }

    #[test]
    fn explicit_sized_mutable_reference_encoder_needs_no_owned_decoder() {
        let mut value = Explicit(41);
        let encoded = co3::soft_encode(&mut value, &mut Default::default());
        unsafe { encoded.as_ptr().write(43) };
        assert_eq!(value.0, 43);
    }

    // A custom unstable DST can still supply its own reference encoder.
    #[derive(RustSpec)]
    struct MutSliceView([u32]);
    impl ReprC for MutSliceView {
        type CType = [u32];
    }
    unsafe impl<'a> SoftEncodeMutUnchecked<'a> for MutSliceView {
        type SoftStore = ();
        unsafe fn soft_encode_mut_unchecked<'itm>(
            &'a mut self,
            _: &'itm mut (),
        ) -> <&'a mut Self as ReprC>::CType
        where
            'a: 'itm,
        {
            let inner = unsafe { co3::encode_unchecked(&mut self.0) };
            unsafe {
                co3::wide::abi::SoftWideRefMut::<Self::CType>::from_raw_parts_mut(
                    inner.as_ptr(),
                    inner.metadata(),
                )
            }
        }
    }
    unsafe impl<'a> SoftEncodeMut<'a> for MutSliceView {
        type Store = ();
        fn soft_encode_mut<'itm>(
            &'a mut self,
            _: &'itm mut (),
        ) -> <&'a mut Self as co3::ReprC>::CType
        where
            'a: 'itm,
        {
            let inner = co3::encode(&mut self.0);
            unsafe {
                co3::wide::abi::SoftWideRefMut::<Self::CType>::from_raw_parts_mut(
                    inner.as_ptr(),
                    inner.metadata(),
                )
            }
        }
    }
    #[test]
    fn explicit_unsized_mutable_encoder_needs_no_value_helper() {
        let mut values = [23_u32];
        let view = unsafe { &mut *(values.as_mut_slice() as *mut [u32] as *mut MutSliceView) };
        let encoded = co3::soft_encode(view, &mut Default::default());
        unsafe { encoded.as_ptr().write(29) };
        assert_eq!(values, [29]);
    }

    #[test]
    fn wide_mutable_wrappers_delegate_to_inner_encoder() {
        let mut values = [alloc::vec![3_u32]];
        let wrapped: &mut core::mem::ManuallyDrop<[Vec<u32>]> = unsafe {
            &mut *(values.as_mut_slice() as *mut [Vec<u32>]
                as *mut core::mem::ManuallyDrop<[Vec<u32>]>)
        };
        let mut store = Default::default();
        let encoded = co3::soft_encode(&mut *wrapped, &mut store);
        assert_eq!(encoded.metadata(), 1);
        assert_eq!(store.sync(), Some(()));
        assert_eq!(values[0], [3]);

        let mut values = [true];
        let wrapped: &mut core::cell::Cell<[bool]> = unsafe {
            &mut *(values.as_mut_slice() as *mut [bool] as *mut core::cell::Cell<[bool]>)
        };
        let mut store = Default::default();
        let encoded = co3::soft_encode(&mut *wrapped, &mut store);
        unsafe { *encoded.as_ptr() = co3::primitive::abi::bool::FALSE };
        assert_eq!(store.sync(), Some(()));
        assert_eq!(values, [false]);

        let mut values = [true];
        let wrapped: &mut core::cell::UnsafeCell<[bool]> = unsafe {
            &mut *(values.as_mut_slice() as *mut [bool] as *mut core::cell::UnsafeCell<[bool]>)
        };
        let mut store = Default::default();
        let encoded = co3::soft_encode(&mut *wrapped, &mut store);
        unsafe { *encoded.as_ptr() = co3::primitive::abi::bool::FALSE };
        assert_eq!(store.sync(), Some(()));
        assert_eq!(values, [false]);
    }

    #[test]
    fn robust_copy_and_move_out_helpers_preserve_the_source_pointer() {
        use crate::convert::SoftEncodeMut;

        let target = 7_u8;
        let mut value = Some(&target);
        let original = core::ptr::from_mut(&mut value).cast();
        let carrier = SoftEncodeMut::soft_encode_mut(&mut value, &mut ());
        assert_eq!(carrier.as_ptr(), original);

        let mut value = crate::option::abi::Option::<u8>::None();
        let original = core::ptr::from_mut(&mut value).cast();
        let carrier = SoftEncodeMut::soft_encode_mut(&mut value, &mut ());
        assert_eq!(carrier.as_ptr(), original);
    }
}
