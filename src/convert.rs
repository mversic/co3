//! Conversion functions and traits for C-compatible representations.
#[cfg(feature = "alloc")]
use alloc::boxed::Box;

#[cfg(feature = "alloc")]
use rust_spec::{RustSpec, Specified, Unspecified};

#[cfg(feature = "repr-rust")]
pub use crate::decoded::{
    SoftDecodeMut, SoftDecodeMutUnchecked, SoftDecodeRef, SoftDecodeRefUnchecked,
};
use crate::lang::{self, Language};
#[cfg(feature = "alloc")]
pub use crate::stored::{AssignFromOwned, Owned};
#[cfg(feature = "repr-rust")]
pub(crate) use crate::stored::{encode_ref_field, encode_ref_field_unchecked};
pub use crate::{
    // FIXME: This trait doesn't belong here
    abi::AbiValidate,
    mutable::{SoftEncodeMut, SoftEncodeMutUnchecked},
    shared::{SoftEncodeRef, SoftEncodeRefUnchecked},
    stored::{
        DecodeMove, DecodeMoveUnchecked, EmptyStore, EncodeMove, EncodeMoveUnchecked, Store,
        decode_move, decode_move_unchecked, encode_move, encode_move_unchecked,
    },
};

/// Facilitates unchecked conversion from a Rust type into a C-compatible representation.
pub trait EncodeUnchecked: EncodeMoveUnchecked {}

/// Facilitates conversion from a Rust type into a corresponding C-compatible representation.
pub trait Encode<L: Language = lang::C>: EncodeMove<L> + EncodeUnchecked {}

/// Facilitates unchecked conversion into a Rust type.
pub trait DecodeUnchecked<'d>: DecodeMoveUnchecked<'d> {}

/// Facilitates conversion into a Rust type from a corresponding C-compatible representation.
pub trait Decode<'d, L: Language = lang::C>: DecodeMove<'d, L> {}

#[cfg(feature = "alloc")]
const _: () = {
    // An unstable box representation can still transfer its allocation when the
    // pointee layout is stable (for example, a boxed slice).
    trait BoxConversionLayout<R: ?Sized> {}
    impl<R: ?Sized> BoxConversionLayout<R> for Specified {}
    impl<R: RustSpec<Layout = Specified> + ?Sized> BoxConversionLayout<R> for Unspecified {}

    impl<R: ?Sized> EncodeUnchecked for Box<R> where
        Self: RustSpec<Layout: BoxConversionLayout<R>> + EncodeMoveUnchecked
    {
    }
    impl<L: Language, R: ?Sized> Encode<L> for Box<R> where
        Self: RustSpec<Layout: BoxConversionLayout<R>> + EncodeMove<L>
    {
    }
    impl<'d, R: ?Sized> DecodeUnchecked<'d> for Box<R> where
        Self: RustSpec<Layout: BoxConversionLayout<R>> + DecodeMoveUnchecked<'d>
    {
    }
    impl<'d, L: Language, R: ?Sized> Decode<'d, L> for Box<R> where
        Self: RustSpec<Layout: BoxConversionLayout<R>> + DecodeMove<'d, L>
    {
    }
};

impl<R: ?Sized> EncodeUnchecked for &R where Self: EncodeMoveUnchecked {}
impl<L: Language, R: ?Sized> Encode<L> for &R where Self: EncodeMove<L> {}
impl<'d, R: ?Sized> DecodeUnchecked<'d> for &'d R where Self: DecodeMoveUnchecked<'d> {}
impl<'d, L: Language, R: ?Sized> Decode<'d, L> for &'d R where Self: DecodeMove<'d, L> {}

impl<R: ?Sized> EncodeUnchecked for &mut R where Self: EncodeMoveUnchecked {}
impl<L: Language, R: ?Sized> Encode<L> for &mut R where Self: EncodeMove<L> {}
impl<'d, R: ?Sized> DecodeUnchecked<'d> for &'d mut R where Self: DecodeMoveUnchecked<'d> {}
impl<'d, L: Language, R: ?Sized> Decode<'d, L> for &'d mut R where Self: DecodeMove<'d, L> {}

