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

#[cfg(feature = "co3")]
fn main() {
    {
        unsafe extern "C" {
            #[link_name = "borrowed_return__borrow_first"]
            fn exported_borrow_first(first: &u32, second: &u32) -> *const u32;
        }
        let inputs = [7, 11];
        for (first, second) in [(&inputs[0], &inputs[1]), (&inputs[1], &inputs[0])] {
            assert!(std::ptr::eq(
                unsafe { exported_borrow_first(first, second) },
                first
            ));
        }
    }
}

#[cfg(any(
    feature = "uniffi",
    feature = "boltffi",
    feature = "abi_stable",
    feature = "stabby"
))]
fn main() {}

#[cfg(feature = "diplomat")]
fn main() {
    {
        unsafe extern "C" {
            #[link_name = "diplomat_external_borrow_first"]
            fn exported_borrow_first(first: &u32, second: &u32) -> *const u32;
        }
        let inputs = [7, 11];
        for (first, second) in [(&inputs[0], &inputs[1]), (&inputs[1], &inputs[0])] {
            assert!(std::ptr::eq(
                unsafe { exported_borrow_first(first, second) },
                first
            ));
        }
    }
}

#[cfg(any(feature = "interoptopus", feature = "safer_ffi"))]
fn main() {
    {
        unsafe extern "C" {
            #[link_name = "borrow_first"]
            fn exported_borrow_first(first: &u32, second: &u32) -> *const u32;
        }
        let inputs = [7, 11];
        for (first, second) in [(&inputs[0], &inputs[1]), (&inputs[1], &inputs[0])] {
            assert!(std::ptr::eq(
                unsafe { exported_borrow_first(first, second) },
                first
            ));
        }
    }
}
