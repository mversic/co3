//! Logic related to the conversion of primitives to and from FFI-compatible representation

use crate::ffi_type;

#[cfg(target_family = "wasm")]
mod wasm {
    use alloc::{boxed::Box, vec::Vec};

    use crate::{
        ir::{Robust, Transparent},
        out_ptr::{FfiOutPtr, FfiOutPtrRead, FfiOutPtrWrite},
        CTypeConvert, FfiReturn, FfiType, FfiWrapperType, Result,
    };

    /// Marker for an integer primitive type that is not recognized by the `WebAssembly`.
    /// This struct is meant only to be used internally, i.e. there are no constructors.
    // NOTE: There are no blanket impls because it's meant to be used only on a specific set of types
    #[derive(Debug, Clone, Copy)]
    pub enum NonWasmIntPrimitive {}

    impl crate::ir::IrTypeFamily for NonWasmIntPrimitive {
        type Ref<'itm> = Transparent;
        type RefMut<'itm> = Transparent;
        type RefSlice<'itm> = &'itm [Robust];
        type RefMutSlice<'itm> = &'itm mut [Robust];
        type Box = Box<Robust>;
        type BoxedSlice = Box<[Robust]>;
        type Vec = Vec<Robust>;
        type Arr<const N: usize> = Robust;
    }

    macro_rules! wasm_repr_impls {
        ( $($src:ty => $dst:ty),+ ) => {$(
            // FIXME: Should it be ReprC?
            // SAFETY: Even if it is not used in `wasm` API it is still a `ReprC` type
            unsafe impl $crate::ReprC for $src {}

            impl $crate::option::Niche<'_> for $src {
                const NICHE_VALUE: $dst = <$dst>::MAX;
            }

            // SAFETY: Conversion of non wasm primitive doesn't use store
            unsafe impl $crate::out_ptr::NonLocal for $src {}

            // SAFETY: Idempotent transmute is always infallible
            unsafe impl $crate::transmute::InfallibleTransmute for $src {}

            // SAFETY: Transmute relation is transitive
            unsafe impl $crate::transmute::Transmute for $src {
                type Target = $dst;

                fn is_valid(target: &Self::Target) -> bool {
                    (<$src>::MIN as $dst..<$src>::MAX as $dst).contains(target)
                }
            }

            impl $crate::ir::Ir for $src {
                type Type = NonWasmIntPrimitive;
            }

            impl FfiType for $src {
                type ReprC = $dst;
            }
            impl FfiOutPtr for $src {
                type OutPtr = $src;
            }

            impl CTypeConvert<'_, NonWasmIntPrimitive, $dst> for $src {
                type RustStore = ();
                type FfiStore = ();

                fn into_repr_c(self, _: &mut ()) -> $dst {
                    self as $dst
                }
                unsafe fn try_from_repr_c(source: $dst, _: &mut ()) -> Result<Self> {
                    <$src>::try_from(source).or(Err(FfiReturn::ConversionFailed))
                }
            }

            impl FfiOutPtrRead for $src {
                unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
                    Ok(out_ptr)
                }
            }
            impl FfiOutPtrWrite for $src {
                unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
                    out_ptr.write(self)
                }
            }

            impl FfiWrapperType for $src {
                type InputType = Self;
                type ReturnType = Self;
            }
            impl $crate::WrapperTypeOf<Self> for $src {
                type Type = Self;
            })+
        };
    }

    wasm_repr_impls! {u8 => u32, i8 => i32, u16 => u32, i16 => i32}
}

/// # Safety
///
/// * the type must be transmutable into an integer
/// * validity function must not return false positives
macro_rules! fieldless_enum_derive {
    ( $src:ty => $dst:ty: {$niche_val:expr}: $validity_fn:expr ) => {
        $crate::ffi_type! {
            unsafe impl Transparent for $src {
                type Target = $dst;

                validation_fn={$validity_fn},
                niche_value=$niche_val
            }
        }

        impl $crate::WrapperTypeOf<$src> for $dst {
            type Type = $src;
        }
    };
}

/// # Safety
///
/// Type must be a robust #[repr(C)]
macro_rules! primitive_derive {
    ( $($primitive:ty),* $(,)? ) => { $(
        unsafe impl $crate::ReprC for $primitive {}
        ffi_type! { impl Robust for $primitive {} } )*
    };
}

fieldless_enum_derive! {
    bool => u8: {2}:
    |i: &u8| *i == 0 || *i == 1
}
fieldless_enum_derive! {
    core::cmp::Ordering => i8: {2}:
    |i: &i8| *i == -1 || *i == 0 || *i == 1
}

