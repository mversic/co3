//! C-compatible representation of [`core::option::Option`].
use core::{mem::MaybeUninit, option::Option as RustOption};

use rust_spec::{
    RustSpec,
    drop::NoDrop,
    layout::Robust,
    mutability::{Exclusive, ExclusiveKind},
    niche::WithoutNiche,
};

use crate::{
    CFnArg, CFnReturn, CType, ReprC, Thin,
    convert::{
        Decode, DecodeMove, EmptyStore, Encode, EncodeMove, EncodeMoveUnchecked, EncodeUnchecked,
        SoftEncodeRef, SoftEncodeRefUnchecked,
    },
    niche::Niche,
    sync::RefSizedEncodeStore,
    transmute::{AbiCompatible, AbiCompatibleValidate},
};

/// A transparent payload with option niche intent and no Rust enum discriminant.
///
/// This stores a valid `T`, including its ordinary validity requirements. It
/// lowers structurally to `OptionSome<T::CType>`; it is distinct from the hidden
/// carrier used to encode native `Option<T>`. A custom niche does not add a tag.
/// Robustness follows `T`, including any `rust-spec` custom-niche classification.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, RustSpec)]
#[repr(transparent)]
pub struct OptionSome<T>(pub T);

macro_rules! impl_niche_payload {
    ($name:ident) => {
        impl<T: $crate::ReprC<CType: Sized>> $crate::ReprC for $name<T> {
            type CType = $name<T::CType>;
        }
        unsafe impl<T: $crate::CType> $crate::CType for $name<T> {}
        unsafe impl<T: $crate::CFnArg> $crate::CFnArg for $name<T> {}
        unsafe impl<T: $crate::CFnReturn> $crate::CFnReturn for $name<T> {}
        impl<T: $crate::convert::Encode> $crate::convert::Encode for $name<T> {}
        impl<T: $crate::convert::EncodeUnchecked> $crate::convert::EncodeUnchecked for $name<T> {}
        impl<'d, T: $crate::convert::Decode<'d>> $crate::convert::Decode<'d> for $name<T> {}
        unsafe impl<T: $crate::transmute::AbiCompatible<CType: Sized>> $crate::transmute::AbiCompatible for $name<T> {}
        unsafe impl<T: $crate::transmute::AbiCompatibleValidate<CType: Sized>> $crate::transmute::AbiCompatibleValidate for $name<T> {
            unsafe fn is_valid(value: &Self::CType) -> bool {
                unsafe { T::is_valid(&value.0) }
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
                    $name(T::soft_encode(self.0, store, owner))
                }
            }
            unsafe impl<T: BorrowedEncodeUnchecked> BorrowedEncodeUnchecked for $name<T> {
                type Owner = T::Owner;
            type SoftStore = T::SoftStore;
                unsafe fn soft_encode_unchecked<'a>(self, store: &'a mut Self::SoftStore, owner: &'a mut Self::Owner) -> Self::CType where Self: 'a {
                    $name(unsafe { T::soft_encode_unchecked(self.0, store, owner) })
                }
            }
            unsafe impl<'d, T: BorrowedDecode<'d>> BorrowedDecode<'d> for $name<T> {
                type Store = T::Store;
                unsafe fn soft_decode<'a: 'd>(value: Self::CType, store: &'a mut Self::Store) -> core::option::Option<Self> {
                    Some($name(unsafe { T::soft_decode(value.0, store)? }))
                }
                unsafe fn soft_decode_unchecked<'a: 'd>(value: Self::CType, store: &'a mut Self::Store) -> Self {
                    $name(unsafe { T::soft_decode_unchecked(value.0, store) })
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
                    $name(T::soft_encode(self.0, store))
                }
            }
            unsafe impl<T: EncodePayloadUnchecked> EncodePayloadUnchecked for $name<T> {
                type SoftStore = T::SoftStore;
                unsafe fn soft_encode_unchecked<'a>(self, store: &'a mut Self::SoftStore) -> Self::CType where Self: 'a {
                    $name(unsafe { T::soft_encode_unchecked(self.0, store) })
                }
            }
            unsafe impl<'d, T: DecodePayload<'d>> DecodePayload<'d> for $name<T> {
                type Store = T::Store;
                unsafe fn soft_decode<'a: 'd>(value: Self::CType, store: &'a mut Self::Store) -> core::option::Option<Self> {
                    Some($name(unsafe { T::soft_decode(value.0, store)? }))
                }
                unsafe fn soft_decode_unchecked<'a: 'd>(value: Self::CType, store: &'a mut Self::Store) -> Self {
                    $name(unsafe { T::soft_decode_unchecked(value.0, store) })
                }
            }
        };
    };
}
pub(crate) use impl_niche_payload;
impl_niche_payload!(OptionSome);

