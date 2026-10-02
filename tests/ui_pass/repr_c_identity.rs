use co3::{CFnArg, ReprC, ffi, rust_spec::RustSpec};

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr_c(identity)]
#[repr(C)]
struct Integer(i32);

// Defining this name proves that `ReprC` did not synthesize `CInteger`.
struct CInteger;
struct IntegerData;
struct CIntegerData;

#[derive(RustSpec, ReprC)]
#[repr_c(identity)]
#[repr(C)]
struct NonCopy(i32);

struct CNonCopy;
struct NonCopyData;
struct CNonCopyData;

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr_c(identity)]
#[repr(transparent)]
struct Generic<T>(T);

#[derive(RustSpec, ReprC)]
#[repr_c(identity)]
#[repr(transparent)]
struct Bytes([u8]);

static_assertions::assert_type_eq_all!(<Bytes as ReprC>::CType, Bytes);
static_assertions::assert_not_impl_any!(Bytes: co3::borrow::Borrow);

static_assertions::assert_type_eq_all!(<Integer as ReprC>::CType, Integer);
static_assertions::assert_type_eq_all!(<NonCopy as ReprC>::CType, NonCopy);
static_assertions::assert_type_eq_all!(<Generic<u32> as ReprC>::CType, Generic<u32>);
static_assertions::assert_impl_all!(NonCopy: co3::Encode);
static_assertions::assert_not_impl_any!(NonCopy: CFnArg);
static_assertions::assert_not_impl_any!(Generic<bool>: ReprC);

ffi! {
    #![unsafe(export("C"))]

    fn identity(value: Integer) -> Integer;
}

fn identity(value: Integer) -> Integer {
    value
}

fn main() {}
