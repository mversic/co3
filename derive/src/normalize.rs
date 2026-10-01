use quote::quote;
use syn::{ItemFn, Result, parse_quote, visit_mut::VisitMut};

use crate::{DeclKind, DispatchGroups, ForeignItem, ImportMode, ParsedItem, parse};

pub(crate) enum NormalizedItem {
    Foreign(ForeignItem),
    Raw(parse::RawFnDecl),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NormalizationMode {
    Ffi(DeclKind),
    Raw,
}

impl NormalizationMode {
    fn macro_name(self) -> &'static str {
        match self {
            Self::Ffi(_) => "ffi!",
            Self::Raw => "raw!",
        }
    }
}

pub(crate) fn normalize_items(
    items: Vec<ParsedItem>,
    mode: NormalizationMode,
) -> Result<(Vec<NormalizedItem>, Vec<parse::ParsedAlias>)> {
    let mut normalized = Vec::new();
    let mut aliases = Vec::new();
    for item in items {
        match item {
            ParsedItem::Raw(raw_decl) if matches!(mode, NormalizationMode::Ffi(_)) => {
                crate::validate::validate_raw_signature(&raw_decl.sig)?;
                let item = ItemFn {
                    attrs: raw_decl.attrs,
                    vis: raw_decl.vis,
                    modifiers: Default::default(),
                    sig: raw_decl.sig,
                    block: Box::new(parse_quote!({})),
                };
                let ForeignItem::Fn(mut item) = ParsedItem::Fn(item).normalize()? else {
                    unreachable!("function declaration normalizes to a function")
                };
                if mode == NormalizationMode::Ffi(DeclKind::Extern) {
                    item.import_mode = ImportMode::Raw;
                }
                normalized.push(NormalizedItem::Foreign(ForeignItem::Fn(item)));
            }
            ParsedItem::Raw(raw_decl) => normalized.push(NormalizedItem::Raw(*raw_decl)),
            ParsedItem::Alias(alias) => aliases.push(alias),
            ParsedItem::Fn(item) if mode == NormalizationMode::Raw => {
                let ForeignItem::Fn(parsed) = ParsedItem::Fn(item.clone()).normalize()? else {
                    unreachable!("function declaration normalizes to a function")
                };
                let ident = &item.sig.ident;
                normalized.push(NormalizedItem::Raw(parse::RawFnDecl {
                    attrs: item.attrs,
                    vis: item.vis,
                    callee: quote!(#ident),
                    sig: item.sig,
                    owner: None,
                    dispatch_args: parsed.dispatch_args,
                }));
            }
            ParsedItem::Impl(mut item) => {
                let raw_dispatch = if mode == NormalizationMode::Raw {
                    let ForeignItem::Impl(parsed) = ParsedItem::Impl(item.clone()).normalize()?
                    else {
                        unreachable!("impl declaration normalizes to an impl")
                    };
                    Some(parsed)
                } else {
                    None
                };
                let self_ty = item.self_ty.clone();
                let trait_path = item.trait_.as_ref().map(|(path, _)| path.clone());
                let mut impl_items = Vec::new();
                let mut raw_impl_items = Vec::new();
                for impl_item in core::mem::take(&mut item.items) {
                    let syn::ImplItem::Fn(mut method) = impl_item else {
                        impl_items.push(impl_item);
                        continue;
                    };
                    let marker = method
                        .attrs
                        .iter()
                        .position(|attr| attr.path().is_ident("raw"));
                    if mode == NormalizationMode::Raw {
                        if let Some(marker) = marker {
                            return Err(syn::Error::new_spanned(
                                &method.attrs[marker],
                                "omit `raw` on `raw!` companion declarations",
                            ));
                        }
                    } else if let Some(marker) = marker {
                        method.attrs.remove(marker);
                        crate::validate::validate_raw_signature(&method.sig)?;
                        if mode == NormalizationMode::Ffi(DeclKind::Export) {
                            impl_items.push(syn::ImplItem::Fn(method));
                            continue;
                        }
                    } else {
                        impl_items.push(syn::ImplItem::Fn(method));
                        continue;
                    }
                    let method_name = method.sig.ident.clone();
                    let callee = if let Some(trait_path) = &trait_path {
                        quote!(<#self_ty as #trait_path>::#method_name)
                    } else {
                        quote!(<#self_ty>::#method_name)
                    };
                    let mut sig = method.sig;
                    if let Some(syn::FnArg::Receiver(receiver)) = sig.inputs.first() {
                        let mut receiver_ty = crate::utils::receiver_ty(receiver);
                        crate::ffi_fn::SelfConcretizer { self_ty: &self_ty }
                            .visit_type_mut(&mut receiver_ty);
                        let attrs = &receiver.attrs;
                        let receiver_arg: syn::FnArg =
                            parse_quote!(#(#attrs)* __co3_self: #receiver_ty);
                        sig.inputs[0] = receiver_arg;
                    }
                    crate::ffi_fn::SelfConcretizer { self_ty: &self_ty }
                        .visit_signature_mut(&mut sig);
                    if matches!(mode, NormalizationMode::Ffi(_)) {
                        method.sig = sig;
                        raw_impl_items.push(syn::ImplItem::Fn(method));
                    } else {
                        normalized.push(NormalizedItem::Raw(parse::RawFnDecl {
                            attrs: method.attrs,
                            vis: method.vis,
                            callee,
                            sig,
                            owner: Some(parse::RawFnOwner {
                                attrs: crate::utils::cfg_attrs(&item.attrs).cloned().collect(),
                                generics: item.generics.clone(),
                                trait_path: trait_path.clone(),
                                self_ty: self_ty.clone(),
                            }),
                            dispatch_args: raw_dispatch.as_ref().map_or_else(
                                DispatchGroups::default,
                                |parsed| {
                                    parsed.dispatch_args.combined_with(
                                        parsed
                                            .method_dispatch_args
                                            .get(&method_name)
                                            .unwrap_or(&DispatchGroups::default()),
                                    )
                                },
                            ),
                        }));
                    }
                }
                if !raw_impl_items.is_empty() {
                    let mut raw_impl = item.clone();
                    raw_impl.items = raw_impl_items;
                    let ForeignItem::Impl(mut raw_impl) = ParsedItem::Impl(raw_impl).normalize()?
                    else {
                        unreachable!("impl declaration normalizes to an impl")
                    };
                    raw_impl.import_mode = ImportMode::Raw;
                    normalized.push(NormalizedItem::Foreign(ForeignItem::Impl(raw_impl)));
                }
                item.items = impl_items;
                if !item.items.is_empty() {
                    normalized.push(NormalizedItem::Foreign(ParsedItem::Impl(item).normalize()?));
                }
            }
            item => normalized.push(NormalizedItem::Foreign(item.normalize()?)),
        }
    }
    reject_implicit_extern_abi(&normalized, mode.macro_name())?;
    Ok((normalized, aliases))
}

pub(crate) fn partition_items(
    items: Vec<NormalizedItem>,
) -> (Vec<ForeignItem>, Vec<parse::RawFnDecl>) {
    let mut foreign = Vec::new();
    let mut raw = Vec::new();
    for item in items {
        match item {
            NormalizedItem::Foreign(item) => foreign.push(item),
            NormalizedItem::Raw(item) => raw.push(item),
        }
    }
    (foreign, raw)
}

pub(crate) fn pack_normalized_items(items: Vec<NormalizedItem>) -> Result<Vec<NormalizedItem>> {
    let (foreign, raw) = partition_items(items);
    let mut items = crate::pack_items(foreign)?
        .into_iter()
        .map(NormalizedItem::Foreign)
        .collect::<Vec<_>>();
    items.extend(raw.into_iter().map(NormalizedItem::Raw));
    Ok(items)
}

fn reject_implicit_extern_abi(items: &[NormalizedItem], macro_name: &str) -> Result<()> {
    fn check(sig: &syn::Signature, macro_name: &str) -> Result<()> {
        if let Some(abi) = &sig.abi
            && abi.name.is_none()
        {
            return Err(syn::Error::new_spanned(
                abi,
                format!("`extern fn` declarations inside `{macro_name}` require an explicit ABI"),
            ));
        }
        Ok(())
    }

    for item in items {
        match item {
            NormalizedItem::Foreign(ForeignItem::Fn(item)) => check(&item.item.sig, macro_name)?,
            NormalizedItem::Foreign(ForeignItem::Impl(item)) => {
                for impl_item in &item.item.items {
                    if let syn::ImplItem::Fn(method) = impl_item {
                        check(&method.sig, macro_name)?;
                    }
                }
            }
            NormalizedItem::Raw(raw_decl) => check(&raw_decl.sig, macro_name)?,
            NormalizedItem::Foreign(ForeignItem::Type(_) | ForeignItem::Static(_)) => {}
        }
    }
    Ok(())
}
