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

#[cfg(feature = "co3")]
fn main() {
    co3::ffi! {
        #![unsafe(extern("C"))]

        #[symbol_name = "byte_alias__fill"]
        fn exported_fill(buf: &mut [u8]);
    }

    let mut buf = [9, 8, 7];
    exported_fill(&mut buf);
    assert_eq!(buf, [0, 1, 2]);
}

#[cfg(any(
    feature = "uniffi",
    feature = "diplomat",
    feature = "interoptopus",
    feature = "safer_ffi",
    feature = "boltffi"
))]
fn main() {}
