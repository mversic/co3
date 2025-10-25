#![cfg(feature = "derive")]
use std::{alloc, collections::BTreeMap, mem::MaybeUninit};

use co3::{
    Decode, Encode, ExternC, FfiReturn, FfiTuple1, FfiTuple2, out_ptr::OutPtrRead,
    slice::OutBoxedSlice,
};

co3::handles! {OpaqueStruct}
co3::def_fns! { dealloc }

pub trait Target {
    type Target;

    fn target(self) -> Self::Target;
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, ExternC)]
pub struct Name(String);
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, ExternC)]
pub struct Value(String);

#[derive(Debug, Clone, PartialEq, Eq, Default, ExternC)]
pub struct OpaqueStruct {
    name: Option<Name>,
    tokens: Vec<Value>,
    params: BTreeMap<Name, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ExternC)]
#[repr(u8)]
pub enum FieldlessEnum {
    A,
    B,
    C,
}

#[derive(Debug, Clone, PartialEq, Eq, ExternC)]
#[repr(C)]
pub enum DataCarryingEnum {
    A(OpaqueStruct),
    B(u32),
    // TODO: Support this
    //C(T),
    D,
}

#[derive(Clone, Copy, PartialEq, Eq, ExternC)]
#[repr(C)]
pub struct RobustReprCStruct<T, U> {
    a: u8,
    b: T,
    c: U,
    d: core::mem::ManuallyDrop<i16>,
}

#[co3::carbonate]
impl OpaqueStruct {
    pub fn new(name: Name) -> Self {
        Self {
            name: Some(name),
            tokens: Vec::new(),
            params: Default::default(),
        }
    }

    pub fn consume_self(self) {}

    #[must_use]
    pub fn with_tokens(mut self, tokens: impl IntoIterator<Item = impl Into<Value>>) -> Self {
        self.tokens = tokens.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    // NOTE: used `-> OpaqueStruct` instead of `-> Self` to showcase that the signature is supported
    pub fn with_params(mut self, params: impl IntoIterator<Item = (Name, Value)>) -> OpaqueStruct {
        self.params = params.into_iter().collect();
        self
    }

    pub fn get_param(&self, name: &Name) -> Option<&Value> {
        self.params.get(name)
    }

    pub fn params(&self) -> impl ExactSizeIterator<Item = (&Name, &Value)> {
        self.params.iter()
    }

    pub fn remove_param(&mut self, param: &Name) -> Option<Value> {
        self.params.remove(param)
    }

    pub fn fallible_int_output(flag: bool) -> Result<u32, &'static str> {
        if flag { Ok(42) } else { Err("fail") }
    }

    pub fn fallible_empty_tuple_output(flag: bool) -> Result<(), &'static str> {
        if flag { Ok(()) } else { Err("fail") }
    }
}

#[co3::carbonate]
pub fn freestanding_with_boxed_slice(item: Box<[u8]>) -> Box<[u8]> {
    item
}

#[co3::carbonate]
pub fn freestanding_with_option(item: Option<u8>) -> Option<u8> {
    item
}

// FIXME: Depends on the fix in disjoint_impls
//#[co3::carbonate]
//pub fn freestanding_with_option_tuple(item: Option<(u32, u32)>) -> Option<(u32, u32)> {
//    item
//}

#[co3::carbonate]
pub fn freestanding_with_option_with_niche_ref(item: &Option<bool>) -> &Option<bool> {
    item
}

#[co3::carbonate]
pub fn freestanding_with_option_without_niche_ref(item: &Option<u8>) -> &Option<u8> {
    item
}

#[co3::carbonate]
pub fn freestanding_with_primitive(byte: u8) -> u8 {
    byte
}

#[co3::carbonate]
pub fn freestanding_with_fieldless_enum(enum_: FieldlessEnum) -> FieldlessEnum {
    enum_
}

#[co3::carbonate]
pub fn freestanding_with_data_carrying_enum(enum_: DataCarryingEnum) -> DataCarryingEnum {
    enum_
}

