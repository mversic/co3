//! Referent-specific encoding for mutable-reference paths.

#[cfg(feature = "alloc")]
use crate::stored::{EncodeMove, EncodeMoveUnchecked};
use crate::{ReprC, stored::Store};
use core::{
    cell::{Cell, UnsafeCell},
    mem::ManuallyDrop,
};

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
    unsafe fn encode_mut_unchecked(
        &'a mut self,
        store: &mut Self::SoftStore,
    ) -> <&'a mut Self as ReprC>::CType
    where
        &'a mut Self: ReprC<CType: Sized>;
}

/// Checked encoding of a mutable reference with deferred writeback.
///
/// # Safety
/// The returned carrier must satisfy the checked mutable-reference contract.
/// It grants borrowed access, without transferring ownership to the caller.
/// If `Store` implements `EmptyStore`, the carrier must not borrow from it.
pub unsafe trait EncodeMut<'a>: EncodeMutUnchecked<'a> {
    type Store: Store + Default;

    fn encode_mut(&'a mut self, store: &mut Self::Store) -> <&'a mut Self as ReprC>::CType
    where
        &'a mut Self: ReprC<CType: Sized>;
}

// Each implementation owns its temporary value and decodes it exactly once on sync.
// Keeping this per-type avoids a stable-layout blanket impl overlapping downstream impls.
macro_rules! impl_copy_mut {
    ([$($params:tt)*] $ty:ty) => {
        unsafe impl<'_mut, $($params)*> $crate::stored::EncodeMutUnchecked<'_mut> for $ty
        where
            Self: $crate::ReprC<CType: Sized> + $crate::stored::EncodeMoveUnchecked
                + $crate::stored::DecodeOwned<'_mut, Store: $crate::stored::EmptyStore + '_mut> + '_mut,
            &'_mut mut Self: $crate::ReprC<CType = $crate::reference::CRefMut<<Self as $crate::ReprC>::CType, true>>,
            Self: Copy,
        {
            type SoftStore = $crate::sync::RefMutSizedEncodeStore<'_mut, Self, <Self as $crate::stored::EncodeMoveUnchecked>::SoftStore>;
            unsafe fn encode_mut_unchecked(&'_mut mut self, store: &mut Self::SoftStore) -> <&'_mut mut Self as $crate::ReprC>::CType {
                store.encode(self, |original, fields| {
                    let value = *original;
                    unsafe { $crate::stored::EncodeMoveUnchecked::soft_encode_unchecked(value, fields) }
                })
            }
        }
        unsafe impl<'_mut, $($params)*> $crate::stored::EncodeMut<'_mut> for $ty
        where
            Self: $crate::ReprC<CType: Sized> + $crate::stored::EncodeMove
                + $crate::stored::DecodeOwned<'_mut, Store: $crate::stored::EmptyStore + '_mut> + '_mut,
            &'_mut mut Self: $crate::ReprC<CType = $crate::reference::CRefMut<<Self as $crate::ReprC>::CType, true>>,
            Self: $crate::stored::EncodeMutUnchecked<'_mut>, Self: Copy,
        {
            type Store = $crate::sync::RefMutSizedEncodeStore<'_mut, Self, <Self as $crate::stored::EncodeMove>::Store>;
            fn encode_mut(&'_mut mut self, store: &mut Self::Store) -> <&'_mut mut Self as $crate::ReprC>::CType {
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
    unsafe fn encode_mut_unchecked(
        &'a mut self,
        store: &mut Self::SoftStore,
    ) -> <&'a mut Self as ReprC>::CType {
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
    fn encode_mut(&'a mut self, store: &mut Self::Store) -> <&'a mut Self as ReprC>::CType {
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
            unsafe fn encode_mut_unchecked(
                &'a mut self,
                store: &mut Self::SoftStore,
            ) -> <&'a mut Self as ReprC>::CType {
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
            fn encode_mut(&'a mut self, store: &mut Self::Store) -> <&'a mut Self as ReprC>::CType {
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
impl_copy_mut!([T: Copy, E: Copy] crate::result::CResult<T, E>);
impl_copy_mut!([T: Copy, E: Copy] crate::result::Result<T, E>);

macro_rules! impl_move_out_mut {
    ([$($params:tt)*] $ty:ty where $($bounds:tt)*) => {
        unsafe impl<'_mut, $($params)*> $crate::stored::EncodeMutUnchecked<'_mut> for $ty
        where
            Self: $crate::ReprC<CType: Sized> + $crate::stored::EncodeMoveUnchecked
                + $crate::stored::DecodeOwned<'_mut, Store: $crate::stored::EmptyStore + '_mut> + '_mut,
            &'_mut mut Self: $crate::ReprC<CType = $crate::reference::CRefMut<<Self as $crate::ReprC>::CType, true>>,
            $($bounds)*
        {
            type SoftStore = $crate::sync::RefMutSizedEncodeStore<'_mut, Self, <Self as $crate::stored::EncodeMoveUnchecked>::SoftStore>;
            unsafe fn encode_mut_unchecked(&'_mut mut self, store: &mut Self::SoftStore) -> <&'_mut mut Self as $crate::ReprC>::CType {
                store.encode(self, |original, fields| {
                    let value = core::mem::take(original);
                    unsafe { $crate::stored::EncodeMoveUnchecked::soft_encode_unchecked(value, fields) }
                })
            }
        }
        unsafe impl<'_mut, $($params)*> $crate::stored::EncodeMut<'_mut> for $ty
        where
            Self: $crate::ReprC<CType: Sized> + $crate::stored::EncodeMove
                + $crate::stored::DecodeOwned<'_mut, Store: $crate::stored::EmptyStore + '_mut> + '_mut,
            &'_mut mut Self: $crate::ReprC<CType = $crate::reference::CRefMut<<Self as $crate::ReprC>::CType, true>>,
            Self: $crate::stored::EncodeMutUnchecked<'_mut>, $($bounds)*
        {
            type Store = $crate::sync::RefMutSizedEncodeStore<'_mut, Self, <Self as $crate::stored::EncodeMove>::Store>;
            fn encode_mut(&'_mut mut self, store: &mut Self::Store) -> <&'_mut mut Self as $crate::ReprC>::CType {
                store.encode(self, |original, fields| {
                    let value = core::mem::take(original);
                    $crate::stored::EncodeMove::soft_encode(value, fields)
                })
            }
        }
    };
}
impl_move_out_mut!([T] crate::option::COption<T> where);
impl_move_out_mut!([T] crate::option::Option<T> where);
#[cfg(feature = "alloc")]
impl_move_out_mut!([T] alloc::vec::Vec<T> where T: crate::ReprC<CType: crate::CType + Sized>);
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
unsafe impl<'a, T: ReprC<CType: crate::CType + Copy + Sized> + 'a> EncodeMutUnchecked<'a> for [T]
where
    &'a mut T: crate::stored::EncodeMoveUnchecked
        + ReprC<CType = crate::reference::CRefMut<T::CType, true>>,
    &'a mut [T]: ReprC<CType = crate::slice::CWideRefMut<[T::CType], true>>,
{
    type SoftStore = crate::sync::RefMutWideEncodeStore<
        'a,
        [T],
        alloc::boxed::Box<[<&'a mut T as crate::stored::EncodeMoveUnchecked>::SoftStore]>,
        alloc::boxed::Box<[crate::reference::CRefMut<T::CType, true>]>,
    >;

    unsafe fn encode_mut_unchecked(
        &'a mut self,
        store: &mut Self::SoftStore,
    ) -> <&'a mut Self as ReprC>::CType {
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
unsafe impl<'a, T: ReprC<CType: crate::CType + Copy + Sized> + 'a> EncodeMut<'a> for [T]
where
    &'a mut T: crate::stored::EncodeMove + ReprC<CType = crate::reference::CRefMut<T::CType, true>>,
    &'a mut [T]: ReprC<CType = crate::slice::CWideRefMut<[T::CType], true>>,
{
    type Store = crate::sync::RefMutWideEncodeStore<
        'a,
        [T],
        alloc::boxed::Box<[<&'a mut T as crate::stored::EncodeMove>::Store]>,
        alloc::boxed::Box<[crate::reference::CRefMut<T::CType, true>]>,
    >;

    fn encode_mut(&'a mut self, store: &mut Self::Store) -> <&'a mut Self as ReprC>::CType {
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
unsafe impl<
    'a,
    'b: 'a,
    R: ReprC + rust_spec::RustSpec<Size = rust_spec::size::Sized<K>>,
    K: rust_spec::size::SizedKind,
> EncodeMutUnchecked<'a> for &'b mut R
where
    R: EncodeMutUnchecked<'a>,
{
    type SoftStore = crate::sync::RefMutReferenceEncodeStore<
        crate::reference::CRefMut<R::CType, true>,
        R::SoftStore,
    >;

    unsafe fn encode_mut_unchecked(
        &'a mut self,
        store: &mut Self::SoftStore,
    ) -> <&'a mut Self as ReprC>::CType {
        let value = unsafe { R::encode_mut_unchecked(&mut **self, &mut store.inner) };
        store.original = Some(value);
        crate::reference::CRefMut::from_mut(store.value.insert(value))
    }
}

#[cfg(feature = "alloc")]
unsafe impl<
    'a,
    'b: 'a,
    R: ReprC + rust_spec::RustSpec<Size = rust_spec::size::Sized<K>>,
    K: rust_spec::size::SizedKind,
> EncodeMut<'a> for &'b mut R
where
    R: EncodeMut<'a>,
{
    type Store = crate::sync::RefMutReferenceEncodeStore<
        crate::reference::CRefMut<R::CType, true>,
        R::Store,
    >;

    fn encode_mut(&'a mut self, store: &mut Self::Store) -> <&'a mut Self as ReprC>::CType {
        let value = R::encode_mut(&mut **self, &mut store.inner);
        store.original = Some(value);
        crate::reference::CRefMut::from_mut(store.value.insert(value))
    }
}

pub(crate) use impl_copy_mut;
