include!("../support/mod.rs");
include!("../support/unstable_point.rs");

#[cfg(not(feature = "diplomat"))]
#[cfg_attr(feature = "abi_stable", abi_stable::sabi_extern_fn)]
#[cfg_attr(feature = "stabby", no_mangle)]
#[cfg_attr(feature = "stabby", stabby::stabby)]
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

#[cfg(feature = "abi_stable")]
abi_stable_signature!(read_point: (&Point) -> u32);

#[cfg(feature = "co3")]
fn main() {
    use CallerPoint as Point;

    for (x, y) in [(7, 1024), (13, 65537)] {
        let point = Point { x, y };

        let result = {
            unsafe extern "C" {
                #[link_name = "unstable_shared__read_point"]
                fn exported_read(point: &Point) -> u32;
            }
            unsafe { exported_read(&point) }
        };

        assert_eq!(result, y - u32::from(x));
    }
}

#[cfg(feature = "uniffi")]
fn main() {
    for (x, y) in [(7, 1024), (13, 65537)] {
        let point = Point { x, y };

        let result = {
            let input = <Point as uniffi::Lower<UniFfiTag>>::lower(point);
            let mut status = uniffi::RustCallStatus::default();
            let output = uniffi_co3_framework_comparisons_fn_func_read_point(input, &mut status);
            assert_eq!(status.code, uniffi::RustCallStatusCode::Success);
            output
        };

        assert_eq!(result, y - u32::from(x));
    }
}

#[cfg(feature = "diplomat")]
fn main() {
    use CallerPoint as Point;

    for (x, y) in [(7, 1024), (13, 65537)] {
        let point = Point { x, y };

        let result = {
            unsafe extern "C" {
                #[link_name = "diplomat_external_read_point"]
                fn exported_read(point: &Point) -> u32;
            }
            unsafe { exported_read(&point) }
        };

        assert_eq!(result, y - u32::from(x));
    }
}

#[cfg(feature = "interoptopus")]
fn main() {
    use CallerPoint as Point;

    for (x, y) in [(7, 1024), (13, 65537)] {
        let point = Point { x, y };

        let result = {
            unsafe extern "C" {
                #[link_name = "read_point"]
                fn exported_read(point: &Point) -> u32;
            }
            unsafe { exported_read(&point) }
        };

        assert_eq!(result, y - u32::from(x));
    }
}

#[cfg(feature = "safer_ffi")]
fn main() {
    for (x, y) in [(7, 1024), (13, 65537)] {
        let point = Point { x, y };

        let result = read_point(&point);

        assert_eq!(result, y - u32::from(x));
    }
}

#[cfg(feature = "boltffi")]
fn main() {
    for (x, y) in [(7, 1024), (13, 65537)] {
        let point = Point { x, y };

        let result = unsafe { boltffi_function_15unstable_shared10read_point(&point) };
        assert_eq!(result, y - u32::from(x));
    }
}

#[cfg(any(feature = "abi_stable", feature = "stabby"))]
fn main() {
    for (x, y) in [(7, 1024), (13, 65537)] {
        let point = Point { x, y };

        let result = {
            let exported_read: extern "C" fn(&Point) -> u32 = read_point;
            exported_read(&point)
        };

        assert_eq!(result, y - u32::from(x));
    }
}
