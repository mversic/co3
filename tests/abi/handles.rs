use co3::{export, extern_C, extern_type, external::ExternRef};
use webassembly_test::webassembly_test;

use crate::{Custom, ExtraCustom};

#[extern_type(
    Drop::drop = "abi_Drop_drop",
    Clone::clone = "abi_Clone_clone",
    Eq::eq = "abi_Eq_eq",
    Ord::cmp = "abi_Ord_cmp",
    Custom::inc = "abi_Custom_inc",
    Custom::dec = "abi_Custom_dec",
    Custom::touch = "abi_Custom_touch",
    Custom::add_and_get = "abi_Custom_add_and_get",
    Custom::seeded = "abi_Custom_seeded",
    ExtraCustom::bump2 = "abi_ExtraCustom_bump2"
)]
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Custom, ExtraCustom)]
#[repr_C(opaque)]
pub struct Handle<T>;
co3::handles! {1, Handle<bool>}

extern_C! {
    #[link_name = "abi_Handle_bool_new"]
    pub fn handle_new(id: u8) -> Handle<bool>;

    #[link_name = "abi_Handle_bool_id"]
    pub fn handle_id(handle: ExternRef<'_, Handle<bool>>) -> u8;

    #[link_name = "roundtrip"]
    pub fn roundtrip(input: ExternRef<'_, Handle<bool>>) -> ExternRef<'_, Handle<bool>>;
}

mod provider {
    use core::marker::PhantomData;

    use super::Custom;
    use crate::ExtraCustom;
    use co3::{ExternC, ReprC, external::ExternRef};

    co3::handles! {1, Handle<bool>}

    co3::def_fns! {
        Drop: { Handle<bool> },
        Clone: { Handle<bool> },
        Eq: { super::Handle<bool> },
        Ord: { crate::handles::provider::Handle<bool> },
        Custom: { Handle<bool> },
        ExtraCustom: { Handle<bool> },
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, ReprC)]
    #[repr_C(opaque)]
    pub struct Handle<T> {
        id: u8,
        _marker: PhantomData<T>,
    }

    impl<T> Custom for Handle<T> {
        fn inc(mut self) -> Self {
            self.id += 1;
            self
        }

        fn dec(mut self) -> Self {
            self.id -= 1;
            self
        }

        fn touch(&mut self) {
            self.id = self.id.wrapping_add(1);
        }

        fn add_and_get(&mut self, inc: u8) -> u8 {
            self.id = self.id.wrapping_add(inc);
            self.id
        }

        fn seeded(id: u8) -> Self {
            Self {
                id,
                _marker: PhantomData,
            }
        }
    }

    impl<T> ExtraCustom for Handle<T> {
        fn bump2(mut self) -> Self {
            self.id += 2;
            self
        }
    }

    #[export("C")]
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

    #[export("C")]
    pub fn roundtrip(input: ExternRef<'_, Handle<bool>>) -> ExternRef<'_, Handle<bool>> {
        input
    }
}

#[test]
#[webassembly_test]
fn opaque_handle_cross_boundary() {
    use crate::{Custom as _, ExtraCustom as _};

    let handle = handle_new(41);
    let handle = handle.inc();
    let handle = handle.dec();
    let handle = handle.bump2();
    let mut handle = handle;
    handle.touch();
    let seen = handle.add_and_get(3);
    let seeded = Handle::<bool>::seeded(5);
    assert_eq!(47, handle_id(handle.as_ref()));
    assert_eq!(47, seen);
    assert_eq!(5, handle_id(seeded.as_ref()));

    let mut direct = handle_new(10);
    direct.touch();
    assert_eq!(13, direct.add_and_get(2));
    assert_eq!(7, handle_id(Handle::<bool>::seeded(7).as_ref()));
    assert_eq!(11, handle_id(direct.bump2().as_ref()));

    let handle_ref = roundtrip(handle.as_ref());
    let cloned: Handle<bool> = Clone::clone(&handle_ref);
    assert!(*handle_ref == cloned);
}
