#![cfg(feature = "derive")]
use std::{alloc, marker::PhantomData, mem::MaybeUninit, num::NonZeroU64};

use co3::{
    Decode, Encode, ExternC, FfiReturn, CTuple2,
    out_ptr::OutPtrRead,
    slice::{OutBoxedSlice, CSlice},
};
use webassembly_test::webassembly_test;

co3::def_fns! { dealloc }

#[derive(Clone, Copy, PartialEq, Eq, Debug, ExternC)]
#[repr(transparent)]
pub struct TransparentWithoutNiche(u64);

#[derive(Clone, Copy, PartialEq, Eq, Debug, ExternC)]
#[repr(transparent)]
pub struct GenericTransparentStruct<P>(NonZeroU64, PhantomData<P>);

impl<P> GenericTransparentStruct<P> {
    fn new(value: u64) -> Self {
        Self(NonZeroU64::new(value).unwrap(), PhantomData)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, ExternC)]
#[mineral(
    unsafe(is_valid = |target: &Self::Target|
        *target != GenericTransparentStruct::new(1)
    )
)]
#[repr(transparent)]
pub struct TransparentStruct {
    payload: GenericTransparentStruct<()>,
    _zst1: [u8; 0],
    _zst2: (),
    _zst3: PhantomData<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, ExternC)]
#[mineral(
    NICHE_VALUE = [0; 4],
    unsafe(is_valid = |target: &Self::Target|
        target.iter().all(|&x| x != 0)
    )
)]
#[repr(transparent)]
pub struct RobustTargetTransparent([u8; 4]);

#[co3::carbonate]
pub fn array_of_transparent(arr: &mut [TransparentStruct; 1]) -> &mut [TransparentStruct; 1] {
    arr
}

#[co3::carbonate]
pub fn transparent_with_niche(
    arr: Option<RobustTargetTransparent>,
) -> Option<RobustTargetTransparent> {
    arr
}

#[co3::carbonate]
pub fn transparent_without_niche(
    arr: Option<TransparentWithoutNiche>,
) -> Option<TransparentWithoutNiche> {
    arr
}

#[co3::carbonate]
pub fn transparent_with_inner_niche(
    arr: Option<GenericTransparentStruct<u32>>,
) -> Option<GenericTransparentStruct<u32>> {
    arr
}

#[co3::carbonate]
impl TransparentStruct {
    pub fn new(payload: GenericTransparentStruct<()>) -> Self {
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

    pub fn payload_mut(&mut self) -> &mut GenericTransparentStruct<()> {
        &mut self.payload
    }
}

#[co3::carbonate]
pub fn self_to_self(value: TransparentStruct) -> TransparentStruct {
    value
}

#[co3::carbonate]
pub fn vec_to_vec(value: Vec<TransparentStruct>) -> Vec<TransparentStruct> {
    value
}

#[co3::carbonate]
pub fn slice_to_slice(value: &[TransparentStruct]) -> &[TransparentStruct] {
    value
}

#[test]
#[webassembly_test]
fn take_and_return_transparent_array_ref() {
    let value = TransparentStruct::new(GenericTransparentStruct::new(42));

    let mut array = [value; 1];
    let ptr: *mut [u64; 1] = (&mut array).encode(&mut ());
    let mut output = MaybeUninit::new(core::ptr::null_mut());

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            __array_of_transparent(ptr, output.as_mut_ptr())
        );

        assert_eq!(
            &[value; 1],
            <&[TransparentStruct; 1]>::decode(output.assume_init(), &mut ()).unwrap()
        );
    }
}

#[test]
#[webassembly_test]
fn take_and_return_option_of_transparent_with_niche() {
    let value = Some(RobustTargetTransparent([1; 4]));
    let mut output = MaybeUninit::new([0u8; 4]);

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            __transparent_with_niche(value.encode(&mut ()), output.as_mut_ptr())
        );

        assert_eq!(
            value,
            Decode::decode(output.assume_init(), &mut ()).unwrap()
        );
    }
}

#[test]
#[webassembly_test]
fn take_and_return_option_of_transparent_without_niche() {
    let value = Some(TransparentWithoutNiche(42));
    let mut output: MaybeUninit<CTuple2<u8, u64>> = MaybeUninit::new(CTuple2(1, 0));

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            __transparent_without_niche(value.encode(&mut ()), output.as_mut_ptr())
        );

        assert_eq!(
            value,
            Decode::decode(output.assume_init(), &mut ()).unwrap()
        );
    }
}

