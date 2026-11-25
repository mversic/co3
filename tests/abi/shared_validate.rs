use core::{
    cell::{Cell, UnsafeCell},
    mem::{ManuallyDrop, MaybeUninit},
    num::NonZeroU8,
};

use co3::{
    ReprC, lang,
    marker::AbiCompatible,
    rust_spec::{
        RustSpec,
        layout::{NonRobust, Robust},
        mutability::{Exclusive, WithInterior},
    },
};
use static_assertions::{assert_impl_all, assert_not_impl_any};

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct Record<T> {
    value: T,
}

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Transparent<T>(T);

#[derive(RustSpec, ReprC)]
#[repr(C)]
#[rust_spec(non_robust)]
#[repr_c(is_valid = |value| *value != 0)]
struct Custom {
    value: u8,
}

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct ConcreteCellOwner {
    value: Box<UnsafeCell<NonZeroU8>>,
}

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct Mixed<T> {
    flag: bool,
    value: T,
}

#[derive(RustSpec, ReprC)]
#[repr(C, u8)]
enum Choice<T> {
    Value(T) = 1,
    Empty,
}

#[derive(RustSpec, ReprC)]
#[repr(C)]
#[rust_spec(non_robust)]
#[repr_c(is_valid = |value| *value != 0)]
struct AuditedCustom {
    value: u8,
}

