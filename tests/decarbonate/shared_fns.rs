use co3::{decarbonate, extern_type, external::ExternRef};
use webassembly_test::webassembly_test;

#[extern_type(link_crate = "decarbonate_")]
/// Struct without a repr attribute is opaque by default
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
// NOTE: Replaced by the [`extern_type`] macro
pub struct FfiStruct<T>;

#[decarbonate(link_name = "FfiStruct__new")]
pub fn ffi_struct_new(name: String) -> FfiStruct<bool> {
    unreachable!("replaced by decarbonate")
}

#[test]
#[webassembly_test]
fn import_shared_fns() {
    let ffi_struct = ffi_struct_new("ipso facto".to_string());
    let ref_ffi_struct: ExternRef<FfiStruct<_>> = ExternRef::new(&ffi_struct);
    let cloned_ffi_struct: FfiStruct<_> = Clone::clone(&ref_ffi_struct);

    assert!(*ref_ffi_struct == cloned_ffi_struct);
    assert!(*ref_ffi_struct >= cloned_ffi_struct);
}

mod ffi {
    use std::alloc;

    use co3::{ExternC, FfiReturn, slice::RawSliceMut};

    co3::handles! {ExternFfiStruct}

    co3::def_fns! {
        Drop: {ExternFfiStruct},
        Clone: {ExternFfiStruct},
        Eq: {ExternFfiStruct},
        Ord: {ExternFfiStruct}
    }

    co3::def_fns! { dealloc }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, ExternC)]
    #[mineral(opaque)]
    #[repr(C)]
    pub struct ExternFfiStruct(pub String);

    #[unsafe(no_mangle)]
    unsafe extern "C" fn FfiStruct__new(
        input: RawSliceMut<u8>,
        output: *mut *mut ExternFfiStruct,
    ) -> FfiReturn {
        unsafe {
            let string = String::from_utf8(input.into_rust().expect("Defined").to_vec());
            let opaque = Box::new(ExternFfiStruct(string.expect("Valid UTF8 string")));

            output.write(Box::into_raw(opaque));
        }

        FfiReturn::Ok
    }
}
