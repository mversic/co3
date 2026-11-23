use std::marker::PhantomData;

use co3::{ReprC, ffi, rust_spec::RustSpec};

#[derive(Clone, Copy, PartialEq, Eq, Debug, RustSpec, ReprC)]
#[repr_c(identity)]
#[repr(transparent)]
pub struct TransparentWithoutNiche(u64);

#[derive(Clone, Copy, PartialEq, Eq, Debug, RustSpec, ReprC)]
#[repr_c(identity)]
#[repr(transparent)]
pub struct GenericTransparentStruct<P>(u64, PhantomData<P>);

impl<P> GenericTransparentStruct<P> {
    fn new(value: u64) -> Self {
        Self(value, PhantomData)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, RustSpec, ReprC)]
#[repr_c(identity)]
#[repr(transparent)]
pub struct TransparentStruct {
    payload: GenericTransparentStruct<()>,
    _zst1: [u8; 0],
    _zst2: (),
    _zst3: PhantomData<u8>,
}

ffi! {
    #![unsafe(export("C"))]
    #![symbol_prefix = "export_transparent"]

    fn array_of_transparent(arr: &[TransparentStruct; 1]) -> &[TransparentStruct; 1];
    fn transparent_option(arr: Option<TransparentWithoutNiche>) -> Option<TransparentWithoutNiche>;

    impl TransparentStruct {
        fn with_payload(self, payload: GenericTransparentStruct<()>) -> Self;
        fn payload(&self) -> &GenericTransparentStruct<()>;
    }

    fn self_to_self(value: TransparentStruct) -> TransparentStruct;
    fn vec_to_vec(value: move Vec<TransparentStruct>) -> move Vec<TransparentStruct>;
    fn slice_to_slice(value: &[TransparentStruct]) -> &[TransparentStruct];
}

pub fn array_of_transparent(arr: &[TransparentStruct; 1]) -> &[TransparentStruct; 1] {
    arr
}

pub fn transparent_option(arr: Option<TransparentWithoutNiche>) -> Option<TransparentWithoutNiche> {
    arr
}

impl TransparentStruct {
    fn new(payload: GenericTransparentStruct<()>) -> Self {
        Self {
            payload,
            _zst1: [],
            _zst2: (),
            _zst3: PhantomData,
        }
    }

    #[must_use]
    pub fn with_payload(mut self, payload: GenericTransparentStruct<()>) -> Self {
        self.payload = payload;
        self
    }

    pub fn payload(&self) -> &GenericTransparentStruct<()> {
        &self.payload
    }
}

pub fn self_to_self(value: TransparentStruct) -> TransparentStruct {
    value
}

pub fn vec_to_vec(value: Vec<TransparentStruct>) -> Vec<TransparentStruct> {
    value
}

pub fn slice_to_slice(value: &[TransparentStruct]) -> &[TransparentStruct] {
    value
}

mod imported {
    use super::*;

    ffi! {
        #![unsafe(extern("C"))]
        #![symbol_prefix = "export_transparent"]

        pub fn array_of_transparent(arr: &[TransparentStruct; 1]) -> &[TransparentStruct; 1];
        pub fn transparent_option(arr: Option<TransparentWithoutNiche>) -> Option<TransparentWithoutNiche>;
        pub fn self_to_self(value: TransparentStruct) -> TransparentStruct;
        pub fn vec_to_vec(value: move Vec<TransparentStruct>) -> move Vec<TransparentStruct>;
        pub fn slice_to_slice(value: &[TransparentStruct]) -> &[TransparentStruct];

        #[symbol_name = "export_transparent__TransparentStruct__with_payload"]
        pub fn with_payload(value: TransparentStruct, payload: GenericTransparentStruct<()>) -> TransparentStruct;
        #[symbol_name = "export_transparent__TransparentStruct__payload"]
        pub fn payload(value: &TransparentStruct) -> &GenericTransparentStruct<()>;
    }
}

#[test]
fn transparent_values_cross_exported_abi() {
    let value = TransparentStruct::new(GenericTransparentStruct::new(42));
    let array = [value];

    assert_eq!(&array, imported::array_of_transparent(&array));

    let option = Some(TransparentWithoutNiche(42));
    assert_eq!(option, imported::transparent_option(option));
    assert_eq!(value, imported::self_to_self(value));
}

#[test]
fn stable_mutable_encoder_uses_the_original_pointer() {
    static_assertions::assert_impl_all!(TransparentStruct:
        co3::convert::SoftEncodeMut<'static, Store = ()>, co3::convert::SoftEncodeMutUnchecked<'static, SoftStore = ()>);
    let mut value = TransparentStruct::new(GenericTransparentStruct::new(42));
    let original = core::ptr::from_mut(&mut value);
    let encoded = co3::soft_encode(&mut value, &mut Default::default());
    assert_eq!(encoded.as_ptr(), original);
}

#[test]
fn transparent_collections_cross_exported_abi() {
    let values = vec![
        TransparentStruct::new(GenericTransparentStruct::new(2)),
        TransparentStruct::new(GenericTransparentStruct::new(3)),
        TransparentStruct::new(GenericTransparentStruct::new(4)),
    ];

    assert_eq!(values, imported::vec_to_vec(values.clone()));
    assert_eq!(values.as_slice(), imported::slice_to_slice(&values));
}

#[test]
fn transparent_methods_cross_exported_abi() {
    let value = TransparentStruct::new(GenericTransparentStruct::new(42));
    let replacement = GenericTransparentStruct::new(24);

    let output = imported::with_payload(value, replacement);
    assert_eq!(replacement, output.payload);
    assert_eq!(&replacement, imported::payload(&output));
}