primitive_derive! { u32, i32, u64, i64 }
#[cfg(not(target_family = "wasm"))]
primitive_derive! { u8, i8, u16, i16 }

// FIXME: Rust has taken care of 128 bit representation in FFI
macro_rules! int128_derive {
    ($($src:ty => $dst:ident),+$(,)?) => {$(
        /// Ffi-safe representation of [`u128`]
        #[doc = concat!(" Ffi-safe representation of [", stringify!($src), "]")]
        #[derive(Clone, Copy, Debug, Default)]
        #[repr(transparent)]
        pub struct $dst($crate::FfiTuple2<u64, u64>);

        // SAFETY: Transparent to `FfiTuple<u64, u64>` which is `$crate::ReprC`
        unsafe impl $crate::ReprC for $dst where $crate::FfiTuple2<u64, u64>: $crate::ReprC {}

        impl $crate::Cloned for $src {}

        // SAFETY: `u128/i128` doesn't use local store during conversion
        unsafe impl $crate::out_ptr::NonLocal for $src {}

        impl $crate::ir::Ir for $src {
            type Type = Self;
        }

        impl $crate::FfiType for $src {
            type ReprC = $dst;
        }

        impl $crate::out_ptr::FfiOutPtr for $src {
            type OutPtr = Self::ReprC;
        }

        impl $crate::repr_c::CTypeConvert<'_, Self, $dst> for $src {
            type RustStore = ();
            type FfiStore = ();

            fn into_repr_c(self, _: &mut Self::RustStore) -> $dst {
                self.into()
            }

            // SAFETY: calling this function is safe since no pointers involved in conversion
            unsafe fn try_from_repr_c(value: $dst, _: &mut Self::FfiStore) -> $crate::Result<Self> {
                Ok(value.into())
            }
        }

        impl $crate::out_ptr::FfiOutPtrWrite for $src {
            unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
                $crate::repr_c::write_non_local::<_, Self>(self, out_ptr);
            }
        }

        impl $crate::out_ptr::FfiOutPtrRead for $src {
            unsafe fn try_read_out(out_ptr: Self::OutPtr) -> $crate::Result<Self> {
                $crate::repr_c::read_non_local::<Self, Self>(out_ptr)
            }
        }

        impl $crate::FfiWrapperType for $src {
            type InputType = Self;
            type ReturnType = Self;
        }
    )*};
}

// Ffi-safe u128/i128 conversions
int128_derive! { u128 => FfiU128, i128 => FfiI128 }

impl From<u128> for FfiU128 {
    // Truncation is done on purpose
    #[allow(clippy::cast_possible_truncation)]
    fn from(value: u128) -> Self {
        let lo = value as u64;
        let hi = (value >> 64) as u64;
        FfiU128(crate::FfiTuple2(hi, lo))
    }
}

impl From<FfiU128> for u128 {
    // Truncation is done on purpose
    #[allow(clippy::cast_possible_truncation)]
    fn from(FfiU128(crate::FfiTuple2(hi, lo)): FfiU128) -> Self {
        (u128::from(hi) << 64) | u128::from(lo)
    }
}

impl From<i128> for FfiI128 {
    #[allow(clippy::cast_sign_loss)] // Intended behavior
    fn from(value: i128) -> Self {
        FfiI128(FfiU128::from(value as u128).0)
    }
}

impl From<FfiI128> for i128 {
    #[allow(clippy::cast_possible_wrap)] // Intended behavior
    fn from(value: FfiI128) -> Self {
        u128::from(FfiU128(value.0)) as i128
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_u128() {
        let values = [
            u128::MAX,
            u128::from(u64::MAX),
            u128::from(u32::MAX),
            u128::from(u16::MAX),
            u128::from(u8::MAX),
            0,
        ];

        for value in values {
            assert_eq!(value, FfiU128::from(value).into());
        }
    }

    #[test]
    fn conversion_i128() {
        let values = [
            i128::MAX,
            i128::from(i64::MAX),
            i128::from(i32::MAX),
            i128::from(i16::MAX),
            i128::from(i8::MAX),
            0,
            i128::from(i8::MIN),
            i128::from(i16::MIN),
            i128::from(i32::MIN),
            i128::from(i64::MIN),
            i128::MIN,
        ];

        for value in values {
            assert_eq!(value, FfiI128::from(value).into());
        }
    }
}
