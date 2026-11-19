//! C-compatible representation of [`core::option::Option`].

use core::mem::MaybeUninit;

use rust_spec::{
    RustSpec,
    drop::NoDrop,
    layout::Robust,
    mutability::{Exclusive, ExclusiveKind},
    niche::WithoutNiche,
};

use crate::{
    CFnArg, CFnReturn, CType, ReprC, Thin,
    convert::AbiCompatibleValidate,
    convert::{
        Decode, DecodeMove, EmptyStore, Encode, EncodeMove, EncodeMoveUnchecked, EncodeUnchecked,
        SoftEncodeRef, SoftEncodeRefUnchecked,
    },
    marker::AbiCompatible,
    sync::RefSizedEncodeStore,
};

/// An option with an explicit, stable discriminant representation.
#[repr(u8)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, RustSpec)]
pub enum Option<T> {
    #[default]
    None,
    Some(T),
}

impl<T> From<core::option::Option<T>> for Option<T> {
    fn from(value: core::option::Option<T>) -> Self {
        match value {
            None => Self::None,
            Some(value) => Self::Some(value),
        }
    }
}

impl<T: ReprC<CType: Sized>> ReprC for Option<T> {
    type CType = crate::marker::Stable<abi::Option<T::CType>>;
}

macro_rules! option_codecs {
    ($encode:ident, $unchecked:ident, $decode:ident) => {
        unsafe impl<T: $unchecked> $unchecked for Option<T> {
            type SoftStore = core::option::Option<T::SoftStore>;

            unsafe fn soft_encode_unchecked<'itm>(
                self,
                store: &'itm mut Self::SoftStore,
            ) -> Self::CType
            where
                Self: 'itm,
            {
                match self {
                    Self::None => {
                        *store = None;
                        crate::marker::Stable(abi::Option::None())
                    }
                    Self::Some(value) => crate::marker::Stable(abi::Option::Some(unsafe {
                        value.soft_encode_unchecked(store.insert(Default::default()))
                    })),
                }
            }
        }
        unsafe impl<T: $encode> $encode for Option<T> {
            type Store = core::option::Option<T::Store>;

            fn soft_encode<'a>(self, store: &'a mut Self::Store) -> Self::CType
            where
                Self: 'a,
            {
                match self {
                    Self::None => {
                        *store = None;
                        crate::marker::Stable(abi::Option::None())
                    }
                    Self::Some(value) => crate::marker::Stable(abi::Option::Some(
                        $encode::soft_encode(value, store.insert(Default::default())),
                    )),
                }
            }
        }
        unsafe impl<'d, T: $decode<'d>> $decode<'d> for Option<T> {
            type Store = core::option::Option<T::Store>;

            unsafe fn soft_decode<'a: 'd>(
                source: Self::CType,
                store: &'a mut Self::Store,
            ) -> core::option::Option<Self> {
                match source.0.into_option()? {
                    None => {
                        *store = None;
                        Some(Self::None)
                    }
                    Some(value) => Some(Self::Some(unsafe {
                        T::soft_decode(value, store.insert(Default::default()))?
                    })),
                }
            }

            unsafe fn soft_decode_unchecked<'a: 'd>(
                source: Self::CType,
                store: &'a mut Self::Store,
            ) -> Self {
                match unsafe { source.0.into_option().unwrap_unchecked() } {
                    None => {
                        *store = None;
                        Self::None
                    }
                    Some(value) => Self::Some(unsafe {
                        T::soft_decode_unchecked(value, store.insert(Default::default()))
                    }),
                }
            }
        }
    };
}
option_codecs!(EncodeMove, EncodeMoveUnchecked, DecodeMove);
const _: () = {
    use crate::borrow::{BorrowedDecode, BorrowedEncode, BorrowedEncodeUnchecked};
    unsafe impl<T: BorrowedEncodeUnchecked> BorrowedEncodeUnchecked for Option<T> {
        type Owner = core::option::Option<T::Owner>;
        type SoftStore = core::option::Option<T::SoftStore>;

        unsafe fn soft_encode_unchecked<'itm>(
            self,
            store: &'itm mut Self::SoftStore,
            owner: &'itm mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'itm,
        {
            match self {
                Self::None => {
                    *store = None;
                    *owner = None;
                    crate::marker::Stable(abi::Option::None())
                }
                Self::Some(value) => crate::marker::Stable(abi::Option::Some(unsafe {
                    value.soft_encode_unchecked(
                        store.insert(Default::default()),
                        owner.insert(Default::default()),
                    )
                })),
            }
        }
    }
    unsafe impl<T: BorrowedEncode> BorrowedEncode for Option<T> {
        type Store = core::option::Option<T::Store>;

        fn soft_encode<'a>(
            self,
            store: &'a mut Self::Store,
            owner: &'a mut Self::Owner,
        ) -> Self::CType
        where
            Self: 'a,
        {
            match self {
                Self::None => {
                    *store = None;
                    *owner = None;
                    crate::marker::Stable(abi::Option::None())
                }
                Self::Some(value) => {
                    crate::marker::Stable(abi::Option::Some(BorrowedEncode::soft_encode(
                        value,
                        store.insert(Default::default()),
                        owner.insert(Default::default()),
                    )))
                }
            }
        }
    }
    unsafe impl<'d, T: BorrowedDecode<'d>> BorrowedDecode<'d> for Option<T> {
        type Store = core::option::Option<T::Store>;

        unsafe fn soft_decode<'a: 'd>(
            source: Self::CType,
            store: &'a mut Self::Store,
        ) -> core::option::Option<Self> {
            match source.0.into_option()? {
                None => {
                    *store = None;
                    Some(Self::None)
                }
                Some(value) => Some(Self::Some(unsafe {
                    T::soft_decode(value, store.insert(Default::default()))?
                })),
            }
        }

        unsafe fn soft_decode_unchecked<'a: 'd>(
            source: Self::CType,
            store: &'a mut Self::Store,
        ) -> Self {
            match unsafe { source.0.into_option().unwrap_unchecked() } {
                None => {
                    *store = None;
                    Self::None
                }
                Some(value) => Self::Some(unsafe {
                    T::soft_decode_unchecked(value, store.insert(Default::default()))
                }),
            }
        }
    }
};

