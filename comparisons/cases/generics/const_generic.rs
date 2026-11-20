// CO3 exports selected const-generic instantiations.
include!("../support/mod.rs");

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn first<const N: usize>(input: &[u8; N]) -> u8 {
    input.first().copied().unwrap_or_default()
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub fn first<const N: usize>(input: &[u8; N]) -> u8 {
        input.first().copied().unwrap_or_default()
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn first<const N: usize>(input: &[u8; N]) -> u8 where use<N> @ (<4> | <8>);
}

fn main() {}
