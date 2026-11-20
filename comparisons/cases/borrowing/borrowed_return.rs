// The result borrows from one specific argument.
include!("../support/mod.rs");

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn borrow_first<'a>(first: &'a u32, _second: &u32) -> &'a u32 {
    first
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub fn borrow_first<'a>(first: &'a u32, _second: &u32) -> &'a u32 {
        first
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn borrow_first<'a>(first: &'a u32, _second: &u32) -> &'a u32;
}

fn main() {}
