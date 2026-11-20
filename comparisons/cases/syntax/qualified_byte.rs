// A qualified primitive path misses the mutable byte-slice special case.
include!("../support/mod.rs");

use core::primitive;

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn fill(buf: &mut [primitive::u8]) {
    buf.fill(42);
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    use core::primitive;

    pub fn fill(buf: &mut [primitive::u8]) {
        buf.fill(42);
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn fill(buf: &mut [primitive::u8]);
}

fn main() {}
