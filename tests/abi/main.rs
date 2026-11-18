// Unlike static_assertions' generic helper, this helper carries the
// reference-mapping prerequisite of SoftEncodeRef and SoftEncodeRefUnchecked.
use co3::{CType, ReprC};

trait TransparentCarrier: Copy {
    type Payload: Copy;

    fn payload(&self) -> Self::Payload {
        // These carriers are repr(transparent), and both sides are Copy.
        unsafe { core::mem::transmute_copy(self) }
    }
}

impl<R: ReprC<CType: Copy + CType + Sized>> TransparentCarrier for co3::option::COptionSome<R> {
    type Payload = R::CType;
}

impl<Ok: ReprC<CType: Copy + CType + Sized>, Err: ReprC> TransparentCarrier
    for co3::result::CResultOk<Ok, Err>
{
    type Payload = Ok::CType;
}

impl<Ok: ReprC, Err: ReprC<CType: Copy + CType + Sized>> TransparentCarrier
    for co3::result::CResultErr<Ok, Err>
{
    type Payload = Err::CType;
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
mod option_decode_store;
mod raw_pointer;
mod raw_selected;
mod raw_tagged_dispatch;
mod raw_wrapper_abi;
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