// FIXME: implement compile test
//#[co3::carbonate]
//pub fn freestanding_with_array(arr: [u8; 1]) -> [u8; 1] {
//    arr
//}

#[co3::carbonate]
pub fn freestanding_with_array_ref(arr: &[u8; 1]) -> &[u8; 1] {
    arr
}

#[co3::carbonate]
pub fn freestanding_with_array_in_struct(arr: ([u8; 1],)) -> ([u8; 1],) {
    arr
}

#[co3::carbonate]
pub fn freestanding_with_repr_c_struct(
    struct_: RobustReprCStruct<u32, i16>,
) -> RobustReprCStruct<u32, i16> {
    struct_
}

#[co3::carbonate]
pub fn get_vec_of_boxed_opaques() -> Vec<Box<OpaqueStruct>> {
    vec![Box::new(get_new_struct())]
}

#[co3::carbonate]
pub fn take_and_return_array_of_opaques(a: &[OpaqueStruct; 2]) -> &[OpaqueStruct; 2] {
    a
}

#[co3::carbonate]
pub fn freestanding_with_nested_vec(_vec: Vec<Vec<Vec<u8>>>) {}

#[cfg(feature = "non_robust_ref_mut")]
#[co3::carbonate]
pub fn take_non_robust_ref_mut(val: &mut str) -> &mut str {
    val
}

#[co3::carbonate]
pub fn take_vec_ref(a: &Vec<u8>) {
    assert_eq!(a, &vec![1, 2])
}

#[co3::carbonate]
pub fn take_tuple_ref(a: &(u8, u8)) -> &(u8, u8) {
    a
}

#[co3::carbonate]
impl Target for OpaqueStruct {
    type Target = Option<Name>;

    fn target(self) -> <Self as Target>::Target {
        self.name
    }
}

#[co3::carbonate]
pub fn reference_from_slice(a: &[u8]) -> &u8 {
    &a[0]
}

fn get_default_params() -> [(Name, Value); 2] {
    [
        (Name(String::from("Nomen")), Value(String::from("Omen"))),
        (Name(String::from("Nomen2")), Value(String::from("Omen2"))),
    ]
}

fn get_new_struct() -> OpaqueStruct {
    let name = Name(String::from("X"));

    unsafe {
        let mut ffi_struct = MaybeUninit::new(core::ptr::null_mut());

        assert_eq!(
            FfiReturn::Ok,
            OpaqueStruct__new(name.encode(&mut ()), ffi_struct.as_mut_ptr())
        );

        let ffi_struct = ffi_struct.assume_init();
        Decode::decode(ffi_struct, &mut ()).unwrap()
    }
}

fn get_new_struct_with_params() -> OpaqueStruct {
    let ffi_struct = get_new_struct();
    let params = get_default_params().to_vec();

    let mut output = MaybeUninit::new(core::ptr::null_mut());

    let mut store = Default::default();
    let params_ffi = params.encode(&mut store);
    assert_eq!(FfiReturn::Ok, unsafe {
        OpaqueStruct__with_params(ffi_struct.encode(&mut ()), params_ffi, output.as_mut_ptr())
    });

    unsafe { Decode::decode(output.assume_init(), &mut ()).expect("valid") }
}

#[test]
#[webassembly_test::webassembly_test]
#[cfg(feature = "non_robust_ref_mut")]
fn non_robust_ref_mut() {
    use co3::slice::RefMutSlice;

    let mut owned = "queen".to_owned();
    let ffi_struct: &mut str = owned.as_mut();
    let mut output = MaybeUninit::new(RefMutSlice::from_raw_parts_mut(core::ptr::null_mut(), 0));
    let ffi_type: RefMutSlice<u8> = ffi_struct.encode(&mut ());

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            __take_non_robust_ref_mut(ffi_type, output.as_mut_ptr())
        );

        let output: &mut str = OutPtrRead::try_read_out(output.assume_init()).unwrap();
        assert_eq!(output, owned.as_mut());
    }
}

