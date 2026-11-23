//! Stable vector ownership and its robust C-compatible representation.

#[cfg(feature = "repr-rust")]
use crate::{convert::EncodeUnchecked, marker::Native};

use alloc::boxed::Box;
use core::{
    hash::Hash,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, ReprC,
    convert::{AbiCompatibleValidate, Decode, DecodeMove, Encode, EncodeMove, EncodeMoveUnchecked},
    marker::{AbiCompatible, Zeroable},
    wide::impl_identity_carrier,
};
#[cfg(feature = "alloc")]
use crate::{
    borrow::{
        self, borrowed_decode_slice, decode_borrowed_elements, decode_borrowed_elements_unchecked,
    },
    stored::map_vec,
};

/// A vector with a stable three-field representation.
#[repr(C)]
pub struct Vec<T> {
    data: NonNull<T>,
    capacity: usize,
    len: usize,
}

impl<T> Vec<T> {
    pub const fn new() -> Self {
        Self {
            data: NonNull::dangling(),
            capacity: if core::mem::size_of::<T>() == 0 {
                usize::MAX
            } else {
                0
            },
            len: 0,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        alloc::vec::Vec::with_capacity(capacity).into()
    }

    pub const fn len(&self) -> usize {
        self.len
    }
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub const fn capacity(&self) -> usize {
        self.capacity
    }
    pub const fn as_ptr(&self) -> *const T {
        self.data.as_ptr()
    }
    pub const fn as_mut_ptr(&mut self) -> *mut T {
        self.data.as_ptr()
    }

    pub fn into_raw_parts(self) -> (*mut T, usize, usize) {
        let value = ManuallyDrop::new(self);
        (value.data.as_ptr(), value.len, value.capacity)
    }

    pub fn push(&mut self, value: T) {
        // Restore ownership even if allocation or element destruction unwinds.
        let mut owner = VecOwner {
            target: self,
            value: None,
        };
        owner.value = Some(core::mem::take(owner.target).into());
        owner.value.as_mut().unwrap().push(value);
    }

    pub fn pop(&mut self) -> Option<T> {
        let mut owner = VecOwner {
            target: self,
            value: None,
        };
        owner.value = Some(core::mem::take(owner.target).into());
        owner.value.as_mut().unwrap().pop()
    }
}

impl<T> Default for Vec<T> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T: Clone> Clone for Vec<T> {
    fn clone(&self) -> Self {
        self.to_vec().into()
    }
}
impl<T: core::fmt::Debug> core::fmt::Debug for Vec<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(&**self, f)
    }
}
// SAFETY: Vec uniquely owns its elements, just like alloc::vec::Vec.
unsafe impl<T: Send> Send for Vec<T> {}
unsafe impl<T: Sync> Sync for Vec<T> {}

impl<T> FromIterator<T> for Vec<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        iter.into_iter().collect::<alloc::vec::Vec<T>>().into()
    }
}
impl<T> IntoIterator for Vec<T> {
    type Item = T;
    type IntoIter = alloc::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        alloc::vec::Vec::from(self).into_iter()
    }
}
impl<'a, T> IntoIterator for &'a Vec<T> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<'a, T> IntoIterator for &'a mut Vec<T> {
    type Item = &'a mut T;
    type IntoIter = core::slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<T> Drop for Vec<T> {
    fn drop(&mut self) {
        // SAFETY: This wrapper uniquely owns the original allocation.
        drop(unsafe {
            alloc::vec::Vec::from_raw_parts(self.data.as_ptr(), self.len, self.capacity)
        });
    }
}

impl<T> Deref for Vec<T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        // SAFETY: The fields retain the invariants of the original Vec.
        unsafe { core::slice::from_raw_parts(self.data.as_ptr(), self.len) }
    }
}
impl<T> DerefMut for Vec<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        // SAFETY: This wrapper uniquely owns the allocation.
        unsafe { core::slice::from_raw_parts_mut(self.data.as_ptr(), self.len) }
    }
}

