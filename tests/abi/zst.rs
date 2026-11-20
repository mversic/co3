use core::{
    cell::{Cell, UnsafeCell},
    mem::{ManuallyDrop, MaybeUninit},
    num::NonZero,
};

use co3::{CFnArg, CFnReturn, CType, ReprC, marker::AbiCompatible, rust_spec::RustSpec};
use static_assertions::{assert_impl_all, assert_not_impl_any};

#[derive(Debug, Clone, PartialEq, Eq, RustSpec, ReprC)]
pub struct NoReprStruct<T: ?Sized> {
    b: Box<T>,
}

#[derive(Debug, Clone, PartialEq, Eq, RustSpec, ReprC)]
pub enum NoReprEnum<T: ?Sized> {
    A(Box<T>),
}

#[derive(Debug, Clone, PartialEq, Eq, RustSpec, ReprC)]
#[repr(C)]
pub struct ReprCStruct<T: ?Sized> {
    b: Box<T>,
}

#[derive(Debug, Clone, PartialEq, Eq, RustSpec, ReprC)]
#[repr(u8)]
pub enum ReprCEnum<T: ?Sized> {
    #[repr_c(allow_zero_tag)]
    A(Box<T>),
}

#[derive(Debug, Clone, PartialEq, Eq, RustSpec, ReprC)]
#[repr(C, u8)]
pub enum ReprCDataEnum<T: ?Sized> {
    #[repr_c(allow_zero_tag)]
    A(Box<T>),
}

#[derive(Debug, Clone, PartialEq, Eq, RustSpec, ReprC)]
#[repr(transparent)]
pub struct TransparentStruct<T: ?Sized>(Box<T>);

#[derive(Debug, Clone, PartialEq, Eq, RustSpec, ReprC)]
#[repr(transparent)]
pub enum TransparentEnum<T: ?Sized> {
    A(Box<T>),
}

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Source<T>(T);

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
#[repr_c(identity)]
struct IdentityUnit;

#[unsafe(export_name = "zst__return_unit")]
extern "C" fn return_void() {}

#[unsafe(export_name = "zst__return_tuple")]
extern "C" fn return_void_tuple() {}

co3::ffi! {
    #![unsafe(extern("C"))]
    #![symbol_prefix = "zst"]

    fn return_unit() -> Source<()>;

    fn return_tuple() -> move ((), ());
}

#[test]
fn unit_like_returns_import_void_functions() {
    let Source(()) = return_unit();
    let ((), ()) = return_tuple();
}

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct SourceZst;

type CNoReprStructZst = <NoReprStruct<()> as ReprC>::CType;
type CNoReprEnumZst = <NoReprEnum<()> as ReprC>::CType;
type CReprCStructZst = <ReprCStruct<()> as ReprC>::CType;
type CReprCEnumZst = <ReprCEnum<()> as ReprC>::CType;
type CReprCDataEnumZst = <ReprCDataEnum<()> as ReprC>::CType;
type CTransparentStructZst = <TransparentStruct<()> as ReprC>::CType;
type CTransparentEnumZst = <TransparentEnum<()> as ReprC>::CType;

