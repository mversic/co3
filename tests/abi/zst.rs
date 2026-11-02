use core::{
    cell::{Cell, UnsafeCell},
    mem::{ManuallyDrop, MaybeUninit},
    num::NonZero,
};

use co3::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC,
    borrow::{Borrow, Borrowed},
    reference::CRefMut,
    rust_spec::RustSpec,
    transmute::AbiCompatible,
};
use static_assertions::{assert_impl_all, assert_not_impl_any, assert_type_eq_all};

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
            assert_impl_all!($ty: Decode<'static>, Encode);
            assert_impl_all!($ty: co3::stored::DecodeMove<'static>, co3::stored::EncodeMove);

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
fn zero_sized_wrappers_are_not_function_values() {
    assert_not_impl_any!(Borrowed<()>: CFnArg, CFnReturn);
    assert_not_impl_any!(UnsafeCell<()>: CFnArg, CFnReturn);
    assert_not_impl_any!(Cell<()>: CFnArg, CFnReturn);
    assert_not_impl_any!(MaybeUninit<()>: CFnArg, CFnReturn);
    assert_not_impl_any!(ManuallyDrop<()>: CFnArg, CFnReturn);

    assert_impl_all!((): CFnReturn);
    assert_impl_all!(UnsafeCell<u8>: CFnArg, CFnReturn);
    assert_impl_all!(Cell<u8>: CFnArg, CFnReturn);
    assert_impl_all!(MaybeUninit<u8>: CFnArg, CFnReturn);
    assert_impl_all!(ManuallyDrop<u8>: CFnArg, CFnReturn);
}

#[test]
fn source_types_get_function_markers_only_when_nonzero() {
    assert_impl_all!(Source<u32>: CType, CFnArg, CFnReturn, Borrow);
    assert_type_eq_all!(<Source<u32> as Borrow>::Borrowed, Source<u32>);

    assert_impl_all!(Source<()>: CType, Borrow);
    assert_not_impl_any!(Source<()>: CFnArg, CFnReturn);
    assert_impl_all!(SourceZst: CType, Borrow);
    assert_not_impl_any!(SourceZst: CFnArg, CFnReturn);
}

#[test]
fn niche_optimized_option_and_result_are_function_values() {
    type MaybeNonZero = Option<NonZero<u8>>;
    type Callback = Option<extern "C" fn()>;
    type Success = Result<NonZero<u8>, ()>;
    type Failure = Result<(), NonZero<u8>>;

    assert_impl_all!(MaybeNonZero: CType, CFnArg, CFnReturn, Borrow);
    assert_impl_all!(Callback: CType, CFnArg, CFnReturn, Borrow);
    assert_impl_all!(Success: CType, CFnArg, CFnReturn, Borrow);
    assert_impl_all!(Failure: CType, CFnArg, CFnReturn, Borrow);
    assert_type_eq_all!(<MaybeNonZero as Borrow>::Borrowed, MaybeNonZero);
    assert_type_eq_all!(<Success as Borrow>::Borrowed, Success);
    assert_type_eq_all!(<Failure as Borrow>::Borrowed, Failure);
}

#[test]
fn wrappers_preserve_their_borrowed_representations() {
    assert_impl_all!(Cell<u8>: Borrow);
    assert_type_eq_all!(<Cell<u8> as Borrow>::Borrowed, Cell<u8>);

    assert_impl_all!(ManuallyDrop<u8>: Borrow);
    assert_type_eq_all!(<ManuallyDrop<u8> as Borrow>::Borrowed, ManuallyDrop<u8>);

    assert_impl_all!(CRefMut<u8, true>: Borrow);
    assert_type_eq_all!(<CRefMut<u8, true> as Borrow>::Borrowed, CRefMut<u8, true>);
}

#[test]
fn transparent_enum_carrier_constructs_its_variant() {
    let carrier = CTransparentEnum::<()>::A(co3::boxed::CBox::from_box(Box::new(())));
    assert_eq!(carrier.tag(), ());
    let decoded = unsafe { co3::decode::<TransparentEnum<()>>(carrier) }.unwrap();
    assert_eq!(decoded, TransparentEnum::A(Box::new(())));
}