impl<T> From<alloc::vec::Vec<T>> for Vec<T> {
    fn from(value: alloc::vec::Vec<T>) -> Self {
        let (data, len, capacity) = value.into_raw_parts();
        Self {
            // SAFETY: Vec always provides a non-null data pointer, including for ZSTs.
            data: unsafe { NonNull::new_unchecked(data) },
            len,
            capacity,
        }
    }
}

impl<T: Eq> Eq for Vec<T> {}
impl<T: PartialEq> PartialEq for Vec<T> {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl<T: Ord> Ord for Vec<T> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self[..].cmp(&other[..])
    }
}
impl<T: PartialOrd> PartialOrd for Vec<T> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self[..].partial_cmp(&other[..])
    }
}

impl<T: Hash> Hash for Vec<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self[..].hash(state);
    }
}
impl<T> AsRef<[T]> for Vec<T> {
    fn as_ref(&self) -> &[T] {
        self
    }
}
impl<T> AsMut<[T]> for Vec<T> {
    fn as_mut(&mut self) -> &mut [T] {
        self
    }
}
impl<T> Extend<T> for Vec<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        let mut owner = VecOwner {
            target: self,
            value: None,
        };
        owner.value = Some(core::mem::take(owner.target).into());
        owner.value.as_mut().unwrap().extend(iter);
    }
}

struct VecOwner<'a, T> {
    target: &'a mut Vec<T>,
    value: Option<alloc::vec::Vec<T>>,
}

impl<T> Drop for VecOwner<'_, T> {
    fn drop(&mut self) {
        if let Some(value) = self.value.take() {
            *self.target = value.into();
        }
    }
}

unsafe impl<T: RustSpec<Mutability: rust_spec::mutability::PointerMutability>> RustSpec for Vec<T> {
    type Layout = T::Layout;
    type Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>;
    type __Alignment = <usize as RustSpec>::__Alignment;
    type Trap = rust_spec::layout::NonRobust;
    type Niche = rust_spec::niche::WithNiche<rust_spec::Unspecified>;
    type Drop = rust_spec::drop::CustomDrop<rust_spec::drop::AutoDrop>;
    type Mutability = <T::Mutability as rust_spec::mutability::PointerMutability>::Output;
    type Opacity = rust_spec::NonOpaque;
    type __IndirectTrap = T::Trap;
}

impl<T: ReprC<CType: Sized>> ReprC for Vec<T> {
    type CType = abi::Vec<T::CType>;
}

impl<T> Encode for Vec<T> where Self: EncodeMove + crate::convert::EncodeUnchecked {}
impl<T> crate::convert::EncodeUnchecked for Vec<T> where Self: EncodeMoveUnchecked {}
impl<'d, T> Decode<'d> for Vec<T> where Self: DecodeMove<'d> {}
impl<T: ReprC<CType: CType + Sized>> crate::niche::Niche for Vec<T> {
    const NICHE: Self::CType = abi::Vec::null();

    fn is_niche(value: &Self::CType) -> bool {
        value.as_ptr().is_null()
    }
}
unsafe impl<T: AbiCompatibleValidate<CType: CType + Sized>> AbiCompatibleValidate for Vec<T>
where
    T: RustSpec<
            Layout = rust_spec::Specified,
            Mutability: rust_spec::mutability::PointerMutability,
        >,
{
    unsafe fn is_valid(value: &Self::CType) -> bool {
        if value.data.is_null() || value.len > value.capacity {
            return false;
        }
        // The caller establishes allocation validity. Validate only initialized elements;
        // each element validator must respect interior mutability.
        (0..value.len).all(|i| unsafe { T::is_valid(&*value.data.add(i)) })
    }
}
unsafe impl<T: AbiCompatible<CType: CType + Sized>> AbiCompatible for Vec<T> where
    T: RustSpec<
            Layout = rust_spec::Specified,
            Mutability: rust_spec::mutability::PointerMutability,
        >
{
}

