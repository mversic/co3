// FIXME: I think I should check variance of phantom data

use crate::{Extern, ir::External};

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct ExternRef<'a, T>(*const Extern, core::marker::PhantomData<&'a T>);

#[repr(transparent)]
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
