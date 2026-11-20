// A generic alias hides the slice constructor.
include!("../support/mod.rs");

pub type Buffer<T> = [T];

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn fill(buf: &mut Buffer<u8>) {
    buf.fill(42);
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub type Buffer<T> = [T];

    pub fn fill(buf: &mut Buffer<u8>) {
        buf.fill(42);
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn fill(buf: &mut Buffer<u8>);
}

fn main() {}
