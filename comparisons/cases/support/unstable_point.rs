// The record under test has no explicit representation.



use co3::convert::{lang};

#[cfg(not(feature = "diplomat"))]
#[cfg_attr(feature = "co3", derive(co3::rust_spec::RustSpec, co3::ReprC))]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::derive_ReprC)]
#[cfg_attr(feature = "boltffi", derive(Clone, Copy))]
#[cfg_attr(feature = "boltffi", boltffi::data)]
#[cfg_attr(feature = "abi_stable", derive(abi_stable::StableAbi))]
#[cfg_attr(feature = "stabby", stabby::stabby)]
pub struct Point {
    pub x: u8,
    pub y: u32,
}

// The C caller declares its own record in field order.
#[cfg(any(feature = "co3", feature = "diplomat", feature = "interoptopus"))]
#[repr(C)]
struct CallerPoint {
    x: u8,
    y: u32,
}
