//! C-compatible representation of [`core::option::Option`].
use core::mem::MaybeUninit;

use rust_spec::{RustSpec, niche::WithoutNiche};

use crate::{
    CFnArg, CFnReturn, Decode, Encode, ExternC, ReprC,
    borrow::{Borrow, BorrowCast, BorrowCastMut, FromBorrow},
    stored::{DecodeOwned, EmptyStore, EncodeOwned},
    transmute::CheckedTransmute,
};

/// FFI-safe equivalent of [`core::option::Option`] for [`crate::ReprC`] types
#[repr(C)]
pub struct ReprCOption<T> {
    tag: u8,
    payload: MaybeUninit<T>,
}

impl<T: core::fmt::Debug> core::fmt::Debug for ReprCOption<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.tag {
            0 => f.write_str("ReprCOption::None"),
            1 => f
                .debug_tuple("ReprCOption::Some")
                .field(unsafe { self.payload.assume_init_ref() })
                .finish(),
            tag => f
                .debug_struct("ReprCOption::<invalid>")
                .field("tag", &tag)
                .finish(),
        }
    }
}

impl<T: PartialEq> PartialEq for ReprCOption<T> {
    fn eq(&self, other: &Self) -> bool {
        match (self.tag, other.tag) {
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
impl<T: PartialOrd> PartialOrd for ReprCOption<T> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
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

impl<T> ReprCOption<T> {
    pub(crate) const NICHE_VALUE: Self = Self {
        tag: 2,
        payload: MaybeUninit::zeroed(),
    };

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

    fn forward_payload<U>(self) -> ReprCOption<U> {
        let mut output = MaybeUninit::<ReprCOption<U>>::zeroed();

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

impl<T: Copy> Copy for ReprCOption<T> {}
impl<T: Copy> Clone for ReprCOption<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> From<Option<T>> for ReprCOption<T> {
    fn from(value: Option<T>) -> Self {
        match value {
            Some(value) => Self::Some(value),
            None => Self::None(),
        }
    }
}

impl<T> TryFrom<ReprCOption<T>> for Option<T> {
    type Error = ();

    fn try_from(value: ReprCOption<T>) -> Result<Self, Self::Error> {
        match value.tag {
            0 => Ok(None),
            1 => Ok(Some(unsafe { value.payload.assume_init() })),
            _ => Err(()),
        }
    }
}

unsafe impl<T: RustSpec> RustSpec for ReprCOption<T> {
    type Layout = T::Layout;
    type Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>;
    type Alignment = T::Alignment;
    type Trap = T::Trap;
    type Niche = WithoutNiche;
    type Mutability = rust_spec::mutability::Exclusive;
    type __IndirectTrap = T::__IndirectTrap;
}

unsafe impl<T: Borrow> Borrow for ReprCOption<T> {
    type Borrowed<'itm>
        = ReprCOption<T::Borrowed<'itm>>
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
                ReprCOption::Some(payload.borrow(owner))
            }
            _ => self.forward_payload(),
        }
    }
}
impl<'itm, T: FromBorrow<'itm>> FromBorrow<'itm> for ReprCOption<T> {
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

impl<T: ExternC<CType: Sized>> ExternC for ReprCOption<T> {
    type CType = ReprCOption<T::CType>;
}
unsafe impl<T: EncodeOwned<CType: Copy>> EncodeOwned for ReprCOption<T> {
    type Store = T::Store;

    #[inline(always)]
    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
    where
        Self: 'itm,
    {
        match self.tag {
            1 => {
                let payload = unsafe { self.payload.assume_init() };
                ReprCOption::Some(payload.soft_encode(store))
            }
            _ => self.forward_payload(),
        }
    }
}
unsafe impl<'d, T: DecodeOwned<'d, CType: Copy>> DecodeOwned<'d> for ReprCOption<T> {
    type Store = T::Store;

    #[inline(always)]
    unsafe fn soft_decode<'itm: 'd>(
        source: Self::CType,
        store: &'itm mut Self::Store,
    ) -> Option<Self> {
        match source.tag {
            1 => {
                let payload = unsafe { T::soft_decode(source.payload.assume_init(), store)? };
                Some(Self::Some(payload))
            }
            _ => Some(source.forward_payload()),
        }
    }
}

impl<T: Encode<CType: Copy>> Encode for ReprCOption<T> {}
impl<'d, T: Decode<'d, CType: Copy>> Decode<'d> for ReprCOption<T> {}

unsafe impl<T: CheckedTransmute<CType: Copy>> CheckedTransmute for ReprCOption<T> {
    #[inline(always)]
    unsafe fn is_valid(target: &Self::CType) -> bool {
        match target.tag {
            1 => unsafe { T::is_valid(&*target.payload.as_ptr()) },
            _ => true,
        }
    }
}

unsafe impl<T: ReprC> ReprC for ReprCOption<T> {}
unsafe impl<T: ReprC + Copy, Abi> CFnArg<Abi> for ReprCOption<T> {}
unsafe impl<T: ReprC + Copy, Abi> CFnReturn<Abi> for ReprCOption<T> {}

unsafe impl<T: BorrowCast<AsConst: Copy> + Copy> BorrowCast for ReprCOption<T> {
    type AsConst = ReprCOption<T::AsConst>;
}
unsafe impl<T: BorrowCastMut<AsMut: Copy> + Copy> BorrowCastMut for ReprCOption<T> {
    type AsMut = ReprCOption<T::AsMut>;
}

unsafe impl<T: EmptyStore> EmptyStore for Option<T> {}
