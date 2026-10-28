//! C-compatible representation of [`core::option::Option`].
use core::mem::MaybeUninit;
use core::option::Option as CoreOption;

use rust_spec::{
    RustSpec,
    mutability::{Exclusive, ExclusiveKind},
    niche::WithoutNiche,
};

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, EncodeUnchecked, ReprC, Thin,
    borrow::{Borrow, BorrowCast, BorrowCastMut, FromBorrow},
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
    type Borrowed<'a>
        = Option<T::Borrowed<'a>>
    where
        Self: 'a;
    type Owner = T::Owner;

    fn borrow<'a>(self, owner: &'a mut Self::Owner) -> Self::Borrowed<'a>
    where
        Self: 'a,
    {
        match self {
            Self::None => Option::None,
            Self::Some(value) => Option::Some(value.borrow(owner)),
        }
    }
}

impl<'a, T: FromBorrow<'a>> FromBorrow<'a> for Option<T> {
    fn from_borrow(value: Self::Borrowed<'a>) -> Self {
        match value {
            Option::None => Self::None,
            Option::Some(value) => Self::Some(T::from_borrow(value)),
        }
    }
}

impl<T: ReprC<CType: Sized>> ReprC for Option<T> {
    type CType = COption<T::CType>;
}

unsafe impl<T: EncodeMoveUnchecked> EncodeMoveUnchecked for Option<T> {
    type SoftStore = CoreOption<T::SoftStore>;

    unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
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

    unsafe fn encode_ref_unchecked(
        &'a self,
        store: &mut Self::SoftStore,
    ) -> <&'a Self as ReprC>::CType {
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

    fn encode_ref(&'a self, store: &mut Self::Store) -> <&'a Self as ReprC>::CType {
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
    type Store = T::Store;

    unsafe fn soft_decode<'a: 'd>(
        source: Self::CType,
        store: &'a mut Self::Store,
    ) -> CoreOption<Self> {
        match source.tag {
            0 => Some(Self::None),
            1 => Some(Self::Some(unsafe {
                T::soft_decode(source.payload.assume_init(), store)?
            })),
            _ => None,
        }
    }
}

impl<T: Encode> Encode for Option<T> {}
impl<T: EncodeUnchecked> EncodeUnchecked for Option<T> {}
impl<'d, T: Decode<'d>> Decode<'d> for Option<T> {}

unsafe impl<T: AbiCompatible<CType: Copy>> AbiCompatible for Option<T> {
    unsafe fn is_valid(target: &Self::CType) -> bool {
        match target.tag {
            0 => true,
            1 => unsafe { T::is_valid(target.payload.assume_init_ref()) },
            _ => false,
        }
    }
}

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

impl<T> COption<T> {
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

    fn forward_payload<U>(&self) -> COption<U> {
        let mut output = MaybeUninit::<COption<U>>::zeroed();

        unsafe {
            let output_ptr = output.as_mut_ptr();
            core::ptr::addr_of_mut!((*output_ptr).tag).write(self.tag);

            core::ptr::copy_nonoverlapping(
                self.payload.as_ptr().cast::<u8>(),
                core::ptr::addr_of_mut!((*output_ptr).payload).cast::<u8>(),
                core::cmp::min(size_of::<T>(), size_of::<U>()),
            );

            output.assume_init()
        }
    }
}

impl<T: Copy> Copy for COption<T> {}
impl<T: Copy> Clone for COption<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Default for COption<T> {
    fn default() -> Self {
        Self::None()
    }
}

unsafe impl<T: Borrow> Borrow for COption<T> {
    type Borrowed<'itm>
        = COption<T::Borrowed<'itm>>
    where
        Self: 'itm;

    type Owner = T::Owner;

    #[inline(always)]
    fn borrow<'itm>(self, owner: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        match self.tag {
            1 => {
                let payload = unsafe { self.payload.assume_init() };
                COption::Some(payload.borrow(owner))
            }
            _ => self.forward_payload(),
        }
    }
}
impl<'itm, T: FromBorrow<'itm>> FromBorrow<'itm> for COption<T> {
    #[inline(always)]
    fn from_borrow(source: Self::Borrowed<'itm>) -> Self {
        match source.tag {
            1 => {
                let payload = unsafe { source.payload.assume_init() };
                Self::Some(T::from_borrow(payload))
            }
            _ => source.forward_payload(),
        }
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

impl<T: ReprC<CType: Sized>> ReprC for COption<T> {
    type CType = COption<T::CType>;
}
unsafe impl<T: EncodeMove> EncodeMove for COption<T> {
    type Store = CoreOption<T::Store>;

    #[inline(always)]
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        match self.tag {
            1 => {
                let payload = unsafe { self.payload.assume_init() };
                COption::Some(payload.soft_encode(store.insert(Default::default())))
            }
            _ => self.forward_payload(),
        }
    }
}
unsafe impl<T: EncodeMoveUnchecked> EncodeMoveUnchecked for COption<T> {
    type SoftStore = CoreOption<T::SoftStore>;

