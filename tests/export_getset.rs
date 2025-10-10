#![cfg(feature = "derive")]
#![cfg(feature = "getset")]

use std::mem::MaybeUninit;

use co3::ExternC;
use getset::{Getters, MutGetters, Setters};

#[derive(Debug, Clone, PartialEq, Eq, ExternC)]
pub struct Name(String);

#[co3::carbonate]
#[derive(Clone, Setters, Getters, MutGetters, ExternC)]
#[getset(get = "pub")]
pub struct FfiStruct {
    #[getset(set = "pub", get_mut = "pub")]
    id: u32,
    name: Name,
}

#[test]
#[webassembly_test::webassembly_test]
fn export_getset_get() {
    let init_name = Name("Name".to_owned());
    let ffi_struct = &mut FfiStruct {
        id: 1,
        name: init_name.clone(),
    };

    let mut id = MaybeUninit::<*mut u32>::new(core::ptr::null_mut());
    let mut name = MaybeUninit::<*const Name>::new(core::ptr::null());

    unsafe {
        FfiStruct__set_id(<*mut _>::from(ffi_struct), 2);
        assert_eq!(&2, ffi_struct.id());

        FfiStruct__id_mut(<*mut _>::from(ffi_struct), id.as_mut_ptr());
        let id = &mut *id.assume_init();
        assert_eq!(&mut 2, id);

        FfiStruct__name(ffi_struct, name.as_mut_ptr());
        let name = &*name.assume_init();

        assert_eq!(&init_name, name);
    }
}
