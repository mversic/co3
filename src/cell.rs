//! Raw access to a value stored behind interior mutability.

use core::cell::{Cell, UnsafeCell};

/// Provides a raw mutable pointer to a value that supports interior mutation.
///
/// Calling [`Self::get`] does not grant permission to dereference the returned
/// pointer. Callers must still uphold Rust's aliasing and any type-specific
/// borrowing rules.
///
/// # Safety
///
/// [`Self::get`] must return a properly aligned pointer to the storage within
/// `self` that may legally be mutated through a shared reference. The pointer
/// must remain valid for the lifetime of `self`.
pub unsafe trait InteriorMut {
    /// The value accessible through this interior-mutable container.
    type Target: ?Sized;

    /// Returns a raw mutable pointer to [`Self::Target`].
    fn get(&self) -> *mut Self::Target;
}

unsafe impl<T: ?Sized> InteriorMut for UnsafeCell<T> {
    type Target = T;

    #[inline]
    fn get(&self) -> *mut Self::Target {
        UnsafeCell::get(self)
    }
}

unsafe impl<T> InteriorMut for [UnsafeCell<T>] {
    type Target = [T];

    #[inline]
    fn get(&self) -> *mut Self::Target {
        let data = UnsafeCell::raw_get(self.as_ptr());
        core::ptr::slice_from_raw_parts_mut(data, self.len())
    }
}

unsafe impl<T: ?Sized> InteriorMut for Cell<T> {
    type Target = T;

    #[inline]
    fn get(&self) -> *mut Self::Target {
        Cell::as_ptr(self)
    }
}

unsafe impl<T> InteriorMut for [Cell<T>] {
    type Target = [T];

    #[inline]
    fn get(&self) -> *mut Self::Target {
        // Cell<T> is a transparent wrapper around UnsafeCell<T>. raw_get also
        // works for an empty slice, where there is no first Cell to borrow.
        let data = UnsafeCell::raw_get(self.as_ptr().cast::<UnsafeCell<T>>());
        core::ptr::slice_from_raw_parts_mut(data, self.len())
    }
}

unsafe impl<R: InteriorMut + ?Sized> InteriorMut for &R {
    type Target = R::Target;

    #[inline]
    fn get(&self) -> *mut Self::Target {
        InteriorMut::get(*self)
    }
}

unsafe impl<R: InteriorMut + ?Sized> InteriorMut for &mut R {
    type Target = R::Target;

    #[inline]
    fn get(&self) -> *mut Self::Target {
        InteriorMut::get(&**self)
    }
}

#[cfg(feature = "alloc")]
unsafe impl<R: InteriorMut + ?Sized> InteriorMut for alloc::boxed::Box<R> {
    type Target = R::Target;

    #[inline]
    fn get(&self) -> *mut Self::Target {
        InteriorMut::get(self.as_ref())
    }
}

#[cfg(test)]
mod slice_tests {
    use super::*;

    #[test]
    fn shared_slice_pointers_cover_cells_and_empty_slices() {
        let cells = [UnsafeCell::new(1_u8)];
        let view = crate::encode(&cells[..]);
        unsafe { view.data().write(2) };
        assert_eq!(unsafe { *cells[0].get() }, 2);

        let cells = [Cell::new(3_u8)];
        let view = crate::encode(&cells[..]);
        unsafe { view.data().write(4) };
        assert_eq!(cells[0].get(), 4);

        let empty_unsafe_cells: [UnsafeCell<u8>; 0] = [];
        assert_eq!(crate::encode(&empty_unsafe_cells[..]).len(), 0);
        let empty_cells: [Cell<u8>; 0] = [];
        assert_eq!(crate::encode(&empty_cells[..]).len(), 0);
    }
}

#[cfg(test)]
#[cfg(all(feature = "alloc", feature = "derive"))]
mod tests {
    use core::cell::UnsafeCell;

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    use crate::{Encode, ReprC, rust_spec::RustSpec};

    #[derive(RustSpec, ReprC)]
    #[repr(C)]
    struct DerivedInteriorMut {
        value: UnsafeCell<u8>,
    }

    #[derive(RustSpec, ReprC)]
    #[repr(C)]
    struct MultipleFields {
        first: UnsafeCell<u8>,
        second: u8,
    }

    #[derive(RustSpec, ReprC)]
    enum SingleVariant {
        Value(UnsafeCell<u8>),
    }

    #[derive(RustSpec, ReprC)]
    #[repr(u8)]
    enum TaggedSingleVariant {
        Value(UnsafeCell<u8>),
    }

    #[test]
    fn repr_c_derive_exposes_interior_mutable_address() {
        assert_impl_all!(DerivedInteriorMut: InteriorMut);
        assert_impl_all!(SingleVariant: InteriorMut);
        assert_not_impl_any!(TaggedSingleVariant: InteriorMut);
        assert_impl_all!(&DerivedInteriorMut: Encode);
        assert_not_impl_any!(MultipleFields: InteriorMut);

        let value = DerivedInteriorMut {
            value: UnsafeCell::new(7),
        };
        let pointer = InteriorMut::get(&value);

        assert_eq!(pointer, value.value.get());

        let value = SingleVariant::Value(UnsafeCell::new(11));
        let pointer = InteriorMut::get(&value);
        let SingleVariant::Value(field) = &value;

        assert_eq!(pointer, field.get());
    }
}
