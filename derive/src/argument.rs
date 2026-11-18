//! Argument decisions shared by ABI lowering and conversion glue.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    FnArg, Ident, Type,
    parse::{Parse, ParseStream},
    parse_quote,
    punctuated::Punctuated,
};

use crate::{
    ffi_fn,
    utils::{cfg_attrs, co3_path, receiver_ty, soft_for_arg},
};

pub(crate) struct Argument<'a> {
    pub(crate) attrs: &'a [syn::Attribute],
    pub(crate) pat: syn::Pat,
    pub(crate) ty: Type,
    pub(crate) kind: ValueKind,
    by_val: bool,
    pub(crate) soft: bool,
    pub(crate) identity: bool,
    pub(crate) unpack: Unpack,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ValueKind {
    /// Ordinary value conversion without an ownership marker; not a Rust Copy bound.
    Plain,
    /// Borrowed conversion by default, or moved conversion when by_val is requested.
    RequiresMove,
}

impl ValueKind {
    pub(crate) fn for_type(ty: &Type) -> Self {
        if ffi_fn::is_implicitly_by_value(ty) {
            Self::Plain
        } else {
            Self::RequiresMove
        }
    }
}

pub(crate) enum Unpack {
    None,
    Single(Box<Type>),
    Pair(Box<UnpackPair>),
}

/// Parsed before type substitution; inference is resolved separately for each
/// signature stage. Validation and generation use the same attribute grammar.
pub(crate) enum UnpackSyntax {
    None,
    Single(Box<Type>),
    Pair(Box<[UnpackPart; 2]>),
}

pub(crate) struct UnpackPart {
    logical: Type,
    abi: Option<Type>,
}

impl Parse for UnpackPart {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let logical = input.parse()?;
        let abi = if input.peek(syn::Token![=>]) {
            input.parse::<syn::Token![=>]>()?;
            let abi: Type = input.parse()?;
            if matches!(abi, Type::Infer(_)) {
                return Err(syn::Error::new_spanned(
                    abi,
                    "the ABI type after `=>` cannot be `_`",
                ));
            }
            Some(abi)
        } else {
            None
        };
        Ok(Self { logical, abi })
    }
}

pub(crate) struct UnpackPair {
    pub(crate) first: ResolvedPart,
    pub(crate) second: ResolvedPart,
    pub(crate) inferred_source: Option<Type>,
}

pub(crate) struct ResolvedPart {
    pub(crate) source: Type,
    /// The ABI part expected by Pack2/Unpack2 before any explicit erasure.
    pub(crate) logical: Type,
    pub(crate) abi: Type,
    pub(crate) erased: bool,
}

impl<'a> Argument<'a> {
    pub(crate) fn new(input: &'a FnArg) -> Self {
        let (attrs, pat, ty) = match input {
            FnArg::Typed(arg) => (&arg.attrs, (*arg.pat).clone(), (*arg.ty).clone()),
            FnArg::Receiver(receiver) => (
                &receiver.attrs,
                parse_quote!(__co3_self),
                receiver_ty(receiver),
            ),
        };
        Self::from_parts(attrs, pat, ty)
    }

    fn from_parts(attrs: &'a [syn::Attribute], pat: syn::Pat, ty: Type) -> Self {
        let kind = ValueKind::for_type(&ty);
        let unpack = Unpack::resolve(attrs, &ty).expect("validated #[unpack] attribute");
        Self {
            attrs,
            pat,
            kind,
            by_val: attrs.iter().any(ffi_fn::is_by_val_attr),
            soft: soft_for_arg(attrs),
            identity: ffi_fn::is_identity_primitive(&ty),
            ty,
            unpack,
        }
    }

    pub(crate) fn name(&self) -> &Ident {
        ffi_fn::item_fn_input_ident(&self.pat)
    }

    pub(crate) fn is_receiver(&self) -> bool {
        matches!(&self.pat, syn::Pat::Ident(pat) if pat.ident == "__co3_self")
    }

    pub(crate) fn with_type(&self, ty: Type) -> Self {
        Self::from_parts(self.attrs, self.pat.clone(), ty)
    }

    pub(crate) fn cfg(&self) -> impl Iterator<Item = &syn::Attribute> {
        cfg_attrs(self.attrs)
    }

