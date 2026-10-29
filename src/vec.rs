//! Stable vector ownership and its robust C-compatible representation.
use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, EncodeUnchecked, ReprC,
    borrow::Borrowed,
    slice::impl_identity_carrier,
    stored::{DecodeOwned, EncodeMove, EncodeMoveUnchecked},
    transmute::AbiCompatible,
};
use core::{
    hash::Hash,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};
use rust_spec::RustSpec;

/// A vector with a stable three-field representation.
#[repr(C)]
pub struct Vec<T> {
    data: NonNull<T>,
    capacity: usize,
    len: usize,
}

/// An owned allocation represented by its data pointer, length, and capacity.
/// A null pointer is reserved as a niche and does not represent a vector.
#[repr(C)]
#[derive(RustSpec)]
pub struct CVec<T> {
    data: *mut T,
    capacity: usize,
    len: usize,
}

struct VecOwner<'a, T> {
    target: &'a mut Vec<T>,
    value: Option<alloc::vec::Vec<T>>,
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
    pub fn as_mut_ptr(&mut self) -> *mut T {
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

impl<T> Drop for VecOwner<'_, T> {
    fn drop(&mut self) {
        if let Some(value) = self.value.take() {
            *self.target = value.into();
        }
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
unsafe impl<T> crate::borrow::Borrow for Vec<T> {
    type Borrowed = Borrowed<Self>;

    fn borrow(self) -> Self::Borrowed {
        Borrowed::new(self)
    }
    fn from_borrow(value: Self::Borrowed) -> Self {
        Borrowed::into_inner(value)
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

impl<T> Copy for CVec<T> {}
impl<T> Clone for CVec<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: CType> Default for CVec<T> {
    fn default() -> Self {
        Self::null()
    }
}

impl<T: CType> CVec<T> {
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

    /// Constructs a robust header without validating its allocation or contents.
    pub const fn from_raw_parts(data: *mut T, len: usize, capacity: usize) -> Self {
        Self {
            data,
            len,
            capacity,
        }
    }
}

impl<T> CVec<T> {
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
    /// For a non-null pointer with `len <= capacity`, the parts must satisfy
    /// `alloc::vec::Vec::from_raw_parts`, including initialized elements, correct
    /// allocation layout, and unique ownership. Copying this header does not
    /// duplicate ownership; reconstruct the allocation at most once.
    pub unsafe fn into_vec(self) -> Option<alloc::vec::Vec<T>> {
        let data = NonNull::new(self.data)?;
        if self.len > self.capacity {
            return None;
        }
        Some(unsafe { alloc::vec::Vec::from_raw_parts(data.as_ptr(), self.len, self.capacity) })
    }
}
impl_identity_carrier! { CVec, [T: CType], [T] }

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

impl<T> From<Vec<T>> for alloc::vec::Vec<T> {
    fn from(value: Vec<T>) -> Self {
        let (data, len, capacity) = value.into_raw_parts();
        // SAFETY: The consumed wrapper transfers its uniquely owned allocation.
        unsafe { alloc::vec::Vec::from_raw_parts(data, len, capacity) }
    }
}

impl<T: ReprC<CType: Sized>> ReprC for Vec<T> {
    type CType = CVec<T::CType>;
}

#[cfg(feature = "alloc")]
impl<R: ReprC<CType: Sized>> ReprC for alloc::vec::Vec<R> {
    type CType = <Vec<R> as ReprC>::CType;
}
#[cfg(feature = "alloc")]
impl<R> Encode for alloc::vec::Vec<R> where Self: EncodeMove {}
#[cfg(feature = "alloc")]
impl<R> EncodeUnchecked for alloc::vec::Vec<R> where Self: EncodeMoveUnchecked {}
#[cfg(feature = "alloc")]
impl<'d, R> Decode<'d> for alloc::vec::Vec<R> where Self: DecodeOwned<'d> {}

// The header has C layout; reachable element layout and mutability follow T.
unsafe impl<T: RustSpec<Mutability: rust_spec::mutability::PointerMutability>> RustSpec for Vec<T> {
    type Layout = T::Layout;
    type Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>;
    type __Alignment = <usize as RustSpec>::__Alignment;
    type Trap = rust_spec::layout::NonRobust;
    type Niche = rust_spec::niche::WithNiche<rust_spec::Stable>;
    type Drop = rust_spec::drop::CustomDrop<rust_spec::drop::AutoDrop>;
    type Mutability = <T::Mutability as rust_spec::mutability::PointerMutability>::Output;
    type Opacity = rust_spec::NonOpaque;
    type __IndirectTrap = T::Trap;
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
impl<T> Encode for Vec<T> where Self: EncodeMove + crate::EncodeUnchecked {}
impl<T> crate::EncodeUnchecked for Vec<T> where Self: EncodeMoveUnchecked {}
impl<'d, T> Decode<'d> for Vec<T> where Self: DecodeOwned<'d> {}
impl<T: ReprC<CType: CType + Sized>> crate::niche::Niche for Vec<T> {
    const NICHE: Self::CType = CVec::null();
}
unsafe impl<T: AbiCompatible<CType: CType + Sized>> AbiCompatible for Vec<T>
where
    T: RustSpec<Layout = rust_spec::Stable, Mutability: rust_spec::mutability::PointerMutability>,
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
impl<T> core::fmt::Debug for CVec<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CVec")
            .field("data", &self.data)
            .field("len", &self.len)
            .field("capacity", &self.capacity)
            .finish()
    }
}

impl<T> Eq for CVec<T> {}
impl<T> PartialEq for CVec<T> {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data && self.len == other.len && self.capacity == other.capacity
    }
}

unsafe impl<T: CType> crate::borrow::BorrowCast for CVec<T> {
    type AsConst = Borrowed<Self>;
}

impl<T: CType> crate::boxed::FromWideBox<[T]> for CVec<T> {
    unsafe fn from_raw_parts(data: NonNull<T>, len: usize) -> Self {
        Self::from_vec(
            unsafe {
                alloc::boxed::Box::from_raw(core::ptr::slice_from_raw_parts_mut(data.as_ptr(), len))
            }
            .into_vec(),
        )
    }
    unsafe fn from_shared_parts(data: NonNull<T>, len: usize) -> Self {
        Self::from_raw_parts(
            data.as_ptr(),
            len,
            if core::mem::size_of::<T>() == 0 {
                usize::MAX
            } else {
                len
            },
        )
    }
    fn data(&self) -> *mut T {
        self.data
    }
    fn metadata(&self) -> usize {
        self.len
    }
}
