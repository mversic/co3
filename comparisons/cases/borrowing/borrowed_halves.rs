// Return two slices borrowed from the same buffer.
include!("../support/mod.rs");

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn halves(input: &[u8]) -> (&[u8], &[u8]) {
    input.split_at(input.len() / 2)
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub fn halves(input: &[u8]) -> (&[u8], &[u8]) {
        input.split_at(input.len() / 2)
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn halves(input: &[u8]) -> (&[u8], &[u8]);
}

#[cfg(feature = "co3")]
mod caller {
    co3::ffi! {
        #![unsafe(extern("C"))]
        pub fn halves(input: &[u8]) -> (&[u8], &[u8]);
    }
}

#[cfg(feature = "co3")]
fn main() {
    {
        let data = [11, 22, 33, 44, 55];
        let (left, right) = caller::halves(&data);
        assert!(std::ptr::eq(left, &data[..2]));
        assert!(std::ptr::eq(right, &data[2..]));
    }
}

#[cfg(any(
    feature = "uniffi",
    feature = "diplomat",
    feature = "interoptopus",
    feature = "safer_ffi",
    feature = "boltffi",
    feature = "abi_stable",
    feature = "stabby"
))]
fn main() {}
