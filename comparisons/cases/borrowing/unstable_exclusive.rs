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

#[cfg(feature = "abi_stable")]
abi_stable_signature!(update: (&mut Point) -> ());

#[cfg(not(feature = "uniffi"))]
fn main() {
    #[cfg(any(feature = "co3", feature = "diplomat", feature = "interoptopus"))]
    use CallerPoint as Point;

    let mut point = Point { x: 7, y: 1024 };
    for expected in [(8, 1031), (9, 1039)] {
        #[cfg(any(feature = "co3", feature = "diplomat", feature = "interoptopus"))]
        {
            unsafe extern "C" {
                #[cfg_attr(feature = "co3", link_name = "unstable_exclusive__update")]
                #[cfg_attr(feature = "diplomat", link_name = "diplomat_external_update")]
                #[cfg_attr(feature = "interoptopus", link_name = "update")]
                fn exported_update(point: &mut Point);
            }
            unsafe { exported_update(&mut point) };
        }
        #[cfg(any(feature = "abi_stable", feature = "stabby"))]
        {
            let exported_update: extern "C" fn(&mut Point) = update;
            exported_update(&mut point);
        }
        #[cfg(feature = "safer_ffi")]
        update(&mut point);
        #[cfg(feature = "boltffi")]
        assert!(unsafe { boltffi_function_18unstable_exclusive6update(&mut point) }.is_ok());
        assert_eq!((point.x, point.y), expected);
    }
}

#[cfg(feature = "uniffi")]
fn main() {}
