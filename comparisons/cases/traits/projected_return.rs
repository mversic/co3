// Export a selected implementation whose return type is a trait projection.
include!("../support/mod.rs");

pub trait Source {
    type Item;
    fn read(&self) -> Self::Item;
}

impl Source for u32 {
    type Item = u64;
    fn read(&self) -> u64 {
        u64::from(*self)
    }
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn read<T: Source>(input: &T) -> <T as Source>::Item {
    input.read()
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    use crate::Source;

    pub fn read<T: Source>(input: &T) -> <T as Source>::Item {
        input.read()
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn read<T: Source>(input: &T) -> <T as Source>::Item where use<T> @ (<u32>);
}

fn main() {}
