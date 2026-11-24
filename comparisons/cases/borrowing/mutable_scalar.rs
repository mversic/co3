// General mutable scalar borrowing.
include!("../support/mod.rs");

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn increment(input: &mut u32) {
    *input += 1;
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub fn increment(input: &mut u32) {
        *input += 1;
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn increment(input: &mut u32);
}

fn main() {
    #[cfg(any(feature = "co3", feature = "diplomat", feature = "safer_ffi", feature = "interoptopus"))]
    {
        unsafe extern "C" {
            #[cfg_attr(feature = "co3", link_name = "mutable_scalar__increment")]
            #[cfg_attr(feature = "diplomat", link_name = "diplomat_external_increment")]
            #[cfg_attr(any(feature = "safer_ffi", feature = "interoptopus"), link_name = "increment")]
            fn exported_increment(value: &mut u32);
        }
        let mut value = 7;
        unsafe { exported_increment(&mut value) };
        assert_eq!(value, 8);
    }

    #[cfg(feature = "boltffi")]
    {
        // This entry point takes the integer by value and returns only the status.
        let status = unsafe { boltffi_function_14mutable_scalar9increment(7) };
        assert!(status.is_ok());
    }
}
