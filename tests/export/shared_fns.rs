use std::{cmp::Ordering, ffi::c_void};

use co3::{boxed::CBoxedSlice, ffi};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FfiStruct {
    name: String,
}

ffi! {
    #![unsafe(export("C"))]
    #![symbol_prefix = "export_shared"]

    type FfiStruct;

    impl FfiStruct {
        fn new(name: move String) -> move Box<Self>;
        fn clone_box(&self) -> move Box<Self>;
        fn equals(&self, other: &Self) -> bool;
        fn compare(&self, other: &Self) -> Ordering;
    }

    impl Drop for FfiStruct {
        fn drop(&mut self);
    }
}

impl FfiStruct {
    fn new(name: String) -> Box<Self> {
        Box::new(Self { name })
    }

    fn clone_box(&self) -> Box<Self> {
        Box::new(self.clone())
    }

    fn equals(&self, other: &Self) -> bool {
        self == other
    }

    fn compare(&self, other: &Self) -> Ordering {
        self.cmp(other)
    }
}

unsafe extern "C" {
    #[link_name = "export_shared__FfiStruct__new"]
    fn new_raw(name: CBoxedSlice<u8>) -> *mut c_void;

    #[link_name = "export_shared__FfiStruct__clone_box"]
    fn clone_raw(value: *const c_void) -> *mut c_void;

    #[link_name = "export_shared__FfiStruct__equals"]
    fn equals_raw(value: *const c_void, other: *const c_void) -> u8;

    #[link_name = "export_shared__FfiStruct__compare"]
    fn compare_raw(value: *const c_void, other: *const c_void) -> i8;

    #[link_name = "export_shared__Drop__FfiStruct__drop"]
    fn drop_raw(value: *mut c_void);
}

#[test]
fn opaque_helpers_cross_exported_abi() {
    let value = unsafe { new_raw(co3::encode(String::from("X"))) };
    assert!(!value.is_null());

    let cloned = unsafe { clone_raw(value) };
    assert!(!cloned.is_null());

    let equal = unsafe { equals_raw(value, cloned) };
    assert_eq!(Some(true), unsafe { co3::decode(equal) });

    let ordering = unsafe { compare_raw(value, cloned) };
    assert_eq!(Some(Ordering::Equal), unsafe { co3::decode(ordering) });

    unsafe {
        drop_raw(value);
        drop_raw(cloned);
    }
}
