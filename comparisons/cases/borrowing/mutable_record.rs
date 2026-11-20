// Mutable references to user-defined records.
include!("../support/mod.rs");

#[derive(Clone, Copy)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "boltffi", boltffi::data)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::derive_ReprC)]
#[cfg_attr(feature = "co3", derive(co3::rust_spec::RustSpec, co3::ReprC))]
#[repr(C)]
pub struct Point {
    pub x: u32,
    pub y: u32,
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn update(point: &mut Point) {
    point.x += 1;
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    #[derive(Clone, Copy)]
    #[repr(C)]
    pub struct Point {
        pub x: u32,
        pub y: u32,
    }

    pub fn update(point: &mut Point) {
        point.x += 1;
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn update(point: &mut Point);
}

fn main() {}
