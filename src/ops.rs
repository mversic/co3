//! Operators

use crate::{
    CFnArg, CFnReturn, Decode, Encode, ReprC,
    stored::{EmptyStore, Store},
};

macro_rules! define_fn_trait {
    ($name:ident, $arity:tt; $( $arg_ty:ident : $carg:ident : $arg:ident : $c_ty:ident ),* $(,)?) => {
        #[doc = concat!("The _C fn_ call operator of arity ", stringify!($arity), " that accepts Rust values.")]
        pub trait $name: Copy where Option<Self>: ReprC { $(

            #[doc = concat!("The ABI type of argument `", stringify!($arg), "`.")]
            type $carg: CFnArg;)*

            /// The ABI return type.
            type Output: CFnReturn;

            /// Performs the call operation.
            ///
            /// Encodes arguments, calls the function, decodes and returns the result.
            ///
            /// # Safety
            ///
            /// The caller must uphold the safety requirements of calling `self` and of
            /// decoding its return value with [`crate::decode`].
            #[allow(clippy::too_many_arguments)]
            unsafe fn call<'d, $( $arg_ty, )* B>(self $(, $arg: $arg_ty)*) -> Option<B>
            where $(
                $arg_ty: Encode<CType = Self::$carg, Store: EmptyStore>, )*
                B: Decode<'d, CType = Self::Output, Store: EmptyStore> + 'd;

            /// Performs the call operation where some arguments contain soft references.
            ///
            /// Soft encodes arguments, calls the function, synchronizes soft updates, decodes and
            /// returns the result.
            ///
            /// # Safety
            ///
            /// The caller must uphold the safety requirements of calling `self` and of
            /// decoding its return value with [`crate::decode`].
            #[allow(clippy::too_many_arguments)]
            unsafe fn soft_call<$( $arg_ty, )* B>(self $(, $arg: $arg_ty)*) -> Option<B>
            where $(
                $arg_ty: Encode<CType = Self::$carg>, )*
                B: for<'r> Decode<'r, CType = Self::Output, Store: EmptyStore>;
        }

        impl_fn_pointer_for_all_abis!($name; $( $arg_ty : $carg : $arg : $c_ty ),*);
    };
}

macro_rules! impl_fn_pointer {
    ($trait:ident; $abi:literal; $( $arg_ty:ident : $carg:ident : $arg:ident : $c_ty:ident ),* $(,)?) => {
        impl<$( $c_ty: CFnArg, )* U: CFnReturn> $trait for unsafe extern $abi fn($( $c_ty ),*) -> U
        where Option<Self>: ReprC
        { $(
            type $carg = $c_ty; )*
            type Output = U;

            #[allow(clippy::too_many_arguments)]
            unsafe fn call<'d, $( $arg_ty, )* B>(self $(, $arg: $arg_ty)*) -> Option<B>
            where
                $( $arg_ty: Encode<CType = $c_ty, Store: EmptyStore>, )*
                B: Decode<'d, CType = U, Store: EmptyStore> + 'd,
            {
                let result = unsafe { self($(crate::encode($arg)),*) };
                unsafe { crate::decode(result) }
            }

            #[allow(clippy::too_many_arguments, non_snake_case)]
            unsafe fn soft_call<$( $arg_ty, )* B>(self $(, $arg: $arg_ty)*) -> Option<B>
            where
                $( $arg_ty: Encode<CType = $c_ty>, )*
                B: for<'r> Decode<'r, CType = U, Store: EmptyStore>,
            {
                $(let mut $arg_ty = Default::default();)*
                let result = unsafe { self($(crate::soft_encode($arg, &mut $arg_ty)),*) };
                #[allow(unused_mut)]
                let mut sync_ok = true;
                $(sync_ok &= Store::sync($arg_ty).is_some();)*
                unsafe { crate::decode(result) }.filter(|_| sync_ok)
            }
        }
    };
}