impl<R: EncodeUnchecked, const N: usize> EncodeUnchecked for [R; N] where Self: EncodeMoveUnchecked {}
impl<L: Language, R: Encode<L>, const N: usize> Encode<L> for [R; N] where Self: EncodeMove<L> {}
impl<'d, R: DecodeUnchecked<'d>, const N: usize> DecodeUnchecked<'d> for [R; N] where
    Self: DecodeMoveUnchecked<'d>
{
}
impl<'d, L: Language, R: Decode<'d, L>, const N: usize> Decode<'d, L> for [R; N] where
    Self: DecodeMove<'d, L>
{
}

#[cfg(test)]
mod tests {
    use super::{
        Decode, DecodeMove, DecodeMoveUnchecked, DecodeUnchecked, Encode, EncodeMove,
        EncodeMoveUnchecked, EncodeUnchecked, Language, lang,
    };
    use crate::ReprC;

    #[derive(Debug, PartialEq, Eq)]
    struct Flag(u8);

    impl ReprC for Flag {
        type CType = u8;
    }

    // One implementation serves every language, with a shared representation.
    unsafe impl EncodeMoveUnchecked for Flag {
        type SoftStore = ();

        unsafe fn soft_encode_unchecked<'itm>(self, _: &'itm mut ()) -> u8
        where
            Self: 'itm,
        {
            self.0
        }
    }

    unsafe impl<L: Language> EncodeMove<L> for Flag {
        type Store = ();

        fn soft_encode<'itm>(self, _: &'itm mut ()) -> u8
        where
            Self: 'itm,
        {
            self.0
        }
    }

    unsafe impl<'d> DecodeMoveUnchecked<'d> for Flag {
        type SoftStore = ();
        unsafe fn soft_decode_unchecked<'itm: 'd>(source: u8, _: &'itm mut ()) -> Self {
            Self(source)
        }
    }
    unsafe impl<'d, L: Language> DecodeMove<'d, L> for Flag {
        type Store = ();

        unsafe fn soft_decode<'itm: 'd>(source: u8, _: &'itm mut ()) -> Option<Self> {
            if source > 1 { None } else { Some(Self(source)) }
        }
    }

    impl EncodeUnchecked for Flag {}
    impl<L: Language> Encode<L> for Flag {}
    impl<'d> DecodeUnchecked<'d> for Flag {}
    impl<'d, L: Language> Decode<'d, L> for Flag {}

    #[test]
    fn custom_languages_override_individual_checks() {
        struct Custom;
        impl Language for Custom {
            fn validate_bool(_: u8) -> bool {
                true
            }
        }
        assert!(lang::checks::boolean::<Custom>(2));
        assert!(!lang::checks::unicode_scalar::<Custom>(0xd800));
        assert!(!lang::checks::utf8::<Custom>(&[255]));
        assert!(!lang::checks::boolean::<lang::C>(2));
        assert!(!lang::checks::boolean::<lang::ObjectiveC>(2));
        assert!(lang::checks::boolean::<lang::Cpp>(2));
    }

    #[test]
    fn language_parameter_propagates_without_changing_representation() {
        fn round_trip<L: Language>() {
            fn check_markers<L: Language, T: Encode<L> + for<'d> Decode<'d, L>>() {}
            check_markers::<L, Flag>();
            let source = <Flag as EncodeMove<L>>::soft_encode(Flag(1), &mut ());
            // SAFETY: The source encodes a valid Flag and contains no references.
            let decoded = unsafe { <Flag as DecodeMove<'_, L>>::soft_decode(source, &mut ()) };
            assert_eq!(decoded, Some(Flag(1)));
        }

        round_trip::<lang::C>();

        round_trip::<lang::Cpp>();
        round_trip::<lang::Java>();
        round_trip::<lang::Swift>();
        round_trip::<lang::ObjectiveC>();
        round_trip::<lang::CSharp>();
        round_trip::<lang::Python>();

        struct Custom;
        impl Language for Custom {}
        round_trip::<Custom>();
    }

    #[cfg(feature = "derive")]
    #[test]
    fn deriving_preserves_explicit_original_decoder_bounds() {
        #[derive(crate::rust_spec::RustSpec, crate::ReprC, Debug, PartialEq)]
        #[repr(transparent)]
        struct Bounded<T: for<'a> crate::convert::Decode<'a>>(T);

        let encoded = unsafe { crate::encode::<_, lang::C>(Bounded(true)) };
        let decoded = unsafe { crate::decode::<Bounded<bool>, lang::Cpp>(encoded) };
        assert_eq!(decoded, Some(Bounded(true)));
    }
}
