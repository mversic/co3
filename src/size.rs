#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, vec::Vec};
use disjoint_impls::disjoint_impls;

/// [`Sized`] type whose size is 0
pub enum Zst {}
/// Type that has a non-zero size
pub enum SizedType {}
pub enum SliceLike {}
pub enum TraitObjectLike {}
pub enum ExternTypeLike {}

// TODO: Not used atm
/// [`Zst`] type that has no valid values
pub enum Uninhabited {}

disjoint_impls! {
    pub trait SizeFamily {
        type Kind;
    }

    impl<T: SizeFamily<Kind = Uninhabited>> SizeFamily for Option<T> {
        type Kind = Zst;
    }
    impl<T: SizeFamily<Kind = Zst>> SizeFamily for Option<T> {
        type Kind = SizedType;
    }
    impl<T: SizeFamily<Kind = SizedType>> SizeFamily for Option<T> {
        type Kind = SizedType;
    }

    // TODO: Implement for Result
}

pub trait Dst {
    type Preamble;
    type Payload: ?Sized;
}

pub trait SliceDst: Dst {
    type Elem;

    fn len(&self) -> usize;
    fn as_ptr(&self) -> *const Self::Elem;
    fn as_mut_ptr(&mut self) -> *mut Self::Elem;

    unsafe fn from_raw_parts<'a>(data: *const Self::Elem, len: usize) -> &'a Self;
    unsafe fn from_raw_parts_mut<'a>(data: *mut Self::Elem, len: usize) -> &'a mut Self;
}

impl<R> Dst for [R] {
    type Preamble = ();
    type Payload = Self;
}

impl<R> SizeFamily for [R] {
    type Kind = SliceLike;
}

impl<T: ?Sized> SizeFamily for &T {
    type Kind = SizedType;
}

impl<T: ?Sized> SizeFamily for &mut T {
    type Kind = SizedType;
}

#[cfg(feature = "alloc")]
impl<T: ?Sized> SizeFamily for Box<T> {
    type Kind = SizedType;
}

#[cfg(feature = "alloc")]
impl<T> SizeFamily for Vec<T> {
    type Kind = SizedType;
}

impl<T: SizeFamily, const N: usize> SizeFamily for [T; N] {
    type Kind = T::Kind;
}

impl<R> SliceDst for [R] {
    type Elem = R;

    fn len(&self) -> usize {
        <[R]>::len(self)
    }

    fn as_ptr(&self) -> *const Self::Elem {
        <[R]>::as_ptr(self)
    }

    fn as_mut_ptr(&mut self) -> *mut Self::Elem {
        <[R]>::as_mut_ptr(self)
    }

    unsafe fn from_raw_parts<'a>(data: *const Self::Elem, len: usize) -> &'a Self {
        unsafe { core::slice::from_raw_parts(data, len) }
    }

    unsafe fn from_raw_parts_mut<'a>(data: *mut Self::Elem, len: usize) -> &'a mut Self {
        unsafe { core::slice::from_raw_parts_mut(data, len) }
    }
}

impl Dst for str {
    type Preamble = ();
    type Payload = Self;
}

impl SizeFamily for str {
    type Kind = SliceLike;
}

impl SliceDst for str {
    type Elem = u8;

    fn len(&self) -> usize {
        str::len(self)
    }

    fn as_ptr(&self) -> *const Self::Elem {
        str::as_ptr(self)
    }

    fn as_mut_ptr(&mut self) -> *mut Self::Elem {
        str::as_mut_ptr(self)
    }

    unsafe fn from_raw_parts<'a>(data: *const Self::Elem, len: usize) -> &'a Self {
        let slice = unsafe { core::slice::from_raw_parts(data, len) };
        unsafe { core::str::from_utf8_unchecked(slice) }
    }

    unsafe fn from_raw_parts_mut<'a>(data: *mut Self::Elem, len: usize) -> &'a mut Self {
        let slice = unsafe { core::slice::from_raw_parts_mut(data, len) };
        unsafe { core::str::from_utf8_unchecked_mut(slice) }
    }
}
