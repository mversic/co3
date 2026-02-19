use co3::ExternC;
use webassembly_test::webassembly_test;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ExternC)]
#[repr(u8)]
pub enum Ambiguous {
    AmbiguousX,
    AmbiguousY,
    Inherent,
}

#[derive(Clone, Copy, ExternC)]
#[mineral(opaque)]
pub struct OpaqueStruct {
    // NOTE: replaced by co3::decarbonate
}

trait AmbiguousX {
    fn ambiguous() -> Ambiguous;
}

trait AmbiguousY {
    fn ambiguous() -> Ambiguous;
}

#[co3::decarbonate]
impl AmbiguousX for OpaqueStruct {
    fn ambiguous() -> Ambiguous {
        unreachable!("replaced by co3::decarbonate")
    }
}

#[co3::decarbonate]
impl AmbiguousY for OpaqueStruct {
    fn ambiguous() -> Ambiguous {
        unreachable!("replaced by co3::decarbonate")
    }
}

#[co3::decarbonate]
impl OpaqueStruct {
    pub fn ambiguous() -> Ambiguous {
        unreachable!("replaced by co3::decarbonate")
    }
}

mod provider {
    use super::*;

    #[derive(Clone, Copy, ExternC)]
    #[mineral(opaque)]
    pub struct OpaqueStruct;

    trait AmbiguousX {
        fn ambiguous() -> Ambiguous;
    }

    trait AmbiguousY {
        fn ambiguous() -> Ambiguous;
    }

    #[co3::carbonate]
    impl AmbiguousX for OpaqueStruct {
        fn ambiguous() -> Ambiguous {
            Ambiguous::AmbiguousX
        }
    }

    #[co3::carbonate]
    impl AmbiguousY for OpaqueStruct {
        fn ambiguous() -> Ambiguous {
            Ambiguous::AmbiguousY
        }
    }

    #[co3::carbonate]
    impl OpaqueStruct {
        pub fn ambiguous() -> Ambiguous {
            Ambiguous::Inherent
        }
    }
}

#[test]
#[webassembly_test]
fn disambiguated_method_call() {
    assert_eq!(
        Ambiguous::AmbiguousX,
        <OpaqueStruct as AmbiguousX>::ambiguous()
    );

    assert_eq!(
        Ambiguous::AmbiguousY,
        <OpaqueStruct as AmbiguousY>::ambiguous()
    );

    assert_eq!(Ambiguous::Inherent, OpaqueStruct::ambiguous());
}
