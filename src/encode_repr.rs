//! Projection of ABI carriers for checked encoding.

use crate::{CType, ReprC};

/// The ABI representation produced by checked encoding.
///
/// # Safety
/// `Self` and `Encoded` must have identical ABI layout, including field offsets
/// and pointer metadata. Projection must preserve ownership and valid bit
/// patterns. It may erase carrier lifetimes, but must never extend them.
pub unsafe trait EncodeRepr: CType + Sized {
    type Encoded: CType;
}

/// Checked encoding's representation of a Rust type.
pub type Encoded<T> = <<T as ReprC>::CType as EncodeRepr>::Encoded;

/// Change the carrier type without changing its ABI bits.
pub fn project<C: EncodeRepr>(value: C) -> C::Encoded {
    const {
        assert!(size_of::<C>() == size_of::<C::Encoded>());
    }
    let value = core::mem::ManuallyDrop::new(value);
    // SAFETY: EncodeRepr guarantees ABI and ownership preservation. CType has
    // no Rust references or drop behavior; this does not access any pointee.
    unsafe { core::mem::transmute_copy(&*value) }
}

/// Store projected bits in an ordinary robust carrier. This does not establish
/// pointee validity or extend backing storage's lifetime; access through the
/// carrier remains subject to its unsafe conversion contracts.
#[doc(hidden)]
pub fn unproject<C: EncodeRepr>(value: C::Encoded) -> C {
    const {
        assert!(size_of::<C>() == size_of::<C::Encoded>());
    }
    let value = core::mem::ManuallyDrop::new(value);
    // SAFETY: Both carriers are robust and EncodeRepr preserves ABI layout
    // and ownership. No pointee is accessed.
    unsafe { core::mem::transmute_copy(&*value) }
}

/// Encode a field using its ordinary robust carrier as storage.
#[doc(hidden)]
pub fn encode_field<T: crate::stored::EncodeMove>(value: T, store: &mut T::Store) -> T::CType {
    unproject::<T::CType>(value.soft_encode(store))
}

/// Encode raw ABI bits for generated FFI glue. The foreign call's safety contract
/// controls pointee lifetimes; the carrier alone does not establish them.
#[doc(hidden)]
pub fn encode_abi<T: crate::Encode>(value: T) -> T::CType
where
    T::Store: crate::stored::EmptyStore,
{
    unproject::<T::CType>(crate::encode(value))
}

/// Encode raw ABI bits while the generated call retains the conversion store.
#[doc(hidden)]
pub fn soft_encode_abi<T: crate::Encode>(value: T, store: &mut T::Store) -> T::CType {
    encode_field(value, store)
}
