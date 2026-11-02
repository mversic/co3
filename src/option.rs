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
    CFnArg, CFnReturn, CType, Decode, Encode, EncodeUnchecked, ReprC, Thin,
    borrow::Borrow,
    niche::Niche,
    stored::{
        DecodeMove, EmptyStore, EncodeMove, EncodeMoveUnchecked, EncodeRef, EncodeRefUnchecked,
    },
    sync::RefSizedEncodeStore,
    transmute::{AbiCompatible, AbiCompatibleValidate},
};

/// An option with an explicit, stable discriminant representation.
#[repr(u8)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, RustSpec)]
pub enum Option<T> {
    #[default]
    None,
    Some(T),
}

/// FFI-safe equivalent of [`core::option::Option`].
#[repr(C)]
pub struct COption<T: ReprC<CType: Sized>> {
    tag: u8,
    payload: MaybeUninit<T::CType>,
}

/// Transparent carrier for a native `Option<T>` encoded in `T`'s niche.
///
/// The absent value is defined by `T::NICHE`; it need not be a null pointer or zero.
#[derive(RustSpec)]
#[repr(transparent)]
pub struct COptionSome<R: ReprC<CType: Sized>>(R::CType);

impl<R: ReprC<CType: Sized>> COptionSome<R> {
    pub(crate) const fn from_carrier(value: R::CType) -> Self {
        Self(value)
    }
    pub(crate) const fn carrier(&self) -> &R::CType {
        &self.0
    }

    pub(crate) fn into_carrier(self) -> R::CType {
        self.0
    }

    /// Return whether the carrier contains the source type's niche value.
    pub(crate) fn is_niche(&self) -> bool
    where
        R: Niche<CType: PartialEq>,
    {
        self.0 == R::NICHE
    }
}

impl<R: Niche<CType: Sized>> COptionSome<R> {
    pub const fn new(value: R::CType) -> Self {
        Self(value)
    }
}

impl<R: Niche<CType: Sized>> ReprC for COptionSome<R> {
    type CType = Self;
}
unsafe impl<R: Niche<CType: Sized>> CType for COptionSome<R> {}
unsafe impl<R: Niche<CType: CFnArg + Sized>> CFnArg for COptionSome<R> {}
unsafe impl<R: Niche<CType: CFnReturn + Sized>> CFnReturn for COptionSome<R> {}

unsafe impl<R: Niche<CType: Sized>> Borrow for COptionSome<R> {
    type Borrowed = Self;

    fn borrow(self) -> Self {
        self
    }

    fn from_borrow(source: Self) -> Self {
        source
    }
}

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

unsafe impl<T: Borrow> Borrow for Option<T> {
    type Borrowed = Option<T::Borrowed>;

    fn borrow(self) -> Self::Borrowed {
        match self {
            Self::None => Option::None,
            Self::Some(v) => Option::Some(v.borrow()),
        }
    }
    fn from_borrow(source: Self::Borrowed) -> Self {
        match source {
            Option::None => Self::None,
            Option::Some(v) => Self::Some(T::from_borrow(v)),
        }
    }
}

impl<T: ReprC<CType: Sized>> ReprC for Option<T> {
    type CType = COption<T>;
}

unsafe impl<T: EncodeMoveUnchecked> EncodeMoveUnchecked for Option<T> {
    type SoftStore = RustOption<T::SoftStore>;

    unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
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

unsafe impl<T: EncodeMove> EncodeMove for Option<T> {
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
            Self::Some(value) => COption::Some(value.soft_encode(store.insert(Default::default()))),
        }
    }
}

unsafe impl<'a, T: ReprC<CType: Copy> + 'a, __Interior: ExclusiveKind> EncodeRefUnchecked<'a>
    for Option<T>
where
    Self: RustSpec<Mutability = Exclusive<__Interior>, Size: Thin> + 'a,
    &'a T: EncodeMoveUnchecked<CType = crate::reference::CRef<T>>,
{
    type SoftStore =
        RefSizedEncodeStore<Self, RustOption<<&'a T as EncodeMoveUnchecked>::SoftStore>>;

    unsafe fn encode_ref_unchecked<'itm>(
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

unsafe impl<'a, T: ReprC<CType: Copy> + 'a, __Interior: ExclusiveKind> EncodeRef<'a> for Option<T>
where
    Self: RustSpec<Mutability = Exclusive<__Interior>, Size: Thin> + 'a,
    &'a T: EncodeMove<CType = crate::reference::CRef<T>>,
{
    type Store = RefSizedEncodeStore<Self, RustOption<<&'a T as EncodeMove>::Store>>;

    fn encode_ref<'itm>(&'a self, store: &'itm mut Self::Store) -> <&'a Self as ReprC>::CType
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

unsafe impl<'d, T: DecodeMove<'d>> DecodeMove<'d> for Option<T> {
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

impl<T: ReprC<CType: core::fmt::Debug + Sized>> core::fmt::Debug for COption<T> {
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

impl<T: ReprC<CType: PartialEq + Sized>> PartialEq for COption<T> {
    fn eq(&self, other: &Self) -> bool {
        match (self.tag(), other.tag) {
            (0, 0) => true,
            (1, 1) => {
                let self_payload = unsafe { self.payload.assume_init_ref() };
                let other_payload = unsafe { other.payload.assume_init_ref() };

                self_payload == other_payload
            }
            (left, right) if left >= 2 && right >= 2 => left == right,
            _ => false,
        }
    }
}
impl<T: ReprC<CType: PartialOrd + Sized>> PartialOrd for COption<T> {
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

impl<T: ReprC<CType: Sized>> Default for COption<T> {
    fn default() -> Self {
        Self::None()
    }
}

impl<T: ReprC<CType: Sized>> COption<T> {
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

impl<T: ReprC<CType: Sized>> COption<T> {
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

impl<T: ReprC<CType: Copy + Sized>> Copy for COption<T> {}
impl<T: ReprC<CType: Copy + Sized>> Clone for COption<T> {
    fn clone(&self) -> Self {
        *self
    }
}
unsafe impl<T: ReprC<CType: Sized>> Borrow for COption<T> {
    type Borrowed = Self;

    fn borrow(self) -> Self {
        self
    }
    fn from_borrow(source: Self) -> Self {
        source
    }
}

unsafe impl<T: ReprC<CType: RustSpec + Sized>> RustSpec for COption<T> {
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

crate::slice::impl_identity_carrier! { COption, [T: ReprC<CType: Sized>], [T] }

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
