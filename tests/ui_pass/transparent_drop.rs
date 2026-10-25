use co3::{
    ReprC,
    rust_spec::{
        RustSpec,
        drop::{AutoDrop, NoDrop},
    },
};

#[derive(RustSpec, ReprC)]
#[repr_c(transparent)]
struct Transparent<T>(T);

#[derive(RustSpec, ReprC)]
#[repr_c(transparent)]
struct ConcreteBox(Box<u8>);

static_assertions::assert_impl_all!(Transparent<u8>: RustSpec<Drop = NoDrop>);
static_assertions::assert_impl_all!(Transparent<Box<u8>>: RustSpec<Drop = AutoDrop>);
static_assertions::assert_type_eq_all!(<Transparent<u8> as ReprC>::CType, u8);
static_assertions::assert_type_eq_all!(
    <Transparent<Box<u8>> as ReprC>::CType,
    <ConcreteBox as ReprC>::CType,
    <Box<u8> as ReprC>::CType
);

fn main() {
    let value = Transparent(Box::new(42u8));
    let encoded = co3::encode(value);
    let decoded = unsafe { co3::decode::<Transparent<Box<u8>>>(encoded) }.unwrap();
    assert_eq!(*decoded.0, 42);
}
