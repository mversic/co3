// Mutable references to user-defined records.

include!("../support/mod.rs");

#[derive(Clone, Copy)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "boltffi", boltffi::data)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::derive_ReprC)]
#[cfg_attr(feature = "co3", derive(co3::rust_spec::RustSpec, co3::ReprC))]
#[repr(C)]
pub struct Point {
    pub x: u32,
    pub y: u32,
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn update(point: &mut Point) {
    point.x += 1;
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {

    #[diplomat::attr(auto, mut_struct_ref)]
    #[derive(Clone, Copy)]
    #[repr(C)]
    pub struct Point {
        pub x: u32,
        pub y: u32,
    }

    pub fn update(point: &mut Point) {
        point.x += 1;
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn update(point: &mut Point);
}

#[cfg(feature = "co3")]
fn main() {
    co3::ffi! {
        #![unsafe(extern("C"))]

        #[symbol_name = "mutable_record__update"]
        fn imported_update(point: &mut Point);
    }

    validate(imported_update);
}

#[cfg(feature = "uniffi")]
fn main() {}

#[cfg(feature = "diplomat")]
fn main() {
    unsafe extern "C" {
        #[link_name = "diplomat_external_update"]
        fn exported_update(point: &mut TestPoint);
    }
    validate(|point| {
        unsafe { exported_update(point) };
    });
}

#[cfg(any(feature = "interoptopus", feature = "safer_ffi"))]
fn main() {
    unsafe extern "C" {
        #[link_name = "update"]
        fn exported_update(point: &mut TestPoint);
    }
    validate(|point| {
        unsafe { exported_update(point) };
    });
}

#[cfg(feature = "boltffi")]
fn main() {
    validate(|point| {
        assert!(unsafe { boltffi_function_14mutable_record6update(point) }.is_ok());
    });
}

#[cfg(feature = "diplomat")]
type TestPoint = diplomat_case::Point;
#[cfg(not(feature = "diplomat"))]
type TestPoint = Point;

fn validate(call: impl FnOnce(&mut TestPoint)) {
    let mut point = TestPoint { x: 7, y: 11 };
    call(&mut point);
    assert_eq!((point.x, point.y), (8, 11));
}
