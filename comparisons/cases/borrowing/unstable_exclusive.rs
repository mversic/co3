include!("../support/mod.rs");
// The record under test has no explicit representation.

#[cfg(not(feature = "diplomat"))]
#[cfg_attr(feature = "co3", derive(co3::rust_spec::RustSpec, co3::ReprC))]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::derive_ReprC)]
#[cfg_attr(feature = "boltffi", derive(Clone, Copy))]
#[cfg_attr(feature = "boltffi", boltffi::data)]
pub struct Point {
    pub x: u8,
    pub y: u32,
}

// The C caller declares its own record in field order.
#[cfg(any(feature = "co3", feature = "diplomat", feature = "interoptopus"))]
#[cfg_attr(feature = "co3", derive(co3::rust_spec::RustSpec, co3::ReprC))]
#[repr(C)]
struct CallerPoint {
    x: u8,
    y: u32,
}

#[cfg(not(feature = "diplomat"))]
#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
pub fn update(point: &mut Point) {
    point.y += u32::from(point.x);
    point.x += 1;
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    #[diplomat::attr(auto, mut_struct_ref)]
    pub struct Point {
        pub x: u8,
        pub y: u32,
    }

    pub fn update(point: &mut Point) {
        point.y += u32::from(point.x);
        point.x += 1;
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]
    fn update(#[soft] point: &mut Point);
}

#[cfg(feature = "co3")]
fn main() {
    co3::ffi! {
        #![unsafe(extern("C"))]

        #[symbol_name = "unstable_exclusive__update"]
        fn imported_update(point: &mut CallerPoint);
    }

    validate(imported_update);
}

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

#[cfg(feature = "interoptopus")]
fn main() {
    unsafe extern "C" {
        #[link_name = "update"]
        fn exported_update(point: &mut TestPoint);
    }
    validate(|point| {
        unsafe { exported_update(point) };
    });
}

#[cfg(feature = "safer_ffi")]
fn main() {
    validate(|point| {
        update(point);
    });
}

#[cfg(feature = "boltffi")]
fn main() {
    validate(|point| {
        assert!(unsafe { boltffi_function_18unstable_exclusive6update(point) }.is_ok());
    });
}

#[cfg(feature = "uniffi")]
fn main() {}

#[cfg(any(feature = "co3", feature = "diplomat", feature = "interoptopus"))]
type TestPoint = CallerPoint;
#[cfg(not(any(feature = "co3", feature = "diplomat", feature = "interoptopus")))]
type TestPoint = Point;

fn validate(mut call: impl FnMut(&mut TestPoint)) {
    let mut point = TestPoint { x: 7, y: 1024 };
    for expected in [(8, 1031), (9, 1039)] {
        call(&mut point);
        assert_eq!((point.x, point.y), expected);
    }
}