unsafe impl<'a, T: ReprC<CType: Copy> + 'a, __Interior: ExclusiveKind> SoftEncodeRefUnchecked<'a>
    for Option<T>
where
    &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
    Self: RustSpec<Mutability = Exclusive<__Interior>, Size: Thin> + 'a,
    &'a T: EncodeMoveUnchecked<CType: crate::reference::RefCarrier<T>>,
{
    type SoftStore =
        RefSizedEncodeStore<Self, core::option::Option<<&'a T as EncodeMoveUnchecked>::SoftStore>>;

    unsafe fn soft_encode_ref_unchecked<'itm>(
        &'a self,
        store: &'itm mut Self::SoftStore,
    ) -> <&'a Self as ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|fields| match self {
            Self::None => {
                *fields = None;
                crate::marker::Stable(abi::Option::None())
            }
            Self::Some(value) => crate::marker::Stable(abi::Option::Some(unsafe {
                crate::stored::encode_ref_field_unchecked(value, fields.insert(Default::default()))
            })),
        })
    }
}

unsafe impl<'a, T: ReprC<CType: Copy> + 'a, __Interior: ExclusiveKind> SoftEncodeRef<'a>
    for Option<T>
where
    &'a Self: crate::ReprC<CType = crate::reference::abi::SoftRef<<Self as ReprC>::CType>>,
    Self: RustSpec<Mutability = Exclusive<__Interior>, Size: Thin> + 'a,
    &'a T: EncodeMove<CType: crate::reference::RefCarrier<T>>,
{
    type Store = RefSizedEncodeStore<Self, core::option::Option<<&'a T as EncodeMove>::Store>>;

    fn soft_encode_ref<'itm>(
        &'a self,
        store: &'itm mut Self::Store,
    ) -> <&'a Self as crate::ReprC>::CType
    where
        'a: 'itm,
    {
        store.encode(|fields| match self {
            Self::None => {
                *fields = None;
                crate::marker::Stable(abi::Option::None())
            }
            Self::Some(value) => crate::marker::Stable(abi::Option::Some(
                crate::stored::encode_ref_field(value, fields.insert(Default::default())),
            )),
        })
    }
}

impl<T: Encode> Encode for Option<T> {}
impl<T: EncodeUnchecked> EncodeUnchecked for Option<T> {}
impl<'d, T: Decode<'d>> Decode<'d> for Option<T> {}

unsafe impl<T: AbiCompatibleValidate<CType: Copy>> AbiCompatibleValidate for Option<T> {
    unsafe fn is_valid(target: &Self::CType) -> bool {
        match target.tag {
            0 => true,
            1 => unsafe { T::is_valid(target.payload.assume_init_ref()) },
            _ => false,
        }
    }
}
unsafe impl<T: AbiCompatible<CType: Copy>> AbiCompatible for Option<T> {}

