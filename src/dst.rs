#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, vec::Vec};

pub enum Sized_ {}
pub enum SliceLike {}
pub enum TraitObjectLike {}
pub enum ExternTypeLike {}

pub(crate) trait UnSized {}

impl UnSized for SliceLike {}
impl UnSized for TraitObjectLike {}

pub trait DstFamily {
    type Kind;
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

impl<R> DstFamily for [R] {
    type Kind = SliceLike;
}

impl<T: ?Sized> DstFamily for &T {
    type Kind = Sized_;
}

impl<T: ?Sized> DstFamily for &mut T {
    type Kind = Sized_;
}

#[cfg(feature = "alloc")]
impl<T: ?Sized> DstFamily for Box<T> {
    type Kind = Sized_;
}

#[cfg(feature = "alloc")]
impl<T> DstFamily for Vec<T> {
    type Kind = Sized_;
}

impl<T, const N: usize> DstFamily for [T; N] {
    type Kind = Sized_;
}

impl<T> DstFamily for Option<T> {
    type Kind = Sized_;
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

impl DstFamily for str {
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