#[test]
fn shared_validation_is_transitive() {
    assert_impl_all!(UnsafeCell<u8>: RustSpec<Mutability = WithInterior<Robust>>);
    assert_impl_all!(Box<UnsafeCell<NonZeroU8>>:
        RustSpec<Mutability = Exclusive<WithInterior<NonRobust>>>);
    assert_impl_all!(UnsafeCell<NonZeroU8>: AbiCompatible);
    assert_impl_all!(Cell<NonZeroU8>: AbiCompatible);
    assert_impl_all!(&Record<Box<u8>>: co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_impl_all!(&Transparent<Box<u8>>: co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_impl_all!(&UnsafeCell<u8>: co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_impl_all!(&Cell<u8>: co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_impl_all!(&Custom: co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_impl_all!(&AuditedCustom: co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_not_impl_any!(&UnsafeCell<NonZeroU8>: co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_not_impl_any!(&Cell<NonZeroU8>: co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_not_impl_any!(&Record<Box<UnsafeCell<NonZeroU8>>>: co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_not_impl_any!(&Transparent<Box<UnsafeCell<NonZeroU8>>>: co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_not_impl_any!(&ConcreteCellOwner: co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_not_impl_any!(&Choice<Box<UnsafeCell<NonZeroU8>>>: co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_not_impl_any!(&[Box<UnsafeCell<NonZeroU8>>]: co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_not_impl_any!(&[Box<UnsafeCell<NonZeroU8>>; 2]: co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_not_impl_any!(&Option<Box<UnsafeCell<NonZeroU8>>>: co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_not_impl_any!(&Box<UnsafeCell<NonZeroU8>>: AbiCompatible, co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_not_impl_any!(&&mut UnsafeCell<NonZeroU8>: AbiCompatible, co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_not_impl_any!(&mut &Box<UnsafeCell<NonZeroU8>>: AbiCompatible);
    assert_impl_all!(&mut UnsafeCell<NonZeroU8>: AbiCompatible, co3::convert::DecodeMove<'static>);
    assert_impl_all!(Box<UnsafeCell<NonZeroU8>>: AbiCompatible, co3::convert::DecodeMove<'static>);
    assert_impl_all!(&Mixed<UnsafeCell<u8>>: co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_impl_all!(&[Mixed<UnsafeCell<u8>>]: co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_not_impl_any!(&Mixed<UnsafeCell<NonZeroU8>>: co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_impl_all!(Mixed<Box<UnsafeCell<NonZeroU8>>>:
        RustSpec<Mutability = Exclusive<WithInterior<NonRobust>>>);
    assert_not_impl_any!(&Mixed<Box<UnsafeCell<NonZeroU8>>>: co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
}

#[test]
fn maybe_uninit_cells_allow_shared_validation_without_inspecting_contents() {
    type UninitCell = MaybeUninit<UnsafeCell<NonZeroU8>>;
    assert_impl_all!(&UninitCell: AbiCompatible, co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_impl_all!(&MaybeUninit<Cell<NonZeroU8>>: AbiCompatible, co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_impl_all!(&Box<UninitCell>: AbiCompatible, co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_impl_all!(&MaybeUninit<Box<UnsafeCell<NonZeroU8>>>:
        AbiCompatible, co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);
    assert_not_impl_any!(&ManuallyDrop<UnsafeCell<NonZeroU8>>:
        AbiCompatible, co3::convert::EncodeMove, co3::convert::DecodeMove<'static>);

    let value = UninitCell::zeroed();
    let encoded = unsafe { co3::encode::<_, lang::C>(&value) };
    let decoded = unsafe { co3::decode::<&UninitCell, lang::C>(encoded) }.unwrap();
    assert!(core::ptr::eq(decoded, &value));

    let boxed = Box::new(UninitCell::zeroed());
    let encoded = unsafe { co3::encode::<_, lang::C>(&boxed) };
    let decoded = unsafe { co3::decode::<&Box<UninitCell>, lang::C>(encoded) }.unwrap();
    assert!(core::ptr::eq(decoded, &boxed));

    // Zero is invalid for NonZeroU8, but valid as uninterpreted MaybeUninit bytes.
    let representation = core::mem::MaybeUninit::<UnsafeCell<co3::marker::NonZero<u8>>>::new(
        core::cell::UnsafeCell::new(unsafe { core::mem::zeroed() }),
    );
    let encoded =
        unsafe { co3::reference::abi::Ref::from_raw(core::ptr::from_ref(&representation)) };
    assert!(unsafe { co3::decode::<&UninitCell, lang::C>(encoded) }.is_some());
}

#[test]
fn shared_vector_requires_robust_cells_not_robust_elements() {
    type Element = Mixed<UnsafeCell<u8>>;
    assert_impl_all!(Element: RustSpec<Trap = NonRobust, Mutability = WithInterior<Robust>>);
    assert_impl_all!(&Vec<Element>: co3::convert::EncodeMove);
    assert_not_impl_any!(&Vec<Mixed<UnsafeCell<NonZeroU8>>>: co3::convert::EncodeMove);

    let values = vec![Mixed {
        flag: true,
        value: UnsafeCell::new(7_u8),
    }];
    let mut store = Default::default();
    let encoded = unsafe { co3::soft_encode::<_, lang::C>(&values, &mut store) };
    let header = unsafe { encoded.into_ref() }.unwrap();
    assert_eq!(
        header.as_ptr().cast_const().cast::<Element>(),
        values.as_ptr()
    );
    assert_eq!(header.len(), values.len());
}

#[test]
fn robust_cells_do_not_hide_other_invalid_fields() {
    let source = Mixed {
        flag: true,
        value: UnsafeCell::new(7_u8),
    };
    let encoded = unsafe { co3::encode::<_, lang::C>(&source) };
    assert!(unsafe { co3::decode::<&Mixed<UnsafeCell<u8>>, lang::C>(encoded) }.is_some());

    // The invalid boolean exists only in the robust C representation.
    let invalid = unsafe {
        <Mixed<UnsafeCell<u8>> as ReprC>::CType::from_fields(
            co3::primitive::abi::bool(2),
            core::cell::UnsafeCell::new(7),
        )
    };
    let encoded = unsafe { co3::reference::abi::Ref::from_raw(core::ptr::from_ref(&invalid)) };
    assert!(unsafe { co3::decode::<&Mixed<UnsafeCell<u8>>, lang::C>(encoded) }.is_none());
    let encoded =
        unsafe { co3::wide::abi::WideRef::from_raw_parts(core::ptr::from_ref(&invalid), 1) };
    assert!(unsafe { co3::decode::<&[Mixed<UnsafeCell<u8>>], lang::C>(encoded) }.is_none());
}

#[test]
fn shared_plain_data_and_exclusive_cells_still_decode() {
    let value = Box::new(7_u8);
    let encoded = unsafe { co3::encode::<_, lang::C>(&value) };
    let decoded = unsafe { co3::decode::<&Box<u8>, lang::C>(encoded) }.unwrap();
    assert_eq!(**decoded, 7);

    let source = co3::boxed::abi::Box::from_box(Box::new(core::cell::UnsafeCell::new(
        co3::marker::NonZero(7_u8),
    )));
    let decoded = unsafe { co3::decode::<Box<UnsafeCell<NonZeroU8>>, lang::C>(source) }.unwrap();
    assert_eq!((*decoded).into_inner().get(), 7);
}

#[test]
fn box_shared_encoders_preserve_headers() {
    macro_rules! check {
        ($value:expr $(, len = $len:expr)?) => {{
            let value = $value;
            let expected = core::ptr::from_ref(&*value).cast::<()>();
            let mut store = Default::default();
            let encoded = unsafe { co3::soft_encode::<_, lang::C>(&value, &mut store) };
            // Every concrete Box carrier tested here is Copy; only its header is read.
            let header = unsafe { *encoded.as_ptr() };
            assert_eq!(header.as_ptr().cast::<()>().cast_const(), expected);
            $(assert_eq!(header.metadata(), $len);)?

            let mut store = Default::default();
            let encoded = unsafe { co3::soft_encode_unchecked(&value, &mut store) };
            let header = unsafe { *encoded.as_ptr() };
            assert_eq!(header.as_ptr().cast::<()>().cast_const(), expected);
            $(assert_eq!(header.metadata(), $len);)?
        }};
    }

    check!(Box::new(7_u8));
    check!(Box::new(UnsafeCell::new(7_u8)));
    check!(Box::<[u8]>::from([1, 2, 3]), len = 3);
    check!(
        Box::<[UnsafeCell<u8>]>::from([UnsafeCell::new(1), UnsafeCell::new(2)]),
        len = 2
    );
    check!(std::ffi::CString::new("hello").unwrap().into_boxed_c_str());
}

#[test]
fn shared_nonrobust_cell_round_trips_unchecked() {
    let value = UnsafeCell::new(true);
    let encoded = unsafe { co3::encode_unchecked(&value) };
    let decoded = unsafe { co3::decode_unchecked::<&UnsafeCell<bool>>(encoded) };
    assert!(core::ptr::eq(decoded, &value));
    unsafe { *decoded.get() = false };
    assert!(!value.into_inner());

    let value = UnsafeCell::new(true);
    let mut encode_store = ();
    let mut decode_store = ();
    let encoded = unsafe { co3::soft_encode_unchecked(&value, &mut encode_store) };
    let decoded =
        unsafe { co3::soft_decode_unchecked::<&UnsafeCell<bool>>(encoded, &mut decode_store) };
    assert!(core::ptr::eq(decoded, &value));
    unsafe { *decoded.get() = false };
    assert!(!value.into_inner());
}

#[test]
fn derived_shared_nonrobust_cell_round_trips_unchecked() {
    let value = Record {
        value: UnsafeCell::new(true),
    };
    let encoded = unsafe { co3::encode_unchecked(&value) };
    let decoded = unsafe { co3::decode_unchecked::<&Record<UnsafeCell<bool>>>(encoded) };
    assert!(core::ptr::eq(decoded, &value));
    unsafe { *decoded.value.get() = false };
    assert!(!value.value.into_inner());
}

#[test]
fn unchecked_mutable_bool_borrows_the_original_storage() {
    let mut value = true;
    let original = core::ptr::from_mut(&mut value);
    let encoded = unsafe { co3::encode_unchecked(&mut value) };
    let decoded = unsafe { co3::decode_unchecked::<&mut bool>(encoded) };
    assert!(core::ptr::eq(decoded, original));
    *decoded = false;
    assert!(!value);
}

mod rust_export {
    use core::cell::Cell;

    use co3::{ffi, rust_spec::RustSpec};

    #[derive(RustSpec)]
    #[rust_spec(opaque)]
    struct Handle {
        _value: Cell<bool>,
    }

    fn touch(_value: &Handle) {}

    pub fn flip(value: &Cell<bool>) {
        value.set(!value.get());
    }

    ffi! {
        #![unsafe(export("C"))]
        #![lang = "Rust"]
        #![symbol_prefix = "unchecked_cell"]
        type Handle;
        fn touch(value: &Handle);
        fn flip(value: &Cell<bool>);
    }
}

mod rust_import {
    use core::cell::Cell;

    use co3::ffi;

    ffi! {
        #![unsafe(extern("C"))]
        #![lang = "Rust"]
        #![symbol_prefix = "unchecked_cell"]
        pub fn flip(value: &Cell<bool>);
    }
}

#[test]
fn rust_boundary_uses_unchecked_shared_cell_conversion() {
    let value = Cell::new(true);
    rust_import::flip(&value);
    assert!(!value.get());
}
