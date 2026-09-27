use co3::{ffi, ops::CFn1, raw};

fn identity<T>(value: T) -> T {
    value
}

fn cloned<T: Clone>(value: T) -> T {
    value.clone()
}

fn const_value<const N: usize>() -> usize {
    N
}

fn borrowed<'a>(value: &'a u8) -> &'a u8 {
    value
}

struct Host<T>(core::marker::PhantomData<T>);

impl<T: Clone> Host<T> {
    fn echo(value: T) -> T {
        value
    }

    fn method<U: Clone>(value: U) -> U {
        value.clone()
    }
}

raw! {
    pub fn identity<T>(value: move T) -> move T;
    pub fn cloned<T: Clone>(value: T) -> move T;
    pub fn const_value<const N: usize>() -> usize;
    pub fn borrowed<'a>(value: &'a u8) -> &'a u8;

    impl<T: Clone> Host<T> {
        pub fn echo(value: move T) -> move T;
        pub fn method<U: Clone>(value: move U) -> move U;
    }
}

ffi! {
    #![unsafe(extern("C"))]

    type StringRaw = raw extern "C" fn(move String) -> move String;
    type BytesRaw = raw extern "C" fn(move Vec<u8>) -> move Vec<u8>;
}

fn main() {
    let identity: StringRaw = identity_raw::<String>;
    let value: String = unsafe { identity.call(String::from("hello")) }.unwrap();
    assert_eq!(value, "hello");

    assert_eq!(unsafe { cloned_raw::<u8>(7) }, 7);
    assert_eq!(unsafe { const_value_raw::<4>() }, 4);
    let byte = 9_u8;
    assert_eq!(unsafe { *borrowed_raw(&byte) }, 9);

    let echo: BytesRaw = Host::<Vec<u8>>::echo_raw;
    let value: Vec<u8> = unsafe { echo.call(vec![1_u8, 2, 3]) }.unwrap();
    assert_eq!(value, [1, 2, 3]);

    let method: StringRaw = Host::<u8>::method_raw::<String>;
    let value: String = unsafe { method.call(String::from("method")) }.unwrap();
    assert_eq!(value, "method");
}
