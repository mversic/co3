use crate::{Decode, Encode, ExternC, Store};

/// Encode (`Drop`) types as shared references instead of transferring ownership.
///
/// With the notable exception of `Opaque` types which always carry ownership, this feature
/// prevents ownership transfer of dynamically allocated memory across FFI boundary.
///
/// # Performance Considerations
///
/// This feature introduces otherwise unnecessary clone on decode paths of `Drop` types
pub trait RefExternC: ExternC {
    type RefCType;
}

pub trait EncodeAsRef: Encode + RefExternC {
    type RefStore: Store + Default;

    fn encode_as_ref<'itm>(self, store: &'itm mut Self::RefStore) -> Self::RefCType
    where
        Self: 'itm;
}

pub trait DecodeFromRef<'d>: Decode<'d> + RefExternC<RefCType: 'd> {
    type RefStore: Store + Default;
    ///
    /// # Safety
    ///
    /// Refer to [`Decode`]
    unsafe fn decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::RefStore,
    ) -> Option<Self>;
}
