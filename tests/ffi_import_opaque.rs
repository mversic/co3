#![cfg(feature = "derive")]
use std::collections::BTreeMap;

use co3::external::ExternRef;

co3::handles! {OpaqueStruct, Value}
co3::decl_fns! {Drop, Clone, Eq}

#[co3::extern_type]
#[derive(Clone, PartialEq, Eq)]
// NOTE: struct's body is replaced by co3!
pub struct Value;

#[co3::extern_type]
#[derive(Clone, PartialEq, Eq)]
// NOTE: struct's body is replaced by co3!
pub struct OpaqueStruct;

#[co3::decarbonate]
impl Value {
    pub fn new(input: String) -> Self {
        unreachable!("replaced by co3::decarbonate")
    }
}

#[co3::decarbonate]
impl OpaqueStruct {
    pub fn new(name: u8) -> Self {
        unreachable!("replaced by co3::decarbonate")
    }

    #[must_use]
    pub fn with_params(self, params: impl IntoIterator<Item = (u8, Value)>) -> OpaqueStruct {
        unreachable!("replaced by co3::decarbonate")
    }

    pub fn get_param(&self, name: &u8) -> Option<&Value> {
        unreachable!("replaced by co3::decarbonate")
    }

    pub fn params(&self) -> impl ExactSizeIterator<Item = &Value> {
        unreachable!("replaced by co3::decarbonate")
    }

    pub fn fallible_int_output(flag: bool) -> Result<u8, &'static str> {
        unreachable!("replaced by co3::decarbonate")
    }
}

#[co3::decarbonate]
pub fn freestanding_returns_opaque_item(input: &OpaqueStruct) -> &OpaqueStruct {
    unreachable!("replaced by co3::decarbonate")
}

#[co3::decarbonate]
pub fn freestanding_returns_opaque_boxed_ref(input: &Box<OpaqueStruct>) -> &Box<OpaqueStruct> {
    unreachable!("replaced by co3::decarbonate")
}

#[co3::decarbonate]
pub fn some_fn(input: &Vec<OpaqueStruct>) {
    unreachable!("replaced by co3::decarbonate")
}

fn make_new_opaque(name: u8, params: BTreeMap<u8, Value>) -> OpaqueStruct {
    let opaque = OpaqueStruct::new(name);
    opaque.with_params(params.into_iter().collect())
}

#[test]
#[webassembly_test::webassembly_test]
fn constructor() {
    let name = 42_u8;

    let opaque = OpaqueStruct::new(name);
    let mut expected_result = ffi::ExternOpaqueStruct {
        name: Some(name),
        tokens: vec![],
        params: Default::default(),
    };
    let opaque: &mut ffi::ExternOpaqueStruct = unsafe { core::mem::transmute(opaque) };
    assert_eq!(&mut expected_result, opaque);
}

#[test]
#[webassembly_test::webassembly_test]
fn return_option_ref() {
    let name = 42_u8;

    let value: Value = Value::new("Dummy param value".to_owned());
    let mut params = BTreeMap::default();
    params.insert(name, value.clone());

    let opaque = make_new_opaque(name, params);

    let param: Option<ExternRef<Value>> = opaque.get_param(&name);
    compare_opaque_eq::<_, ffi::ExternValue>(&value, &param.expect("Defined"));
}

#[test]
#[webassembly_test::webassembly_test]
fn take_and_return_opaque_ref() {
    let name = 42u8;
    let value: Value = Value::new("Dummy param value".to_owned());
    let mut params = BTreeMap::default();
    params.insert(name, value);

    let opaque: OpaqueStruct = make_new_opaque(name, params);

    let opaque_ref: ExternRef<OpaqueStruct> = freestanding_returns_opaque_item(&opaque);
    compare_opaque_eq::<_, ffi::ExternOpaqueStruct>(&opaque, &opaque_ref);
}

#[test]
#[webassembly_test::webassembly_test]
fn take_and_return_opaque_boxed_ref() {
    let name = 42u8;
    let value: Value = Value::new("Dummy param value".to_owned());
    let mut params = BTreeMap::default();
    params.insert(name, value);

    let opaque: Box<OpaqueStruct> = Box::new(make_new_opaque(name, params));
    let opaque_ref: ExternRef<OpaqueStruct> = freestanding_returns_opaque_item(&opaque);
    compare_opaque_eq::<_, ffi::ExternOpaqueStruct>(&opaque, &opaque_ref);
}

#[test]
#[webassembly_test::webassembly_test]
fn fallible_output() {
    assert_eq!(Ok(42), OpaqueStruct::fallible_int_output(true));
    // TODO:
    //assert!(OpaqueStruct::fallible_int_output(false).is_err());
}

fn compare_opaque_eq<T, U: PartialEq + core::fmt::Debug>(opaque1: &T, opaque2: &T) {
    unsafe {
        let opaque1: &*const U = &*(core::ptr::from_ref(opaque1)).cast::<*const U>();
        let opaque2: &*const U = &*(core::ptr::from_ref(opaque2)).cast::<*const U>();

        assert_eq!(**opaque1, **opaque2)
    }
}

