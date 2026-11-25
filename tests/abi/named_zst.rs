use core::marker::PhantomData;

use co3::{ReprC, lang, rust_spec::RustSpec};

#[derive(RustSpec, ReprC, Debug, PartialEq)]
struct Unit;

#[derive(RustSpec, ReprC, Debug, PartialEq)]
struct Empty {}

#[derive(RustSpec, ReprC, Debug, PartialEq)]
struct Marker<T: ?Sized>(PhantomData<T>);

#[derive(RustSpec, ReprC, Debug, PartialEq)]
#[repr(transparent)]
struct TransparentMarker<T: ?Sized>(PhantomData<T>);

#[derive(RustSpec, ReprC, Debug, PartialEq)]
enum Singleton {
    Only,
}

#[test]
fn named_zero_sized_values_round_trip() {
    let encoded = unsafe { co3::encode::<_, lang::C>(Unit) };
    assert_eq!(unsafe { co3::decode::<Unit, lang::C>(encoded) }, Some(Unit));
    let encoded = unsafe { co3::encode::<_, lang::C>(Empty {}) };
    assert_eq!(
        unsafe { co3::decode::<Empty, lang::C>(encoded) },
        Some(Empty {})
    );
    let encoded = unsafe { co3::encode::<_, lang::C>(Marker::<bool>(PhantomData)) };
    assert_eq!(
        unsafe { co3::decode::<Marker<bool>, lang::C>(encoded) },
        Some(Marker(PhantomData))
    );
    let encoded = unsafe { co3::encode::<_, lang::C>(TransparentMarker::<[bool]>(PhantomData)) };
    assert_eq!(
        unsafe { co3::decode::<TransparentMarker<[bool]>, lang::C>(encoded) },
        Some(TransparentMarker(PhantomData))
    );
    let encoded = unsafe { co3::encode::<_, lang::C>(Singleton::Only) };
    assert_eq!(
        unsafe { co3::decode::<Singleton, lang::C>(encoded) },
        Some(Singleton::Only)
    );
}
