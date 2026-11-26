// The inner reference outlives the outer borrow, explicitly naming Rust's implied bound.
include!("../support/mod.rs");

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn read_nested<'outer, 'inner: 'outer>(input: &'outer &'inner u32) -> u32 {
    **input
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub fn read_nested<'outer, 'inner: 'outer>(input: &'outer &'inner u32) -> u32 {
        **input
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn read_nested<'outer, 'inner: 'outer>(input: &'outer &'inner u32) -> u32;
}

#[cfg(feature = "co3")]
fn main() {
    co3::ffi! {
        #![unsafe(extern("C"))]

        #[symbol_name = "nested_reference_named__read_nested"]
        fn imported_read<'outer, 'inner: 'outer>(input: &'outer &'inner u32) -> u32;
    }

    validate(|input| imported_read(input));
}

#[cfg(any(
    feature = "uniffi",
    feature = "diplomat",
    feature = "interoptopus",
    feature = "boltffi"
))]
fn main() {}

#[cfg(feature = "safer_ffi")]
fn main() {
    unsafe extern "C" {
        #[link_name = "read_nested"]
        fn exported_read(input: &&u32) -> u32;
    }
    validate(|input| unsafe { exported_read(input) });
}

fn validate(mut call: impl FnMut(&&u32) -> u32) {
    for value in [17, 42] {
        assert_eq!(call(&&value), value);
    }
}
