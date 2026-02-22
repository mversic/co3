use co3::{ExternC, extern_c};
use webassembly_test::webassembly_test;

trait AmbiguousX<T> {
    fn ambiguous() -> Ambiguous;
}

trait AmbiguousY {
    fn ambiguous() -> Ambiguous;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ExternC)]
#[repr(u8)]
enum Ambiguous {
    AmbiguousX,
    AmbiguousY,
    Fn,
}

#[derive(Debug, Clone, PartialEq, ExternC)]
#[repr(transparent)]
struct MyType<T>(Box<T>);

#[co3::decarbonate(link_crate = "decarbonate")]
impl AmbiguousX<u32> for MyType<u32> {
    fn ambiguous() -> Ambiguous {
        unreachable!("replaced by co3::decarbonate")
    }
}

#[co3::decarbonate(link_crate = "overriden_by_link_name")]
impl AmbiguousX<u64> for MyType<u64> {
    #[co3::decarbonate(link_name = "decarbonate_AmbiguousX_u64_MyType_u64_ambiguous")]
    fn ambiguous() -> Ambiguous {
        unreachable!("replaced by co3::decarbonate")
    }
}

extern_c! {
    #[link_crate = "overriden_by_link_name"]
    impl AmbiguousY for MyType<u32> {
        #[link_name = "decarbonate_AmbiguousY_MyType_u32_ambiguous"]
        fn ambiguous() -> Ambiguous;
    }

    #[link_crate = "decarbonate"]
    impl AmbiguousY for MyType<u64> {
        fn ambiguous() -> Ambiguous;
    }

    #[link_crate = "overriden_by_link_name"]
    impl MyType<u32> {
        #[link_name = "decarbonate_MyType_u32_ambiguous"]
        fn ambiguous() -> MyType<u32>;
    }

    #[link_crate = "decarbonate"]
    impl MyType<u64> {
        fn ambiguous() -> MyType<u64>;
    }

    #[link_name = "decarbonate_ambiguous"]
    fn ambiguous() -> MyType<u32>;

    #[link_crate = "overriden_by_link_name"]
    #[link_name = "decarbonate_AmbiguousX_u32_MyType_u32_ambiguous"]
    fn duplicate_x() -> Ambiguous;
}

mod provider {
    use super::*;

    #[derive(Clone, Copy, ExternC)]
    #[mineral(opaque)]
    struct MyType<T>(T);

    #[co3::carbonate]
    impl AmbiguousX<u32> for MyType<u32> {
        fn ambiguous() -> Ambiguous {
            Ambiguous::AmbiguousX
        }
    }

    #[co3::carbonate]
    impl AmbiguousX<u64> for MyType<u64> {
        fn ambiguous() -> Ambiguous {
            Ambiguous::AmbiguousX
        }
    }

    #[co3::carbonate]
    impl AmbiguousY for MyType<u32> {
        fn ambiguous() -> Ambiguous {
            Ambiguous::AmbiguousY
        }
    }

    #[co3::carbonate]
    impl AmbiguousY for MyType<u64> {
        fn ambiguous() -> Ambiguous {
            Ambiguous::AmbiguousY
        }
    }

    #[co3::carbonate]
    impl MyType<u32> {
        pub fn ambiguous() -> MyType<u32> {
            MyType(42)
        }
    }

    #[co3::carbonate]
    impl MyType<u64> {
        pub fn ambiguous() -> MyType<u32> {
            MyType(43)
        }
    }

    #[co3::carbonate]
    pub fn ambiguous() -> MyType<u32> {
        MyType(420)
    }
}

#[test]
#[webassembly_test]
fn extern_abi() {
    assert_eq!(
        Ambiguous::AmbiguousX,
        <MyType::<u32> as AmbiguousX<u32>>::ambiguous()
    );
    assert_eq!(
        Ambiguous::AmbiguousX,
        <MyType::<u64> as AmbiguousX<u64>>::ambiguous()
    );

    assert_eq!(
        Ambiguous::AmbiguousY,
        <MyType::<u32> as AmbiguousY>::ambiguous()
    );
    assert_eq!(
        Ambiguous::AmbiguousY,
        <MyType::<u64> as AmbiguousY>::ambiguous()
    );

    assert_eq!(MyType(Box::new(42)), MyType::<u32>::ambiguous());
    assert_eq!(MyType(Box::new(43)), MyType::<u64>::ambiguous());

    assert_eq!(MyType(Box::new(420)), ambiguous());

    assert_eq!(Ambiguous::AmbiguousX, duplicate_x());
}
