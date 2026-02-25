use co3::{ExternC, ReprC, export, extern_, extern_C};
use webassembly_test::webassembly_test;

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
struct MyType<T>(Box<T>);

extern_! {
    #![abi = "Rust"]
    #![link(crate = "import")]

    impl AmbiguousX<u32, 4> for MyType<u32> {
        const K: bool = true;
        type U = i8;

        #[link_name = "kita"]
        fn ambiguous(a: &[Self::U; 4]) -> Ambiguous;
    }

    impl MyType<u64> {
        #[link_name = "kita1"]
        unsafe extern "C" fn ambiguous() -> MyType<u64>;
    }

    #[link_name = "kita2"]
    pub unsafe extern "C" fn ambiguous2_imported() -> Ambiguous;
}

extern_C! {
    #![link(crate = "import")]

    impl AmbiguousX<u64, 3> for MyType<u64> {
        const K: bool = false;
        type U = u8;

        fn ambiguous(a: &[Self::U; 3]) -> Ambiguous;
    }

    impl AmbiguousY for MyType<u64> {
        #[link_name = "ambiguous"]
        extern "C" fn ambiguous() -> Ambiguous;
    }

    impl MyType<u32> {
        #[link_name = "import_MyType_u32_ambiguous"]
        fn ambiguous() -> MyType<u32>;
    }

    #[link_name = "ambiguous1"]
    fn ambiguous1_imported() -> Ambiguous;
}

mod provider {
    use super::*;

    #[derive(Clone, Copy, ReprC)]
    #[repr_C(opaque)]
    #[repr(transparent)]
    struct MyType<T>(T);

    #[export("C")]
    impl AmbiguousX<u64, 3> for MyType<u64> {
        const K: bool = false;
        type U = u8;

        fn ambiguous(_a: &[Self::U; 3]) -> Ambiguous {
            Ambiguous::AmbiguousX
        }
    }

    #[export("Rust")]
    impl AmbiguousX<u32, 4> for MyType<u32> {
        const K: bool = true;
        type U = i8;

        #[unsafe(export_name = "kita")]
        fn ambiguous(_a: &[Self::U; 4]) -> Ambiguous {
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

    #[export("C")]
    impl AmbiguousY for MyType<u32> {
        #[export(skip)]
        extern "C" fn ambiguous() -> Ambiguous {
            Ambiguous::AmbiguousY
        }
    }

    #[export("Rust")]
    impl MyType<u64> {
        #[unsafe(export_name = "kita1")]
        pub unsafe extern "C" fn ambiguous() -> Box<MyType<u64>> {
            Box::new(MyType(42))
        }
    }

    #[export("C")]
    impl MyType<u32> {
        pub const fn ambiguous() -> MyType<u32> {
            MyType(420)
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
#[webassembly_test]
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

    assert_eq!(MyType(Box::new(420)), MyType::<u32>::ambiguous());
    assert_eq!(MyType(Box::new(42)), unsafe { MyType::<u64>::ambiguous() });

    assert_eq!(Ambiguous::Fn, ambiguous1_imported());
    assert_eq!(Ambiguous::Fn, unsafe { ambiguous2_imported() });
}
