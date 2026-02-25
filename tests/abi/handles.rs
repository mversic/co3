use co3::extern_C;
use webassembly_test::webassembly_test;

trait Custom {
    fn inc(self) -> Self;
}

co3::handles! {
    Handle::<bool, u8> = 1,
    Handle::<u8, bool>,
}

extern_C! {
    type Handle<T, U>;

    #[dispatch]
    impl<T, U> Drop for Handle<T, U> {
        fn drop(&mut self);
    }

    #[dispatch(
        T = [bool, u8],
        U = [u8, bool]
    )]
    impl Handle<bool, u8> {
        #[id_pos(Self: 1)]
        #[link_name = "handle_as_ref"]
        fn as_ref(&self) -> Result<&Self, u8>;
    }

    #[dispatch(
        Self = [
            Handle<bool, u8>,
            Handle<u8, bool>
        ]
    )]
    impl<T> Clone for T {
        #[link_name = "abi_Clone_clone"]
        fn clone(&self) -> Self;
    }

    #[dispatch(
        Self = [
            Handle<bool, u8>,
            Handle<u8, bool>
        ]
    )]
    impl<T> Default for T {
        #[link_name = "default"]
        #[id_pos(Self: 0)]
        fn default() -> Self;
    }

    #[dispatch(
        T = [Handle<bool, u8>],
    )]
    impl<T> PartialEq for T {
        #[link_name = "abi_Eq_eq"]
        fn eq(&self, other: &Self) -> bool;
    }

    #[dispatch(
        T = [Handle<bool, u8>],
        U = [Handle<u8, bool>]
    )]
    impl<T, U> PartialEq<U> for T {
        #[link_name = "abi_Eq_eq_2"]
        #[id_pos(Self: 1)]
        fn eq(&self, other: &U) -> bool;
    }

    #[dispatch(
        Self = [Handle<bool, u8>]
    )]
    impl<T> Custom for T {
        #[link_name = "abi_Custom_inc"]
        fn inc(self) -> Self;
    }
}

mod provider {
    use core::marker::PhantomData;

    use co3::{ReprC, export_C, handles};

    use super::Custom;

    handles! {
        Handle<bool, u8> = 1,
        Handle<u8, bool>,
    }

    #[derive(Debug, Default, Clone, PartialEq, Eq, ReprC)]
    #[reprC(opaque)]
    pub struct Handle<T, U> {
        id: u8,
        _marker: PhantomData<(T, U)>,
    }

    impl PartialEq<Handle<u8, bool>> for Handle<bool, u8> {
        fn eq(&self, other: &Handle<u8, bool>) -> bool {
            self.id == other.id
        }
    }

    impl<T, U> Custom for Handle<T, U> {
        fn inc(mut self) -> Self {
            self.id += 1;
            self
        }
    }

    impl<T, U> Handle<T, U> {
        fn as_ref(&self) -> Result<&Self, u8> {
            Ok(self)
        }
    }

    export_C! {
        #[dispatch(
            Self = [Handle<bool, u8>, Handle<u8, bool>]
        )]
        #[unsafe(export_name = "drop")]
        trait Drop {
            fn drop(&mut self);
        }

        #[dispatch(
            T = [bool, u8],
            U = [u8, bool]
        )]
        impl<T, U> Handle<T, U> {
            #[id_pos(Self: 1)]
            #[unsafe(export_name = "handle_as_ref")]
            fn as_ref(&self) -> Result<&Self, u8>;
        }

        #[dispatch(
            Self = [Handle<bool, u8>, Handle<u8, bool>]
        )]
        trait Clone {
            #[unsafe(export_name = "abi_Clone_clone")]
            fn clone(&self) -> Self;
        }

        #[dispatch(
            Self = [Handle<bool, u8>, Handle<u8, bool>]
        )]
        trait Default {
            #[unsafe(export_name = "default")]
            fn default() -> Self;
        }

        #[dispatch(
            Self = [Handle<bool, u8>]
        )]
        trait PartialEq {
            #[id_pos(Self: 0)]
            #[unsafe(export_name = "abi_Eq_eq")]
            fn eq(&self, other: &Self) -> bool;
        }

        #[dispatch(
            Self = [Handle<bool, u8>],
            TU = [Handle<u8, bool>],
        )]
        trait PartialEq<TU> {
            #[id_pos(Self: 1)]
            #[unsafe(export_name = "abi_Eq_eq_2")]
            fn eq(&self, other: &TU) -> bool;
        }

        #[dispatch(
            Self = [Handle<bool, u8>]
        )]
        trait Custom {
            #[unsafe(export_name = "abi_Custom_inc")]
            fn inc(self) -> Self;
        }
    }
}

#[test]
#[webassembly_test]
fn opaque_handles() {
    let handle: Handle<bool, u8> = Default::default();
    let handle_ref = handle.as_ref().unwrap();
    assert!(PartialEq::eq(&*handle_ref, &handle));

    let cloned = Clone::clone(&handle);
    assert!(PartialEq::eq(&handle, &cloned));

    let other: Handle<u8, bool> = Default::default();
    let other_cloned = Clone::clone(&other);

    // Cross-type equality is exported separately and keyed by Self handle id.
    assert!(PartialEq::<Handle<u8, bool>>::eq(&handle, &other));
    assert!(PartialEq::<Handle<u8, bool>>::eq(&cloned, &other_cloned));

    let incremented = Custom::inc(handle);
    assert!(!PartialEq::<Handle<u8, bool>>::eq(&incremented, &other));

    let incremented_cloned = Clone::clone(&incremented);
    assert!(PartialEq::eq(&incremented, &incremented_cloned));
}