#[cfg(feature = "alloc")]
unsafe impl<'d, T: borrow::BorrowedDecode<'d>> borrow::BorrowedDecode<'d> for Vec<T>
where
    T: ReprC<CType: CType + Copy>,
    Self: ReprC<CType = abi::Vec<<T as ReprC>::CType>>,
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

// Elements can need encoding storage even when the vector layout is stable and robust.
#[cfg(feature = "alloc")]
unsafe impl<R: EncodeMove> EncodeMove for Vec<R>
where
    R: ReprC<CType: CType + Sized>,
    Self: EncodeMoveUnchecked + ReprC<CType = abi::Vec<<R as ReprC>::CType>>,
{
    type Store = Box<[R::Store]>;

    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        abi::Vec::from_vec(encode_vec_elements(self.into(), store))
    }
}

#[cfg(feature = "alloc")]
#[inline(always)]
fn encode_vec_elements<R: EncodeMove>(
    value: alloc::vec::Vec<R>,
    store: &mut Box<[R::Store]>,
) -> alloc::vec::Vec<R::CType> {
    let capacity = value.capacity();
    map_vec(
        value.into_iter(),
        capacity,
        store,
        crate::stored::EncodeMove::soft_encode,
    )
}

impl<T> From<Vec<T>> for alloc::vec::Vec<T> {
    fn from(value: Vec<T>) -> Self {
        let (data, len, capacity) = value.into_raw_parts();
        // SAFETY: The consumed wrapper transfers its uniquely owned allocation.
        unsafe { alloc::vec::Vec::from_raw_parts(data, len, capacity) }
    }
}

#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
impl<R: ReprC<CType: Sized>> ReprC for alloc::vec::Vec<R> {
    type CType = Native<abi::Vec<R::CType>>;
}
#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
impl<R> Encode for alloc::vec::Vec<R> where Self: EncodeMove {}
#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
impl<R> EncodeUnchecked for alloc::vec::Vec<R> where Self: EncodeMoveUnchecked {}
#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
impl<'d, R> Decode<'d> for alloc::vec::Vec<R> where Self: DecodeMove<'d> {}

// The header has C layout; reachable element layout and mutability follow T.

#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
unsafe impl<T> borrow::BorrowedEncodeUnchecked for alloc::vec::Vec<T>
where
    T: ReprC<CType: CType + Sized>,
    Vec<T>: borrow::BorrowedEncodeUnchecked + ReprC<CType = abi::Vec<<T as ReprC>::CType>>,
{
    type Owner = <Vec<T> as borrow::BorrowedEncodeUnchecked>::Owner;
    type SoftStore = <Vec<T> as borrow::BorrowedEncodeUnchecked>::SoftStore;

    unsafe fn soft_encode_unchecked<'itm>(
        self,
        store: &'itm mut Self::SoftStore,
        owner: &'itm mut Self::Owner,
    ) -> Self::CType
    where
        Self: 'itm,
    {
        Native(unsafe {
            <Vec<T> as borrow::BorrowedEncodeUnchecked>::soft_encode_unchecked(
                self.into(),
                store,
                owner,
            )
        })
    }
}

#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
unsafe impl<T> borrow::BorrowedEncode for alloc::vec::Vec<T>
where
    T: ReprC<CType: CType + Sized>,
    Vec<T>: borrow::BorrowedEncode + ReprC<CType = abi::Vec<<T as ReprC>::CType>>,
{
    type Store = <Vec<T> as borrow::BorrowedEncode>::Store;

    fn soft_encode<'itm>(
        self,
        store: &'itm mut Self::Store,
        owner: &'itm mut Self::Owner,
    ) -> Self::CType
    where
        Self: 'itm,
    {
        Native(<Vec<T> as borrow::BorrowedEncode>::soft_encode(
            self.into(),
            store,
            owner,
        ))
    }
}

