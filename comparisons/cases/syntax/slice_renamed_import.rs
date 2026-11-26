// Same mutable slice as control_mutable_bytes, with the element spelling from renamed_import.
include!("../support/mod.rs");

use core::primitive::u8 as Byte;

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn fill(buf: &mut [Byte]) {
    buf.fill(42);
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    use core::primitive::u8 as Byte;

    pub fn fill(buf: &mut [Byte]) {
        buf.fill(42);
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

        #[symbol_name = "slice_renamed_import__fill"]
        fn exported_fill(buf: &mut [u8]);
    }

    let mut buf = [9, 8, 7];
    exported_fill(&mut buf);
    assert_eq!(buf, [42; 3]);
}

#[cfg(any(
    feature = "uniffi",
    feature = "diplomat",
    feature = "interoptopus",
    feature = "safer_ffi",
    feature = "boltffi"
))]
fn main() {}
