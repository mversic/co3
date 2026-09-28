use co3::{Tag, ffi};

trait Values {
    const BASE: u8;
    const VALUE: u8;
}

struct Host<T>(core::marker::PhantomData<T>);

impl Values for Host<u8> {
    const BASE: u8 = 1;
    const VALUE: u8 = 2;
}

impl Values for Host<u16> {
    const BASE: u8 = 1;
    const VALUE: u8 = 2;
}

#[derive(Tag)]
#[tag(u8, unsafe(1))]
struct Tagged;

impl Values for Host<Tagged> {
    const BASE: u8 = 1;
    const VALUE: u8 = 2;
}

struct Boolean;

impl Boolean {
    const READY: bool = true;
}

trait Number {
    const VALUE: usize;
}

struct NumberHost<const N: usize>;

impl<const N: usize> Number for NumberHost<N> {
    const VALUE: usize = N;
}

ffi! {
    #![unsafe(export("C"))]

    impl<T> Values for Host<T>
    where
        use<T> @ (<u8> | <u16>),
    {
        const BASE: u8 = 1;
        const VALUE: u8 = Self::BASE + 1;
    }

    impl<dyn(u8) T> Values for Host<T>
    where
        use<T> @ <Tagged>,
    {
        const BASE: u8 = 1;
        const VALUE: u8 = Self::BASE + 1;
    }

    impl Boolean {
        const READY: bool = true;
    }

    impl<const N: usize> Number for NumberHost<N>
    where
        use<N> @ (<1> | <2>),
    {
        const VALUE: usize = N;
    }
}

fn main() {}
