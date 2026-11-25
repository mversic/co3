use co3::{
    ReprC, lang,
    rust_spec::{
        RustSpec,
        drop::{AutoDrop, NoDrop},
    },
};

#[derive(RustSpec, ReprC)]
struct Transparent<T>(T);

#[derive(RustSpec, ReprC)]
struct ConcreteBox(Box<u8>);

static_assertions::assert_impl_all!(Transparent<u8>: RustSpec<Drop = NoDrop>);
static_assertions::assert_impl_all!(Transparent<Box<u8>>: RustSpec<Drop = AutoDrop>);
static_assertions::assert_type_eq_all!(
    <<Transparent<u8> as ReprC>::CType as ReprC>::CType,
    <Transparent<u8> as ReprC>::CType
);
static_assertions::assert_type_eq_all!(
    <<Transparent<Box<u8>> as ReprC>::CType as ReprC>::CType,
    <Transparent<Box<u8>> as ReprC>::CType
);
static_assertions::assert_type_eq_all!(
    <<ConcreteBox as ReprC>::CType as ReprC>::CType,
    <ConcreteBox as ReprC>::CType
);
static_assertions::assert_type_ne_all!(
    <Transparent<Box<u8>> as ReprC>::CType,
    <ConcreteBox as ReprC>::CType,
    co3::boxed::abi::Box<u8>
);

fn main() {
    let value = Transparent(Box::new(42u8));
    let encoded = unsafe { co3::encode::<_, lang::C>(value) };
    let decoded = unsafe { co3::decode::<Transparent<Box<u8>>, lang::C>(encoded) }.unwrap();
    assert_eq!(*decoded.0, 42);
}