/// An option with an explicit, stable discriminant representation.
#[repr(u8)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, RustSpec)]
pub enum Option<T> {
    #[default]
    None,
    Some(T),
}

/// FFI-safe equivalent of [`core::option::Option`].
/// `STABLE` identifies the co3 source family (`true`) or native Rust family (`false`).
#[repr(C)]
#[doc(hidden)]
pub struct COption<T: ReprC<CType: Sized>, const STABLE: bool = false> {
    tag: u8,
    payload: MaybeUninit<T::CType>,
}

/// Transparent carrier for a native `Option<T>` encoded in `T`'s niche.
///
/// The absent value is defined by `T::NICHE`; it need not be a null pointer or zero.
#[derive(RustSpec)]
#[repr(transparent)]
#[doc(hidden)]
pub struct COptionSome<R: ReprC<CType: Sized>>(pub R::CType);

impl<R: Niche<CType: Sized>> ReprC for COptionSome<R> {
    type CType = Self;
}
unsafe impl<R: Niche<CType: Sized>> CType for COptionSome<R> {}
unsafe impl<R: Niche<CType: CFnArg + Sized>> CFnArg for COptionSome<R> {}
unsafe impl<R: Niche<CType: CFnReturn + Sized>> CFnReturn for COptionSome<R> {}

crate::__impl_borrowed_identity!([R: Niche<CType: Sized>] COptionSome<R>);

unsafe impl<R: Niche<CType: Sized>> EncodeMoveUnchecked for COptionSome<R> {
    type SoftStore = ();

    unsafe fn soft_encode_unchecked<'a>(self, _: &'a mut ()) -> Self::CType
    where
        Self: 'a,
    {
        self
    }
}
unsafe impl<R: Niche<CType: Sized>> EncodeMove for COptionSome<R> {
    type Store = ();

    fn soft_encode<'a>(self, _: &'a mut ()) -> Self::CType
    where
        Self: 'a,
    {
        self
    }
}
unsafe impl<'d, R: Niche<CType: Sized>> DecodeMove<'d> for COptionSome<R> {
    type Store = ();

    unsafe fn soft_decode<'a: 'd>(source: Self::CType, _: &'a mut ()) -> RustOption<Self> {
        Some(source)
    }
}
impl<R: Niche<CType: Sized>> Encode for COptionSome<R> {}
impl<R: Niche<CType: Sized>> EncodeUnchecked for COptionSome<R> {}
impl<'d, R: Niche<CType: Sized>> Decode<'d> for COptionSome<R> {}
unsafe impl<R: Niche<CType: Sized>> AbiCompatibleValidate for COptionSome<R> {
    unsafe fn is_valid(_: &Self::CType) -> bool {
        true
    }
}
unsafe impl<R: Niche<CType: Sized>> AbiCompatible for COptionSome<R> {}

impl<R: ReprC<CType: Copy + Sized>> Copy for COptionSome<R> {}
impl<R: ReprC<CType: Clone + Sized>> Clone for COptionSome<R> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<R: ReprC<CType: core::fmt::Debug + Sized>> core::fmt::Debug for COptionSome<R> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("COptionSome").field(&self.0).finish()
    }
}
impl<R: ReprC<CType: PartialEq + Sized>> PartialEq for COptionSome<R> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl<R: ReprC<CType: Eq + Sized>> Eq for COptionSome<R> {}
impl<R: ReprC<CType: PartialOrd + Sized>> PartialOrd for COptionSome<R> {
    fn partial_cmp(&self, other: &Self) -> RustOption<core::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}
impl<R: ReprC<CType: Ord + Sized>> Ord for COptionSome<R> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}
impl<R: ReprC<CType: core::hash::Hash + Sized>> core::hash::Hash for COptionSome<R> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<T: ReprC<CType: Sized>> ReprC for Option<T> {
    type CType = COption<T, true>;
}

