use syn::{ItemFn, Result, parse_quote};

use crate::{Co3Fn, DeclKind, DispatchGroups, ForeignItem, ImportMode, ParsedItem, parse};

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
                let mut item = Co3Fn {
                    item,
                    dispatch_args: raw_decl.dispatch_args,
                    import_mode: ImportMode::Regular,
                };
                if mode == NormalizationMode::Ffi(DeclKind::Extern) {
                    item.import_mode = ImportMode::Raw;
                }
                normalized.push(NormalizedItem::Foreign(ForeignItem::Fn(item)));
            }
            ParsedItem::Raw(raw_decl) => normalized.push(NormalizedItem::Raw(*raw_decl)),
            ParsedItem::Alias(alias) => aliases.push(alias),
            ParsedItem::Fn(parsed) if mode == NormalizationMode::Raw => {
                let item = parsed.item;
                let ident = &item.sig.ident;
                normalized.push(NormalizedItem::Raw(parse::RawFnDecl {
                    attrs: item.attrs,
                    vis: item.vis,
                    callee: parse_quote!(#ident),
                    sig: item.sig,
                    owner: None,
                    dispatch_args: parsed.dispatch_args,
                }));
            }
            ParsedItem::Impl(parse::ParsedImpl {
                item: mut parsed,
                raw_methods,
            }) => {
                let item = &mut parsed.item;
                crate::normalize_dyn_self_tag_ids(item);
                let was_empty = item.items.is_empty();
                let self_ty = item.self_ty.clone();
                let trait_path = item.trait_.as_ref().map(|(path, _)| path.clone());
                let mut impl_items = Vec::new();
                let mut raw_impl_items = Vec::new();
                for (index, impl_item) in core::mem::take(&mut item.items).into_iter().enumerate() {
                    let syn::ImplItem::Fn(mut method) = impl_item else {
                        impl_items.push(impl_item);
                        continue;
                    };
                    let raw = raw_methods.get(&index);
                    if mode == NormalizationMode::Raw {
                        if let Some(span) = raw {
                            return Err(syn::Error::new(
                                *span,
                                "omit `raw` on `raw!` companion declarations",
                            ));
                        }
                    } else if raw.is_some() {
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
                        parse_quote!(<#self_ty as #trait_path>::#method_name)
                    } else {
                        parse_quote!(<#self_ty>::#method_name)
                    };
                    let mut sig = method.sig;
                    if let Some(syn::FnArg::Receiver(receiver)) = sig.inputs.first() {
                        let receiver_ty = crate::utils::receiver_ty(receiver);
                        let attrs = &receiver.attrs;
                        let receiver_arg: syn::FnArg =
                            parse_quote!(#(#attrs)* __co3_self: #receiver_ty);
                        sig.inputs[0] = receiver_arg;
                    }
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
                            dispatch_args: parsed.dispatch_args.combined_with(
                                parsed
                                    .method_dispatch_args
                                    .get(&method_name)
                                    .unwrap_or(&DispatchGroups::default()),
                            ),
                        }));
                    }
                }
                item.items = impl_items;
                if !raw_impl_items.is_empty() {
                    let mut raw = parsed.clone();
                    raw.item.items = raw_impl_items;
                    raw.import_mode = ImportMode::Raw;
                    let mut raw_dispatch = std::collections::HashMap::new();
                    for method in &raw.item.items {
                        if let syn::ImplItem::Fn(method) = method
                            && let Some(groups) =
                                parsed.method_dispatch_args.remove(&method.sig.ident)
                        {
                            raw_dispatch.insert(method.sig.ident.clone(), groups);
                        }
                    }
                    raw.method_dispatch_args = raw_dispatch;
                    normalized.push(NormalizedItem::Foreign(ForeignItem::Impl(raw)));
                }
                if !parsed.item.items.is_empty() || was_empty {
                    normalized.push(NormalizedItem::Foreign(ForeignItem::Impl(parsed)));
                }
            }
            ParsedItem::Type(ty) => normalized.push(NormalizedItem::Foreign(ForeignItem::Type(ty))),
            ParsedItem::Static(item) => {
                normalized.push(NormalizedItem::Foreign(ForeignItem::Static(item)))
            }
            ParsedItem::Fn(item) => normalized.push(NormalizedItem::Foreign(ForeignItem::Fn(item))),
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
