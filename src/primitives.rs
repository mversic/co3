//! Logic related to the conversion of primitives to and from FFI-compatible representation

use crate::reprC;

//#[cfg(target_family = "wasm")]
//mod wasm {
//    use alloc::{boxed::Box, vec::Vec};
//
//    use crate::{
//        Decode, Encode, ExternC,
//        ir::{ReprFamily, Robust, Transmuted},
//        out_ptr::{OutPtr, OutPtrRead, OutPtrWrite},
//    };
//
//    /// Marker for an integer primitive type that is not recognized by the `WebAssembly`.
//    /// This struct is meant only to be used internally, i.e. there are no constructors.
//    // NOTE: There are no blanket impls because it's meant to be used only on a specific set of types
//    #[derive(Debug, Clone, Copy)]
//    pub enum NonWasmIntPrimitive {}
//
//    impl<R> ReprFamily for &R
//    where
//        R: ReprFamily<Type = NonWasmIntPrimitive>,
//    {
//        type Kind = Transparent;
//    }
//    impl<R> ReprFamily for &mut R
//    where
//        R: ReprFamily<Type = NonWasmIntPrimitive>,
//    {
//        type Kind = Transparent;
//    }
//    impl<'itm, R> ReprFamily for &'itm [R]
//    where
//        R: ReprFamily<Type = NonWasmIntPrimitive>,
//    {
//        type Kind = &'itm [Robust];
//    }
//    impl<'itm, R> ReprFamily for &'itm mut [R]
//    where
//        R: ReprFamily<Type = NonWasmIntPrimitive>,
//    {
//        type Kind = &'itm mut [Transparent];
//    }
//    #[cfg(feature = "owned-as-ref")]
//    impl<R> ReprFamily for Box<R>
//    where
//        R: ReprFamily<Type = NonWasmIntPrimitive>,
//    {
//        type Kind = Box<Robust>;
//    }
//    #[cfg(feature = "owned-as-ref")]
//    impl<R> ReprFamily for Box<[R]>
//    where
//        R: ReprFamily<Type = NonWasmIntPrimitive>,
//    {
//        type Kind = Box<[Robust]>;
//    }
//    #[cfg(feature = "owned-as-ref")]
//    impl<R> ReprFamily for Vec<R>
//    where
//        R: ReprFamily<Type = NonWasmIntPrimitive>,
//    {
//        type Kind = Vec<Robust>;
//    }
//    // FIXME: Check comment in `impl IrReprFamily for Robust`
//    // This should be just: type `Arr<const N: usize> = Robust`;
//    impl<R, const N: usize> ReprFamily for [R; N]
//    where
//        R: ReprFamily<Type = NonWasmIntPrimitive>,
//    {
//        type Kind = Robust;
//    }
//
//    macro_rules! wasm_repr_impls {
//        ( $($src:ty => $dst:ty),+ ) => {$(
//            // FIXME: Should it be ReprC?
//            // SAFETY: Even if it is not used in `wasm` API it is still a `ReprC` type
//            unsafe impl $crate::ReprC for $src {}
//
//            impl $crate::niche::Niche for $src {
//                const NICHE_VALUE: $dst = <$dst>::MAX;
//            }
//
//            unsafe impl<'a> $crate::transmute::CheckedTransmute for &'a $src {
//                type Target = &'a $dst;
//
//                fn is_valid(target: &Self::Target) -> bool {
//                    unimplemented!()
//                }
//            }
//            unsafe impl<'a> $crate::transmute::CheckedTransmute for &'a mut $src {
//                type Target = &'a mut $dst;
//
//                fn is_valid(target: &Self::Target) -> bool {
//                    unimplemented!()
//                }
//            }
//
//            unsafe impl Encodable for &$src {}
//
//            impl $crate::ir::ReprFamily for $src {
//                type Kind = NonWasmIntPrimitive;
//            }
//
//            impl ExternC for $src {
//                type CType = $dst;
//            }
//            impl OutPtr for $src {
//                type OutPtr = $src;
//            }
//
//            impl Encode for $src {
//                type Store = ();
//
//                fn encode<'itm>(self, (): &mut ()) -> Self::CType where Self: 'itm {
//                    self as $dst
//                }
//            }
//
//            impl Decode<'_> for $src {
//                type Store = ();
//
//                unsafe fn decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
//                    <$src>::try_from(source).ok()
//                }
//            }
//
//            impl OutPtrRead for $src {
//                unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
//                    Some(out_ptr)
//                }
//            }
//            impl OutPtrWrite for $src {
//                unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
//                    unsafe {out_ptr.write(self)}
//                }
//            })+
//        };
//    }
//
//    wasm_repr_impls! {u8 => u32, i8 => i32, u16 => u32, i16 => i32}
//}

/// # Safety
///
/// * the type must be transmutable into an integer
/// * validity function must not return false positives
macro_rules! fieldless_enum_derive {
    ( $src:ty => $dst:ty: {$niche_val:expr}: $validity_fn:expr ) => {
        reprC! {
            unsafe impl Transparent for $src {
                type Target = $dst;

                const NICHE_VALUE: Self::CType = $niche_val;
                fn is_valid(target: &Self::Target) -> bool {
                    $validity_fn(target)
                }
            }
        }
    };
}

/// # Safety
///
/// Type must be a robust #[repr(C)]
macro_rules! primitive_derive {
    ( $($primitive:ty),* $(,)? ) => { $(
        reprC! { unsafe impl Robust for $primitive {} } )*
    };
}

fieldless_enum_derive! {
    char => <u32 as crate::ExternC>::CType: {0x110000}:
    |i: &Self::Target| char::from_u32(*i).is_some()
}
fieldless_enum_derive! {
    bool => <u8 as crate::ExternC>::CType: {2}:
    |i: &Self::Target| *i == 0 || *i == 1
}
fieldless_enum_derive! {
    core::cmp::Ordering => <i8 as crate::ExternC>::CType: {2}:
    |i: &Self::Target| *i == -1 || *i == 0 || *i == 1
}

primitive_derive! { u32, i32, u64, i64, u128, i128, f32, f64 }
primitive_derive! { u8, i8, u16, i16 }