#[test]
#[webassembly_test::webassembly_test]
fn constructor() {
    let ffi_struct = get_new_struct();
    assert_eq!(Some(Name(String::from('X'))), ffi_struct.name);
    assert!(ffi_struct.params.is_empty());
}

#[test]
#[webassembly_test::webassembly_test]
fn builder_method() {
    let ffi_struct = get_new_struct_with_params();

    assert_eq!(2, ffi_struct.params.len());
    assert_eq!(
        ffi_struct.params,
        get_default_params().into_iter().collect()
    );
}

#[test]
#[webassembly_test::webassembly_test]
fn consume_self() {
    let ffi_struct = get_new_struct();

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            OpaqueStruct__consume_self(ffi_struct.encode(&mut ()).cast())
        );
    }
}

#[test]
#[webassembly_test::webassembly_test]
fn into_iter_item_impl_into() {
    let tokens = vec![
        Value(String::from("My omen")),
        Value(String::from("Your omen")),
    ];

    let mut ffi_struct = get_new_struct();
    let mut tokens_store = Box::default();
    let tokens_ffi = tokens.clone().encode(&mut tokens_store);

    let mut output = MaybeUninit::new(core::ptr::null_mut());

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            OpaqueStruct__with_tokens(ffi_struct.encode(&mut ()), tokens_ffi, output.as_mut_ptr())
        );

        ffi_struct = Decode::decode(output.assume_init(), &mut ()).expect("valid");

        assert_eq!(2, ffi_struct.tokens.len());
        assert_eq!(ffi_struct.tokens, tokens);
    }
}

#[test]
#[webassembly_test::webassembly_test]
fn mutate_opaque() {
    let param_name = Name(String::from("Nomen"));
    let mut ffi_struct = get_new_struct_with_params();
    let mut removed = MaybeUninit::new(core::ptr::null_mut());

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            OpaqueStruct__remove_param(
                (&mut ffi_struct).encode(&mut ()),
                &param_name,
                removed.as_mut_ptr(),
            )
        );

        let removed = removed.assume_init();
        let removed = Option::decode(removed, &mut ()).unwrap();
        assert_eq!(Some(Value(String::from("Omen"))), removed);
        assert!(!ffi_struct.params.contains_key(&param_name));
    }
}

#[test]
#[webassembly_test::webassembly_test]
fn return_option() {
    let ffi_struct = get_new_struct_with_params();

    let mut param1 = MaybeUninit::new(core::ptr::null());
    let mut param2 = MaybeUninit::new(core::ptr::null());

    let name1 = Name(String::from("Non"));
    assert_eq!(FfiReturn::Ok, unsafe {
        OpaqueStruct__get_param((&ffi_struct).encode(&mut ()), &name1, param1.as_mut_ptr())
    });
    let param1 = unsafe { param1.assume_init() };
    assert!(param1.is_null());
    let param1: Option<&Value> = unsafe { Decode::decode(param1, &mut ()).unwrap() };
    assert!(param1.is_none());

    let name2 = Name(String::from("Nomen"));
    assert_eq!(FfiReturn::Ok, unsafe {
        OpaqueStruct__get_param((&ffi_struct).encode(&mut ()), &name2, param2.as_mut_ptr())
    });

    unsafe {
        let param2 = param2.assume_init();
        assert!(!param2.is_null());
        let param2: Option<&Value> = Decode::decode(param2, &mut ()).unwrap();
        assert_eq!(Some(&Value(String::from("Omen"))), param2);
    }
}

#[test]
#[webassembly_test::webassembly_test]
fn take_and_return_boxed_slice() {
    let input: Box<[u8]> = [12u8, 42u8].into();
    let mut output = MaybeUninit::new(OutBoxedSlice::from_raw_parts(core::ptr::null_mut(), 0));
    let mut in_store = Default::default();

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            __freestanding_with_boxed_slice(input.encode(&mut in_store), output.as_mut_ptr())
        );

        let output = output.assume_init();
        assert_eq!(output.len(), 2);
        let boxed_slice = Box::<[u8]>::try_read_out(output).expect("Valid");
        assert_eq!(boxed_slice, [12u8, 42u8].into());
    }
}

