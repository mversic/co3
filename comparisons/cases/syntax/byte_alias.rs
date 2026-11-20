// A type alias hides u8 from UniFFI's mutable byte-slice syntax check.
include!("../support/mod.rs");

pub type Byte = u8;

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn fill(buf: &mut [Byte]) {
    for (i, byte) in buf.iter_mut().enumerate() {
        *byte = i as u8;
    }
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub type Byte = u8;

    pub fn fill(buf: &mut [Byte]) {
        for (i, byte) in buf.iter_mut().enumerate() {
            *byte = i as u8;
        }
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn fill(buf: &mut [Byte]);
}

fn main() {}
