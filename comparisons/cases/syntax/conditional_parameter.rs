// The inactive optional parameter disappears before Rust type checking.
include!("../support/mod.rs");

#[cfg(feature = "tracing")]
pub type TraceId = u32;

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn event_id(input: u32, #[cfg(feature = "tracing")] _trace_id: crate::TraceId) -> u32 {
    input
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub fn event_id(input: u32, #[cfg(feature = "tracing")] _trace_id: crate::TraceId) -> u32 {
        input
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn event_id(input: u32, #[cfg(feature = "tracing")] _trace_id: crate::TraceId) -> u32;
}

fn main() {}
