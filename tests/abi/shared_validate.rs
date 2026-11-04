use core::{
    cell::{Cell, UnsafeCell},
    mem::{ManuallyDrop, MaybeUninit},
    num::NonZeroU8,
};

use co3::{
    ReprC,
    rust_spec::{
        RustSpec,
        layout::{NonRobust, Robust},
        mutability::{Exclusive, WithInterior},
    },
    stored::{DecodeMove, EncodeMove},
    transmute::AbiCompatible,
};
use static_assertions::{assert_impl_all, assert_not_impl_any};

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct Record<T> {
    value: T,
}

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
#[repr_c(transparent)]
struct Transparent<T>(T);

#[derive(RustSpec, ReprC)]
#[repr(C)]
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
    assert_impl_all!(&Record<Box<u8>>: EncodeMove, DecodeMove<'static>);
    assert_impl_all!(&Transparent<Box<u8>>: EncodeMove, DecodeMove<'static>);
    assert_impl_all!(&UnsafeCell<u8>: EncodeMove, DecodeMove<'static>);
    assert_impl_all!(&Cell<u8>: EncodeMove, DecodeMove<'static>);
    assert_impl_all!(&Custom: EncodeMove, DecodeMove<'static>);
    assert_impl_all!(&AuditedCustom: EncodeMove, DecodeMove<'static>);
    assert_not_impl_any!(&UnsafeCell<NonZeroU8>: EncodeMove, DecodeMove<'static>);
    assert_not_impl_any!(&Cell<NonZeroU8>: EncodeMove, DecodeMove<'static>);
    assert_not_impl_any!(&Record<Box<UnsafeCell<NonZeroU8>>>: EncodeMove, DecodeMove<'static>);
    assert_not_impl_any!(&Transparent<Box<UnsafeCell<NonZeroU8>>>: EncodeMove, DecodeMove<'static>);
    assert_not_impl_any!(&ConcreteCellOwner: EncodeMove, DecodeMove<'static>);
    assert_not_impl_any!(&Choice<Box<UnsafeCell<NonZeroU8>>>: EncodeMove, DecodeMove<'static>);
    assert_not_impl_any!(&[Box<UnsafeCell<NonZeroU8>>]: EncodeMove, DecodeMove<'static>);
    assert_not_impl_any!(&[Box<UnsafeCell<NonZeroU8>>; 2]: EncodeMove, DecodeMove<'static>);
    assert_not_impl_any!(&Option<Box<UnsafeCell<NonZeroU8>>>: EncodeMove, DecodeMove<'static>);
    assert_not_impl_any!(&Box<UnsafeCell<NonZeroU8>>: AbiCompatible, EncodeMove, DecodeMove<'static>);
    assert_not_impl_any!(&&mut UnsafeCell<NonZeroU8>: AbiCompatible, EncodeMove, DecodeMove<'static>);
    assert_not_impl_any!(&mut &Box<UnsafeCell<NonZeroU8>>: AbiCompatible);
    assert_impl_all!(&mut UnsafeCell<NonZeroU8>: AbiCompatible, DecodeMove<'static>);
    assert_impl_all!(Box<UnsafeCell<NonZeroU8>>: AbiCompatible, DecodeMove<'static>);
    assert_impl_all!(&Mixed<UnsafeCell<u8>>: EncodeMove, DecodeMove<'static>);
    assert_impl_all!(&[Mixed<UnsafeCell<u8>>]: EncodeMove, DecodeMove<'static>);
    assert_not_impl_any!(&Mixed<UnsafeCell<NonZeroU8>>: EncodeMove, DecodeMove<'static>);
    assert_impl_all!(MixedView<Box<UnsafeCell<NonZeroU8>>>:
        RustSpec<Mutability = Exclusive<WithInterior<NonRobust>>>);
    assert_not_impl_any!(&MixedView<Box<UnsafeCell<NonZeroU8>>>: EncodeMove, DecodeMove<'static>);
}

#[test]
fn maybe_uninit_cells_allow_shared_validation_without_inspecting_contents() {
    type UninitCell = MaybeUninit<UnsafeCell<NonZeroU8>>;
    assert_impl_all!(&UninitCell: AbiCompatible, EncodeMove, DecodeMove<'static>);
    assert_impl_all!(&MaybeUninit<Cell<NonZeroU8>>: AbiCompatible, EncodeMove, DecodeMove<'static>);
    assert_impl_all!(&Box<UninitCell>: AbiCompatible, EncodeMove, DecodeMove<'static>);
    assert_impl_all!(&MaybeUninit<Box<UnsafeCell<NonZeroU8>>>:
        AbiCompatible, EncodeMove, DecodeMove<'static>);
    assert_not_impl_any!(&ManuallyDrop<UnsafeCell<NonZeroU8>>:
        AbiCompatible, EncodeMove, DecodeMove<'static>);

    let value = UninitCell::zeroed();
    let encoded = co3::encode(&value);
    let decoded = unsafe { co3::decode::<&UninitCell>(encoded) }.unwrap();
    assert!(core::ptr::eq(decoded, &value));

    let boxed = Box::new(UninitCell::zeroed());
    let encoded = co3::encode(&boxed);
    let decoded = unsafe { co3::decode::<&Box<UninitCell>>(encoded) }.unwrap();
    assert!(core::ptr::eq(decoded, &boxed));

    // Zero is invalid for NonZeroU8, but valid as uninterpreted MaybeUninit bytes.
    let representation = MaybeUninit::new(UnsafeCell::new(unsafe { core::mem::zeroed() }));
    let encoded = unsafe {
        co3::reference::CSoftRefMut::from_raw(core::ptr::from_ref(&representation).cast_mut())
    };
    assert!(unsafe { co3::decode::<&UninitCell>(encoded) }.is_some());
}

#[test]
fn shared_vector_requires_robust_cells_not_robust_elements() {
    type Element = Mixed<UnsafeCell<u8>>;
    assert_impl_all!(Element: RustSpec<Trap = NonRobust, Mutability = WithInterior<Robust>>);
    assert_impl_all!(&Vec<Element>: EncodeMove);
    assert_not_impl_any!(&Vec<Mixed<UnsafeCell<NonZeroU8>>>: EncodeMove);

    let values = vec![Mixed {
        flag: true,
        value: UnsafeCell::new(7_u8),
    }];
    let mut store = Default::default();
    let encoded = co3::soft_encode(&values, &mut store);
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
    let encoded = co3::encode(&source);
    assert!(unsafe { co3::decode::<&Mixed<UnsafeCell<u8>>>(encoded) }.is_some());

    // The invalid boolean exists only in the robust C representation.
    let invalid = unsafe {
        CMixed::<UnsafeCell<u8>>::from_fields(co3::primitives::CBool(2), UnsafeCell::new(7))
    };
    let encoded = unsafe {
        co3::reference::CSoftRefMut::from_raw(core::ptr::from_ref(&invalid).cast_mut())
    };
    assert!(unsafe { co3::decode::<&Mixed<UnsafeCell<u8>>>(encoded) }.is_none());
    let encoded = unsafe {
        co3::wide::CSoftWideRefMut::from_raw_parts_mut(
            core::ptr::from_ref(&invalid).cast_mut(),
            1,
        )
    };
    assert!(unsafe { co3::decode::<&[Mixed<UnsafeCell<u8>>]>(encoded) }.is_none());
}

#[test]
fn shared_plain_data_and_exclusive_cells_still_decode() {
    let value = Box::new(7_u8);
    let encoded = co3::encode(&value);
    let decoded = unsafe { co3::decode::<&Box<u8>>(encoded) }.unwrap();
    assert_eq!(**decoded, 7);

    let source = co3::boxed::CBox::from_box(Box::new(UnsafeCell::new(co3::NonZero(7_u8))));
    let decoded = unsafe { co3::decode::<Box<UnsafeCell<NonZeroU8>>>(source) }.unwrap();
    assert_eq!((*decoded).into_inner().get(), 7);
}

#[test]
fn box_shared_encoders_preserve_headers() {
    macro_rules! check {
        ($value:expr $(, len = $len:expr)?) => {{
            let value = $value;
            let expected = core::ptr::from_ref(&*value).cast::<()>();
            let mut store = Default::default();
            let encoded = co3::soft_encode(&value, &mut store);
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
