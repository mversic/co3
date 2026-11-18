//! Utilities for opaque pointer handles required for tagged dispatch.

use crate::convert::Encode;

/// Groups tagged types that use the same tag type.
pub trait TagFamily {
    type Kind: Encode + Copy;
}

/// Provides a tag identifying a concrete type during tagged dispatch.
///
/// # Safety
///
/// If two types in the same tag family use the same tag, dispatch may reinterpret a value as the
/// wrong type.
pub unsafe trait Tagged: TagFamily {
    /// Unique tag of the type within its tag family.
    const TAG: Self::Kind;
}
