// Unlike static_assertions' generic helper, this helper carries the
// reference-mapping prerequisite of SoftEncodeRef and SoftEncodeRefUnchecked.
use co3::{CType, marker::Native};

trait TransparentCarrier: Copy {
    type Payload: Copy;

    fn payload(&self) -> Self::Payload {
        // These carriers are repr(transparent), and both sides are Copy.
        unsafe { core::mem::transmute_copy(self) }
    }
}

impl<T: CType + Copy> TransparentCarrier for co3::option::abi::OptionSome<T> {
    type Payload = T;
}
impl<T: CType + Copy, E: CType> TransparentCarrier for co3::result::abi::ResultOk<T, E> {
    type Payload = T;
}
impl<T: CType, E: CType + Copy> TransparentCarrier for co3::result::abi::ResultErr<T, E> {
    type Payload = E;
}

impl<T: CType + TransparentCarrier> TransparentCarrier for Native<T> {
    type Payload = T::Payload;
}

macro_rules! assert_not_shared_encoder {
    ($ty:ty: $($encoder:path),+ $(,)?) => {
        const _: fn() = || {
            use co3::ReprC;
            $(
                {
                    trait AmbiguousIfImpl<A> { fn check() {} }
                    impl<T: ?Sized> AmbiguousIfImpl<()> for T {}
                    impl<T: ?Sized + $encoder> AmbiguousIfImpl<u8> for T
                    where
                        &'static T: ReprC<CType: Sized>,
                    {}
                    let _ = <$ty as AmbiguousIfImpl<_>>::check;
                }
            )+
        };
    };
}

mod alignment;
mod boxed_mutability;
mod cfg_args;
mod clippy;
mod cstr;
mod ctype_mapping;
mod custom_repr;
mod cvec;
mod data_enum_explicit;
mod direct_interior_encode;
mod exhaustive_enum;
mod handles;
mod identity_primitives;
mod interior_decode;
mod mutable_ref_encode;
mod niche_payload;
mod niche_value;
mod raw_pointer;
mod raw_selected;
mod raw_tagged_dispatch;
mod raw_wrapper_abi;
mod recursive;
mod reference_decode;
mod reference_ownership;
mod regular_drop;
mod shared_validate;
mod statics;
mod string_ref_decode;
mod vec_mutability;
mod wide;
mod wide_encode_store;
mod wide_metadata;
mod zst;

mod ownership_call;
