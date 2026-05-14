#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, vec::Vec};
use core::{convert::Infallible, ptr::NonNull};

use disjoint_impls::disjoint_impls;

/// Type whose layout is known without pointer metadata.
pub struct Sized<K>(core::marker::PhantomData<K>, Infallible);

/// Types that are always behind a pointer
pub struct MetaSized<K>(core::marker::PhantomData<K>, Infallible);

/// [`Zst`] type that has no valid values.
pub enum Uninhabited {}

/// [`Sized`] type whose size is 0.
pub enum Zst {}

/// Types with a constant non-zero size known at compile time. Check [`core::marker::Sized`].
pub enum SizedType {}

/// Slices and DSTs whose last field is a slice.
pub enum SliceLike {}

/// Trait objects and DSTs whose last field is a trait object.
pub enum DynTraitLike {}

/// Extern types and DSTs whose last field is an extern type.
pub enum ExternTypeLike {}

pub(crate) trait PointeeSized {}

/// Pointers to types implementing this trait alias are “thin”.
/// [Related](https://doc.rust-lang.org/core/ptr/traitalias.Thin.html)
pub(crate) trait Thin {}

impl PointeeSized for ExternTypeLike {}
impl<K> PointeeSized for MetaSized<K> {}

impl Thin for ExternTypeLike {}
impl<S> Thin for crate::size::Sized<S> {}

disjoint_impls! {
    pub trait SizeFamily {
        type Kind;
    }

    impl<T: SizeFamily<Kind = Sized<Uninhabited>>> SizeFamily for Option<T> {
        type Kind = Sized<Zst>;
    }
    impl<T: SizeFamily<Kind = Sized<Zst>>> SizeFamily for Option<T> {
        type Kind = Sized<SizedType>;
    }
    impl<T: SizeFamily<Kind = Sized<SizedType>>> SizeFamily for Option<T> {
        type Kind = Sized<SizedType>;
    }

    // TODO: Implement for Result
}

pub trait Wide {
    type Data;
    type Metadata;

    fn metadata(&self) -> Self::Metadata;

    fn as_ptr(&self) -> *const Self::Data;
    fn as_mut_ptr(&mut self) -> *mut Self::Data;
    #[cfg(feature = "alloc")]
    fn into_non_null(self: Box<Self>) -> NonNull<Self::Data>;

    unsafe fn from_raw_parts<'a>(data: *const Self::Data, metadate: Self::Metadata) -> &'a Self;
    unsafe fn from_raw_parts_mut<'a>(
        data: *mut Self::Data,
        metadate: Self::Metadata,
    ) -> &'a mut Self;

    #[cfg(feature = "alloc")]
    unsafe fn from_non_null(data: NonNull<Self::Data>, metadate: Self::Metadata) -> Box<Self>;
}

impl<R> SizeFamily for [R] {
    type Kind = MetaSized<SliceLike>;
}

impl<T: ?core::marker::Sized> SizeFamily for &T {
    type Kind = Sized<SizedType>;
}

impl<T: ?core::marker::Sized> SizeFamily for &mut T {
    type Kind = Sized<SizedType>;
}

#[cfg(feature = "alloc")]
impl<T: ?core::marker::Sized> SizeFamily for Box<T> {
    type Kind = Sized<SizedType>;
}

#[cfg(feature = "alloc")]
impl<T> SizeFamily for Vec<T> {
    type Kind = Sized<SizedType>;
}

impl<T: SizeFamily<Kind = Sized<K>>, K, const N: usize> SizeFamily for [T; N] {
    type Kind = Sized<K>;
}

impl<R> Wide for [R] {
    type Data = R;
    type Metadata = usize;

    fn metadata(&self) -> Self::Metadata {
        Self::len(self)
    }

    fn as_ptr(&self) -> *const Self::Data {
        Self::as_ptr(self)
    }

    fn as_mut_ptr(&mut self) -> *mut Self::Data {
        Self::as_mut_ptr(self)
    }

    #[cfg(feature = "alloc")]
    fn into_non_null(self: Box<Self>) -> NonNull<Self::Data> {
        let ptr = Box::into_raw(self).cast::<Self::Data>();
        unsafe { NonNull::new_unchecked(ptr) }
    }

    unsafe fn from_raw_parts<'a>(data: *const Self::Data, len: Self::Metadata) -> &'a Self {
        unsafe { core::slice::from_raw_parts(data, len) }
    }

    unsafe fn from_raw_parts_mut<'a>(data: *mut Self::Data, len: Self::Metadata) -> &'a mut Self {
        unsafe { core::slice::from_raw_parts_mut(data, len) }
    }

    #[cfg(feature = "alloc")]
    unsafe fn from_non_null(data: NonNull<Self::Data>, len: Self::Metadata) -> Box<Self> {
        unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(data.as_ptr(), len)) }
    }
}

impl Wide for str {
    type Data = u8;
    type Metadata = usize;

    fn metadata(&self) -> Self::Metadata {
        Self::len(self)
    }

    fn as_ptr(&self) -> *const Self::Data {
        self.as_bytes().as_ptr()
    }

    fn as_mut_ptr(&mut self) -> *mut Self::Data {
        unimplemented!("DANGER ZONE")
        //self.as_bytes_mut().as_mut_ptr()
    }

    #[cfg(feature = "alloc")]
    fn into_non_null(self: Box<Self>) -> NonNull<Self::Data> {
        self.into_boxed_bytes().into_non_null()
    }

    unsafe fn from_raw_parts<'a>(data: *const Self::Data, len: Self::Metadata) -> &'a Self {
        let slice = unsafe { <[u8]>::from_raw_parts(data, len) };
        unsafe { core::str::from_utf8_unchecked(slice) }
    }

    unsafe fn from_raw_parts_mut<'a>(data: *mut Self::Data, len: Self::Metadata) -> &'a mut Self {
        let slice = unsafe { <[u8]>::from_raw_parts_mut(data, len) };
        unsafe { core::str::from_utf8_unchecked_mut(slice) }
    }

    #[cfg(feature = "alloc")]
    unsafe fn from_non_null(data: NonNull<Self::Data>, len: Self::Metadata) -> Box<Self> {
        let slice = unsafe { <[u8]>::from_non_null(data, len) };
        let slice = Box::into_raw(slice);
        let str = unsafe { core::str::from_utf8_unchecked_mut(&mut *slice) };

        unsafe { Box::from_raw(str) }
    }
}
