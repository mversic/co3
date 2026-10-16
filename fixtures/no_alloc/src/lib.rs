#![no_std]

co3::ffi! {
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
        fn new(input: &str) -> move OwnedValue;
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
