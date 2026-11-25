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

#[cfg(feature = "co3")]
fn main() {
    {
        let mut values = [500, 700, 900];

        caller::fill(&mut values);

        assert_eq!(values, [42; 3]);
    }
}

#[cfg(any(
    feature = "uniffi",
    feature = "interoptopus",
    feature = "safer_ffi",
    feature = "abi_stable",
    feature = "stabby"
))]
fn main() {}

#[cfg(feature = "diplomat")]
fn main() {
    {
        let mut values = [500, 700, 900];

        {
            unsafe extern "C" {
                fn diplomat_external_fill(values: diplomat_runtime::DiplomatSliceMut<'_, u32>);
            }
            unsafe { diplomat_external_fill(values.as_mut_slice().into()) };
        }

        assert_eq!(values, [42; 3]);
    }
}

#[cfg(feature = "boltffi")]
fn main() {
    {
        let mut values = [500, 700, 900];

        assert!(
            unsafe { boltffi_function_13mutable_slice4fill(values.as_mut_ptr(), values.len()) }
                .is_ok()
        );
        assert_eq!(values, [42; 3]);
    }
}
