use co3::{extern_C, external::ExternRef, handle::Handle};

trait Custom {
    fn inc(self, by: Vec<u32>) -> Self;
}

co3::handles! {
    Opaque::<bool, u8> = 1,
    Opaque::<bool, u32>,
    Opaque<u8, bool>,
}

extern_C! {
    type Opaque<T, U>;

    #[dispatch]
    impl<T, U> Drop for Opaque<T, U> {
        fn drop(self_id: Self::Id, &mut self);
    }

    #[dispatch(
        Self = [
            Opaque<bool, u8>,
            Opaque<u8, bool>,
        ]
    )]
    impl<T> Clone for T {
        #[link_name = "abi_Clone_clone"]
        fn clone(self_id: Self::Id, &self) -> Self;
    }

    #[dispatch(
        Self = [
            Opaque<bool, u8>,
            Opaque<u8, bool>
        ]
    )]
    impl<T> Default for T {
        #[link_name = "default"]
        fn default(self_id: Self::Id) -> Self;
    }

    #[dispatch(
        T = [
            Opaque<bool, u8>,
            Opaque<u8, bool>
        ],
    )]
    impl<T> PartialEq for T {
        #[link_name = "abi_Eq_eq"]
        fn eq(self_id: Self::Id, &self, other: &Self) -> bool;
    }

    #[dispatch(
        T = [Opaque<bool, u8>],
        U = [Opaque<u8, bool>]
    )]
    impl<T, U> PartialEq<U> for T {
        #[link_name = "abi_Eq_eq_2"]
        fn eq(&self, self_id: Self::Id, other: &U) -> bool;
    }

    #[dispatch(
        Self = [Opaque<bool, u8>]
    )]
    impl<T> Custom for T {
        #[link_name = "custom_inc_as_ref"]
        fn inc(self_id: Self::Id, self, by: Vec<u32>) -> Self;
    }

    #[dispatch(
        Self = [Opaque<u8, bool>]
    )]
    impl<T> Custom for T {
        #[link_name = "custom_inc_move"]
        fn inc(self_id: Self::Id, self, move by: Vec<u32>) -> Self;
    }
}

mod provider {
    use core::marker::PhantomData;

    use co3::{ReprC, export_C, handles};

    use super::Custom;

    handles! {
        Opaque::<bool, u8> = 1,
        Opaque<bool, u32>,
        Opaque<u8, bool>,
    }

    #[derive(Debug, Default, Clone, PartialEq, Eq, ReprC)]
    #[reprC(opaque)]
    pub struct Opaque<T, U> {
        id: u8,
        _marker: PhantomData<(T, U)>,
    }

    impl PartialEq<Opaque<u8, bool>> for Opaque<bool, u8> {
        fn eq(&self, other: &Opaque<u8, bool>) -> bool {
            self.id == other.id
        }
    }

    impl<T, U> Custom for Opaque<T, U> {
        fn inc(mut self, by: Vec<u32>) -> Self {
            by.into_iter().for_each(|by| self.id += by as u8);
            self
        }
    }

    impl<T, U> Opaque<T, U> {
        fn as_ref(&self) -> Result<&Self, u8> {
            Ok(self)
        }
    }

    export_C! {
        #[dispatch(
            Self = [
                Opaque<bool, u8>,
                Opaque<u8, bool>,
            ]
        )]
        #[unsafe(export_name = "drop")]
        trait Drop {
            fn drop(self_id: Self::Id, &mut self) {
                // FIXME: This is quite incorrect I think?
                let _ = self_id;
            }
        }

        #[dispatch(
            Self = [
                Opaque<bool, u8>,
                Opaque<u8, bool>,
            ],
        )]
        trait Clone {
            #[unsafe(export_name = "abi_Clone_clone")]
            fn clone(self_id: Self::Id, &self) -> Self {
                self::<self_id>.clone()
            }
        }

        #[dispatch(
            Self = [
                Opaque<bool, u8>,
                Opaque<u8, bool>,
            ]
        )]
        trait Default {
            #[unsafe(export_name = "default")]
            fn default(self_id: Self::Id) -> Self {
                <Self::<self_id> as Default>::default()
            }
        }

        #[dispatch(
            Self = [Opaque<bool, u8>]
        )]
        trait PartialEq {
            #[unsafe(export_name = "abi_Eq_eq")]
            fn eq(self_id: Self::Id, &self, other: &Self) -> bool {
                self::<self_id>.eq(other::<self_id>)
            }
        }

        #[dispatch(
            Self = [Opaque<bool, u8>],
            TU = [Opaque<u8, bool>],
        )]
        trait PartialEq<TU> {
            #[unsafe(export_name = "abi_Eq_eq_2")]
            fn eq(&self, self_id: Self::Id, other_id: Self::Id, other: &TU) -> bool {
                self::<self_id>.eq(other::<other_id>)
            }
        }

        #[dispatch(
            Self = [Opaque<bool, u8>]
        )]
        trait Custom {
            #[unsafe(export_name = "custom_inc_as_ref")]
            fn inc(self_id: Self::Id, self, by: Vec<u32>) -> Self {
                self::<self_id>.inc(by)
            }
        }

        #[dispatch(
            Self = [Opaque<u8, bool>]
        )]
        trait Custom {
            #[unsafe(export_name = "custom_inc_move")]
            fn inc(self_id: Self::Id, self, move by: Vec<u32>) -> Self {
                self::<self_id>.inc(by)
            }
        }
    }
}

#[test]
fn opaque_handles() {
    let handle: Opaque<bool, u8> = Default::default();
    let handle_ref: ExternRef<_> = handle.as_ref().unwrap();
    assert!(PartialEq::eq(&*handle_ref, &handle));

    let cloned = Clone::clone(&handle);
    assert!(PartialEq::eq(&handle, &cloned));

    let other: Opaque<u8, bool> = Default::default();
    let other_cloned = Clone::clone(&other);

    // Cross-type equality is exported separately and keyed by Self handle id.
    assert!(PartialEq::<Opaque<u8, bool>>::eq(&handle, &other));
    assert!(PartialEq::<Opaque<u8, bool>>::eq(&cloned, &other_cloned));

    let incremented = Custom::inc(handle, vec![2]);
    assert!(!PartialEq::<Opaque<u8, bool>>::eq(&incremented, &other));

    let incremented_cloned = Clone::clone(&incremented);
    assert!(PartialEq::eq(&incremented, &incremented_cloned));

    let owned_handle: Opaque<u8, bool> = Default::default();
    let owned_incremented = Custom::inc(owned_handle, vec![2]);
    let owned_incremented_cloned = Clone::clone(&owned_incremented);

    assert!(PartialEq::eq(&owned_incremented, &owned_incremented_cloned));
}
