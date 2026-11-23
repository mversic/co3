use co3::{ReprC, rust_spec::RustSpec};
use core::marker::PhantomData;

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
    let encoded = co3::encode(Unit);
    assert_eq!(unsafe { co3::decode::<Unit>(encoded) }, Some(Unit));
    let encoded = co3::encode(Empty {});
    assert_eq!(unsafe { co3::decode::<Empty>(encoded) }, Some(Empty {}));
    let encoded = co3::encode(Marker::<bool>(PhantomData));
    assert_eq!(
        unsafe { co3::decode::<Marker<bool>>(encoded) },
        Some(Marker(PhantomData))
    );
    let encoded = co3::encode(TransparentMarker::<[bool]>(PhantomData));
    assert_eq!(
        unsafe { co3::decode::<TransparentMarker<[bool]>>(encoded) },
        Some(TransparentMarker(PhantomData))
    );
    let encoded = co3::encode(Singleton::Only);
    assert_eq!(
        unsafe { co3::decode::<Singleton>(encoded) },
        Some(Singleton::Only)
    );
}
