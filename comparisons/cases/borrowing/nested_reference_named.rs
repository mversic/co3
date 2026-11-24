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

fn main() {
    #[cfg(any(feature = "co3", feature = "safer_ffi"))]
    {
        unsafe extern "C" {
            #[cfg_attr(feature = "co3", link_name = "nested_reference_named__read_nested")]
            #[cfg_attr(feature = "safer_ffi", link_name = "read_nested")]
            fn exported_read(input: &&u32) -> u32;
        }
        for value in [17, 42] {
            assert_eq!(unsafe { exported_read(&&value) }, value);
        }
    }
}