impl<T: ReprC<CType: Copy>> crate::niche::Niche for Option<T> {
    const NICHE: Self::CType = crate::marker::Stable(abi::Option::niche());

    fn is_niche(value: &Self::CType) -> bool {
        value.is_niche()
    }
}

/// A transparent payload with option niche intent and no Rust enum discriminant.
///
/// This stores a valid `T`, including its ordinary validity requirements. It
/// lowers to `crate::marker::Stable<abi::OptionSome<T::CType>>`. Native niche-encoded options use
/// the bare carrier. A custom niche does not add a tag.
/// Robustness follows `T`, including any `rust-spec` custom-niche classification.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, RustSpec)]
#[repr(transparent)]
pub struct OptionSome<T>(pub T);

macro_rules! impl_niche_payload {
    ($name:ident => $ctype:ty) => {
        impl<T: $crate::ReprC<CType: Sized>> $crate::ReprC for $name<T> {
            type CType = $ctype;
        }
        unsafe impl<T: $crate::CType> $crate::CType for $name<T> {}
        unsafe impl<T: $crate::CFnArg> $crate::CFnArg for $name<T> {}
        unsafe impl<T: $crate::CFnReturn> $crate::CFnReturn for $name<T> {}
        impl<T: $crate::convert::Encode> $crate::convert::Encode for $name<T> {}
        impl<T: $crate::convert::EncodeUnchecked> $crate::convert::EncodeUnchecked for $name<T> {}
        impl<'d, T: $crate::convert::Decode<'d>> $crate::convert::Decode<'d> for $name<T> {}
        unsafe impl<T: $crate::marker::AbiCompatible<CType: Sized>> $crate::marker::AbiCompatible for $name<T> {}
        unsafe impl<T: $crate::convert::AbiCompatibleValidate<CType: Sized>> $crate::convert::AbiCompatibleValidate for $name<T> {
            unsafe fn is_valid(value: &Self::CType) -> bool {
                unsafe { T::is_valid(&value.0.0) }
            }
        }
        $crate::option::impl_niche_payload!(@codecs $name,
            $crate::convert::EncodeMove, $crate::convert::EncodeMoveUnchecked, $crate::convert::DecodeMove);
        const _: () = {
            use $crate::borrow::BorrowedEncode;
            use $crate::borrow::BorrowedEncodeUnchecked;
            use $crate::borrow::BorrowedDecode;
            unsafe impl<T: BorrowedEncode> BorrowedEncode for $name<T> {
                type Store = T::Store;
                fn soft_encode<'a>(self, store: &'a mut Self::Store, owner: &'a mut Self::Owner) -> Self::CType where Self: 'a {
                    crate::marker::Stable(abi::$name::from_payload(T::soft_encode(self.0, store, owner)))
                }
            }
            unsafe impl<T: BorrowedEncodeUnchecked> BorrowedEncodeUnchecked for $name<T> {
                type Owner = T::Owner;
            type SoftStore = T::SoftStore;
                unsafe fn soft_encode_unchecked<'a>(self, store: &'a mut Self::SoftStore, owner: &'a mut Self::Owner) -> Self::CType where Self: 'a {
                    crate::marker::Stable(abi::$name::from_payload(unsafe { T::soft_encode_unchecked(self.0, store, owner) }))
                }
            }
            unsafe impl<'d, T: BorrowedDecode<'d>> BorrowedDecode<'d> for $name<T> {
                type Store = T::Store;
                unsafe fn soft_decode<'a: 'd>(value: Self::CType, store: &'a mut Self::Store) -> core::option::Option<Self> {
                    Some($name(unsafe { T::soft_decode(value.0.0, store)? }))
                }
                unsafe fn soft_decode_unchecked<'a: 'd>(value: Self::CType, store: &'a mut Self::Store) -> Self {
                    $name(unsafe { T::soft_decode_unchecked(value.0.0, store) })
                }
            }
        };
    };
    (@codecs $name:ident, $encode:path, $unchecked:path, $decode:path) => {
        const _: () = {
            use $encode as EncodePayload;
            use $unchecked as EncodePayloadUnchecked;
            use $decode as DecodePayload;
            unsafe impl<T: EncodePayload> EncodePayload for $name<T> {
                type Store = T::Store;
                fn soft_encode<'a>(self, store: &'a mut Self::Store) -> Self::CType where Self: 'a {
                    crate::marker::Stable(abi::$name::from_payload(T::soft_encode(self.0, store)))
                }
            }
            unsafe impl<T: EncodePayloadUnchecked> EncodePayloadUnchecked for $name<T> {
                type SoftStore = T::SoftStore;
                unsafe fn soft_encode_unchecked<'a>(self, store: &'a mut Self::SoftStore) -> Self::CType where Self: 'a {
                    crate::marker::Stable(abi::$name::from_payload(unsafe { T::soft_encode_unchecked(self.0, store) }))
                }
            }
            unsafe impl<'d, T: DecodePayload<'d>> DecodePayload<'d> for $name<T> {
                type Store = T::Store;
                unsafe fn soft_decode<'a: 'd>(value: Self::CType, store: &'a mut Self::Store) -> core::option::Option<Self> {
                    Some($name(unsafe { T::soft_decode(value.0.0, store)? }))
                }
                unsafe fn soft_decode_unchecked<'a: 'd>(value: Self::CType, store: &'a mut Self::Store) -> Self {
                    $name(unsafe { T::soft_decode_unchecked(value.0.0, store) })
                }
            }
        };
    };
}
pub(crate) use impl_niche_payload;
impl_niche_payload!(OptionSome => crate::marker::Stable<abi::OptionSome<T::CType>>);