#[test]
#[webassembly_test::webassembly_test]
fn take_and_return_option_without_niche() {
    let input = Some(42u8);
    let mut output = MaybeUninit::new(FfiTuple2(0, unsafe { core::mem::zeroed() }));

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            __freestanding_with_option(input.encode(&mut ()), output.as_mut_ptr())
        );

        let output = output.assume_init();
        assert_eq!(input, OutPtrRead::try_read_out(output).expect("Valid"));
    }
}

#[test]
#[webassembly_test::webassembly_test]
fn take_and_return_option_with_tuple() {
    unimplemented!()
    //let input = Some(true);
    //let mut output = MaybeUninit::new(0);
    //let mut in_store = Default::default();

    //unsafe {
    //    assert_eq!(
    //        FfiReturn::Ok,
    //        __freestanding_with_option_with_niche_ref(
    //            (&input).encode(&mut in_store),
    //            output.as_mut_ptr()
    //        )
    //    );

    //    let output = output.assume_init();
    //    assert_eq!(
    //        input,
    //        Option::<bool>::try_read_out(output).expect("Valid")
    //    );
    //}
}

#[test]
#[webassembly_test::webassembly_test]
fn take_and_return_option_with_niche_ref() {
    //    let input = Some(true);
    //    let mut output = MaybeUninit::new(0);
    //    let mut in_store = Default::default();
    //
    //    unsafe {
    //        assert_eq!(
    //            FfiReturn::Ok,
    //            __freestanding_with_option_with_niche_ref(
    //                (&input).encode(&mut in_store),
    //                output.as_mut_ptr()
    //            )
    //        );
    //
    //        let output = output.assume_init();
    //        assert_eq!(
    //            input,
    //            *Option<bool>::try_read_out(output).expect("Valid")
    //        );
    //    }
}

#[test]
#[webassembly_test::webassembly_test]
fn take_and_return_option_without_niche_ref() {
    #[cfg(not(target_family = "wasm"))]
    let input = Some(42u8);
    #[cfg(target_family = "wasm")]
    let input = Some(42u32);
    #[cfg(not(target_family = "wasm"))]
    let init_val = FfiTuple2(0_u8, 0_u8);
    #[cfg(target_family = "wasm")]
    let init_val = FfiTuple2(0_u32, 0_u32);

    let mut output = MaybeUninit::new(init_val);
    let mut in_store = Default::default();

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            __freestanding_with_option_without_niche_ref(
                (&input).encode(&mut in_store),
                output.as_mut_ptr()
            )
        );

        let output = output.assume_init();
        assert_eq!(input, Option::<u8>::try_read_out(output).expect("Valid"));
    }
}

#[test]
#[webassembly_test::webassembly_test]
fn return_iterator() {
    let ffi_struct = get_new_struct_with_params();
    let mut out_params = MaybeUninit::new(OutBoxedSlice::from_raw_parts(core::ptr::null_mut(), 0));

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            OpaqueStruct__params((&ffi_struct).encode(&mut ()), out_params.as_mut_ptr())
        );

        let out_params = out_params.assume_init();
        assert_eq!(out_params.len(), 2);
        let vec = Vec::<(&Name, &Value)>::try_read_out(out_params).expect("Valid");

        let default_params = get_default_params();
        assert_eq!((&default_params[0].0, &default_params[0].1), vec[0]);
        assert_eq!((&default_params[1].0, &default_params[1].1), vec[1]);
    }
}

#[test]
#[webassembly_test::webassembly_test]
fn return_result() {
    let mut output = MaybeUninit::new(0);

    unsafe {
        assert_eq!(
            FfiReturn::ExecutionFail,
            OpaqueStruct__fallible_int_output(false.encode(&mut ()), output.as_mut_ptr())
        );
        assert_eq!(0, output.assume_init());
        assert_eq!(
            FfiReturn::Ok,
            OpaqueStruct__fallible_int_output(true.encode(&mut ()), output.as_mut_ptr())
        );
        assert_eq!(42, output.assume_init());
    }
}

