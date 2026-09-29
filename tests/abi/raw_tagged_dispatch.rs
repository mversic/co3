use co3::{ReprC, Tag, raw, rust_spec::RustSpec, tag::Tagged};

#[derive(Clone, Copy, ReprC, RustSpec, Tag)]
#[tag(u8, unsafe(1))]
#[repr_c(identity)]
#[repr(transparent)]
struct First(u8);

#[derive(Clone, Copy, ReprC, RustSpec, Tag)]
#[tag(u8, unsafe(2))]
#[repr_c(identity)]
#[repr(transparent)]
struct Second(u8);

trait Value {
    fn value(self) -> u8;
}

impl Value for First {
    fn value(self) -> u8 {
        self.0 + 10
    }
}

impl Value for Second {
    fn value(self) -> u8 {
        self.0 + 20
    }
}

fn read<T: Value>(value: T) -> u8 {
    value.value()
}

struct Host;

struct GenericHost<T>(core::marker::PhantomData<T>);

impl Host {
    fn read<T: Value>(value: T) -> u8 {
        value.value() + 1
    }
}

impl<T: Value> GenericHost<T> {
    fn read(value: T) -> u8 {
        value.value() + 2
    }
}

raw! {
    pub fn read<dyn(u8) T = u8>(value: move T) -> u8
    where use<T> @ (<First> | <Second>);

    impl Host {
        pub fn read<dyn(u8) T = u8>(value: move T) -> u8
        where use<T> @ (<First> | <Second>);
    }

    impl<dyn(u8) T = u8> GenericHost<T>
    where use<T> @ (<First> | <Second>)
    {
        pub fn read(value: move T) -> u8;
    }
}

#[test]
fn companions_dispatch_by_tag() {
    let free: unsafe extern "C" fn(u8, u8) -> u8 = read_raw;
    let method: unsafe extern "C" fn(u8, u8) -> u8 = Host::read_raw;
    let generic_method: unsafe extern "C" fn(u8, u8) -> u8 = GenericHost::<First>::read_raw;
    assert_eq!(unsafe { free(First::TAG, 3) }, 13);
    assert_eq!(unsafe { free(Second::TAG, 3) }, 23);
    assert_eq!(unsafe { method(First::TAG, 3) }, 14);
    assert_eq!(unsafe { method(Second::TAG, 3) }, 24);
    assert_eq!(unsafe { generic_method(First::TAG, 3) }, 15);
    assert_eq!(unsafe { generic_method(Second::TAG, 3) }, 25);
}
