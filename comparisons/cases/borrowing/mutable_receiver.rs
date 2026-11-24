// Export increment(&mut self) on a record whose field is visible to the caller.



use co3::convert::{lang};

include!("../support/mod.rs");

#[cfg(not(any(feature = "diplomat", feature = "safer_ffi")))]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "boltffi", boltffi::data)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "co3", derive(co3::rust_spec::RustSpec, co3::ReprC))]
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Counter {
    pub input: u32,
}

#[cfg(not(any(feature = "diplomat", feature = "safer_ffi")))]
#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::data(impl))]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
impl Counter {
    pub fn increment(&mut self) {
        self.input += 1;
    }
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
use co3::convert::{lang};




    #[diplomat::attr(auto, mut_struct_ref)]
    #[repr(C)]
    pub struct Counter {
        pub input: u32,
    }

    impl Counter {
        pub fn increment(&mut self) {
            self.input += 1;
        }
    }
}

#[cfg(feature = "safer_ffi")]
compile_error!("This fixture needs a safer-ffi free-function adapter for the method.");

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    impl Counter {
        fn increment(&mut self);
    }
}

fn main() {
    #[cfg(any(feature = "co3", feature = "diplomat", feature = "boltffi"))]
    {
        #[cfg(feature = "diplomat")]
        use diplomat_case::Counter;
        let mut counter = Counter { input: 7 };
        for expected in [8, 9] {
            #[cfg(any(feature = "co3", feature = "diplomat"))]
            {
                unsafe extern "C" {
                    #[cfg_attr(feature = "co3", link_name = "mutable_receiver__Counter__increment")]
                    #[cfg_attr(feature = "diplomat", link_name = "Counter_increment")]
                    fn exported_increment(counter: &mut Counter);
                }
                unsafe { exported_increment(&mut counter) };
            }
            #[cfg(feature = "boltffi")]
            {
                // BoltFFI returns the updated record through a separate output pointer.
                let status = unsafe {
                    boltffi_method_record_16mutable_receiver7Counter9increment(
                        counter,
                        &mut counter,
                    )
                };
                assert!(status.is_ok());
            }
            assert_eq!(counter.input, expected);
        }
    }
}
