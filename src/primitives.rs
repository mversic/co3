//! Logic related to the conversion of primitives to and from FFI-compatible representation

use crate::{
    CFnArg, CFnReturn, ReprC, ReprFamily,
    borrow::{Borrow, BorrowCast, ToOwned},
    reprC,
    stored::SoftEncodeOwned,
};

/// # Safety
///
/// * the type must be transmutable into an integer
/// * validity function must not return false positives
macro_rules! fieldless_enum_derive {
    ( $src:ty => $dst:ty: {$niche_val:expr}: $validity_fn:expr ) => {
        reprC! {
            unsafe impl NoDropSizedTransmuted for $src {
                type Target = $dst;

                const NICHE_VALUE: Self::CType = $niche_val;
                fn is_valid(target: &Self::Target) -> bool {
                    $validity_fn(target)
                }
            }
        }

        impl Borrow for $src {
            type Borrowed<'itm>
                = Self
            where
                Self: 'itm;

            type Owner = ();

            #[inline(always)]
            fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
            where
                Self: 'itm,
            {
                self
            }
        }

        impl<'itm> ToOwned<'itm> for $src {
            #[inline(always)]
            fn to_owned(source: Self::Borrowed<'itm>) -> Self {
                source
            }
        }
    };
}

/// # Safety
///
/// Type must be a robust #[repr(C)]
macro_rules! primitive_derive {
    ( $($primitive:ty),* $(,)? ) => { $(
        reprC! { unsafe impl SizedRobust for $primitive {} }

        unsafe impl BorrowCast for $primitive {
            type AsConst = Self;
            type AsMut = Self;
        })*
    };
}

fieldless_enum_derive! {
    char => u32: {0x110000}:
    |i: &Self::Target| char::from_u32(*i).is_some()
}
fieldless_enum_derive! {
    bool => u8: {2}:
    |i: &Self::Target| *i == 0 || *i == 1
}
fieldless_enum_derive! {
    core::cmp::Ordering => i8: {2}:
    |i: &Self::Target| *i == -1 || *i == 0 || *i == 1
}

primitive_derive! { usize, isize, u8, i8, u16, i16, u32, i32, u64, i64, u128, i128, f32, f64 }

macro_rules! impl_fn_types {
    ( $( ( $( $arg:ident ),* ) ),* $(,)? ) => {$(
        // FIXME: I'm not sure if arguments are required to be ReprC, what if fn pointer is opaque?
        // or should we create new function with argument conversion?
        unsafe impl<$($arg: CFnArg,)* R: CFnReturn> ReprC for unsafe extern "C" fn($($arg),*) -> R {}

        impl<$($arg,)* R> ReprFamily for unsafe extern "C" fn($($arg),*) -> R {
            type Kind = Self;
        }
        //impl<$($arg),*> ReprFamily for unsafe extern "C" fn($($arg),*) {
        //    type Kind = Self;
        //}

        impl<$($arg: CFnArg,)* R: CFnReturn> crate::ExternC for unsafe extern "C" fn($($arg),*) -> R {
            type CType = Self;
        }

        unsafe impl<$($arg: CFnArg,)* R: CFnReturn> crate::handle::Erase for unsafe extern "C" fn($($arg),*) -> R {
            type Erased = Self;
        }
        //impl<const AS_REF: bool, $($arg: CFnArg,)* R: CFnReturn> crate::Encode<false> for unsafe extern "C" fn($($arg),*) -> R {
        //    type Store = ();

        //    fn encode<'itm>(self, (): &mut ()) -> Self::CType where Self: 'itm {
        //        self
        //    }
        //}
        impl<$($arg: CFnArg,)* R: CFnReturn> SoftEncodeOwned for unsafe extern "C" fn($($arg),*) -> R {
            type Store = ();

            #[inline(always)]
            fn encode<'itm>(self, (): &mut ()) -> Self::CType where Self: 'itm {
                self
            }
        }
        impl<$($arg: CFnArg,)* R: CFnReturn> crate::SoftEncode for unsafe extern "C" fn($($arg),*) -> R {}

        unsafe impl<$($arg: CFnArg,)* R: CFnReturn> ReprC for Option<unsafe extern "C" fn($($arg),*) -> R> {}
        //crate::reprC! { impl<$($arg: CFnArg,)* R: CFnReturn> SizedRobust for Option<unsafe extern "C" fn($($arg),*) -> R> {} }
        )*
    }
}

impl_fn_types! {
    (),
    (A),
    (A, B),
    (A, B, C),
    (A, B, C, D),
    (A, B, C, D, E),
    (A, B, C, D, E, F),
    (A, B, C, D, E, F, G),
    (A, B, C, D, E, F, G, H),
    (A, B, C, D, E, F, G, H, I),
    (A, B, C, D, E, F, G, H, I, J),
    (A, B, C, D, E, F, G, H, I, J, K),
    (A, B, C, D, E, F, G, H, I, J, K, L),
}
