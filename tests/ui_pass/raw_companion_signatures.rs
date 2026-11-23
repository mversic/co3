use co3::{ReprC, ops::CFn1, raw};

fn existing(value: u8) -> u8 {
    value
}

raw! {
    fn existing(value: u8) -> u8;
}

fn identity<T>(value: T) -> T {
    value
}

raw! {
    fn identity<T>(value: move T) -> move T;
}

co3::ffi! {
    #![unsafe(extern("C"))]
    type ArrayCompanion = raw fn(move [u8; 2]) -> move [u8; 2];
    type TakesReference<'a, T> = raw fn(&'a T);
    type ReturnsReference<'a, T> = raw fn(&'a T) -> &'a T;
}

static_assertions::assert_not_impl_any!(ArrayCompanion: ReprC);

fn accept(_callback: for<'a> unsafe extern "C" fn()) {}

raw! {
    pub fn accept(callback: for<'a> unsafe extern "C" fn());
}

fn borrowed_generic<'a, T>(value: &'a T) -> &'a T {
    value
}

raw! {
    fn borrowed_generic<'a, T>(value: &'a T) -> &'a T;
}

fn take<'a, T>(_value: &'a T) {}

struct ArgumentHost;

impl ArgumentHost {
    fn take<'a, T>(_value: &'a T) {}
}

raw! {
    fn take<'a, T>(value: &'a T);

    impl ArgumentHost {
        fn take<'a, T>(value: &'a T);
    }
}

struct MethodHost;

impl MethodHost {
    fn borrowed_generic<'a, T>(value: &'a T) -> &'a T {
        value
    }
}

raw! {
    impl MethodHost {
        fn borrowed_generic<'a, T>(value: &'a T) -> &'a T;
    }
}

fn main() {
    let _: unsafe extern "C" fn(<[u8; 2] as ReprC>::CType) -> <[u8; 2] as ReprC>::CType =
        identity_raw::<[u8; 2]>;

    let _: TakesReference<'_, u8> = take_raw::<u8>;
    let _: TakesReference<'_, u8> = ArgumentHost::take_raw::<u8>;
    let _: ReturnsReference<'_, u8> = borrowed_generic_raw::<u8>;
    let _: ReturnsReference<'_, u8> = MethodHost::borrowed_generic_raw::<u8>;
    let _ = accept_raw;
    let callback: unsafe extern "C" fn(_) -> _ = existing_raw;
    assert_eq!(unsafe { callback.call(7u8) }, Some(7u8));
}
