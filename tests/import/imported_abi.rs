use co3::{ReprC, export, extern_, extern_C};

trait AmbiguousX<T, const N: usize> {
    #[expect(unused)]
    const K: bool;
    type U;

    fn ambiguous(a: &[Self::U; N]) -> Ambiguous;
}

trait AmbiguousY {
    extern "C" fn ambiguous() -> Ambiguous;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ReprC)]
#[repr(u8)]
enum Ambiguous {
    AmbiguousX,
    AmbiguousY,
    Fn,
}

#[derive(Debug, Clone, PartialEq, ReprC)]
#[repr(transparent)]
struct MyType<T>(T);

extern_! {
    #![abi = "Rust"]
    #![link(crate = "import")]

    impl AmbiguousX<u32, 4> for MyType<u32> {
        const K: bool = true;
        type U = i8;

        #[link_name = "kita"]
        fn ambiguous(a: &[<Self as AmbiguousX<u32, 4>>::U; 4]) -> Ambiguous;
    }

    impl MyType<u64> {
        #[link_name = "kita1"]
        unsafe extern "C" fn ambiguous() -> Box<Self>;
    }

    #[link_name = "kita2"]
    pub unsafe extern "C" fn ambiguous2_imported() -> Ambiguous;
}

extern_C! {
    #![link(crate = "import")]

    type MyType2;

    impl Drop for MyType2 {
        fn drop(&mut self);
    }

    impl MyType2 {
        fn new() -> Self;
    }

    impl AmbiguousX<u64, 3> for MyType<u64> {
        const K: bool = false;
        type U = u8;

        fn ambiguous(a: &[<Self as AmbiguousX<u64, 3>>::U; 3]) -> Ambiguous;
    }

    impl AmbiguousY for MyType<u64> {
        #[link_name = "ambiguous"]
        extern "C" fn ambiguous() -> Ambiguous;
    }

    impl MyType<u32> {
        fn ambiguous() -> Self;
    }

    #[link_name = "ambiguous1"]
    fn ambiguous1_imported() -> Ambiguous;
}

mod provider {
    use co3::export_C;

    use super::*;

    #[derive(Clone, Copy, ReprC)]
    #[repr(transparent)]
    struct MyType<T>(T);

    #[repr(transparent)]
    enum MyType2 {
        #[expect(dead_code)]
        A(String),
    }

    export_C! {
        type MyType2;

        impl Drop for MyType2 {
            fn drop(&mut self);
        }
    }

    #[export("C")]
    impl MyType2 {
        fn new() -> Self {
            MyType2::A("KITA".to_owned())
        }
    }

    #[export("C")]
    impl AmbiguousX<u64, 3> for MyType<u64> {
        const K: bool = false;
        type U = u8;

        fn ambiguous(_a: &[<Self as AmbiguousX<u64, 3>>::U; 3]) -> Ambiguous {
            Ambiguous::AmbiguousX
        }
    }

    #[export("Rust")]
    impl AmbiguousX<u32, 4> for MyType<u32> {
        const K: bool = true;
        type U = i8;

        #[unsafe(export_name = "kita")]
        fn ambiguous(_a: &[<Self as AmbiguousX<u32, 4>>::U; 4]) -> Ambiguous {
            Ambiguous::AmbiguousX
        }
    }

    #[export("C")]
    impl AmbiguousY for MyType<u64> {
        #[unsafe(no_mangle)]
        extern "C" fn ambiguous() -> Ambiguous {
            Ambiguous::AmbiguousY
        }
    }

    #[export("Rust")]
    impl MyType<u64> {
        #[unsafe(export_name = "kita1")]
        pub unsafe extern "C" fn ambiguous() -> Box<Self> {
            Box::new(Self(42))
        }
    }

    #[export("C")]
    impl MyType<u32> {
        pub const fn ambiguous() -> Self {
            Self(420)
        }
    }

    #[export("C")]
    #[unsafe(no_mangle)]
    pub const fn ambiguous1() -> Ambiguous {
        Ambiguous::Fn
    }

    #[export("Rust")]
    #[unsafe(export_name = "kita2")]
    pub const unsafe extern "Rust" fn ambiguous2() -> Ambiguous {
        Ambiguous::Fn
    }
}

#[test]
fn extern_abi() {
    assert_eq!(
        Ambiguous::AmbiguousX,
        <MyType::<u64> as AmbiguousX<u64, 3>>::ambiguous(&[1_u8, 2, 3])
    );
    assert_eq!(
        Ambiguous::AmbiguousX,
        <MyType::<u32> as AmbiguousX<u32, 4>>::ambiguous(&[1_i8, 2, 3, 4])
    );

    assert_eq!(
        Ambiguous::AmbiguousY,
        <MyType::<u64> as AmbiguousY>::ambiguous()
    );

    assert_eq!(MyType(420), MyType::<u32>::ambiguous());
    assert_eq!(MyType(42), *unsafe { MyType::<u64>::ambiguous() });

    assert_eq!(Ambiguous::Fn, ambiguous1_imported());
    assert_eq!(Ambiguous::Fn, unsafe { ambiguous2_imported() });

    let my_type2 = MyType2::new();
    let a = my_type2.0.as_ptr().cast::<String>();
    assert_eq!("KITA", unsafe { &*a })
}