    unsafe fn soft_encode_unchecked(self, store: &mut Self::SoftStore) -> Self::CType {
        match self.tag {
            1 => COption::Some(unsafe {
                self.payload
                    .assume_init()
                    .soft_encode_unchecked(store.insert(Default::default()))
            }),
            _ => {
                *store = None;
                self.forward_payload()
            }
        }
    }
}
unsafe impl<'a, T: ReprC<CType: Copy>, __Interior: ExclusiveKind> EncodeRef<'a> for COption<T>
where
    Self: RustSpec<Mutability = Exclusive<__Interior>, Size: Thin> + 'a,
    &'a T: EncodeMove<CType = crate::reference::CRef<T::CType>>,
{
    type Store = RefSizedEncodeStore<Self, CoreOption<<&'a T as EncodeMove>::Store>>;

    fn encode_ref(&'a self, store: &mut Self::Store) -> <&'a Self as ReprC>::CType {
        store.encode(|fields| {
            let store = fields;

            match self.tag {
                1 => {
                    let payload = unsafe { self.payload.assume_init_ref() };

                    COption::Some(crate::stored::encode_ref_field(
                        payload,
                        store.insert(Default::default()),
                    ))
                }
                _ => {
                    *store = None;
                    self.forward_payload()
                }
            }
        })
    }
}
unsafe impl<'a, T: ReprC<CType: Copy>, __Interior: ExclusiveKind> EncodeRefUnchecked<'a>
    for COption<T>
where
    Self: RustSpec<Mutability = Exclusive<__Interior>, Size: Thin> + 'a,
    &'a T: EncodeMoveUnchecked<CType = crate::reference::CRef<T::CType>>,
{
    type SoftStore =
        RefSizedEncodeStore<Self, CoreOption<<&'a T as EncodeMoveUnchecked>::SoftStore>>;

    unsafe fn encode_ref_unchecked(
        &'a self,
        store: &mut Self::SoftStore,
    ) -> <&'a Self as ReprC>::CType {
        store.encode(|fields| {
            let store = fields;

            match self.tag {
                1 => COption::Some(unsafe {
                    crate::stored::encode_ref_field_unchecked(
                        self.payload.assume_init_ref(),
                        store.insert(Default::default()),
                    )
                }),
                _ => {
                    *store = None;
                    self.forward_payload()
                }
            }
        })
    }
}

unsafe impl<'d, T: DecodeOwned<'d>> DecodeOwned<'d> for COption<T> {
    type Store = T::Store;

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> CoreOption<Self> {
        match source.tag {
            1 => {
                let payload = unsafe { T::soft_decode(source.payload.assume_init(), store)? };
                Some(Self::Some(payload))
            }
            _ => Some(source.forward_payload()),
        }
    }

    #[inline(always)]
    unsafe fn soft_decode_unchecked<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Self {
        match source.tag {
            1 => {
                Self::Some(unsafe { T::soft_decode_unchecked(source.payload.assume_init(), store) })
            }
            _ => source.forward_payload(),
        }
    }
}

impl<T: Encode> Encode for COption<T> {}
impl<T: EncodeUnchecked> EncodeUnchecked for COption<T> {}
impl<'d, T: Decode<'d>> Decode<'d> for COption<T> {}

unsafe impl<T: AbiCompatible<CType: Copy>> AbiCompatible for COption<T> {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        match target.tag {
            1 => unsafe { T::is_valid(&*target.payload.as_ptr()) },
            _ => true,
        }
    }
}

unsafe impl<T: CType> CType for COption<T> {}
unsafe impl<T: CType> CFnArg for COption<T> {}
unsafe impl<T: CType> CFnReturn for COption<T> {}

unsafe impl<T: BorrowCast<AsConst: Copy>> BorrowCast for COption<T> {
    type AsConst = COption<T::AsConst>;
}
unsafe impl<T: BorrowCastMut<AsMut: Copy>> BorrowCastMut for COption<T> {
    type AsMut = COption<T::AsMut>;
}

unsafe impl<T: EmptyStore> EmptyStore for CoreOption<T> {}