macro_rules! option_codecs {
    ($encode:ident, $unchecked:ident, $decode:ident) => {
        unsafe impl<T: $unchecked> $unchecked for Option<T> {
            type SoftStore = RustOption<T::SoftStore>;

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
                        COption::None()
                    }
                    Self::Some(value) => COption::Some(unsafe {
                        value.soft_encode_unchecked(store.insert(Default::default()))
                    }),
                }
            }
        }
        unsafe impl<T: $encode> $encode for Option<T> {
            type Store = RustOption<T::Store>;

            fn soft_encode<'a>(self, store: &'a mut Self::Store) -> Self::CType
            where
                Self: 'a,
            {
                match self {
                    Self::None => {
                        *store = None;
                        COption::None()
                    }
                    Self::Some(value) => COption::Some($encode::soft_encode(
                        value,
                        store.insert(Default::default()),
                    )),
                }
            }
        }
        unsafe impl<'d, T: $decode<'d>> $decode<'d> for Option<T> {
            type Store = RustOption<T::Store>;

            unsafe fn soft_decode<'a: 'd>(
                source: Self::CType,
                store: &'a mut Self::Store,
            ) -> RustOption<Self> {
                match source.into_option()? {
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
                match unsafe { source.into_option().unwrap_unchecked() } {
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
        type Owner = RustOption<T::Owner>;
        type SoftStore = RustOption<T::SoftStore>;

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
                    COption::None()
                }
                Self::Some(value) => COption::Some(unsafe {
                    value.soft_encode_unchecked(
                        store.insert(Default::default()),
                        owner.insert(Default::default()),
                    )
                }),
            }
        }
    }
    unsafe impl<T: BorrowedEncode> BorrowedEncode for Option<T> {
        type Store = RustOption<T::Store>;

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
                    COption::None()
                }
                Self::Some(value) => COption::Some(BorrowedEncode::soft_encode(
                    value,
                    store.insert(Default::default()),
                    owner.insert(Default::default()),
                )),
            }
        }
    }
    unsafe impl<'d, T: BorrowedDecode<'d>> BorrowedDecode<'d> for Option<T> {
        type Store = RustOption<T::Store>;

        unsafe fn soft_decode<'a: 'd>(
            source: Self::CType,
            store: &'a mut Self::Store,
        ) -> RustOption<Self> {
            match source.into_option()? {
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
            match unsafe { source.into_option().unwrap_unchecked() } {
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
    &'a Self: crate::ReprC<CType = crate::reference::CSoftRef<Self>>,
    Self: RustSpec<Mutability = Exclusive<__Interior>, Size: Thin> + 'a,
    &'a T: EncodeMoveUnchecked<CType: crate::reference::SharedRefCarrier<T>>,
{
    type SoftStore =
        RefSizedEncodeStore<Self, RustOption<<&'a T as EncodeMoveUnchecked>::SoftStore>>;

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
                COption::None()
            }
            Self::Some(value) => COption::Some(unsafe {
                crate::stored::encode_ref_field_unchecked(value, fields.insert(Default::default()))
            }),
        })
    }
}

unsafe impl<'a, T: ReprC<CType: Copy> + 'a, __Interior: ExclusiveKind> SoftEncodeRef<'a>
    for Option<T>
where
    &'a Self: crate::ReprC<CType = crate::reference::CSoftRef<Self>>,
    Self: RustSpec<Mutability = Exclusive<__Interior>, Size: Thin> + 'a,
    &'a T: EncodeMove<CType: crate::reference::SharedRefCarrier<T>>,
{
    type Store = RefSizedEncodeStore<Self, RustOption<<&'a T as EncodeMove>::Store>>;

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
                COption::None()
            }
            Self::Some(value) => COption::Some(crate::stored::encode_ref_field(
                value,
                fields.insert(Default::default()),
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

impl<T: ReprC<CType: core::fmt::Debug + Sized>, const STABLE: bool> core::fmt::Debug
    for COption<T, STABLE>
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.tag() {
            0 => f.write_str("COption::None"),
            1 => f
                .debug_tuple("COption::Some")
                .field(unsafe { self.payload.assume_init_ref() })
                .finish(),
            tag => f
                .debug_struct("COption::<invalid>")
                .field("tag", &tag)
                .finish(),
        }
    }
}