#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
unsafe impl<'d, T: borrow::BorrowedDecode<'d>> borrow::BorrowedDecode<'d> for alloc::vec::Vec<T>
where
    T: ReprC<CType: CType + Copy>,
    Self: ReprC<CType = Native<abi::Vec<<T as ReprC>::CType>>>,
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
#[cfg(feature = "repr-rust")]
unsafe impl<R: ReprC<CType: CType + Sized>> EncodeMoveUnchecked for alloc::vec::Vec<R>
where
    Vec<R>: EncodeMoveUnchecked + ReprC<CType = abi::Vec<<R as ReprC>::CType>>,
{
    type SoftStore = <Vec<R> as EncodeMoveUnchecked>::SoftStore;

    unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
    where
        Self: 'itm,
    {
        Native(unsafe { Vec::from(self).soft_encode_unchecked(store) })
    }
}

#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
unsafe impl<R: ReprC<CType: CType + Sized>> EncodeMove for alloc::vec::Vec<R>
where
    Vec<R>: EncodeMove + ReprC<CType = abi::Vec<<R as ReprC>::CType>>,
    Self: ReprC<CType = Native<abi::Vec<<R as ReprC>::CType>>>,
{
    type Store = <Vec<R> as EncodeMove>::Store;

    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        Native(Vec::from(self).soft_encode(store))
    }
}

#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
unsafe impl<'d, R: ReprC<CType: CType + Sized>> DecodeMove<'d> for alloc::vec::Vec<R>
where
    Vec<R>: DecodeMove<'d> + ReprC<CType = abi::Vec<<R as ReprC>::CType>>,
{
    type Store = <Vec<R> as DecodeMove<'d>>::Store;
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        unsafe { Vec::<R>::soft_decode(source.0, store) }.map(Into::into)
    }
    unsafe fn soft_decode_unchecked<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Self {
        unsafe { Vec::<R>::soft_decode_unchecked(source.0, store) }.into()
    }
}

#[cfg(feature = "alloc")]
#[cfg(feature = "repr-rust")]
impl<R: ReprC<CType: CType + Sized>> crate::niche::Niche for alloc::vec::Vec<R> {
    const NICHE: Self::CType = Native(abi::Vec::null());

    fn is_niche(value: &Self::CType) -> bool {
        value.as_ptr().is_null()
    }
}

#[doc(hidden)]
pub mod abi {
    use super::*;
    #[cfg(feature = "repr-rust")]
    use crate::boxed::OwnedBoxCarrier;
    #[cfg(feature = "repr-rust")]
    use crate::{marker::Native, wide};

    /// An owned allocation represented by its data pointer, length, and capacity.
    /// A null pointer is reserved as a niche and does not represent a vector.
    /// `T` is the lowered element type stored in the allocation.
    #[derive(RustSpec)]
    /// CO3 sources select this carrier directly; native tagged or container sources wrap it in `Native`.
    #[repr(C)]
    #[doc(hidden)]
    pub struct Vec<T: CType> {
        pub(super) data: *mut T,
        pub(super) capacity: usize,
        pub(super) len: usize,
    }

    unsafe impl<T: CType> Zeroable for Vec<T> {}

    impl<T: CType> Copy for Vec<T> {}

    impl<T: CType> Clone for Vec<T> {
        fn clone(&self) -> Self {
            *self
        }
    }

    impl<T: CType> Default for Vec<T> {
        fn default() -> Self {
            Self::null()
        }
    }

    impl<T: CType> Vec<T> {
        pub const fn null() -> Self {
            Self {
                data: core::ptr::null_mut(),
                len: 0,
                capacity: 0,
            }
        }

        pub fn from_vec(value: alloc::vec::Vec<T>) -> Self {
            let (data, len, capacity) = value.into_raw_parts();
            Self {
                data,
                len,
                capacity,
            }
        }

        /// Constructs a vector carrier from its allocation and initialized length.
        ///
        /// # Safety
        /// For a non-null pointer, `data`, `len`, and `capacity` must describe valid
        /// vector storage, with `len <= capacity`. This includes the aligned dangling
        /// pointer used for zero-capacity vectors and zero-sized elements. The storage
        /// must remain valid while the carrier is used. A null pointer represents no
        /// allocation. This constructor does not transfer ownership by itself.
        pub const unsafe fn from_raw_parts(data: *mut T, len: usize, capacity: usize) -> Self {
            Self {
                data,
                len,
                capacity,
            }
        }
    }

