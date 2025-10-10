use std::mem::MaybeUninit;

use co3::{ExternC, FfiConvert};
use getset::Getters;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ExternC)]
pub struct GenericFfiStruct<T>(T);

#[cfg_attr(feature = "getset", co3::carbonate)]
#[derive(Clone, Copy, Getters, ExternC)]
#[getset(get = "pub")]
pub struct FfiStruct {
    inner: GenericFfiStruct<bool>,
}

#[co3::carbonate]
pub fn freestanding(input: GenericFfiStruct<String>) -> GenericFfiStruct<String> {
    input
}

#[test]
#[cfg(feature = "getset")]
#[webassembly_test::webassembly_test]
fn get_return_generic() {
    let ffi_struct = &FfiStruct {
        inner: GenericFfiStruct(true),
    };
    let mut output = MaybeUninit::<*const GenericFfiStruct<bool>>::new(core::ptr::null());

    unsafe {
        FfiStruct__inner(ffi_struct.encode(&mut ()), output.as_mut_ptr());
        assert_eq!(
            FfiConvert::decode(output.assume_init(), &mut ()),
            Ok(&ffi_struct.inner)
        );
    }
}

#[test]
#[webassembly_test::webassembly_test]
fn freestanding_accept_and_return_generic() {
    let inner = GenericFfiStruct(String::from("hello world"));
    let mut output = MaybeUninit::<*mut GenericFfiStruct<String>>::new(core::ptr::null_mut());

    unsafe {
        __freestanding(inner.clone().encode(&mut ()), output.as_mut_ptr());
        assert_eq!(FfiConvert::decode(output.assume_init(), &mut ()), Ok(inner));
    }
}
