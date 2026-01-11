use core::{marker::PhantomData, ptr::NonNull};

use crate::{
    ir::{ReprFamily, Transparent},
    niche::{Niche, NicheFamily, StableNiche, WithStableNiche},
    transmute::CheckedTransmute,
};

/// Represents the pointee on the far side of an exported opaque pointer at the FFI boundary.
///
/// # Safety
///
/// Implementors must guarantee that:
/// - `Self` has the same representation as [`NonNull<Extern>`].
pub unsafe trait External: Sized {
    /// Returns a shared opaque pointer.
    fn as_ptr(&self) -> *const Extern;

    /// Returns a mutable opaque pointer.
    fn as_mut_ptr(&mut self) -> *mut Extern;
}

/// Wrapper around struct/enum opaque pointer. When wrapped with the [`co3::extern_type`] macro in
/// the crate linking dynamically to some `cdylib` crate, it replaces struct/enum body definition
#[repr(C)]
pub struct Extern {
    __data: [u8; 0],

    // Required for !Send & !Sync & !Unpin.
    //
    // - `*mut u8` is !Send & !Sync. It's wrapped in `PhantomData` not to affect alignment.
    //
    // - `PhantomPinned` is !Unpin. It's wrapped in `PhantomData` because
    //   its memory representation is not guaranteed to be FFI-safe
    __marker: PhantomData<(*mut u8, core::marker::PhantomPinned)>,
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct ExternRef<'a, T>(NonNull<Extern>, PhantomData<&'a T>);

#[repr(transparent)]
pub struct ExternRefMut<'a, T>(NonNull<Extern>, core::marker::PhantomData<&'a mut T>);

impl<T: External> ExternRef<'_, T> {
    pub fn new(inner: &T) -> Self {
        let value = unsafe { NonNull::new_unchecked(inner.as_ptr() as *mut _) };

        Self(value, PhantomData)
    }
}

impl<T: External> ExternRefMut<'_, T> {
    pub fn new(inner: &mut T) -> Self {
        let value = unsafe { NonNull::new_unchecked(inner.as_mut_ptr()) };

        Self(value, PhantomData)
    }
}

impl<T> core::ops::Deref for ExternRef<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        let ptr: *const _ = &self.0.as_ptr();
        unsafe { &*(ptr.cast::<T>()) }
    }
}

impl<T> core::ops::Deref for ExternRefMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        let ptr: *const _ = &self.0.as_ptr();
        unsafe { &*(ptr.cast::<T>()) }
    }
}

impl<T> core::ops::DerefMut for ExternRefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let ptr: *mut _ = &mut self.0.as_ptr();
        unsafe { &mut *(ptr.cast::<T>()) }
    }
}

impl<R> ReprFamily for ExternRef<'_, R> {
    type Kind = Transparent;
}
impl<R> ReprFamily for ExternRefMut<'_, R> {
    type Kind = Transparent;
}

impl<R> NicheFamily for ExternRef<'_, R> {
    type Kind = WithStableNiche;
}
impl<R> NicheFamily for ExternRefMut<'_, R> {
    type Kind = WithStableNiche;
}

unsafe impl<R> CheckedTransmute for ExternRef<'_, R> {
    type Target = *const Extern;

    #[inline(always)]
    fn is_valid(target: &Self::Target) -> bool {
        !target.is_null()
    }
}
unsafe impl<R> CheckedTransmute for ExternRefMut<'_, R> {
    type Target = *const Extern;

    #[inline(always)]
    fn is_valid(target: &Self::Target) -> bool {
        !target.is_null()
    }
}

impl<R> Niche for ExternRef<'_, R> {
    const NICHE_VALUE: Self::CType = core::ptr::null();
}
impl<R> Niche for ExternRefMut<'_, R> {
    const NICHE_VALUE: Self::CType = core::ptr::null_mut();
}

unsafe impl<R> StableNiche for ExternRef<'_, R> {}
unsafe impl<R> StableNiche for ExternRefMut<'_, R> {}