impl<T> From<Option<T>> for core::option::Option<T> {
    fn from(value: Option<T>) -> Self {
        match value {
            Option::None => Self::None,
            Option::Some(value) => Self::Some(value),
        }
    }
}

unsafe impl<T: EmptyStore> EmptyStore for core::option::Option<T> {}

unsafe impl<T> CType for core::option::Option<T> where
    Self: RustSpec<Layout = rust_spec::Stable, Trap = Robust, Drop = NoDrop>
{
}
unsafe impl<T> CFnArg for core::option::Option<T> where
    Self: CType + RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}
unsafe impl<T> CFnReturn for core::option::Option<T> where
    Self: CType + RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}

impl<R: Encode> Encode for core::option::Option<R> where Self: EncodeMove {}
impl<R: EncodeUnchecked> EncodeUnchecked for core::option::Option<R> where Self: EncodeMoveUnchecked {}
impl<'d, R: Decode<'d>> Decode<'d> for core::option::Option<R> where Self: DecodeMove<'d> {}

#[doc(hidden)]
pub mod abi {
    /// Transparent carrier for a niche-encoded native option or a co3 payload wrapper.
    /// The co3 source wraps this carrier in [`crate::marker::Stable`].
    ///
    /// The Rust source type supplies the absent value through `Niche::NICHE`; it need not be a null pointer or zero.
    #[derive(RustSpec)]
    #[repr(transparent)]
    #[doc(hidden)]
    pub struct OptionSome<R: CType>(pub R);

    impl<R: CType> ReprC for OptionSome<R> {
        type CType = Self;
    }
    unsafe impl<R: CType> CType for OptionSome<R> {}
    unsafe impl<R: CType + CFnArg + Sized> CFnArg for OptionSome<R> {}
    unsafe impl<R: CType + CFnReturn + Sized> CFnReturn for OptionSome<R> {}

    crate::borrow::__impl_borrowed_identity!([R: CType] OptionSome<R>);

    unsafe impl<R: CType> EncodeMoveUnchecked for OptionSome<R> {
        type SoftStore = ();

