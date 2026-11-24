// Mutable references to user-defined records.



use co3::convert::{lang};

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
use co3::convert::{lang};




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

fn main() {
    #[cfg(any(
        feature = "co3",
        feature = "diplomat",
        feature = "interoptopus",
        feature = "safer_ffi",
        feature = "boltffi"
    ))]
    {
        #[cfg(feature = "diplomat")]
        use diplomat_case::Point;
        let mut point = Point { x: 7, y: 11 };
        #[cfg(any(
            feature = "co3",
            feature = "diplomat",
            feature = "safer_ffi",
            feature = "interoptopus"
        ))]
        {
            unsafe extern "C" {
                #[cfg_attr(feature = "co3", link_name = "mutable_record__update")]
                #[cfg_attr(feature = "diplomat", link_name = "diplomat_external_update")]
                #[cfg_attr(
                    any(feature = "safer_ffi", feature = "interoptopus"),
                    link_name = "update"
                )]
                fn exported_update(point: &mut Point);
            }
            unsafe { exported_update(&mut point) };
        }
        #[cfg(feature = "boltffi")]
        assert!(unsafe { boltffi_function_14mutable_record6update(&mut point) }.is_ok());
        assert_eq!((point.x, point.y), (8, 11));
    }
}