    impl<T: CType> Vec<T> {
        pub const fn as_ptr(self) -> *mut T {
            self.data
        }
        pub const fn len(self) -> usize {
            self.len
        }
        pub const fn capacity(self) -> usize {
            self.capacity
        }
        pub const fn is_empty(self) -> bool {
            self.len == 0
        }
        /// Reconstructs an owning vector, rejecting null pointers and lengths above capacity.
        ///
        /// # Safety
        ///
        /// For a non-null pointer with `len <= capacity`, the parts must satisfy
        /// `alloc::vec::Vec::from_raw_parts`, including initialized elements, correct
        /// allocation layout, and unique ownership. Copying this header does not
        /// duplicate ownership; reconstruct the allocation at most once.
        pub unsafe fn into_vec(self) -> core::option::Option<alloc::vec::Vec<T>> {
            let data = NonNull::new(self.data)?;
            if self.len > self.capacity {
                return None;
            }
            Some(unsafe { alloc::vec::Vec::from_raw_parts(data.as_ptr(), self.len, self.capacity) })
        }
    }

    #[cfg(feature = "repr-rust")]
    impl<R: CType> OwnedBoxCarrier<wide::abi::WideBox<[R]>> for Vec<R> {
        unsafe fn from_box_carrier(value: wide::abi::WideBox<[R]>) -> Option<Self> {
            let data = NonNull::new(value.as_ptr())?;
            let slice = core::ptr::slice_from_raw_parts_mut(data.as_ptr(), value.metadata());
            // SAFETY: The caller transfers ownership of the boxed allocation.
            Some(Self::from_vec(
                unsafe { alloc::boxed::Box::from_raw(slice) }.into_vec(),
            ))
        }

        unsafe fn into_box_carrier(self) -> wide::abi::WideBox<[R]> {
            // Shrink before discarding capacity, so Box deallocates with the correct layout.
            let value = unsafe { self.into_vec().expect("encoded owned vector is valid") };
            wide::abi::WideBox::from_box(value.into_boxed_slice())
        }
    }

    #[cfg(feature = "repr-rust")]
    impl<R: CType> OwnedBoxCarrier<wide::abi::WideBox<[R]>> for Native<Vec<R>> {
        unsafe fn from_box_carrier(value: wide::abi::WideBox<[R]>) -> Option<Self> {
            unsafe { Vec::from_box_carrier(value) }.map(Self)
        }

        unsafe fn into_box_carrier(self) -> wide::abi::WideBox<[R]> {
            unsafe { self.0.into_box_carrier() }
        }
    }

    impl_identity_carrier! { Vec, [T: CType], [T] }

    crate::wide::impl_carrier_borrow! { Vec, [T: CType], [T] }

