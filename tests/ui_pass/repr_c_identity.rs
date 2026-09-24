use co3::{CFnArg, ExternC, ReprC, ffi, rust_spec::RustSpec};

#[derive(Clone, Copy, RustSpec, ReprC)]
#[reprC(identity)]
#[repr(C)]
struct Integer(i32);

// Defining this name proves that `ReprC` did not synthesize `CInteger`.
struct CInteger;
struct IntegerData;
struct CIntegerData;

#[derive(RustSpec, ReprC)]
#[reprC(identity)]
#[repr(C)]
struct NonCopy(i32);

struct CNonCopy;
struct NonCopyData;
struct CNonCopyData;

#[derive(Clone, Copy, RustSpec, ReprC)]
#[reprC(identity)]
#[repr(transparent)]
struct Generic<T>(T);

static_assertions::assert_type_eq_all!(<Integer as ExternC>::CType, Integer);
static_assertions::assert_type_eq_all!(<NonCopy as ExternC>::CType, NonCopy);
static_assertions::assert_type_eq_all!(<Generic<u32> as ExternC>::CType, Generic<u32>);
static_assertions::assert_impl_all!(NonCopy: co3::Encode);
static_assertions::assert_not_impl_any!(NonCopy: CFnArg);
static_assertions::assert_not_impl_any!(Generic<bool>: ExternC);

ffi! {
    #![unsafe(export("C"))]

    fn identity(value: Integer) -> Integer;
}

fn identity(value: Integer) -> Integer {
    value
}

fn main() {}
