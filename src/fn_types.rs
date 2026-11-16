// Enumerate supported function pointer ABIs and arities before codec generation.
macro_rules! with_fn_types {
    ($callback:path, { $($context:tt)* }) => {
        $crate::fn_types::with_fn_types!(@collect $callback, { $($context)* }, [];
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
        );
    };
    (@collect $callback:path, { $($context:tt)* }, [$($types:tt)*]; ($($arg:ident),*), $($rest:tt)*) => {
        $crate::fn_types::with_fn_types!(@collect $callback, { $($context)* }, [
            $($types)*
            [($($arg),*)] extern "C" fn($($arg),*) -> R;
            [($($arg),*)] unsafe extern "C" fn($($arg),*) -> R;
            [($($arg),*)] extern "C-unwind" fn($($arg),*) -> R;
            [($($arg),*)] unsafe extern "C-unwind" fn($($arg),*) -> R;
            [($($arg),*)] extern "system" fn($($arg),*) -> R;
            [($($arg),*)] unsafe extern "system" fn($($arg),*) -> R;
            [($($arg),*)] extern "system-unwind" fn($($arg),*) -> R;
            [($($arg),*)] unsafe extern "system-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] extern "cdecl" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] unsafe extern "cdecl" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] extern "cdecl-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] unsafe extern "cdecl-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] extern "stdcall" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] unsafe extern "stdcall" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] extern "stdcall-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] unsafe extern "stdcall-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] extern "fastcall" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] unsafe extern "fastcall" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] extern "fastcall-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] unsafe extern "fastcall-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] extern "thiscall" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] unsafe extern "thiscall" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] extern "thiscall-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86")]
            [($($arg),*)] unsafe extern "thiscall-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86_64")]
            [($($arg),*)] extern "sysv64" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86_64")]
            [($($arg),*)] unsafe extern "sysv64" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86_64")]
            [($($arg),*)] extern "sysv64-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86_64")]
            [($($arg),*)] unsafe extern "sysv64-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86_64")]
            [($($arg),*)] extern "win64" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86_64")]
            [($($arg),*)] unsafe extern "win64" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86_64")]
            [($($arg),*)] extern "win64-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "x86_64")]
            [($($arg),*)] unsafe extern "win64-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "arm")]
            [($($arg),*)] extern "aapcs" fn($($arg),*) -> R;
            #[cfg(target_arch = "arm")]
            [($($arg),*)] unsafe extern "aapcs" fn($($arg),*) -> R;
            #[cfg(target_arch = "arm")]
            [($($arg),*)] extern "aapcs-unwind" fn($($arg),*) -> R;
            #[cfg(target_arch = "arm")]
            [($($arg),*)] unsafe extern "aapcs-unwind" fn($($arg),*) -> R;
            #[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "arm", target_arch = "aarch64"))]
            [($($arg),*)] extern "efiapi" fn($($arg),*) -> R;
            #[cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "arm", target_arch = "aarch64"))]
            [($($arg),*)] unsafe extern "efiapi" fn($($arg),*) -> R;
            ]; $($rest)*);
    };
    (@collect $callback:path, { $($context:tt)* }, [$($types:tt)*];) => {
        $callback! { $($context)* [$($types)*] }
    };
}
pub(crate) use with_fn_types;