        unsafe fn soft_encode_unchecked<'a>(self, _: &'a mut ()) -> Self::CType
        where
            Self: 'a,
        {
            self
        }
    }
    unsafe impl<R: CType> EncodeMove for OptionSome<R> {
        type Store = ();

        fn soft_encode<'a>(self, _: &'a mut ()) -> Self::CType
        where
            Self: 'a,
        {
            self
        }
    }
    unsafe impl<'d, R: CType> DecodeMove<'d> for OptionSome<R> {
        type Store = ();

        unsafe fn soft_decode<'a: 'd>(
            source: Self::CType,
            _: &'a mut (),
        ) -> core::option::Option<Self> {
            Some(source)
        }
    }
    impl<R: CType> Encode for OptionSome<R> {}
    impl<R: CType> EncodeUnchecked for OptionSome<R> {}
    impl<'d, R: CType> Decode<'d> for OptionSome<R> {}
    unsafe impl<R: CType> AbiCompatibleValidate for OptionSome<R> {
        unsafe fn is_valid(_: &Self::CType) -> bool {
            true
        }
    }
    unsafe impl<R: CType> AbiCompatible for OptionSome<R> {}

    impl<R: CType + Copy + Sized> Copy for OptionSome<R> {}
    impl<R: CType + Clone + Sized> Clone for OptionSome<R> {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }
    impl<R: CType + core::fmt::Debug + Sized> core::fmt::Debug for OptionSome<R> {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_tuple("OptionSome").field(&self.0).finish()
        }
    }
    impl<R: CType + PartialEq + Sized> PartialEq for OptionSome<R> {
        fn eq(&self, other: &Self) -> bool {
            self.0 == other.0
        }
    }
    impl<R: CType + Eq + Sized> Eq for OptionSome<R> {}
    impl<R: CType + PartialOrd + Sized> PartialOrd for OptionSome<R> {
        fn partial_cmp(&self, other: &Self) -> core::option::Option<core::cmp::Ordering> {
            self.0.partial_cmp(&other.0)
        }
    }
    impl<R: CType + Ord + Sized> Ord for OptionSome<R> {
        fn cmp(&self, other: &Self) -> core::cmp::Ordering {
            self.0.cmp(&other.0)
        }
    }
    impl<R: CType + core::hash::Hash + Sized> core::hash::Hash for OptionSome<R> {
        fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
            self.0.hash(state);
        }
    }

    use super::*;

    impl<R: CType> OptionSome<R> {
        pub(super) fn from_payload(value: R) -> Self {
            Self(value)
        }
    }

    /// FFI-safe equivalent of [`core::option::Option`].
    /// Co3 sources select [`crate::marker::Stable<Self>`]; native Rust sources select this carrier directly.
    #[repr(C)]
    #[doc(hidden)]
    pub struct Option<T: CType> {
        pub(super) tag: u8,
        pub(super) payload: MaybeUninit<T>,
    }

    impl<T: CType + core::fmt::Debug + Sized> core::fmt::Debug for Option<T> {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            match self.tag() {
                0 => f.write_str("Option::None"),
                1 => f
                    .debug_tuple("Option::Some")
                    .field(unsafe { self.payload.assume_init_ref() })
                    .finish(),
                tag => f
                    .debug_struct("Option::<invalid>")
                    .field("tag", &tag)
                    .finish(),
            }
        }
    }

    impl<T: CType + PartialEq + Sized> PartialEq for Option<T> {
        fn eq(&self, other: &Self) -> bool {
            match (self.tag(), other.tag) {
                (0, 0) => true,
                (1, 1) => {
                    let self_payload = unsafe { self.payload.assume_init_ref() };
                    let other_payload = unsafe { other.payload.assume_init_ref() };

                    self_payload == other_payload
                }
                _ => false,
            }
        }
    }

    impl<T: CType + PartialOrd + Sized> PartialOrd for Option<T> {
        fn partial_cmp(&self, other: &Self) -> core::option::Option<core::cmp::Ordering> {
            match (self.tag(), other.tag) {
                (0, 0) => Some(core::cmp::Ordering::Equal),
                (0, 1) => Some(core::cmp::Ordering::Less),
                (1, 0) => Some(core::cmp::Ordering::Greater),
                (1, 1) => {
                    let self_payload = unsafe { self.payload.assume_init_ref() };
                    let other_payload = unsafe { other.payload.assume_init_ref() };

                    self_payload.partial_cmp(other_payload)
                }
                _ => None,
            }
        }
    }

    impl<T: CType> Default for Option<T> {
        fn default() -> Self {
            Self::None()
        }
    }

    impl<T: CType> Option<T> {
        pub(crate) const fn niche() -> Self {
            Self {
                tag: 2,
                payload: MaybeUninit::zeroed(),
            }
        }

        /// Construct a carrier from an optional payload.
        pub fn new(value: core::option::Option<T>) -> Self {
            match value {
                None => Self::None(),
                Some(value) => Self::Some(value),
            }
        }

        /// Construct a carrier from its raw tag and payload.
        ///
        /// # Safety
        ///
        /// If `tag` is 1, `payload` must contain an initialized value.
        pub const unsafe fn from_raw_parts(tag: u8, payload: MaybeUninit<T>) -> Self {
            Self { tag, payload }
        }

        /// Construct no value
        #[expect(non_snake_case)]
        pub const fn None() -> Self {
            Self {
                tag: 0,
                payload: MaybeUninit::zeroed(),
            }
        }

        /// Construct some value
        #[expect(non_snake_case)]
        pub const fn Some(value: T) -> Self {
            Self {
                tag: 1,
                payload: MaybeUninit::new(value),
            }
        }
    }

    impl<T: CType> Option<T> {
        pub(crate) const fn is_niche(&self) -> bool {
            self.tag() == Self::niche().tag()
        }

        /// Return the stored variant tag.
        pub const fn tag(&self) -> u8 {
            self.tag
        }

        /// Return the represented `core::option::Option<T>`, or `None` for an invalid tag.
        pub const fn into_option(self) -> core::option::Option<core::option::Option<T>> {
            match self.tag() {
                0 => Some(None),
                1 => Some(Some(unsafe { self.payload.assume_init() })),
                _ => None,
            }
        }

        /// Borrow only the active payload, rejecting invalid tags.
        pub(crate) const fn as_option(&self) -> core::option::Option<core::option::Option<&T>> {
            match self.tag() {
                0 => Some(None),
                1 => Some(Some(unsafe { self.payload.assume_init_ref() })),
                _ => None,
            }
        }
    }

    impl<T: CType + Copy + Sized> Copy for Option<T> {}

    impl<T: CType + Copy + Sized> Clone for Option<T> {
        fn clone(&self) -> Self {
            *self
        }
    }

    crate::borrow::__impl_borrowed_identity!([T: CType] Option<T>);

    unsafe impl<T: CType + RustSpec + Sized> RustSpec for Option<T> {
        type Layout = <T as RustSpec>::Layout;
        type Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>;
        type __Alignment = <T as RustSpec>::__Alignment;
        type Trap = <T as RustSpec>::Trap;
        type Niche = WithoutNiche;
        type Drop = <T as RustSpec>::Drop;
        type Mutability = <T as RustSpec>::Mutability;
        type Opacity = rust_spec::NonOpaque;
        type __IndirectTrap = <T as RustSpec>::__IndirectTrap;
    }

    crate::wide::impl_identity_carrier! { Option, [T: CType], [T] }
}

