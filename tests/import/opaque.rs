use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]
    #![symbol_prefix = "import_opaque"]

    pub type Value;
    pub type OpaqueStruct;

    impl Drop for Value {
        fn drop(&mut self);
    }

    impl Drop for OpaqueStruct {
        fn drop(&mut self);
    }

    impl Value {
        fn new(input: move String) -> move OwnedValue;
        fn len(&self) -> usize;
    }

    impl ToOwned for Value {
        type Owned = OwnedValue;
        #[symbol_name = "import_opaque_value_clone"]
        fn to_owned(&self) -> move <Self as ToOwned>::Owned;
    }

    impl OpaqueStruct {
        fn new(name: u8) -> move OwnedOpaqueStruct;
        fn name(&self) -> u8;
        fn identity(&self) -> &Self;
        fn value_len(&self, value: &Value) -> usize;
    }
}

mod provider {
    use co3::ffi;
    use co3::rust_spec::RustSpec;

    #[derive(Debug, PartialEq, Eq, RustSpec)]
    #[rust_spec(opaque)]
    pub struct Value(String);

    #[derive(Debug, PartialEq, Eq, RustSpec)]
    #[rust_spec(opaque)]
    pub struct OpaqueStruct {
        name: u8,
    }

    impl Drop for Value {
        fn drop(&mut self) {}
    }

    impl Drop for OpaqueStruct {
        fn drop(&mut self) {}
    }

    impl ToOwned for Value {
        type Owned = OwnedValue;

        fn to_owned(&self) -> Self::Owned {
            Box::new(Self(self.0.clone()))
        }
    }

    ffi! {
        #![unsafe(export("C"))]
        #![symbol_prefix = "import_opaque"]

        type Value;
        type OpaqueStruct;

        impl Drop for Value {
            fn drop(&mut self);
        }

        impl Drop for OpaqueStruct {
            fn drop(&mut self);
        }

        impl Value {
            fn new(input: move String) -> move OwnedValue;
            fn len(&self) -> usize;
        }

        impl ToOwned for Value {
            type Owned = OwnedValue;

            #[symbol_name = "import_opaque_value_clone"]
            fn to_owned(&self) -> move <Self as ToOwned>::Owned;
        }

        impl OpaqueStruct {
            fn new(name: u8) -> move OwnedOpaqueStruct;
            fn name(&self) -> u8;
            fn identity(&self) -> &Self;
            fn value_len(&self, value: &Value) -> usize;
        }
    }

    impl Value {
        fn new(input: String) -> OwnedValue {
            Box::new(Self(input))
        }

        fn len(&self) -> usize {
            self.0.len()
        }
    }

    impl OpaqueStruct {
        fn new(name: u8) -> OwnedOpaqueStruct {
            Box::new(Self { name })
        }

        fn name(&self) -> u8 {
            self.name
        }

        fn identity(&self) -> &Self {
            self
        }

        fn value_len(&self, value: &Value) -> usize {
            value.0.len()
        }
    }
}

#[test]
fn constructs_and_uses_imported_opaque_handles() {
    let value = Value::new(String::from("opaque value"));
    let cloned = value.clone();
    assert_eq!(12, value.len());
    assert_eq!(12, cloned.len());

    let opaque = OpaqueStruct::new(42);
    assert_eq!(42, opaque.name());
    assert_eq!(12, opaque.value_len(&value));
}

#[test]
fn imported_opaque_borrow_preserves_identity() {
    let opaque = OpaqueStruct::new(7);
    let identity = opaque.identity();

    assert!(core::ptr::eq::<OpaqueStruct>(&*opaque, identity));
}

#[test]
fn imported_opaque_drop_sends_pointer() {
    drop(OpaqueStruct::new(7));
}

#[test]
fn owned_handle_has_null_niche() {
    use co3::{ReprC, niche::Niche};
    use core::mem::{align_of, size_of};

    type Carrier = <OwnedOpaqueStruct as ReprC>::CType;
    assert_eq!(size_of::<Option<OwnedOpaqueStruct>>(), size_of::<Carrier>());
    assert_eq!(
        align_of::<Option<OwnedOpaqueStruct>>(),
        align_of::<Carrier>()
    );

    let null = <OwnedOpaqueStruct as Niche>::NICHE;
    assert!(
        unsafe { co3::soft_decode::<OwnedOpaqueStruct>(null, &mut Default::default()) }.is_none()
    );
    assert!(matches!(
        unsafe { co3::soft_decode::<Option<OwnedOpaqueStruct>>(null, &mut None) },
        Some(None)
    ));

    let raw = co3::soft_encode(Some(OpaqueStruct::new(42)), &mut None);
    let decoded = unsafe { co3::soft_decode::<Option<OwnedOpaqueStruct>>(raw, &mut None) }
        .expect("valid carrier")
        .expect("non-null handle");
    assert_eq!(decoded.name(), 42);
}
