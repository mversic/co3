// An alias hides both the slice and its mutable reference.
include!("../support/mod.rs");

pub type BytesMut<'a> = &'a mut [u8];

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn fill(buf: BytesMut<'_>) {
    buf.fill(42);
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub type BytesMut<'a> = &'a mut [u8];

    pub fn fill(buf: BytesMut<'_>) {
        buf.fill(42);
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn fill(buf: BytesMut<'_>);
}

fn main() {}
