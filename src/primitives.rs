//! Logic related to the conversion of primitives to and from FFI-compatible representation

#[cfg(target_family = "wasm")]
mod wasm {
    use alloc::{boxed::Box, vec::Vec};

    use crate::{
        ExternC, FfiConvert, FfiReturn, FfiWrapperType, Result,
        ir::{Robust, Transparent, Ir},
        out_ptr::{OutPtr, OutPtrRead, OutPtrWrite},
    };

    /// Marker for an integer primitive type that is not recognized by the `WebAssembly`.
    /// This struct is meant only to be used internally, i.e. there are no constructors.
    // NOTE: There are no blanket impls because it's meant to be used only on a specific set of types
    #[derive(Debug, Clone, Copy)]
    pub enum NonWasmIntPrimitive {}

    impl<R> Ir for &R where R: Ir<Type = NonWasmIntPrimitive> {
        type Type = Transparent;
    }
    impl<R> Ir for &mut R where R: Ir<Type = NonWasmIntPrimitive> {
        type Type = Transparent;
    }
    impl<'itm, R> Ir for &'itm [R] where R: Ir<Type = NonWasmIntPrimitive> {
        type Type = &'itm [Robust];
    }
    impl<'itm, R> Ir for &'itm mut [R] where R: Ir<Type = NonWasmIntPrimitive> {
        type Type = &'itm mut [Robust];
    }
    impl<R> Ir for Box<R> where R: Ir<Type = NonWasmIntPrimitive> {
        type Type = Box<Robust>;
    }
    impl<R> Ir for Box<[R]> where R: Ir<Type = NonWasmIntPrimitive> {
        type Type = Box<[Robust]>;
    }
    impl<R> Ir for Vec<R> where R: Ir<Type = NonWasmIntPrimitive> {
        type Type = Vec<Robust>;
    }
    impl<R, const N: usize> Ir for [R; N] where R: Ir<Type = NonWasmIntPrimitive> {
        type Type = Robust;
    }

    macro_rules! wasm_repr_impls {
        ( $($src:ty => $dst:ty),+ ) => {$(
            // FIXME: Should it be ReprC?
            // SAFETY: Even if it is not used in `wasm` API it is still a `ReprC` type
            unsafe impl $crate::ReprC for $src {}

            impl $crate::option::Niche for $src {
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

            impl ExternC for $src {
                type CType = $dst;
            }
            impl OutPtr for $src {
                type OutPtr = $src;
            }

            impl FfiConvert<'_> for $src {
                type RustStore = ();
                type FfiStore = ();

                fn encode(self, _: &mut ()) -> Self::CType {
                    self as $dst
                }
                unsafe fn decode(source: Self::CType, _: &mut ()) -> Result<Self> {
                    <$src>::try_from(source).or(Err(FfiReturn::ConversionFailed))
                }
            }

            impl OutPtrRead for $src {
                unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Result<Self> {
                    Ok(out_ptr)
                }
            }
            impl OutPtrWrite for $src {
                unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
                    unsafe {out_ptr.write(self)}
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
        $crate::mineral! {
            unsafe impl Transparent for $src {
                type Target = $dst;

                const NICHE_VALUE: Self::CType = $niche_val;
                fn is_valid(target: &Self::Target) -> bool {
                    $validity_fn(target)
                }
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
        $crate::mineral! { impl Robust for $primitive {} } )*
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

primitive_derive! { u32, i32, u64, i64, u128, i128 }
#[cfg(not(target_family = "wasm"))]
primitive_derive! { u8, i8, u16, i16 }