impl<T: ReprC<CType: PartialEq + Sized>, const STABLE: bool> PartialEq for COption<T, STABLE> {
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
impl<T: ReprC<CType: PartialOrd + Sized>, const STABLE: bool> PartialOrd for COption<T, STABLE> {
    fn partial_cmp(&self, other: &Self) -> RustOption<core::cmp::Ordering> {
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

impl<T: ReprC<CType: Sized>, const STABLE: bool> Default for COption<T, STABLE> {
    fn default() -> Self {
        Self::None()
    }
}

impl<T: ReprC<CType: Sized>, const STABLE: bool> COption<T, STABLE> {
    pub(crate) fn with_source<const OTHER: bool>(self) -> COption<T, OTHER> {
        // The source discriminator has no storage and does not affect layout.
        unsafe { crate::transmute::transmute_owned(self) }
    }

    pub(crate) const fn niche() -> Self {
        Self {
            tag: 2,
            payload: MaybeUninit::zeroed(),
        }
    }

    /// Construct a carrier from an optional payload.
    pub fn new(value: RustOption<T::CType>) -> Self {
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
    pub const unsafe fn from_raw_parts(tag: u8, payload: MaybeUninit<T::CType>) -> Self {
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
    pub const fn Some(value: T::CType) -> Self {
        Self {
            tag: 1,
            payload: MaybeUninit::new(value),
        }
    }
}

impl<T: ReprC<CType: Sized>, const STABLE: bool> COption<T, STABLE> {
    pub(crate) const fn is_niche(&self) -> bool {
        self.tag() == Self::niche().tag()
    }

    /// Return the stored variant tag.
    pub const fn tag(&self) -> u8 {
        self.tag
    }

    /// Return the represented `core::option::Option<T>`, or `None` for an invalid tag.
    pub const fn into_option(self) -> RustOption<RustOption<T::CType>> {
        match self.tag() {
            0 => Some(None),
            1 => Some(Some(unsafe { self.payload.assume_init() })),
            _ => None,
        }
    }

    /// Borrow only the active payload, rejecting invalid tags.
    pub(crate) const fn as_option(&self) -> RustOption<RustOption<&T::CType>> {
        match self.tag() {
            0 => Some(None),
            1 => Some(Some(unsafe { self.payload.assume_init_ref() })),
            _ => None,
        }
    }
}

impl<T: ReprC<CType: Copy + Sized>, const STABLE: bool> Copy for COption<T, STABLE> {}
impl<T: ReprC<CType: Copy + Sized>, const STABLE: bool> Clone for COption<T, STABLE> {
    fn clone(&self) -> Self {
        *self
    }
}
crate::__impl_borrowed_identity!([T: ReprC<CType: Sized>, const STABLE: bool] COption<T, STABLE>);

unsafe impl<T: ReprC<CType: RustSpec + Sized>, const STABLE: bool> RustSpec for COption<T, STABLE> {
    type Layout = <T::CType as RustSpec>::Layout;
    type Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>;
    type __Alignment = <T::CType as RustSpec>::__Alignment;
    type Trap = <T::CType as RustSpec>::Trap;
    type Niche = WithoutNiche;
    type Drop = <T::CType as RustSpec>::Drop;
    type Mutability = <T::CType as RustSpec>::Mutability;
    type Opacity = rust_spec::NonOpaque;
    type __IndirectTrap = <T::CType as RustSpec>::__IndirectTrap;
}

crate::wide::impl_identity_carrier! { COption, [T: ReprC<CType: Sized>, const STABLE: bool], [T, STABLE] }

unsafe impl<T: EmptyStore> EmptyStore for RustOption<T> {}

unsafe impl<T> CType for RustOption<T> where
    Self: RustSpec<Layout = rust_spec::Stable, Trap = Robust, Drop = NoDrop>
{
}
unsafe impl<T> CFnArg for RustOption<T> where
    Self: CType + RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}
unsafe impl<T> CFnReturn for RustOption<T> where
    Self: CType + RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}

impl<R: Encode> Encode for RustOption<R> where Self: EncodeMove {}
impl<R: EncodeUnchecked> EncodeUnchecked for RustOption<R> where Self: EncodeMoveUnchecked {}
impl<'d, R: Decode<'d>> Decode<'d> for RustOption<R> where Self: DecodeMove<'d> {}

impl<T> From<RustOption<T>> for Option<T> {
    fn from(value: RustOption<T>) -> Self {
        match value {
            None => Self::None,
            Some(value) => Self::Some(value),
        }
    }
}
impl<T> From<Option<T>> for RustOption<T> {
    fn from(value: Option<T>) -> Self {
        match value {
            Option::None => Self::None,
            Option::Some(value) => Self::Some(value),
        }
    }
}
