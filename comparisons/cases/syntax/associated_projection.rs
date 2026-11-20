// A concrete associated type projection resolving to u32.
include!("../support/mod.rs");

pub trait Element {
    type Item;
}

pub struct Model;
impl Element for Model {
    type Item = u32;
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn identity(input: <Model as Element>::Item) -> <Model as Element>::Item {
    input
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    use crate::{Element, Model};

    pub fn identity(input: <Model as Element>::Item) -> <Model as Element>::Item {
        input
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn identity(input: <Model as Element>::Item) -> <Model as Element>::Item;
}

fn main() {}