    impl<T: CType> core::fmt::Debug for Vec<T> {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_struct("Vec")
                .field("data", &self.data)
                .field("len", &self.len)
                .field("capacity", &self.capacity)
                .finish()
        }
    }

    impl<T: CType> PartialEq for Vec<T> {
        fn eq(&self, other: &Self) -> bool {
            !self.data.is_null()
                && self.data == other.data
                && self.len == other.len
                && self.capacity == other.capacity
        }
    }

    impl<T: CType> PartialOrd for Vec<T> {
        fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
            if self.data.is_null() || other.data.is_null() {
                None
            } else {
                Some(
                    self.data
                        .cmp(&other.data)
                        .then_with(|| self.len.cmp(&other.len))
                        .then_with(|| self.capacity.cmp(&other.capacity)),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use super::abi;

    #[test]
    fn nullable_vector_layout_is_not_guaranteed() {
        use rust_spec::{RustSpec, Specified, Unspecified, niche::WithNiche};
        use static_assertions::{assert_impl_all, assert_not_impl_any};

        assert_impl_all!(super::Vec<u32>: RustSpec<Layout = Specified, Niche = WithNiche<Unspecified>>);
        assert_impl_all!(Option<super::Vec<u32>>: RustSpec<Layout = Unspecified>);
        assert_impl_all!(Result<super::Vec<u32>, ()>: RustSpec<Layout = Unspecified>);
        assert_impl_all!(crate::string::String: RustSpec<Niche = WithNiche<Unspecified>>);
        assert_impl_all!(Option<crate::string::String>: RustSpec<Layout = Unspecified>);
        assert_not_impl_any!(Option<super::Vec<u32>>: crate::marker::AbiCompatible);

        for value in [None, Some(super::Vec::new()), Some(vec![1u32, 2, 3].into())] {
            let expected = value.clone();
            let decoded = unsafe { crate::decode(crate::encode(value)) };
            assert_eq!(decoded, Some(expected));
        }
    }

    #[test]
    fn raw_vector_roundtrip_preserves_allocation_and_capacity() {
        let mut value = Vec::with_capacity(32);
        value.extend([1u32, 2, 3]);
        let data = value.as_ptr();
        let capacity = value.capacity();
        let carrier = abi::Vec::<u32>::from_vec(value);
        assert_eq!(carrier.as_ptr(), data.cast_mut());
        assert_eq!(carrier.capacity(), capacity);
        let value = unsafe { carrier.into_vec() }.unwrap();
        assert_eq!(value, [1, 2, 3]);
        assert_eq!(value.capacity(), capacity);
    }

    #[test]
    fn empty_and_zero_sized_vectors_roundtrip() {
        let empty = abi::Vec::<u8>::from_vec(Vec::<u8>::new());
        assert!(unsafe { empty.into_vec() }.unwrap().is_empty());
        let zst = abi::Vec::<()>::from_vec(vec![(); 3]);
        assert_eq!(unsafe { zst.into_vec() }.unwrap().len(), 3);
        assert!(unsafe { abi::Vec::<u8>::null().into_vec() }.is_none());
    }

    #[test]
    fn raw_carrier_construction_needs_no_allocation_invariants() {
        let carrier = unsafe { abi::Vec::<u8>::from_raw_parts(core::ptr::null_mut(), 7, 3) };
        assert_eq!(carrier.len(), 7);
        assert_eq!(carrier.capacity(), 3);
    }

    #[test]
    fn derived_carrier_spec_preserves_header_and_pointee_properties() {
        use core::cell::UnsafeCell;

        use static_assertions::assert_impl_all;

        use crate::rust_spec::{
            self, RustSpec, Specified,
            layout::Robust,
            mutability::{Exclusive, WithInterior, WithoutInterior},
        };

        assert_impl_all!(abi::Vec<()>: RustSpec<
            Layout = Specified,
            Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>,
            __Alignment = <usize as RustSpec>::__Alignment,
            Trap = Robust,
            Niche = rust_spec::niche::WithoutNiche,
            Drop = rust_spec::drop::NoDrop,
            Mutability = Exclusive<WithoutInterior>,
            Opacity = rust_spec::NonOpaque,
            __IndirectTrap = Robust,
        >);
        // bool lowers to the robust bool carrier, including inside UnsafeCell.
        assert_impl_all!(abi::Vec<<UnsafeCell<bool> as crate::ReprC>::CType>: RustSpec<
            Layout = Specified,
            Trap = Robust,
            Mutability = Exclusive<WithInterior<Robust>>,
            __IndirectTrap = Robust,
        >);
    }

    #[test]
    fn owned_encoding_rejects_invalid_elements() {
        let invalid =
            abi::Vec::<<bool as crate::ReprC>::CType>::from_vec(vec![crate::primitive::abi::bool(
                2,
            )]);
        assert!(
            unsafe { crate::soft_decode::<super::Vec<bool>>(invalid, &mut Default::default()) }
                .is_none()
        );
    }
}