#[test]
#[webassembly_test]
fn take_and_return_option_of_transparent_with_inner_niche() {
    let value = Some(GenericTransparentStruct::<()>::new(42));
    let mut output: MaybeUninit<u64> = MaybeUninit::new(0);

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            __transparent_with_inner_niche(value.encode(&mut ()), output.as_mut_ptr())
        );

        assert_eq!(
            value,
            Decode::decode(output.assume_init(), &mut ()).unwrap()
        );
    }
}

#[test]
#[webassembly_test]
fn transparent_self_to_self() {
    let transparent_struct = TransparentStruct::new(GenericTransparentStruct::new(42));
    let mut output: MaybeUninit<u64> = MaybeUninit::new(0);

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            __self_to_self(transparent_struct.encode(&mut ()), output.as_mut_ptr())
        );
        assert_eq!(
            Ok(transparent_struct),
            TransparentStruct::decode(output.assume_init(), &mut ())
        );
    }
}

#[test]
#[webassembly_test]
fn transparent_vec_to_vec() {
    let transparent_struct_vec = vec![
        TransparentStruct::new(GenericTransparentStruct::new(1)),
        TransparentStruct::new(GenericTransparentStruct::new(2)),
        TransparentStruct::new(GenericTransparentStruct::new(3)),
    ];

    let mut store = Default::default();
    let mut output = MaybeUninit::new(OutBoxedSlice::from_raw_parts(core::ptr::null_mut(), 0));

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            __vec_to_vec(
                transparent_struct_vec.clone().encode(&mut store),
                output.as_mut_ptr()
            )
        );

        let output = output.assume_init();
        assert_eq!(output.len(), 3);
        let vec = Vec::<TransparentStruct>::try_read_out(output).expect("Valid");
        assert_eq!(transparent_struct_vec, vec);
    }
}

#[test]
#[webassembly_test]
// False positive
fn transparent_slice_to_slice() {
    let transparent_struct_slice = [
        TransparentStruct::new(GenericTransparentStruct::new(1)),
        TransparentStruct::new(GenericTransparentStruct::new(2)),
        TransparentStruct::new(GenericTransparentStruct::new(3)),
    ];
    let mut output = MaybeUninit::new(CSlice::from_raw_parts(core::ptr::null(), 0));

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            __slice_to_slice(
                transparent_struct_slice.as_slice().encode(&mut ()),
                output.as_mut_ptr()
            )
        );

        let output: &[TransparentStruct] =
            OutPtrRead::try_read_out(output.assume_init()).expect("Invalid output");
        assert_eq!(output, transparent_struct_slice);
    }
}

#[test]
#[webassembly_test]
fn transparent_method_consume() {
    let mut transparent_struct = TransparentStruct::new(GenericTransparentStruct::new(42));
    let payload = GenericTransparentStruct::new(24);

    let mut output: MaybeUninit<u64> = MaybeUninit::new(0);

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            TransparentStruct__with_payload(
                transparent_struct.encode(&mut ()),
                payload.encode(&mut ()),
                output.as_mut_ptr()
            )
        );
        transparent_struct =
            TransparentStruct::decode(output.assume_init(), &mut ()).expect("valid");

        assert_eq!(transparent_struct.payload, payload);
    }
}

#[test]
#[webassembly_test]
fn transparent_method_borrow() {
    let transparent_struct = TransparentStruct::new(GenericTransparentStruct::new(42));
    let mut output = MaybeUninit::new(core::ptr::null());

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            TransparentStruct__payload((&transparent_struct).encode(&mut ()), output.as_mut_ptr())
        );
        assert_eq!(
            Ok(&transparent_struct.payload),
            <&GenericTransparentStruct<_>>::decode(output.assume_init(), &mut ())
        );
    }
}

#[test]
fn transparent_method_borrow_mut() {
    let mut transparent_struct = TransparentStruct::new(GenericTransparentStruct::new(42));
    let mut output = MaybeUninit::new(core::ptr::null_mut());

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            TransparentStruct__payload_mut(
                (&mut transparent_struct).encode(&mut ()),
                output.as_mut_ptr()
            )
        );
        assert_eq!(
            Ok(&mut transparent_struct.payload),
            <&mut GenericTransparentStruct<_>>::decode(output.assume_init(), &mut ())
        );
    }
}
