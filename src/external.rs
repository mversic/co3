use crate::{ExternC, WrapperTypeOf, mineral};

/// Represents the pointee on the far side of an exported opaque pointer at the FFI boundary.
///
/// # Safety
///
/// Implementors must guarantee that:
/// - `Self` has the same representation as `*mut` [`Extern`].
pub unsafe trait External {
    /// Constructs `Self` from an opaque pointer.
    ///
    /// # Safety
    ///
    /// The pointer argument must be valid.
    unsafe fn from_extern_ptr(source: *mut Extern) -> Self;

    /// Returns a shared opaque pointer.
    fn as_extern_ptr(&self) -> *const Extern;

    /// Returns a mutable opaque pointer.
    fn as_extern_ptr_mut(&mut self) -> *mut Extern;
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
    __marker: core::marker::PhantomData<(*mut u8, core::marker::PhantomPinned)>,
}

#[derive(Clone, Copy)]
#[repr(transparent)]
// FIXME: I think I should check variance of phantom data
pub struct ExternRef<'a, T>(*const Extern, core::marker::PhantomData<&'a T>);

#[repr(transparent)]
// FIXME: I think I should check variance of phantom data
pub struct ExternRefMut<'a, T>(*mut Extern, core::marker::PhantomData<&'a mut T>);

impl<T: External> ExternRef<'_, T> {
    pub fn new(inner: &T) -> Self {
        Self(inner.as_extern_ptr(), core::marker::PhantomData)
    }
}

impl<T: External> ExternRefMut<'_, T> {
    pub fn new(inner: &mut T) -> Self {
        Self(inner.as_extern_ptr_mut(), core::marker::PhantomData)
    }
}

impl<T> core::ops::Deref for ExternRef<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*(&self.0 as *const *const Extern).cast() }
    }
}

impl<T> core::ops::Deref for ExternRefMut<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*(&self.0 as *const *mut Extern).cast() }
    }
}

impl<T> core::ops::DerefMut for ExternRefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *(&mut self.0 as *mut *mut Extern).cast() }
    }
}

mineral! {
    unsafe impl<R> Transparent for ExternRef<'_, R> {
        type Target = *const Extern;

        const NICHE_VALUE: <Self as ExternC>::CType = core::ptr::null();
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }
}
mineral! {
    unsafe impl<R> Transparent for ExternRefMut<'_, R> {
        type Target = *mut Extern;

        const NICHE_VALUE: <Self as ExternC>::CType = core::ptr::null_mut();
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }
}

impl<'a, R: 'a, T> WrapperTypeOf<*const R> for ExternRef<'a, T> {
    type Type = ExternRef<'a, R>;
}
impl<'a, R: 'a, T> WrapperTypeOf<*mut R> for ExternRefMut<'a, T> {
    type Type = ExternRefMut<'a, R>;
}