#[test]
fn boxed_zst_traits() {
    macro_rules! assert_traits {
        ($ty:ty, $ctype:ty) => {
            assert_impl_all!($ty: co3::convert::Decode<'static>, co3::convert::Encode);
            assert_impl_all!($ty: co3::convert::DecodeMove<'static>, co3::convert::EncodeMove);

            assert_impl_all!($ctype:
                CType,
                CFnArg,
                CFnReturn,
            );

            assert_not_impl_any!($ty: CType, CFnArg, CFnReturn);
        };
    }

    assert_traits!(NoReprStruct<()>, CNoReprStructZst);
    assert_traits!(NoReprEnum<()>, CNoReprEnumZst);
    assert_traits!(ReprCStruct<()>, CReprCStructZst);
    assert_traits!(ReprCEnum<()>, CReprCEnumZst);
    assert_traits!(ReprCDataEnum<()>, CReprCDataEnumZst);
    assert_traits!(TransparentStruct<()>, CTransparentStructZst);
    assert_traits!(TransparentEnum<()>, CTransparentEnumZst);

    assert_impl_all!(ReprCStruct<()>: AbiCompatible);
    assert_impl_all!(ReprCEnum<()>: AbiCompatible);
    assert_impl_all!(ReprCDataEnum<()>: AbiCompatible);
    assert_impl_all!(TransparentStruct<()>: AbiCompatible);
    assert_impl_all!(TransparentEnum<()>: AbiCompatible);

    assert_not_impl_any!(NoReprStruct<()>: AbiCompatible);
    assert_not_impl_any!(NoReprEnum<()>: AbiCompatible);
}

#[test]
fn unit_wrappers_are_return_values() {
    assert_not_impl_any!(UnsafeCell<()>: CFnArg);
    assert_impl_all!(UnsafeCell<()>: CFnReturn);
    assert_not_impl_any!(Cell<()>: CFnArg);
    assert_impl_all!(Cell<()>: CFnReturn);
    assert_not_impl_any!(MaybeUninit<()>: CFnArg);
    assert_impl_all!(MaybeUninit<()>: CFnReturn);
    assert_not_impl_any!(ManuallyDrop<()>: CFnArg);
    assert_impl_all!(ManuallyDrop<()>: CFnReturn);

    assert_impl_all!((): CFnReturn);
    assert_impl_all!(UnsafeCell<u8>: CFnArg, CFnReturn);
    assert_impl_all!(Cell<u8>: CFnArg, CFnReturn);
    assert_impl_all!(MaybeUninit<u8>: CFnArg, CFnReturn);
    assert_impl_all!(ManuallyDrop<u8>: CFnArg, CFnReturn);
}

#[test]
fn unit_like_return_layout_requires_alignment_one() {
    assert_impl_all!(Source<u32>: CType, CFnArg, CFnReturn);

    assert_impl_all!(Source<()>: CType);
    assert_not_impl_any!(Source<()>: CFnArg);
    assert_impl_all!(Source<()>: CFnReturn);
    assert_impl_all!(IdentityUnit: CFnReturn);
    assert_not_impl_any!(IdentityUnit: CFnArg);
    assert_not_impl_any!(Source<[u32; 0]>: CFnReturn);
    assert_impl_all!(Source<[u8; 0]>: CFnReturn);
    assert_impl_all!([u8; 0]: CFnReturn);
    assert_not_impl_any!([u32; 0]: CFnReturn);
    assert_impl_all!(<((), ()) as ReprC>::CType: CFnReturn);
    assert_not_impl_any!(<([u32; 0], ()) as ReprC>::CType: CFnReturn);
    assert_impl_all!(SourceZst: CType);
    assert_impl_all!(SourceZst: CFnReturn);
    assert_not_impl_any!(SourceZst: CFnArg);
}

#[test]
fn niche_optimized_option_and_result_are_function_values() {
    type MaybeNonZero = Option<NonZero<u8>>;
    type Callback = Option<extern "C" fn()>;
    type Success = Result<NonZero<u8>, ()>;
    type Failure = Result<(), NonZero<u8>>;

    assert_impl_all!(MaybeNonZero: CType, CFnArg, CFnReturn);
    assert_impl_all!(Callback: CType, CFnArg, CFnReturn);
    assert_impl_all!(Success: CType, CFnArg, CFnReturn);
    assert_impl_all!(Failure: CType, CFnArg, CFnReturn);
}

#[test]
fn transparent_enum_carrier_constructs_its_variant() {
    let carrier = <TransparentEnum<()> as ReprC>::CType::A(co3::encode(Box::new(())));
    assert_eq!(carrier.tag(), ());
    let decoded = unsafe { co3::decode::<TransparentEnum<()>>(carrier) }.unwrap();
    assert_eq!(decoded, TransparentEnum::A(Box::new(())));
}
