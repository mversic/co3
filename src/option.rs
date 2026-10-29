//! C-compatible representation of [`core::option::Option`].
use crate::transmute::AbiCompatibleValidate;
use core::mem::MaybeUninit;
use core::option::Option as CoreOption;

use rust_spec::drop::NoDrop;
use rust_spec::{
    RustSpec,
    mutability::{Exclusive, ExclusiveKind},
    niche::WithoutNiche,
};

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, EncodeUnchecked, ReprC, Thin,
    borrow::{Borrow, BorrowCast, BorrowCastMut},
    stored::{
        DecodeOwned, EmptyStore, EncodeMove, EncodeMoveUnchecked, EncodeRef, EncodeRefUnchecked,
    },
    sync::RefSizedEncodeStore,
    transmute::AbiCompatible,
};

/// An option with an explicit, stable discriminant representation.
#[repr(u8)]
#[derive(RustSpec, Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Option<T> {
    #[default]
    None,
    Some(T),
}

/// FFI-safe equivalent of [`core::option::Option`].
#[repr(C)]
pub struct COption<T> {
    tag: u8,
    payload: MaybeUninit<T>,
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
    type CType = COption<T::CType>;
}

unsafe impl<T: EncodeMoveUnchecked> EncodeMoveUnchecked for Option<T> {
    type SoftStore = CoreOption<T::SoftStore>;

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
    type Store = CoreOption<T::Store>;

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
    &'a T: EncodeMoveUnchecked<CType = crate::reference::CRef<T::CType>>,
{
    type SoftStore =
        RefSizedEncodeStore<Self, CoreOption<<&'a T as EncodeMoveUnchecked>::SoftStore>>;

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
    &'a T: EncodeMove<CType = crate::reference::CRef<T::CType>>,
{
    type Store = RefSizedEncodeStore<Self, CoreOption<<&'a T as EncodeMove>::Store>>;

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

unsafe impl<'d, T: DecodeOwned<'d>> DecodeOwned<'d> for Option<T> {
    type Store = CoreOption<T::Store>;

    unsafe fn soft_decode<'a: 'd>(
        source: Self::CType,
        store: &'a mut Self::Store,
    ) -> CoreOption<Self> {
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
        match unsafe { source.into_option_unchecked() } {
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

impl<T: core::fmt::Debug> core::fmt::Debug for COption<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.tag {
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

impl<T: PartialEq> PartialEq for COption<T> {
    fn eq(&self, other: &Self) -> bool {
        match (self.tag, other.tag) {
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
impl<T: PartialOrd> PartialOrd for COption<T> {
    fn partial_cmp(&self, other: &Self) -> CoreOption<core::cmp::Ordering> {
        match (self.tag, other.tag) {
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

impl<T: CType> Default for COption<T> {
    fn default() -> Self {
        Self::None()
    }
}

impl<T: CType> COption<T> {
    pub(crate) const fn niche() -> Self {
        Self {
            tag: 2,
            payload: MaybeUninit::zeroed(),
        }
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

impl<T> COption<T> {
    /// Return the stored variant tag.
    pub const fn tag(&self) -> u8 {
        self.tag
    }

    /// Return the represented `core::option::Option<T>`, or `None` for an invalid tag.
    pub fn into_option(self) -> CoreOption<CoreOption<T>> {
        match self.tag {
            0 => Some(None),
            1 => Some(Some(unsafe { self.payload.assume_init() })),
            _ => None,
        }
    }

    pub(crate) unsafe fn into_option_unchecked(self) -> CoreOption<T> {
        match self.tag {
            0 => None,
            1 => Some(unsafe { self.payload.assume_init() }),
            _ => unsafe { core::hint::unreachable_unchecked() },
        }
    }
}

impl<T: Copy> Copy for COption<T> {}
impl<T: Copy> Clone for COption<T> {
    fn clone(&self) -> Self {
        *self
    }
}
unsafe impl<T: CType> Borrow for COption<T> {
    type Borrowed = Self;

    fn borrow(self) -> Self {
        self
    }
    fn from_borrow(source: Self) -> Self {
        source
    }
}

unsafe impl<T: RustSpec> RustSpec for COption<T> {
    type Layout = T::Layout;
    type Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>;
    type __Alignment = T::__Alignment;
    type Trap = T::Trap;
    type Niche = WithoutNiche;
    type Drop = T::Drop;
    type Mutability = T::Mutability;
    type Opacity = rust_spec::NonOpaque;
    type __IndirectTrap = T::__IndirectTrap;
}

crate::slice::impl_identity_carrier! { COption, [T: CType], [T] }

unsafe impl<T: BorrowCast<AsConst: Copy>> BorrowCast for COption<T> {
    type AsConst = COption<T::AsConst>;
}
unsafe impl<T: BorrowCastMut<AsMut: Copy>> BorrowCastMut for COption<T> {
    type AsMut = COption<T::AsMut>;
}

unsafe impl<T: EmptyStore> EmptyStore for CoreOption<T> {}

unsafe impl<T> CType for CoreOption<T> where
    Self: RustSpec<Layout = rust_spec::Stable, Trap = rust_spec::layout::Robust, Drop = NoDrop>
{
}
unsafe impl<T> CFnArg for CoreOption<T> where
    Self: CType + RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}
unsafe impl<T> CFnReturn for CoreOption<T> where
    Self: CType + RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>
{
}
unsafe impl<T> BorrowCast for CoreOption<T>
where
    Self: CType,
{
    type AsConst = Self;
}
unsafe impl<T> BorrowCastMut for CoreOption<T>
where
    Self: CType,
{
    type AsMut = Self;
}

impl<R: Encode> Encode for CoreOption<R> where Self: EncodeMove {}
impl<R: EncodeUnchecked> EncodeUnchecked for CoreOption<R> where Self: EncodeMoveUnchecked {}
impl<'d, R: Decode<'d>> Decode<'d> for CoreOption<R> where Self: DecodeOwned<'d> {}

impl<T> From<CoreOption<T>> for Option<T> {
    fn from(value: CoreOption<T>) -> Self {
        match value {
            None => Self::None,
            Some(value) => Self::Some(value),
        }
    }
}
impl<T> From<Option<T>> for CoreOption<T> {
    fn from(value: Option<T>) -> Self {
        match value {
            Option::None => Self::None,
            Option::Some(value) => Self::Some(value),
        }
    }
}