macro_rules! impl_fn_pointer_for_all_abis {
    ($trait:ident; $( $arg_ty:ident : $carg:ident : $arg:ident : $c_ty:ident ),* $(,)?) => {
        impl_fn_pointer!($trait; "C"; $( $arg_ty : $carg : $arg : $c_ty ),*);
        impl_fn_pointer!($trait; "C-unwind"; $( $arg_ty : $carg : $arg : $c_ty ),*);
        impl_fn_pointer!($trait; "system"; $( $arg_ty : $carg : $arg : $c_ty ),*);
        impl_fn_pointer!($trait; "system-unwind"; $( $arg_ty : $carg : $arg : $c_ty ),*);
        #[cfg(target_arch = "x86")]
        impl_fn_pointer!($trait; "cdecl"; $( $arg_ty : $carg : $arg : $c_ty ),*);
        #[cfg(target_arch = "x86")]
        impl_fn_pointer!($trait; "cdecl-unwind"; $( $arg_ty : $carg : $arg : $c_ty ),*);
        #[cfg(target_arch = "x86")]
        impl_fn_pointer!($trait; "stdcall"; $( $arg_ty : $carg : $arg : $c_ty ),*);
        #[cfg(target_arch = "x86")]
        impl_fn_pointer!($trait; "stdcall-unwind"; $( $arg_ty : $carg : $arg : $c_ty ),*);
        #[cfg(target_arch = "x86")]
        impl_fn_pointer!($trait; "fastcall"; $( $arg_ty : $carg : $arg : $c_ty ),*);
        #[cfg(target_arch = "x86")]
        impl_fn_pointer!($trait; "fastcall-unwind"; $( $arg_ty : $carg : $arg : $c_ty ),*);
        #[cfg(target_arch = "x86")]
        impl_fn_pointer!($trait; "thiscall"; $( $arg_ty : $carg : $arg : $c_ty ),*);
        #[cfg(target_arch = "x86")]
        impl_fn_pointer!($trait; "thiscall-unwind"; $( $arg_ty : $carg : $arg : $c_ty ),*);
        #[cfg(target_arch = "x86_64")]
        impl_fn_pointer!($trait; "sysv64"; $( $arg_ty : $carg : $arg : $c_ty ),*);
        #[cfg(target_arch = "x86_64")]
        impl_fn_pointer!($trait; "sysv64-unwind"; $( $arg_ty : $carg : $arg : $c_ty ),*);
        #[cfg(target_arch = "x86_64")]
        impl_fn_pointer!($trait; "win64"; $( $arg_ty : $carg : $arg : $c_ty ),*);
        #[cfg(target_arch = "x86_64")]
        impl_fn_pointer!($trait; "win64-unwind"; $( $arg_ty : $carg : $arg : $c_ty ),*);
        #[cfg(target_arch = "arm")]
        impl_fn_pointer!($trait; "aapcs"; $( $arg_ty : $carg : $arg : $c_ty ),*);
        #[cfg(target_arch = "arm")]
        impl_fn_pointer!($trait; "aapcs-unwind"; $( $arg_ty : $carg : $arg : $c_ty ),*);
        #[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "arm", target_arch = "aarch64"))]
        impl_fn_pointer!($trait; "efiapi"; $( $arg_ty : $carg : $arg : $c_ty ),*);
    };
}

define_fn_trait!(CFn0, 0;);
define_fn_trait!(CFn1, 1; A1:Arg1:arg1:T1);
define_fn_trait!(CFn2, 2; A1:Arg1:arg1:T1, A2:Arg2:arg2:T2);
define_fn_trait!(CFn3, 3; A1:Arg1:arg1:T1, A2:Arg2:arg2:T2, A3:Arg3:arg3:T3);
define_fn_trait!(CFn4, 4; A1:Arg1:arg1:T1, A2:Arg2:arg2:T2, A3:Arg3:arg3:T3, A4:Arg4:arg4:T4);
define_fn_trait!(CFn5, 5; A1:Arg1:arg1:T1, A2:Arg2:arg2:T2, A3:Arg3:arg3:T3, A4:Arg4:arg4:T4, A5:Arg5:arg5:T5);
define_fn_trait!(CFn6, 6; A1:Arg1:arg1:T1, A2:Arg2:arg2:T2, A3:Arg3:arg3:T3, A4:Arg4:arg4:T4, A5:Arg5:arg5:T5, A6:Arg6:arg6:T6);
define_fn_trait!(CFn7, 7; A1:Arg1:arg1:T1, A2:Arg2:arg2:T2, A3:Arg3:arg3:T3, A4:Arg4:arg4:T4, A5:Arg5:arg5:T5, A6:Arg6:arg6:T6, A7:Arg7:arg7:T7);
define_fn_trait!(CFn8, 8; A1:Arg1:arg1:T1, A2:Arg2:arg2:T2, A3:Arg3:arg3:T3, A4:Arg4:arg4:T4, A5:Arg5:arg5:T5, A6:Arg6:arg6:T6, A7:Arg7:arg7:T7, A8:Arg8:arg8:T8);
define_fn_trait!(CFn9, 9; A1:Arg1:arg1:T1, A2:Arg2:arg2:T2, A3:Arg3:arg3:T3, A4:Arg4:arg4:T4, A5:Arg5:arg5:T5, A6:Arg6:arg6:T6, A7:Arg7:arg7:T7, A8:Arg8:arg8:T8, A9:Arg9:arg9:T9);
define_fn_trait!(CFn10, 10; A1:Arg1:arg1:T1, A2:Arg2:arg2:T2, A3:Arg3:arg3:T3, A4:Arg4:arg4:T4, A5:Arg5:arg5:T5, A6:Arg6:arg6:T6, A7:Arg7:arg7:T7, A8:Arg8:arg8:T8, A9:Arg9:arg9:T9, A10:Arg10:arg10:T10);
define_fn_trait!(CFn11, 11; A1:Arg1:arg1:T1, A2:Arg2:arg2:T2, A3:Arg3:arg3:T3, A4:Arg4:arg4:T4, A5:Arg5:arg5:T5, A6:Arg6:arg6:T6, A7:Arg7:arg7:T7, A8:Arg8:arg8:T8, A9:Arg9:arg9:T9, A10:Arg10:arg10:T10, A11:Arg11:arg11:T11);
define_fn_trait!(CFn12, 12; A1:Arg1:arg1:T1, A2:Arg2:arg2:T2, A3:Arg3:arg3:T3, A4:Arg4:arg4:T4, A5:Arg5:arg5:T5, A6:Arg6:arg6:T6, A7:Arg7:arg7:T7, A8:Arg8:arg8:T8, A9:Arg9:arg9:T9, A10:Arg10:arg10:T10, A11:Arg11:arg11:T11, A12:Arg12:arg12:T12);
