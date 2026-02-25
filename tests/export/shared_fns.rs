use std::{cmp::Ordering, mem::MaybeUninit};

use co3::{Decode, Encode, ExternC, FfiReturn, ReprC, def_fns, export, out_ptr::OutPtrRead};
use webassembly_test::webassembly_test;

co3::handles! {FfiStruct1, FfiStruct2}

def_fns! {
    Drop: {FfiStruct1, FfiStruct2},
    Clone: {FfiStruct1, FfiStruct2},
    Eq: {FfiStruct1, FfiStruct2},
    Ord: {FfiStruct1, FfiStruct2}
}

/// Struct without a repr attribute is opaque by default
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, ReprC)]
pub struct FfiStruct1 {
    name: String,
}

/// Struct with a repr attribute can be forced to become opaque with `#[repr_C(opaque)]`
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, ReprC)]
#[repr_C(opaque)]
#[repr(C)]
pub struct FfiStruct2 {
    name: String,
}

#[export("C")]
impl FfiStruct1 {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}

#[test]
#[webassembly_test]
fn export_shared_fns() {
    use co3::handle::Handle as _;

    let name = String::from("X");

    let ffi_struct1 = unsafe {
        let mut ffi_struct = MaybeUninit::new(core::ptr::null_mut());
        let mut store = Default::default();
        assert_eq! {FfiReturn::Ok, FfiStruct1__new(name.clone().encode(&mut store), ffi_struct.as_mut_ptr())};
        let ffi_struct = ffi_struct.assume_init();
        assert!(!ffi_struct.is_null());
        assert_eq!(FfiStruct1 { name }, *ffi_struct);
        ffi_struct
    };

    unsafe {
        let cloned = {
            let mut cloned = MaybeUninit::<*mut FfiStruct1>::new(core::ptr::null_mut());

            __co3_export::clone(
                FfiStruct1::ID.encode(&mut ()),
                ffi_struct1.cast(),
                cloned.as_mut_ptr().cast(),
            );

            let cloned = Decode::decode(cloned.assume_init(), &mut ()).unwrap();
            assert_eq!(*ffi_struct1, cloned);

            cloned
        };

        let mut is_equal = MaybeUninit::new(1);
        let cloned_ptr = (&cloned).encode(&mut ());

        __co3_export::eq(
            FfiStruct1::ID.encode(&mut ()),
            ffi_struct1.cast(),
            cloned_ptr.cast(),
            is_equal.as_mut_ptr(),
        );
        let is_equal: bool = OutPtrRead::try_read_out(is_equal.assume_init()).unwrap();
        assert!(is_equal);

        let mut ordering = MaybeUninit::new(1);
        __co3_export::ord(
            FfiStruct1::ID.encode(&mut ()),
            ffi_struct1.cast(),
            cloned_ptr.cast(),
            ordering.as_mut_ptr(),
        );
        let ordering: Ordering = OutPtrRead::try_read_out(ordering.assume_init()).unwrap();
        assert_eq!(ordering, Ordering::Equal);

        assert_eq!(
            FfiReturn::Ok,
            __co3_export::drop(FfiStruct1::ID.encode(&mut ()), ffi_struct1.cast())
        );
        assert_eq!(
            FfiReturn::Ok,
            __co3_export::drop(
                FfiStruct1::ID.encode(&mut ()),
                cloned.encode(&mut ()).cast()
            )
        );
    }
}