mod ffi {
    use std::{alloc, collections::BTreeMap};

    use co3::{
        Decode, Encode, ExternC, FfiReturn,
        out_ptr::{OutPtr, OutPtrWrite},
        slice::RefMutSlice,
    };

    co3::handles! {ExternOpaqueStruct, ExternValue}

    co3::def_fns! {
        Drop: { ExternValue, ExternOpaqueStruct },
        Clone: { ExternValue },
        Eq: { ExternValue, ExternOpaqueStruct },
    }

    co3::def_fns! { dealloc }

    #[derive(Debug, Clone, PartialEq, Eq, ExternC)]
    #[mineral(opaque)]
    #[repr(C)]
    pub struct ExternValue(pub String);

    #[derive(Debug, PartialEq, Eq, ExternC)]
    #[mineral(opaque)]
    #[repr(C)]
    pub struct ExternOpaqueStruct {
        pub name: Option<u8>,
        pub tokens: Vec<ExternValue>,
        pub params: BTreeMap<u8, ExternValue>,
    }

    #[unsafe(no_mangle)]
    unsafe extern "C" fn Value__new(
        input: RefMutSlice<u8>,
        output: *mut *mut ExternValue,
    ) -> FfiReturn {
        unsafe {
            let string = String::from_utf8(input.into_rust().expect("Defined").to_vec());
            let opaque = Box::new(ExternValue(string.expect("Valid UTF8 string")));

            output.write(Box::into_raw(opaque));
        }

        FfiReturn::Ok
    }

    #[unsafe(no_mangle)]
    unsafe extern "C" fn OpaqueStruct__new(
        name: <u8 as co3::ExternC>::CType,
        output: *mut *mut ExternOpaqueStruct,
    ) -> FfiReturn {
        unsafe {
            let opaque = Box::new(ExternOpaqueStruct {
                name: Some(Decode::decode(name, &mut ()).expect("Valid num")),
                tokens: vec![],
                params: Default::default(),
            });

            output.write(Box::into_raw(opaque));
        }

        FfiReturn::Ok
    }

    #[unsafe(no_mangle)]
    unsafe extern "C" fn OpaqueStruct__with_params(
        handle: *mut ExternOpaqueStruct,
        params: <Vec<(u8, ExternValue)> as co3::ExternC>::CType,
        output: *mut *mut ExternOpaqueStruct,
    ) -> co3::FfiReturn {
        unsafe {
            let mut handle = *Box::from_raw(handle);
            let mut store = Default::default();

            let params: Vec<(u8, ExternValue)> = Decode::decode(params, &mut store).expect("Valid");

            handle.params = params.into_iter().collect();
            output.write(Box::into_raw(Box::new(handle)));
        }
        FfiReturn::Ok
    }

    #[unsafe(no_mangle)]
    unsafe extern "C" fn OpaqueStruct__get_param(
        handle: *const ExternOpaqueStruct,
        param_name: <&u8 as ExternC>::CType,
        output: *mut *const ExternValue,
    ) -> FfiReturn {
        unsafe {
            let handle = handle.as_ref().expect("Valid");
            let param_name = param_name.as_ref().expect("Valid");
            let value = handle.params.get(param_name);
            OutPtrWrite::write_out(value, output);
        }

        FfiReturn::Ok
    }

    #[unsafe(no_mangle)]
    unsafe extern "C" fn OpaqueStruct__params(
        handle: *const ExternOpaqueStruct,
        output: *mut <Vec<&ExternValue> as OutPtr>::OutPtr,
    ) -> FfiReturn {
        unsafe {
            let handle = handle.as_ref().expect("Valid");
            let params: Vec<_> = handle.params.values().collect();
            OutPtrWrite::write_out(params, output);
        }

        FfiReturn::Ok
    }

    #[unsafe(no_mangle)]
    unsafe extern "C" fn OpaqueStruct__remove_param(
        handle: *mut ExternOpaqueStruct,
        param_name: <&u8 as ExternC>::CType,
        output: *mut *mut ExternValue,
    ) -> FfiReturn {
        unsafe {
            let handle = handle.as_mut().expect("Valid");
            let param_name = param_name.as_ref().expect("Valid");

            output.write(handle.params.remove(param_name).encode(&mut ()));
        }

        FfiReturn::Ok
    }

    #[unsafe(no_mangle)]
    unsafe extern "C" fn OpaqueStruct__fallible_int_output(
        input: <bool as ExternC>::CType,
        output: *mut <u8 as OutPtr>::OutPtr,
    ) -> FfiReturn {
        if input == 0 {
            return FfiReturn::ExecutionFail;
        }

        unsafe {
            output.write(42);
        }

        FfiReturn::Ok
    }

    #[unsafe(no_mangle)]
    unsafe extern "C" fn __freestanding_returns_opaque_item(
        input: *const ExternOpaqueStruct,
        output: *mut *const ExternOpaqueStruct,
    ) -> FfiReturn {
        unsafe {
            output.write(input);
        }

        FfiReturn::Ok
    }
}
