use co3::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC,
    borrow::{BorrowCast, BorrowCastMut},
    reference::CRefMut,
    rust_spec::RustSpec,
    transmute::CheckedTransmute,
};
use core::{
    cell::{Cell, UnsafeCell},
    mem::{ManuallyDrop, MaybeUninit},
    num::NonZero,
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
    A(Box<T>),
}

#[derive(Debug, Clone, PartialEq, Eq, RustSpec, ReprC)]
#[repr(C, u8)]
pub enum ReprCDataEnum<T: ?Sized> {
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
            assert_impl_all!($ty:
                Decode<'static>,
                Encode,
            );

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

    assert_impl_all!(ReprCStruct<()>: CheckedTransmute);
    assert_impl_all!(ReprCEnum<()>: CheckedTransmute);
    assert_impl_all!(ReprCDataEnum<()>: CheckedTransmute);
    assert_impl_all!(TransparentStruct<()>: CheckedTransmute);
    assert_impl_all!(TransparentEnum<()>: CheckedTransmute);

    assert_not_impl_any!(NoReprStruct<()>: CheckedTransmute);
    assert_not_impl_any!(NoReprEnum<()>: CheckedTransmute);
}

#[test]
fn zero_sized_wrappers_are_not_function_values() {
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
    assert_impl_all!(Source<u32>: CType, CFnArg, CFnReturn, BorrowCast, BorrowCastMut);
    assert_type_eq_all!(<Source<u32> as BorrowCast>::AsConst, Source<u32>);
    assert_type_eq_all!(<Source<u32> as BorrowCastMut>::AsMut, Source<u32>);

    assert_impl_all!(Source<()>: CType, BorrowCast, BorrowCastMut);
    assert_not_impl_any!(Source<()>: CFnArg, CFnReturn);
    assert_impl_all!(SourceZst: CType, BorrowCast, BorrowCastMut);
    assert_not_impl_any!(SourceZst: CFnArg, CFnReturn);
}

#[test]
fn niche_optimized_option_and_result_are_function_values() {
    type Optional = Option<NonZero<u8>>;
    type Callback = Option<extern "C" fn()>;
    type Success = Result<NonZero<u8>, ()>;
    type Failure = Result<(), NonZero<u8>>;

    assert_impl_all!(Optional: CType, CFnArg, CFnReturn, BorrowCast, BorrowCastMut);
    assert_impl_all!(Callback: CType, CFnArg, CFnReturn, BorrowCast, BorrowCastMut);
    assert_impl_all!(Success: CType, CFnArg, CFnReturn, BorrowCast, BorrowCastMut);
    assert_impl_all!(Failure: CType, CFnArg, CFnReturn, BorrowCast, BorrowCastMut);
    assert_type_eq_all!(<Optional as BorrowCast>::AsConst, Optional);
    assert_type_eq_all!(<Success as BorrowCastMut>::AsMut, Success);
    assert_type_eq_all!(<Failure as BorrowCast>::AsConst, Failure);
}

#[test]
fn wrapper_borrow_casts_match_their_borrowed_representations() {
    assert_impl_all!(Cell<u8>: BorrowCast, BorrowCastMut);
    assert_type_eq_all!(<Cell<u8> as BorrowCast>::AsConst, UnsafeCell<u8>);
    assert_type_eq_all!(<Cell<u8> as BorrowCastMut>::AsMut, UnsafeCell<u8>);

    assert_impl_all!(ManuallyDrop<u8>: BorrowCast, BorrowCastMut);
    assert_type_eq_all!(<ManuallyDrop<u8> as BorrowCast>::AsConst, ManuallyDrop<u8>);
    assert_type_eq_all!(<ManuallyDrop<u8> as BorrowCastMut>::AsMut, ManuallyDrop<u8>);

    assert_impl_all!(CRefMut<u8, true>: BorrowCast, BorrowCastMut);
    assert_type_eq_all!(<CRefMut<u8, true> as BorrowCast>::AsConst, CRefMut<u8, true>);
    assert_type_eq_all!(<CRefMut<u8, true> as BorrowCastMut>::AsMut, CRefMut<u8, true>);
}

#[test]
fn transparent_enum_carrier_constructs_its_variant() {
    let carrier = CTransparentEnum::<()>::A(co3::boxed::CBox::from_box(Box::new(())));
    assert_eq!(carrier.tag(), ());
    let decoded = unsafe { co3::decode::<TransparentEnum<()>>(carrier) }.unwrap();
    assert_eq!(decoded, TransparentEnum::A(Box::new(())));
}
