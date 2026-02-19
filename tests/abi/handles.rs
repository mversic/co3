#![cfg(feature = "derive")]

use co3::external::ExternRef;
use webassembly_test::webassembly_test;

co3::handles! {Handle<bool>}
co3::decl_fns! {Drop, Clone, Eq, Ord}

#[co3::extern_type]
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
#[mineral(opaque)]
pub struct Handle<T> {
    // NOTE: replaced by co3::decarbonate
}

#[co3::decarbonate]
impl Handle<bool> {
    pub fn new(id: u8) -> Self {
        unreachable!("replaced by co3::decarbonate")
    }

    pub fn bump(self) -> Self {
        unreachable!("replaced by co3::decarbonate")
    }

    pub fn id(&self) -> u8 {
        unreachable!("replaced by co3::decarbonate")
    }
}

#[co3::decarbonate]
pub fn roundtrip(input: ExternRef<'_, Handle<bool>>) -> ExternRef<'_, Handle<bool>> {
    unreachable!("replaced by co3::decarbonate")
}

mod provider {
    use core::marker::PhantomData;

    use co3::{ExternC, external::ExternRef};

    co3::handles! {
        Handle<bool>
    }

    co3::def_fns! {
        Drop: { Handle<bool> },
        Clone: { Handle<bool> },
        Eq: { super::Handle<bool> },
        Ord: { crate::handles::provider::Handle<bool> },
        Custom::bump(self): { Handle<bool> },
    }

    trait Custom {
        fn bump(self) -> Self;
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, ExternC)]
    #[mineral(opaque)]
    pub struct Handle<T> {
        id: u8,
        _marker: PhantomData<T>,
    }

    impl<T> Custom for Handle<T> {
        fn bump(mut self) -> Self {
            self.id += 1;
            self
        }
    }

    #[co3::carbonate]
    impl Handle<bool> {
        pub fn new(id: u8) -> Self {
            Self {
                id,
                _marker: PhantomData,
            }
        }

        pub fn id(&self) -> u8 {
            self.id
        }
    }

    #[co3::carbonate]
    pub fn roundtrip(input: ExternRef<'_, Handle<bool>>) -> ExternRef<'_, Handle<bool>> {
        input
    }
}

#[test]
#[webassembly_test]
fn opaque_handle_cross_boundary() {
    let handle = Handle::new(41);
    let handle = handle.bump();
    assert_eq!(42, handle.id());

    let handle_ref = roundtrip(handle.as_ref());
    let cloned: Handle<bool> = Clone::clone(&handle_ref);
    assert!(*handle_ref == cloned);
}
