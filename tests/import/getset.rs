//
//#[extern_type]
//#[derive(Clone, PartialEq, Eq)]
//pub struct Name;
//
//#[extern_type]
//#[derive(Clone, PartialEq, Eq, Setters, Getters, MutGetters)]
//#[getset(get = "pub")]
//#[reprC(opaque)]
//#[repr(C)]
//pub struct FfiStruct {
//    #[getset(set = "pub", get_mut = "pub")]
//    id: u8,
//    // FIXME: How to tell getset return type for `name()` is ExternRef?
//    //name: Name,
//}
//
//#[extern_C]
//impl Name {
//    pub fn new(name: String) -> Self {
//        unreachable!("replaced by extern_C")
//    }
//}
//
//#[extern_C]
//impl FfiStruct {
//    pub fn new(name: String, id: u8) -> Self {
//        unreachable!("replaced by extern_C")
//    }
//}
//
////#[test]
////#[webassembly_test]
////fn import_shared_fns() {
////    let mut ffi_struct = FfiStruct::new("ipso facto".to_string(), 42);
////    ffi_struct.set_id(84);
////    assert!(&mut 84 == ffi_struct.id_mut());
////
////    assert!(Name::new("ipso facto".to_string()) == *ffi_struct.name());
////}
//
//mod ffi {
//    use std::alloc;
//
//    use co3::{
//        Decode, ExternC, FfiReturn, def_fns,
//        out_ptr::{OutPtr, OutPtrWrite},
//        slice::RawSliceMut,
//    };
//
//    co3::handles! {ExternName, ExternFfiStruct}
//
//    def_fns! { dealloc }
//    def_fns! {
//        Drop: {ExternName, ExternFfiStruct},
//        Clone: {ExternName, ExternFfiStruct},
//        Eq: {ExternName, ExternFfiStruct},
//    }
//
//    #[derive(Debug, Clone, PartialEq, Eq, ReprC)]
//    #[reprC(opaque)]
//    #[repr(C)]
//    pub struct ExternName(String);
//
//    #[derive(Debug, Clone, PartialEq, Eq, ReprC)]
//    #[reprC(opaque)]
//    #[repr(C)]
//    pub struct ExternFfiStruct {
//        id: u8,
//        name: ExternName,
//    }
//
//    #[unsafe(no_mangle)]
//    unsafe extern "C" fn Name__new(
//        input1: RawSliceMut<u8>,
//        output: *mut *mut ExternName,
//    ) -> FfiReturn {
//        unsafe {
//            let string = String::from_utf8(input1.into_rust().expect("Defined").to_vec());
//            let opaque = Box::new(ExternName(string.expect("Valid UTF8 string")));
//            output.write(Box::into_raw(opaque));
//        }
//
//        FfiReturn::Ok
//    }
//
//    #[unsafe(no_mangle)]
//    unsafe extern "C" fn FfiStruct__new(
//        input1: RawSliceMut<u8>,
//        input2: <u8 as ExternC>::CType,
//        output: *mut *mut ExternFfiStruct,
//    ) -> FfiReturn {
//        unsafe {
//            let string = String::from_utf8(input1.into_rust().expect("Defined").to_vec());
//            let num = Decode::decode(input2, &mut ()).expect("Valid num");
//            let name = ExternName(string.expect("Valid UTF8 string"));
//            let opaque = Box::new(ExternFfiStruct { id: num, name });
//
//            output.write(Box::into_raw(opaque));
//        }
//
//        FfiReturn::Ok
//    }
//
//    #[unsafe(no_mangle)]
//    unsafe extern "C" fn FfiStruct__id(
//        input: *const ExternFfiStruct,
//        output: *mut <&u8 as ExternC>::CType,
//    ) -> FfiReturn {
//        unsafe {
//            let input = &*input;
//            output.write(&input.id);
//        }
//
//        FfiReturn::Ok
//    }
//
//    #[unsafe(no_mangle)]
//    unsafe extern "C" fn FfiStruct__id_mut(
//        input: *mut ExternFfiStruct,
//        output: *mut <&mut u8 as ExternC>::CType,
//    ) -> FfiReturn {
//        unsafe {
//            let input = &mut *input;
//            output.write(&mut input.id);
//        }
//
//        FfiReturn::Ok
//    }
//
//    #[unsafe(no_mangle)]
//    unsafe extern "C" fn FfiStruct__set_id(
//        input: *mut ExternFfiStruct,
//        id: <u8 as ExternC>::CType,
//    ) -> FfiReturn {
//        unsafe {
//            let input = &mut *input;
//            input.id = Decode::decode(id, &mut ()).expect("Valid num");
//        }
//
//        FfiReturn::Ok
//    }
//
//    #[unsafe(no_mangle)]
//    unsafe extern "C" fn FfiStruct__name(
//        input: *const ExternFfiStruct,
//        output: *mut <&ExternName as OutPtr>::OutPtr,
//    ) -> FfiReturn {
//        unsafe {
//            let input = &*input;
//            OutPtrWrite::write_out(&input.name, output);
//        }
//
//        FfiReturn::Ok
//    }
//}