#[cfg(test)]
mod tests {
    use core::mem::MaybeUninit;

    use super::abi::{self, Option as AbiOption};
    use crate::reference::abi::SoftRefMut;

    const CONST_OPTION: Option<Option<u32>> = AbiOption::<u32>::Some(7).into_option();

    #[test]
    fn tagged_option_carrier_preserves_active_payloads() {
        assert_eq!(CONST_OPTION, Some(Some(7)));
        assert_eq!(AbiOption::<u32>::new(Some(7)).into_option(), Some(Some(7)));
        assert_eq!(AbiOption::<u32>::new(None).into_option(), Some(None));
        let option = unsafe { AbiOption::<u32>::from_raw_parts(2, MaybeUninit::uninit()) };
        assert!(option != option);
        assert_eq!(option.partial_cmp(&option), None);
        assert_eq!(option.into_option(), None);
        let option = unsafe { AbiOption::<u32>::from_raw_parts(1, MaybeUninit::new(9)) };
        assert_eq!(option.into_option(), Some(Some(9)));
    }

    #[test]
    fn decoding_none_has_no_active_store() {
        let mut untagged = Default::default();
        let decoded =
            unsafe { co3::soft_decode::<Option<u8>>(abi::Option::Some(7), &mut untagged) };
        assert!(matches!(decoded, Some(Some(7))));
        assert!(untagged.is_some());

        let decoded = unsafe { co3::soft_decode::<Option<u8>>(abi::Option::None(), &mut untagged) };
        assert!(matches!(decoded, Some(None)));
        assert!(untagged.is_none());
        assert_eq!(co3::convert::Store::sync(untagged), Some(()));

        let mut niche = Default::default();
        let mut source = co3::primitive::abi::bool::TRUE;
        let encoded = unsafe { SoftRefMut::<<bool as co3::ReprC>::CType>::from_raw(&mut source) };
        let decoded =
            unsafe { co3::soft_decode::<Option<&mut bool>>(abi::OptionSome(encoded), &mut niche) };
        assert!(matches!(decoded, Some(Some(true))));
        assert!(niche.is_some());

        let decoded = unsafe {
            co3::soft_decode::<Option<&mut bool>>(
                abi::OptionSome(SoftRefMut::<<bool as co3::ReprC>::CType>::null()),
                &mut niche,
            )
        };
        assert!(matches!(decoded, Some(None)));
        assert!(niche.is_none());
        assert_eq!(co3::convert::Store::sync(niche), Some(()));
    }
}
