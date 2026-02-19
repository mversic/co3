use co3::external::ExternRef;
use webassembly_test::webassembly_test;

co3::handles! {FfiStruct<bool>}
co3::decl_fns! {Drop, Clone, Eq, Ord}

#[co3::extern_type]
/// Struct without a repr attribute is opaque by default
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
// NOTE: Replaced by the [`co3::extern_type`] macro
pub struct FfiStruct<T>;

#[co3::decarbonate]
impl FfiStruct<bool> {
    pub fn new(name: String) -> Self {
        unreachable!("replaced by co3::decarbonate")
    }
}

#[test]
#[webassembly_test]
fn import_shared_fns() {
    let ffi_struct = FfiStruct::new("ipso facto".to_string());
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
