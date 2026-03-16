use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::TokenStream;
use quote::quote;

use crate::{
    handle::{self, parse_entry_handle_map_attr},
    impl_visitor::path_symbol_name,
    parse_link_attr, parse_link_name_attr,
};

pub(crate) fn validate_ownership_attrs(_attrs: &[syn::Attribute]) -> Result<(), syn::Error> {
    Ok(())
}

pub(crate) fn unsupported_export_entry_attr(attr: &syn::Attribute) -> syn::Error {
    syn::Error::new_spanned(
        attr,
        "supported export entry attributes are `#[unsafe(export_name = \"...\")]`, `#[unsafe(no_mangle)]`, and `#[dispatch(...)]` on polymorphic trait entries",
    )
}

fn unsupported_export_impl_attr(attr: &syn::Attribute) -> syn::Error {
    syn::Error::new_spanned(
        attr,
        "impl export entries do not support entry-level export attrs; place `#[unsafe(export_name = \"...\")]` or `#[unsafe(no_mangle)]` on methods instead",
    )
}

pub(crate) fn ensure_no_handle_arg_attrs(sig: &syn::Signature) -> syn::Result<()> {
    for input in &sig.inputs {
        match input {
            syn::FnArg::Receiver(receiver) => {
                for attr in &receiver.attrs {
                    if attr.path().is_ident("dispatch") {
                        return Err(syn::Error::new_spanned(
                            attr,
                            "`#[dispatch]` is only supported on `trait` export entries",
                        ));
                    }
                }
            }
            syn::FnArg::Typed(arg) => {
                for attr in &arg.attrs {
                    if attr.path().is_ident("dispatch") {
                        return Err(syn::Error::new_spanned(
                            attr,
                            "`#[dispatch]` is only supported on `trait` export entries",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_sig_ownership_attrs(sig: &syn::Signature) -> syn::Result<()> {
    if let Some(receiver) = sig.receiver() {
        validate_ownership_attrs(&receiver.attrs)?;
    }
    for input in &sig.inputs {
        let syn::FnArg::Typed(arg) = input else {
            continue;
        };
        validate_ownership_attrs(&arg.attrs)?;
    }
    Ok(())
}

pub(crate) fn validate_impl_dispatch_map(
    item_impl: &syn::ItemImpl,
    handle_map: Option<&BTreeMap<String, Vec<syn::Type>>>,
    has_dispatch_attr: bool,
    allow_empty_dispatch_marker: bool,
    allow_concrete_self_dispatch: bool,
) -> syn::Result<()> {
    let type_params = item_impl
        .generics
        .params
        .iter()
        .filter_map(|param| match param {
            syn::GenericParam::Type(param) => Some(param.ident.to_string()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    if type_params.is_empty() {
        if handle_map.is_none() {
            return Ok(());
        }
        let handle_map = handle_map.expect("checked above");
        if allow_concrete_self_dispatch && handle_map.keys().all(|selector| selector == "Self") {
            return Ok(());
        }
        for selector in handle_map.keys() {
            if selector != "Self" {
                return Err(syn::Error::new_spanned(
                    item_impl,
                    format!(
                        "dispatch selector `{selector}` does not match any impl type parameter"
                    ),
                ));
            }
        }
        return Err(syn::Error::new_spanned(
            item_impl,
            "`#[dispatch(...)]` mappings on impl entries are only supported on polymorphic impls",
        ));
    }

    let Some(handle_map) = handle_map else {
        if has_dispatch_attr && !allow_empty_dispatch_marker {
            let first_param = item_impl
                .generics
                .params
                .iter()
                .find_map(|param| match param {
                    syn::GenericParam::Type(param) => Some(&param.ident),
                    _ => None,
                });
            if let Some(param) = first_param {
                let self_maps_param = match item_impl.self_ty.as_ref() {
                    syn::Type::Path(type_path) => {
                        type_path.qself.is_none()
                            && type_path.path.segments.len() == 1
                            && type_path.path.segments[0].ident == *param
                    }
                    _ => false,
                };
                let msg = if self_maps_param {
                    format!(
                        "generic extern impl parameter `{}` requires `#[dispatch({} = [Type, ...])]` mapping or `Self` mapping when used as the self type",
                        param, param
                    )
                } else {
                    format!(
                        "generic extern impl parameter `{}` requires `#[dispatch({} = [Type, ...])]` mapping",
                        param, param
                    )
                };
                return Err(syn::Error::new_spanned(param, msg));
            }
            return Err(syn::Error::new_spanned(
                item_impl,
                "polymorphic impl `#[dispatch]` must map at least one type parameter or `Self`",
            ));
        }
        return Ok(());
    };

    for selector in handle_map.keys() {
        if selector != "Self" && !type_params.contains(selector) {
            return Err(syn::Error::new_spanned(
                item_impl,
                format!("dispatch selector `{selector}` does not match any impl type parameter"),
            ));
        }
    }

    Ok(())
}

pub(crate) fn validate_inner_attrs(_attrs: &[syn::Attribute]) -> Result<(), syn::Error> {
    Ok(())
}

fn validate_unique_link_name(attrs: &[syn::Attribute], what: &str) -> Result<(), syn::Error> {
    let mut seen = false;
    for attr in attrs {
        if parse_link_name_attr(attr)?.is_none() {
            continue;
        }
        if seen {
            return Err(syn::Error::new_spanned(
                attr,
                format!("`link_name` can only be provided once per {what}"),
            ));
        }
        seen = true;
    }
    Ok(())
}

pub(crate) fn validate_export_decl_attrs(decls: &[syn::Item]) -> Result<(), syn::Error> {
    fn has_dispatch_arg_attr(attrs: &[syn::Attribute]) -> bool {
        attrs.iter().any(|attr| attr.path().is_ident("dispatch"))
    }
    fn base_handle_selector_type(ty: &syn::Type) -> &syn::Type {
        if let syn::Type::Reference(reference) = ty {
            return &reference.elem;
        }
        ty
    }
    fn is_parametric_handle_selector(ty: &syn::Type, allowed: &BTreeSet<String>) -> bool {
        let ty = base_handle_selector_type(ty);
        let syn::Type::Path(type_path) = ty else {
            return false;
        };
        if type_path.qself.is_some() || type_path.path.segments.len() != 1 {
            return false;
        }
        let seg = &type_path.path.segments[0];
        if !matches!(seg.arguments, syn::PathArguments::None) {
            return false;
        }
        allowed.contains(&seg.ident.to_string())
    }

    for decl in decls {
        match decl {
            syn::Item::Impl(impl_decl) => {
                validate_ownership_attrs(&impl_decl.attrs)?;
                if let Some(attr) = impl_decl.attrs.first() {
                    return Err(unsupported_export_impl_attr(attr));
                }
                for item in &impl_decl.items {
                    let syn::ImplItem::Fn(method) = item else {
                        continue;
                    };
                    validate_ownership_attrs(&method.attrs)?;
                    validate_sig_ownership_attrs(&method.sig)?;
                    ensure_no_handle_arg_attrs(&method.sig)?;
                }
            }
            syn::Item::Trait(trait_item) => {
                let mut entry_handle_map: Option<BTreeMap<String, Vec<syn::Type>>> = None;
                for attr in &trait_item.attrs {
                    if let Some(map) = parse_entry_handle_map_attr(attr)?
                        && entry_handle_map.replace(map).is_some()
                    {
                        return Err(syn::Error::new_spanned(
                            attr,
                            "`handle` mapping can only be provided once per entry",
                        ));
                    }
                }
                for item in &trait_item.items {
                    let syn::TraitItem::Fn(method_item) = item else {
                        continue;
                    };
                    validate_ownership_attrs(&method_item.attrs)?;
                    validate_sig_ownership_attrs(&method_item.sig)?;
                    let Some(key_types) = entry_handle_map.clone() else {
                        return Err(syn::Error::new_spanned(
                            &method_item.sig,
                            "trait export entries require `#[dispatch(...)]` mapping",
                        ));
                    };
                    let allowed_handle_selectors = key_types
                        .keys()
                        .cloned()
                        .chain(trait_item.generics.params.iter().filter_map(
                            |generic| match generic {
                                syn::GenericParam::Type(param) => Some(param.ident.to_string()),
                                _ => None,
                            },
                        ))
                        .collect::<BTreeSet<_>>();
                    for input in &method_item.sig.inputs {
                        let syn::FnArg::Typed(arg) = input else {
                            continue;
                        };
                        if !has_dispatch_arg_attr(&arg.attrs) {
                            continue;
                        }
                        if !is_parametric_handle_selector(&arg.ty, &allowed_handle_selectors) {
                            return Err(syn::Error::new_spanned(
                                &arg.ty,
                                "in trait export entries, `#[dispatch]` arguments must use `Self` or a trait type parameter (for example `T`), not a concrete type",
                            ));
                        }
                    }
                }
            }
            syn::Item::Fn(decl_fn) => {
                validate_ownership_attrs(&decl_fn.attrs)?;
                validate_sig_ownership_attrs(&decl_fn.sig)?;
                ensure_no_handle_arg_attrs(&decl_fn.sig)?;
                if decl_fn.sig.receiver().is_some() {
                    return Err(syn::Error::new_spanned(
                        &decl_fn.sig,
                        "free function export entries cannot declare a receiver",
                    ));
                }
            }
            syn::Item::Struct(decl) => {
                for attr in &decl.attrs {
                    return Err(unsupported_export_entry_attr(attr));
                }
            }
            other => return Err(syn::Error::new_spanned(other, "item not supported")),
        }
    }

    Ok(())
}

pub(crate) fn validate_extern_decl_attrs(decls: &[syn::Item]) -> Result<(), syn::Error> {
    let suggestion = "use method-level `#[link_name = \"...\"]` or macro-level `#![link(...)]`";
    let decl_level_link_error =
        format!("declaration-level `#[link(...)]` is not supported; {suggestion}");

    fn is_bare_dispatch_attr(attr: &syn::Attribute) -> bool {
        attr.path().is_ident("dispatch") && matches!(attr.meta, syn::Meta::Path(_))
    }
    fn impl_trait_is_drop(trait_tokens: &TokenStream) -> bool {
        let tokens: Vec<_> = trait_tokens.clone().into_iter().collect();
        tokens
            .last()
            .is_some_and(|tt| matches!(tt, proc_macro2::TokenTree::Ident(ident) if ident == "Drop"))
    }

    for decl in decls {
        match decl {
            syn::Item::Fn(decl) => {
                validate_unique_link_name(&decl.attrs, "function")?;
                for attr in &decl.attrs {
                    if parse_link_attr(attr)?.is_some() {
                        return Err(syn::Error::new_spanned(attr, decl_level_link_error.clone()));
                    }
                }
            }
            syn::Item::Struct(decl) => {
                for attr in &decl.attrs {
                    if parse_link_attr(attr)?.is_some() {
                        return Err(syn::Error::new_spanned(
                            attr,
                            "link attributes are only supported on imported function declarations",
                        ));
                    }

                    if attr.path().is_ident("link_name") {
                        return Err(syn::Error::new_spanned(
                            attr,
                            format!("type-level `#[link_name]` is not supported; {suggestion}"),
                        ));
                    }
                }
            }
            syn::Item::Impl(decl) => {
                let impl_dispatch_map = handle::dispatch::parse_impl_dispatch_attrs(&decl.attrs)?;
                let impl_has_bare_dispatch = decl.attrs.iter().any(|attr| {
                    if !attr.path().is_ident("dispatch") {
                        return false;
                    }
                    match &attr.meta {
                        syn::Meta::Path(_) => true,
                        syn::Meta::List(list) => list.tokens.is_empty(),
                        _ => false,
                    }
                });
                let allow_empty_dispatch_marker = decl
                    .trait_
                    .as_ref()
                    .is_some_and(|(_, trait_path, _)| impl_trait_is_drop(&quote!(#trait_path)))
                    && impl_has_bare_dispatch;
                validate_impl_dispatch_map(
                    decl,
                    impl_dispatch_map.as_ref(),
                    impl_has_bare_dispatch,
                    allow_empty_dispatch_marker,
                    true,
                )?;
                let drop_trait_impl = matches!(
                    &decl.trait_,
                    Some((_, trait_path, _)) if impl_trait_is_drop(&quote!(#trait_path))
                );
                for attr in &decl.attrs {
                    if parse_link_attr(attr)?.is_some() || attr.path().is_ident("link_name") {
                        return Err(syn::Error::new_spanned(
                            attr,
                            format!(
                                "impl-level `#[link(...)]` attributes are not supported; {suggestion}"
                            ),
                        ));
                    }
                    if drop_trait_impl
                        && attr.path().is_ident("dispatch")
                        && !is_bare_dispatch_attr(attr)
                    {
                        return Err(syn::Error::new_spanned(
                            attr,
                            "`impl Drop` only supports bare `#[dispatch]` (without type mappings)",
                        ));
                    }
                }
                for item in &decl.items {
                    match item {
                        syn::ImplItem::Fn(method) => {
                            validate_unique_link_name(&method.attrs, "method")?;
                            for attr in &method.attrs {
                                if parse_link_attr(attr)?.is_some() {
                                    return Err(syn::Error::new_spanned(
                                        attr,
                                        decl_level_link_error.clone(),
                                    ));
                                }
                                if attr.path().is_ident("dispatch") {
                                    return Err(syn::Error::new_spanned(
                                        attr,
                                        "`#[dispatch]` is not supported on impl items; use impl-level instead",
                                    ));
                                }
                            }
                        }
                        syn::ImplItem::Type(assoc) => {
                            for attr in &assoc.attrs {
                                if parse_link_attr(attr)?.is_some()
                                    || attr.path().is_ident("link_name")
                                {
                                    return Err(syn::Error::new_spanned(
                                        attr,
                                        "link attributes are only supported on imported function declarations",
                                    ));
                                }
                            }
                        }
                        syn::ImplItem::Const(assoc) => {
                            for attr in &assoc.attrs {
                                if parse_link_attr(attr)?.is_some()
                                    || attr.path().is_ident("link_name")
                                {
                                    return Err(syn::Error::new_spanned(
                                        attr,
                                        "link attributes are only supported on imported function declarations",
                                    ));
                                }
                            }
                        }
                        other => {
                            return Err(syn::Error::new_spanned(
                                other,
                                "only methods, associated types, and associated consts are supported in extern impl declarations",
                            ));
                        }
                    }
                }
            }
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "item not supported in `extern_!`/`extern_C!`",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_extern_type_drop_requirements(
    decls: &[syn::Item],
) -> Result<(), syn::Error> {
    fn type_ctor_ident(ty: &syn::Type) -> Option<&syn::Ident> {
        let syn::Type::Path(type_path) = ty else {
            return None;
        };
        type_path.path.segments.last().map(|seg| &seg.ident)
    }

    let mut declared_types: Vec<&syn::ItemStruct> = Vec::new();
    for decl in decls {
        if let syn::Item::Struct(ty_decl) = decl {
            declared_types.push(ty_decl);
        }
    }
    if declared_types.is_empty() {
        return Ok(());
    }

    let mut drop_ctors = std::collections::BTreeSet::<String>::new();
    for decl in decls {
        let syn::Item::Impl(impl_decl) = decl else {
            continue;
        };
        let Some((_, trait_path, _)) = &impl_decl.trait_ else {
            continue;
        };
        if path_symbol_name(trait_path) != "Drop" {
            continue;
        }
        let self_ty = impl_decl.self_ty.as_ref().clone();
        let Some(ident) = type_ctor_ident(&self_ty) else {
            continue;
        };
        drop_ctors.insert(ident.to_string());
    }

    for ty_decl in declared_types {
        if !drop_ctors.contains(&ty_decl.ident.to_string()) {
            return Err(syn::Error::new_spanned(
                &ty_decl.ident,
                "extern_C! type declarations must include a corresponding `impl Drop for Type { ... }` declaration",
            ));
        }
    }

    Ok(())
}

pub(crate) fn is_inner_special_attr(attr: &syn::Attribute) -> bool {
    if attr.path().is_ident("abi") {
        return true;
    }

    parse_link_attr(attr)
        .ok()
        .flatten()
        .is_some_and(|parsed| parsed.link_crate.is_some())
}
