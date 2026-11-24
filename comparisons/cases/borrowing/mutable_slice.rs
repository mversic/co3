// Mutable slices of elements other than u8.
include!("../support/mod.rs");

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn fill(values: &mut [u32]) {
    values.fill(42);
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub fn fill(values: &mut [u32]) {
        values.fill(42);
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn fill(values: &mut [u32]);
}

#[cfg(feature = "co3")]
mod caller {
    co3::ffi! {
        #![unsafe(extern("C"))]
        pub fn fill(values: &mut [u32]);
    }
}

fn main() {
    #[cfg(any(feature = "co3", feature = "diplomat", feature = "boltffi"))]
    {
        let mut values = [500, 700, 900];
        #[cfg(feature = "co3")]
        caller::fill(&mut values);
        #[cfg(feature = "diplomat")]
        {
            unsafe extern "C" {
                fn diplomat_external_fill(values: diplomat_runtime::DiplomatSliceMut<'_, u32>);
            }
            unsafe { diplomat_external_fill(values.as_mut_slice().into()) };
        }
        #[cfg(feature = "boltffi")]
        assert!(unsafe {
            boltffi_function_13mutable_slice4fill(values.as_mut_ptr(), values.len())
        }
        .is_ok());
        assert_eq!(values, [42; 3]);
    }
}