#[test]
#[webassembly_test::webassembly_test]
fn return_empty_tuple_result() {
    unsafe {
        assert_eq!(
            FfiReturn::ExecutionFail,
            OpaqueStruct__fallible_empty_tuple_output(false.encode(&mut ()))
        );
        assert_eq!(
            FfiReturn::Ok,
            OpaqueStruct__fallible_empty_tuple_output(true.encode(&mut ()))
        );
    }
}

//#[test]
//#[webassembly_test::webassembly_test]
//fn array_to_pointer() {
//    let array = [1_u8];
//    let mut store = Option::default();
//    let ptr: *const [u8; 1] = array.encode(&mut store);
//    let mut output = MaybeUninit::new([0_u8]);
//
//    unsafe {
//        assert_eq!(
//            FfiReturn::Ok,
//            __freestanding_with_array(ptr, output.as_mut_ptr())
//        );
//
//        assert_eq!(
//            [1_u8],
//            <[u8; 1]>::decode(output.assume_init(), &mut ()).unwrap()
//        );
//    }
//}

#[test]
#[webassembly_test::webassembly_test]
fn take_and_return_array_ref() {
    let array = [1_u8];
    let ptr: *const [u8; 1] = (&array).encode(&mut ());
    let mut output = MaybeUninit::new(core::ptr::null());

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            __freestanding_with_array_ref(ptr, output.as_mut_ptr())
        );

        assert_eq!(
            &[1_u8; 1],
            <&[u8; 1]>::decode(output.assume_init(), &mut ()).unwrap()
        );
    }
}

#[test]
#[webassembly_test::webassembly_test]
fn array_in_struct() {
    let array = ([1_u8],);
    let ffi_arr: FfiTuple1<[u8; 1]> = array.encode(&mut ((),));
    let mut output = MaybeUninit::new(FfiTuple1([0; 1]));

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            __freestanding_with_array_in_struct(ffi_arr, output.as_mut_ptr())
        );

        assert_eq!(
            ([1_u8],),
            <([u8; 1],)>::decode(output.assume_init(), &mut ((),)).unwrap()
        );
    }
}

#[test]
#[webassembly_test::webassembly_test]
fn repr_c_struct() {
    let struct_ = RobustReprCStruct {
        a: 42,
        b: 7,
        c: 12,
        d: core::mem::ManuallyDrop::new(12),
    };
    let mut output = MaybeUninit::new(RobustReprCStruct {
        a: u8::MAX,
        b: u32::MAX,
        c: i16::MAX,
        d: core::mem::ManuallyDrop::new(-1),
    });

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            __freestanding_with_repr_c_struct(struct_, output.as_mut_ptr())
        );

        assert!(output.assume_init() == struct_);
    }
}

#[test]
#[webassembly_test::webassembly_test]
fn primitive_conversion() {
    let byte: u8 = 1;
    let mut output = MaybeUninit::new(0);

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            __freestanding_with_primitive(byte.encode(&mut ()), output.as_mut_ptr())
        );

        assert_eq!(1, output.assume_init());
    }
}

#[test]
#[webassembly_test::webassembly_test]
fn fieldless_enum_conversion() {
    let fieldless_enum = FieldlessEnum::A;
    let mut output = MaybeUninit::new(2);

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            __freestanding_with_fieldless_enum(fieldless_enum.encode(&mut ()), output.as_mut_ptr())
        );

        let ret_val = OutPtrRead::try_read_out(output.assume_init());
        assert_eq!(FieldlessEnum::A, ret_val.unwrap());
    }
}

#[test]
#[cfg(target_family = "wasm")]
#[webassembly_test::webassembly_test]
fn primitive_conversion_failed() {
    let byte: u32 = u32::MAX;
    let mut output = MaybeUninit::new(0);

    unsafe {
        assert_eq!(
            FfiReturn::ConversionFailed,
            __freestanding_with_primitive(byte, output.as_mut_ptr())
        );

        assert_eq!(0, output.assume_init());
    }
}

