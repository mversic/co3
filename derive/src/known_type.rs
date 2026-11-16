//! Syntactic built-ins whose identities are checked by `type_checks`.
use syn::{GenericArgument, PathArguments, Type, TypePath, parse_quote};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Primitive,
    NonZeroInt,
    Str,
    CStr,
    Option,
    Vec,
    Result,
    NonZero,
    Box,
    ManuallyDrop,
    Cell,
    UnsafeCell,
    MaybeUninit,
    NonNull,
    PhantomData,
}

pub(crate) struct KnownType<'a> {
    pub kind: Kind,
    pub inner: Option<&'a Type>,
    pub arity: usize,
    pub canonical: Type,
    pub alternate: Option<Type>,
    pub canonical_name: String,
}

impl KnownType<'_> {
    pub fn is_sized(&self) -> bool {
        !matches!(self.kind, Kind::Str | Kind::CStr) && !self.is_transparent_wrapper()
    }

    pub fn is_transparent_wrapper(&self) -> bool {
        matches!(
            self.kind,
            Kind::ManuallyDrop | Kind::Cell | Kind::UnsafeCell
        )
    }
}

pub(crate) fn recognize(ty: &Type) -> Option<KnownType<'_>> {
    let Type::Path(path) = ty else { return None };
    recognize_path(path)
}

