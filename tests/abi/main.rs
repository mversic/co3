// Unlike static_assertions' generic helper, this helper carries the
// reference-mapping prerequisite of EncodeRef and EncodeRefUnchecked.
trait TransparentCarrier: Copy {
    type Payload: Copy;

    fn payload(&self) -> Self::Payload {
        // These carriers are repr(transparent), and both sides are Copy.
        unsafe { core::mem::transmute_copy(self) }
    }
}

impl<R: co3::ReprC<CType: Copy + co3::CType + Sized>> TransparentCarrier
    for co3::option::COptionSome<R>
{
    type Payload = R::CType;
}

impl<Ok: co3::ReprC<CType: Copy + co3::CType + Sized>, Err: co3::ReprC> TransparentCarrier
    for co3::result::CResultOk<Ok, Err>
{
    type Payload = Ok::CType;
}

impl<Ok: co3::ReprC, Err: co3::ReprC<CType: Copy + co3::CType + Sized>> TransparentCarrier
    for co3::result::CResultErr<Ok, Err>
{
    type Payload = Err::CType;
}

macro_rules! assert_not_shared_encoder {
    ($ty:ty: $($encoder:path),+ $(,)?) => {
        const _: fn() = || {
            $(
                {
                    trait AmbiguousIfImpl<A> { fn check() {} }
                    impl<T: ?Sized> AmbiguousIfImpl<()> for T {}
                    impl<T: ?Sized + $encoder> AmbiguousIfImpl<u8> for T
                    where
                        &'static T: co3::ReprC<CType: Sized>,
                    {}
                    let _ = <$ty as AmbiguousIfImpl<_>>::check;
                }
            )+
        };
    };
}

macro_rules! assert_shared_encoder {
    ($ty:ty: $($encoder:path),+ $(,)?) => {
        const _: fn() = || {
            fn check<T: ?Sized $(+ $encoder)+>()
            where
                &'static T: co3::ReprC<CType: Sized>,
            {}
            check::<$ty>();
        };
    };
}

mod alignment;
mod borrowed;
mod borrowed_decode;
mod boxed_mutability;
mod callback;
mod cfg_args;
mod clippy;
mod cloned_encode;
mod cstr;
mod ctype_mapping;
mod cvec;
mod custom_repr;
mod data_enum_explicit;
mod direct_interior_encode;
mod exhaustive_enum;
mod field_order;
mod handles;
mod identity_primitives;
mod interior_decode;
mod maybe_uninit_borrow;
mod mutable_ref_encode;
mod niche_value;
mod option_decode_store;
mod raw_selected;
mod raw_tagged_dispatch;
mod raw_wrapper_abi;
mod regular_drop;
mod reference_decode;
mod shared_ref_encode;
mod shared_validate;
mod statics;
mod status_return;
mod stored_encode;
mod unsized_ref_encode;
mod vec_mutability;
mod wide;
mod wide_encode_store;
mod wide_metadata;
mod zst;
