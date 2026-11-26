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
pub fn read_point(point: &Point) -> u32 {
    point.y - u32::from(point.x)
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub struct Point {
        pub x: u8,
        pub y: u32,
    }

    pub fn read_point(point: &Point) -> u32 {
        point.y - u32::from(point.x)
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]
    fn read_point(#[soft] point: &Point) -> u32;
}

#[cfg(feature = "co3")]
fn main() {
    co3::ffi! {
        #![unsafe(extern("C"))]

        #[symbol_name = "unstable_shared__read_point"]
        fn imported_read(point: &CallerPoint) -> u32;
    }

    validate(|x, y| imported_read(&CallerPoint { x, y }));
}

#[cfg(feature = "uniffi")]
fn main() {
    validate(|x, y| {
        let point = Point { x, y };

        let result = {
            let input = <Point as uniffi::Lower<UniFfiTag>>::lower(point);
            let mut status = uniffi::RustCallStatus::default();
            let output = uniffi_co3_framework_comparisons_fn_func_read_point(input, &mut status);
            assert_eq!(status.code, uniffi::RustCallStatusCode::Success);
            output
        };

        result
    });
}

#[cfg(feature = "diplomat")]
fn main() {
    use CallerPoint as Point;

    validate(|x, y| {
        let point = Point { x, y };

        let result = {
            unsafe extern "C" {
                #[link_name = "diplomat_external_read_point"]
                fn exported_read(point: &Point) -> u32;
            }
            unsafe { exported_read(&point) }
        };

        result
    });
}

#[cfg(feature = "interoptopus")]
fn main() {
    use CallerPoint as Point;

    validate(|x, y| {
        let point = Point { x, y };

        let result = {
            unsafe extern "C" {
                #[link_name = "read_point"]
                fn exported_read(point: &Point) -> u32;
            }
            unsafe { exported_read(&point) }
        };

        result
    });
}

#[cfg(feature = "safer_ffi")]
fn main() {
    validate(|x, y| {
        let point = Point { x, y };

        let result = read_point(&point);

        result
    });
}

#[cfg(feature = "boltffi")]
fn main() {
    validate(|x, y| {
        let point = Point { x, y };

        let result = unsafe { boltffi_function_15unstable_shared10read_point(&point) };
        result
    });
}

fn validate(mut call: impl FnMut(u8, u32) -> u32) {
    for (x, y) in [(7, 1024), (13, 65537)] {
        assert_eq!(call(x, y), y - u32::from(x));
    }
}
