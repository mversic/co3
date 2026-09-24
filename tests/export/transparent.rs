use std::marker::PhantomData;

use co3::{
    ReprC, boxed::CBoxedSlice, ffi, option::ReprCOption, rust_spec::RustSpec, slice::CSlice,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug, RustSpec, ReprC)]
#[reprC(identity)]
#[repr(transparent)]
pub struct TransparentWithoutNiche(u64);

#[derive(Clone, Copy, PartialEq, Eq, Debug, RustSpec, ReprC)]
#[reprC(identity)]
#[repr(transparent)]
pub struct GenericTransparentStruct<P>(u64, PhantomData<P>);

impl<P> GenericTransparentStruct<P> {
    fn new(value: u64) -> Self {
        Self(value, PhantomData)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, RustSpec, ReprC)]
#[reprC(identity)]
#[repr(transparent)]
pub struct TransparentStruct {
    payload: GenericTransparentStruct<()>,
    _zst1: [u8; 0],
    _zst2: (),
    _zst3: PhantomData<String>,
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

unsafe extern "C" {
    #[link_name = "export_transparent__array_of_transparent"]
    fn array_of_transparent_raw(
        arr: *const [TransparentStruct; 1],
    ) -> *const [TransparentStruct; 1];

    #[link_name = "export_transparent__transparent_option"]
    fn transparent_option_raw(
        arr: ReprCOption<TransparentWithoutNiche>,
    ) -> ReprCOption<TransparentWithoutNiche>;

    #[link_name = "export_transparent__self_to_self"]
    fn self_to_self_raw(value: TransparentStruct) -> TransparentStruct;

    #[link_name = "export_transparent__vec_to_vec"]
    fn vec_to_vec_raw(value: CBoxedSlice<TransparentStruct>) -> CBoxedSlice<TransparentStruct>;

    #[link_name = "export_transparent__slice_to_slice"]
    fn slice_to_slice_raw(value: CSlice<TransparentStruct>) -> CSlice<TransparentStruct>;

    #[link_name = "export_transparent__TransparentStruct__with_payload"]
    fn with_payload_raw(
        value: TransparentStruct,
        payload: GenericTransparentStruct<()>,
    ) -> TransparentStruct;

    #[link_name = "export_transparent__TransparentStruct__payload"]
    fn payload_raw(value: *const TransparentStruct) -> *const GenericTransparentStruct<()>;
}

#[test]
fn transparent_values_cross_exported_abi() {
    let value = TransparentStruct::new(GenericTransparentStruct::new(42));
    let array = [value];

    let output = unsafe { array_of_transparent_raw(co3::encode(&array)) };
    assert_eq!(
        &array,
        unsafe { co3::decode::<&[TransparentStruct; 1]>(output) }.unwrap()
    );

    let option = Some(TransparentWithoutNiche(42));
    let output = unsafe { transparent_option_raw(co3::encode(option)) };
    assert_eq!(
        option,
        unsafe { co3::decode::<Option<TransparentWithoutNiche>>(output) }.unwrap()
    );

    let output = unsafe { self_to_self_raw(co3::encode(value)) };
    assert_eq!(Some(value), unsafe { co3::decode(output) });
}

#[test]
fn transparent_collections_cross_exported_abi() {
    let values = vec![
        TransparentStruct::new(GenericTransparentStruct::new(2)),
        TransparentStruct::new(GenericTransparentStruct::new(3)),
        TransparentStruct::new(GenericTransparentStruct::new(4)),
    ];

    let output = unsafe { vec_to_vec_raw(co3::encode(values.clone())) };
    assert_eq!(
        values,
        unsafe { co3::decode::<Vec<TransparentStruct>>(output) }.unwrap()
    );

    let output = unsafe { slice_to_slice_raw(co3::encode(values.as_slice())) };
    assert_eq!(
        values.as_slice(),
        unsafe { co3::decode::<&[TransparentStruct]>(output) }.unwrap()
    );
}

#[test]
fn transparent_methods_cross_exported_abi() {
    let value = TransparentStruct::new(GenericTransparentStruct::new(42));
    let replacement = GenericTransparentStruct::new(24);

    let output = unsafe { with_payload_raw(co3::encode(value), co3::encode(replacement)) };
    let output = unsafe { co3::decode::<TransparentStruct>(output) }.unwrap();
    assert_eq!(replacement, output.payload);

    let payload = unsafe { payload_raw(co3::encode(&output)) };
    assert_eq!(Some(&replacement), unsafe { co3::decode(payload) });
}
