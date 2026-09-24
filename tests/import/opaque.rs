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

    impl Clone for OwnedValue {
        #[symbol_name = "import_opaque_value_clone"]
        fn clone(&self) -> move Self;
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

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Value(String);

    #[derive(Debug, PartialEq, Eq)]
    pub struct OpaqueStruct {
        name: u8,
    }

    impl Drop for Value {
        fn drop(&mut self) {}
    }

    impl Drop for OpaqueStruct {
        fn drop(&mut self) {}
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
            fn new(input: move String) -> move Box<Self>;
            fn len(&self) -> usize;
        }

        impl Clone for Box<Value> {
            #[symbol_name = "import_opaque_value_clone"]
            fn clone(&self) -> move Self;
        }

        impl OpaqueStruct {
            fn new(name: u8) -> move Box<Self>;
            fn name(&self) -> u8;
            fn identity(&self) -> &Self;
            fn value_len(&self, value: &Value) -> usize;
        }
    }

    impl Value {
        fn new(input: String) -> Box<Self> {
            Box::new(Self(input))
        }

        fn len(&self) -> usize {
            self.0.len()
        }
    }

    impl OpaqueStruct {
        fn new(name: u8) -> Box<Self> {
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