pub(crate) fn recognize_path(path: &TypePath) -> Option<KnownType<'_>> {
    if path.qself.is_some() {
        return None;
    }
    let segment = path.path.segments.last()?;
    let name = segment.ident.to_string();
    let ident = &segment.ident;
    let (kind, canonical, canonical_name, arity) = match name.as_str() {
        "bool" | "char" | "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "i8" | "i16"
        | "i32" | "i64" | "i128" | "isize" | "f32" | "f64" => (
            Kind::Primitive,
            parse_quote!(core::primitive::#ident),
            format!("core::primitive::{name}"),
            0,
        ),
        "c_char" | "c_schar" | "c_uchar" | "c_short" | "c_ushort" | "c_int" | "c_uint"
        | "c_long" | "c_ulong" | "c_longlong" | "c_ulonglong" | "c_float" | "c_double" => (
            Kind::Primitive,
            parse_quote!(co3::ffi::#ident),
            format!("co3::ffi::{name}"),
            0,
        ),
        "NonZeroU8" | "NonZeroU16" | "NonZeroU32" | "NonZeroU64" | "NonZeroU128"
        | "NonZeroUsize" | "NonZeroI8" | "NonZeroI16" | "NonZeroI32" | "NonZeroI64"
        | "NonZeroI128" | "NonZeroIsize" => (
            Kind::NonZeroInt,
            parse_quote!(core::num::#ident),
            format!("core::num::{name}"),
            0,
        ),
        "str" => (
            Kind::Str,
            parse_quote!(core::primitive::str),
            "core::primitive::str".into(),
            0,
        ),
        "CStr" => (
            Kind::CStr,
            parse_quote!(core::ffi::CStr),
            "core::ffi::CStr".into(),
            0,
        ),
        "Option" => (
            Kind::Option,
            parse_quote!(core::option::Option<()>),
            "core::option::Option".into(),
            1,
        ),
        "Vec" => (
            Kind::Vec,
            parse_quote!(alloc::vec::Vec<()>),
            "alloc::vec::Vec".into(),
            1,
        ),
        "Result" => (
            Kind::Result,
            parse_quote!(core::result::Result<(), ()>),
            "core::result::Result".into(),
            2,
        ),
        "NonZero" => (
            Kind::NonZero,
            parse_quote!(core::num::NonZero<u8>),
            "core::num::NonZero".into(),
            1,
        ),
        "Box" => (
            Kind::Box,
            parse_quote!(co3::boxed::Box<()>),
            "co3::boxed::Box".into(),
            1,
        ),
        "ManuallyDrop" => (
            Kind::ManuallyDrop,
            parse_quote!(core::mem::ManuallyDrop<()>),
            "core::mem::ManuallyDrop".into(),
            1,
        ),
        "Cell" => (
            Kind::Cell,
            parse_quote!(core::cell::Cell<()>),
            "core::cell::Cell".into(),
            1,
        ),
        "UnsafeCell" => (
            Kind::UnsafeCell,
            parse_quote!(core::cell::UnsafeCell<()>),
            "core::cell::UnsafeCell".into(),
            1,
        ),
        "MaybeUninit" => (
            Kind::MaybeUninit,
            parse_quote!(core::mem::MaybeUninit<()>),
            "core::mem::MaybeUninit".into(),
            1,
        ),
        "NonNull" => (
            Kind::NonNull,
            parse_quote!(core::ptr::NonNull<()>),
            "core::ptr::NonNull".into(),
            1,
        ),
        "PhantomData" => (
            Kind::PhantomData,
            parse_quote!(core::marker::PhantomData<()>),
            "core::marker::PhantomData".into(),
            1,
        ),
        _ => return None,
    };
    let modules: &[&str] = match kind {
        Kind::Primitive if name.starts_with("c_") => {
            &["core::ffi", "std::ffi", "std::os::raw", "co3::ffi"]
        }
        Kind::Primitive | Kind::Str => &["core::primitive", "std::primitive"],
        Kind::NonZeroInt => &["core::num", "std::num"],
        Kind::CStr => &["core::ffi", "std::ffi"],
        Kind::Option => &["core::option", "std::option", "co3::option"],
        Kind::Vec => &["alloc::vec", "std::vec", "co3::vec"],
        Kind::Result => &["core::result", "std::result", "co3::result"],
        Kind::NonZero => &["core::num", "std::num", "co3::num"],
        Kind::Box => &["alloc::boxed", "std::boxed", "co3::boxed"],
        Kind::MaybeUninit | Kind::ManuallyDrop => &["core::mem", "std::mem"],
        Kind::Cell | Kind::UnsafeCell => &["core::cell", "std::cell"],
        Kind::NonNull => &["core::ptr", "std::ptr"],
        Kind::PhantomData => &["core::marker", "std::marker"],
    };
    if !crate::utils::builtin_path(&path.path, &name, modules) {
        return None;
    }
    let inner = if arity > 0 {
        let PathArguments::AngleBracketed(args) = &segment.arguments else {
            return None;
        };
        if args.args.len() != arity {
            return None;
        }
        if !args
            .args
            .iter()
            .all(|arg| matches!(arg, GenericArgument::Type(_)))
        {
            return None;
        }
        if arity == 1 {
            let GenericArgument::Type(inner) = args.args.first()? else {
                unreachable!()
            };
            Some(inner)
        } else {
            None
        }
    } else {
        if !segment.arguments.is_empty() {
            return None;
        }
        None
    };
    Some(KnownType {
        kind,
        inner,
        arity,
        canonical,
        alternate: match kind {
            Kind::Option => Some(parse_quote!(co3::option::Option<()>)),
            Kind::Vec => Some(parse_quote!(co3::vec::Vec<()>)),
            Kind::Result => Some(parse_quote!(co3::result::Result<(), ()>)),
            Kind::NonZero => Some(parse_quote!(co3::num::NonZero<u8>)),
            _ => None,
        },
        canonical_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maybe_uninit_is_sized_and_checks_core_identity() {
        for ty in [
            parse_quote!(MaybeUninit<T>),
            parse_quote!(core::mem::MaybeUninit<core::num::NonZeroU32>),
        ] {
            let known = recognize(&ty).unwrap();
            assert_eq!(known.kind, Kind::MaybeUninit);
            assert!(known.is_sized());
            assert_eq!(known.canonical, parse_quote!(core::mem::MaybeUninit<()>));
        }
        assert!(recognize(&parse_quote!(<T as Trait>::MaybeUninit<u32>)).is_none());
    }

    #[test]
    fn box_sizing_and_identity_share_recognition() {
        for ty in [
            parse_quote!(Box<[u8]>),
            parse_quote!(alloc::boxed::Box<(u32,)>),
        ] {
            let known = recognize(&ty).unwrap();
            assert_eq!(known.kind, Kind::Box);
            assert!(known.is_sized());
            assert_eq!(known.canonical, parse_quote!(co3::boxed::Box<()>));
            assert!(known.inner.is_some());
        }
        for ty in [
            parse_quote!(<T as Trait>::Box<u8>),
            parse_quote!(Box<u8, Allocator>),
            parse_quote!(Box<'a>),
            parse_quote!(Alias),
        ] {
            assert!(recognize(&ty).is_none());
        }
    }
}