    pub(crate) fn is_borrowed(&self) -> bool {
        self.kind == ValueKind::RequiresMove && !self.by_val
    }

    /// Import conversion and synchronization must agree on whether a store exists.
    pub(crate) fn needs_encode_store(&self) -> bool {
        !self.identity && self.soft
    }

    pub(crate) fn conversion_ty(&self) -> Type {
        self.ty.clone()
    }

    pub(crate) fn unpack_ty(&self) -> Type {
        if let Unpack::Pair(pair) = &self.unpack
            && let Some(ty) = &pair.inferred_source
        {
            return ty.clone();
        }
        self.ty.clone()
    }

    pub(crate) fn abi_ty(&self) -> Type {
        if self.identity {
            return self.ty.clone();
        }
        let ty = &self.ty;
        if self.is_borrowed() {
            let co3 = co3_path();
            parse_quote!(#co3::borrow::Borrowed< <#ty as #co3::ReprC>::CType >)
        } else {
            parse_quote!(<#ty as co3::ReprC>::CType)
        }
    }

    pub(crate) fn forwarded(&self) -> TokenStream {
        let name = self.name();
        let cfg = self.cfg().collect::<Vec<_>>();
        if matches!(self.unpack, Unpack::Pair(_)) {
            let (data, metadata) = ffi_fn::unpack_arg_names(name);
            quote!(#(#cfg)* #data, #(#cfg)* #metadata)
        } else {
            quote!(#(#cfg)* #name)
        }
    }
}

impl Unpack {
    pub(crate) fn resolve(attrs: &[syn::Attribute], ty: &Type) -> syn::Result<Self> {
        UnpackSyntax::parse(attrs)?.resolve(ty)
    }
}

impl UnpackSyntax {
    pub(crate) fn parse(attrs: &[syn::Attribute]) -> syn::Result<Self> {
        let Some(attr) = attrs.iter().find(|attr| ffi_fn::is_unpack_attr(attr)) else {
            return Ok(Self::None);
        };
        let syntax_error = || syn::Error::new_spanned(attr, "#[unpack] expects exactly two parts");
        if !matches!(attr.meta, syn::Meta::List(_)) {
            return Err(syntax_error());
        }
        let mut parts = attr
            .parse_args_with(Punctuated::<UnpackPart, syn::Token![,]>::parse_terminated)?
            .into_iter()
            .collect::<Vec<_>>();
        if parts.len() == 1 {
            let part = parts.pop().unwrap();
            if part.abi.is_some() {
                return Err(syn::Error::new_spanned(
                    attr,
                    "the one-part #[unpack] form does not support ABI erasure",
                ));
            }
            return Ok(Self::Single(Box::new(part.logical)));
        }
        Ok(Self::Pair(Box::new(
            parts.try_into().map_err(|_| syntax_error())?,
        )))
    }

    pub(crate) fn has_inferred_parts(&self) -> bool {
        matches!(self, Self::Pair(parts) if parts.iter().any(|part| matches!(part.logical, Type::Infer(_))))
    }

    pub(crate) fn resolve(self, ty: &Type) -> syn::Result<Unpack> {
        let inferred = self.has_inferred_parts();
        let [first, second] = match self {
            Self::None => return Ok(Unpack::None),
            Self::Single(part) => {
                return Ok(Unpack::Single(if matches!(*part, Type::Infer(_)) {
                    Box::new(ty.clone())
                } else {
                    part
                }));
            }
            Self::Pair(parts) => *parts,
        };
        let inferred_source = inferred
            .then(|| {
                ffi_fn::option_inner_type(ffi_fn::peel_grouped_type(ty))
                    .map(|inner| parse_quote!(core::option::Option<#inner>))
                    .or_else(|| {
                        ffi_fn::inferred_result_type(ffi_fn::peel_grouped_type(ty))
                            .map(|(source, _)| source)
                    })
                    .or_else(|| {
                        ffi_fn::transparent_unpack_inner_type(ffi_fn::peel_grouped_type(ty)).map(
                            |(name, inner)| match name.to_string().as_str() {
                                "ManuallyDrop" => parse_quote!(core::mem::ManuallyDrop<#inner>),
                                "Cell" => parse_quote!(core::cell::Cell<#inner>),
                                "UnsafeCell" => parse_quote!(core::cell::UnsafeCell<#inner>),
                                _ => unreachable!("unrecognized transparent unpack wrapper"),
                            },
                        )
                    })
            })
            .flatten();
        let resolve = |part: UnpackPart, position| -> syn::Result<ResolvedPart> {
            let logical = ffi_fn::logical_unpack_part(ty, &part.logical, position)?;
            let erased = part.abi.is_some();
            let abi = part.abi.map_or_else(
                || logical.clone(),
                |abi| parse_quote!(<#abi as co3::ReprC>::CType),
            );
            Ok(ResolvedPart {
                source: part.logical,
                logical,
                abi,
                erased,
            })
        };
        Ok(Unpack::Pair(Box::new(UnpackPair {
            first: resolve(first, 1)?,
            second: resolve(second, 2)?,
            inferred_source,
        })))
    }
}

pub(crate) fn analyze<'a>(inputs: impl IntoIterator<Item = &'a FnArg>) -> Vec<Argument<'a>> {
    inputs.into_iter().map(Argument::new).collect()
}

/// Apply facts established by the use site, such as the Copy contract of tag IDs.
pub(crate) fn analyze_with_plain_inputs<'a>(
    inputs: impl IntoIterator<Item = &'a FnArg>,
    plain_inputs: &[Ident],
) -> Vec<Argument<'a>> {
    analyze(inputs)
        .into_iter()
        .map(|mut input| {
            if plain_inputs.contains(input.name()) {
                input.kind = ValueKind::Plain;
            }
            input
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn by_val_selects_ordinary_representation() {
        let cases: [(Type, bool); 14] = [
            (parse_quote!(u32), false),
            (parse_quote!((u8, u32)), false),
            (parse_quote!(&u32), false),
            (parse_quote!(&String), false),
            (parse_quote!(&mut Box<u8>), true),
            (parse_quote!(&core::cell::UnsafeCell<Box<u8>>), true),
            (parse_quote!(Alias), true),
            (parse_quote!((T, U)), true),
            (parse_quote!((Vec<u8>, u32)), true),
            (parse_quote!(Tuple2<u8, u8>), false),
            (parse_quote!(Tuple2<String, u8>), true),
            (parse_quote!(ManuallyDrop<u8>), false),
            (parse_quote!(ManuallyDrop<Box<T>>), true),
            (parse_quote!(MaybeUninit<T>), false),
        ];
        for (ty, wrapped) in cases {
            let borrowed = Argument::from_parts(&[], parse_quote!(value), ty.clone());
            let attrs = [parse_quote!(#[by_val])];
            let moved = Argument::from_parts(&attrs, parse_quote!(value), ty.clone());
            assert_eq!(borrowed.kind, moved.kind);
            assert_eq!(borrowed.is_borrowed(), wrapped);
            assert!(!moved.is_borrowed());
            if wrapped {
                let co3 = co3_path();
                assert_eq!(
                    borrowed.abi_ty(),
                    parse_quote!(#co3::borrow::Borrowed< <#ty as #co3::ReprC>::CType >)
                );
                assert_eq!(moved.abi_ty(), parse_quote!(<#ty as co3::ReprC>::CType));
            } else {
                assert_eq!(borrowed.abi_ty(), moved.abi_ty());
            }
        }
        for arity in 1..=12 {
            let args = vec!["u8"; arity].join(", ");
            let ty: Type = syn::parse_str(&format!("Tuple{arity}<{args}>")).unwrap();
            assert!(!Argument::from_parts(&[], parse_quote!(value), ty).is_borrowed());
        }
    }

    #[test]
    fn use_site_classifies_tag_alias_without_changing_other_aliases() {
        let inputs: [FnArg; 2] = [parse_quote!(id: TagKind), parse_quote!(value: TagKind)];
        let arguments = analyze_with_plain_inputs(&inputs, &[parse_quote!(id)]);
        assert_eq!(arguments[0].kind, ValueKind::Plain);
        assert!(!arguments[0].is_borrowed());
        assert_eq!(
            arguments[0].abi_ty(),
            parse_quote!(<TagKind as co3::ReprC>::CType)
        );
        assert_eq!(arguments[1].kind, ValueKind::RequiresMove);
        assert!(arguments[1].is_borrowed());
    }
}