#[test]
#[webassembly_test::webassembly_test]
fn data_carrying_enum_conversion() {
    let data_carrying_enum = DataCarryingEnum::A(get_new_struct());
    let mut output = MaybeUninit::new(__co3__ReprCDataCarryingEnum {
        tag: 1,
        payload: __co3__DataCarryingEnumPayload {
            B: core::mem::ManuallyDrop::new(42),
        },
    });

    unsafe {
        let mut store = Default::default();
        assert_eq!(
            FfiReturn::Ok,
            __freestanding_with_data_carrying_enum(
                data_carrying_enum.clone().encode(&mut store),
                output.as_mut_ptr()
            )
        );

        let ret_val = OutPtrRead::try_read_out(output.assume_init());
        assert_eq!(data_carrying_enum, ret_val.expect("Conversion failed"));
    }
}

#[test]
#[webassembly_test::webassembly_test]
fn invoke_trait_method() {
    let ffi_struct = get_new_struct_with_params();
    let mut output = MaybeUninit::<*mut Name>::new(core::ptr::null_mut());

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            OpaqueStruct__Target__target(ffi_struct.encode(&mut ()), output.as_mut_ptr())
        );
        let name = Decode::decode(output.assume_init(), &mut ()).unwrap();
        assert_eq!(Name(String::from("X")), name);
    }
}

#[test]
#[webassembly_test::webassembly_test]
fn nested_vec() {
    let vec: Vec<Vec<Vec<u8>>> = vec![];

    unsafe {
        let mut store = Default::default();
        assert_eq!(
            FfiReturn::Ok,
            __freestanding_with_nested_vec(vec.encode(&mut store))
        );
    }
}

#[test]
#[webassembly_test::webassembly_test]
fn return_vec_of_boxed_opaques() {
    let mut output = MaybeUninit::new(OutBoxedSlice::from_raw_parts(core::ptr::null_mut(), 0));

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            __get_vec_of_boxed_opaques(output.as_mut_ptr())
        );
        let output = output.assume_init();
        assert_eq!(output.len(), 1);
        let vec = Vec::<Box<OpaqueStruct>>::try_read_out(output).expect("Valid");
        assert_eq!(Box::new(get_new_struct()), vec[0]);
    }
}

#[test]
#[webassembly_test::webassembly_test]
fn array_of_opaques() {
    let input: [OpaqueStruct; 2] = [Default::default(), Default::default()];
    let mut output = MaybeUninit::new([core::ptr::null_mut(), core::ptr::null_mut()]);
    let mut store = Default::default();

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            __take_and_return_array_of_opaques((&input).encode(&mut store), output.as_mut_ptr())
        );
        let output = output.assume_init();
        let output = <[OpaqueStruct; 2]>::decode(output, &mut ()).unwrap();
        assert_eq!(input, output);
    }
}

#[test]
#[webassembly_test::webassembly_test]
fn borrow_vec() {
    let a: Vec<u8> = vec![1, 2];
    let mut store = Default::default();

    unsafe {
        assert_eq!(FfiReturn::Ok, __take_vec_ref((&a).encode(&mut store)));
    }
}

#[test]
#[webassembly_test::webassembly_test]
fn return_reference_from_slice() {
    let a = vec![1, 2];

    let mut output = MaybeUninit::new(core::ptr::null());

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            __reference_from_slice(a.as_slice().encode(&mut ()), output.as_mut_ptr())
        );

        let output = <&u8>::try_read_out(output.assume_init()).unwrap();
        assert_eq!(output, &a[0]);
    }
}

#[test]
#[webassembly_test::webassembly_test]
fn borrow_local() {
    let a = (1_u8, 2_u8);

    let b: (u8, u8) = {
        let mut store = Default::default();
        let mut output = MaybeUninit::new(FfiTuple2(0, 0));

        unsafe {
            assert_eq!(
                FfiReturn::Ok,
                __take_tuple_ref((&a).encode(&mut store), output.as_mut_ptr())
            );

            OutPtrRead::try_read_out(output.assume_init()).expect("Valid")
        }
    };

    assert_eq!(b, (1, 2));
}
