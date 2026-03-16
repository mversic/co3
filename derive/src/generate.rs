use std::collections::{BTreeMap, BTreeSet};

use darling::FromDeriveInput as _;
use manyhow::emit;
use proc_macro2::TokenStream;
use quote::{ToTokens, quote};

use crate::{
    emitter::Emitter,
    handle::{ExportPolySpec, dispatch, gen_poly_export, parse_entry_handle_map_attr},
    impl_visitor::{FnDescriptor, ImplDescriptor, path_symbol_name},
    input_ownerships, parse_link_attr, parse_link_name_attr, receiver_ownership,
    strip_internal_sig_attrs,
    validate::{
        ensure_no_handle_arg_attrs, unsupported_export_entry_attr, validate_ownership_attrs,
        validate_sig_ownership_attrs,
    },
    wrapper,
};

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

fn apply_sig_attrs_to_fn_descriptor(fn_descriptor: &mut FnDescriptor<'_>, sig: &syn::Signature) {
    if let (Some(receiver), Some(ownership)) =
        (&mut fn_descriptor.receiver, receiver_ownership(sig))
    {
        receiver.set_ownership_mode(ownership);
    }

    for (arg, ownership) in fn_descriptor
        .input_args
        .iter_mut()
        .zip(input_ownerships(sig).into_iter())
    {
        arg.set_ownership_mode(ownership);
    }
}

pub(crate) fn emit_decl_exports(abi: syn::Abi, decls: Vec<syn::Item>) -> TokenStream {
    let mut selected_method_exports = Vec::new();
    let mut generated_items = Vec::new();

    for decl in decls {
        match decl {
            syn::Item::Impl(impl_decl) => {
                if let Err(err) = validate_ownership_attrs(&impl_decl.attrs) {
                    return err.to_compile_error();
                }
                if let Some(attr) = impl_decl.attrs.first() {
                    return unsupported_export_entry_attr(attr).to_compile_error();
                }
                let self_ty = (*impl_decl.self_ty).clone();
                for item in impl_decl.items {
                    let syn::ImplItem::Fn(decl_method) = item else {
                        return syn::Error::new_spanned(
                            self_ty.clone(),
                            "export impl entries only support method declarations",
                        )
                        .to_compile_error();
                    };
                    if let Err(err) = validate_ownership_attrs(&decl_method.attrs) {
                        return err.to_compile_error();
                    }
                    if let Err(err) = validate_sig_ownership_attrs(&decl_method.sig) {
                        return err.to_compile_error();
                    }
                    if let Err(err) = ensure_no_handle_arg_attrs(&decl_method.sig) {
                        return err.to_compile_error();
                    }
                    let mut combined_attrs = impl_decl.attrs.clone();
                    combined_attrs.extend(decl_method.attrs.clone());
                }
            }
            syn::Item::Trait(trait_item) => {
                let trait_ident = &trait_item.ident;
                let trait_generics = &trait_item.generics;
                let (_, trait_ty_generics, _) = trait_generics.split_for_impl();
                let trait_path: syn::Path = syn::parse_quote!(#trait_ident #trait_ty_generics);
                let mut entry_handle_map: Option<BTreeMap<String, Vec<syn::Type>>> = None;
                for attr in &trait_item.attrs {
                    match parse_entry_handle_map_attr(attr) {
                        Ok(Some(map)) => {
                            if entry_handle_map.replace(map).is_some() {
                                return syn::Error::new_spanned(
                                    attr,
                                    "`handle` mapping can only be provided once per entry",
                                )
                                .to_compile_error();
                            }
                        }
                        Ok(None) => {}
                        Err(err) => return err.to_compile_error(),
                    }
                }
                for item in trait_item.items {
                    let syn::TraitItem::Fn(method_item) = item else {
                        return syn::Error::new_spanned(
                            item,
                            "trait export entries only support method declarations",
                        )
                        .to_compile_error();
                    };
                    if let Err(err) = validate_ownership_attrs(&method_item.attrs) {
                        return err.to_compile_error();
                    }
                    if let Err(err) = validate_sig_ownership_attrs(&method_item.sig) {
                        return err.to_compile_error();
                    }
                    let Some(key_types) = entry_handle_map.clone() else {
                        return syn::Error::new_spanned(
                            method_item.sig,
                            "trait export entries require `#[dispatch(...)]` mapping",
                        )
                        .to_compile_error();
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
                            return syn::Error::new_spanned(
                                &arg.ty,
                                "in trait export entries, `#[dispatch]` arguments must use `Self` or a trait type parameter (for example `T`), not a concrete type",
                            )
                            .to_compile_error();
                        }
                    }
                    let method_handle_id_specs =
                        match dispatch::resolve_poly_handle_id_specs_for_sig(
                            &method_item.sig,
                            &method_item.attrs,
                            &[],
                            &key_types,
                        ) {
                            Ok(specs) => specs,
                            Err(err) => return err.to_compile_error(),
                        };
                    let poly = ExportPolySpec {
                        trait_path: Some(trait_path.clone()),
                        trait_generics: trait_item.generics.clone(),
                        method: method_item.sig.ident.clone(),
                        decl_sig: method_item.sig.clone(),
                        body: method_item.default.into_token_stream(),
                        key_types,
                        handle_id_specs: method_handle_id_specs,
                        unsafe_name: None,
                        unsafe_no_mangle: false,
                    };
                    let mut combined_attrs = trait_item.attrs.clone();
                    combined_attrs.extend(method_item.attrs.clone());
                    selected_method_exports.push(gen_poly_export(&poly, Some(&abi)));
                }
            }
            syn::Item::Fn(decl_fn) => {
                if let Err(err) = validate_ownership_attrs(&decl_fn.attrs) {
                    return err.to_compile_error();
                }
                if let Err(err) = validate_sig_ownership_attrs(&decl_fn.sig) {
                    return err.to_compile_error();
                }
                if let Err(err) = ensure_no_handle_arg_attrs(&decl_fn.sig) {
                    return err.to_compile_error();
                }
                let sig = decl_fn.sig;
                if sig.receiver().is_some() {
                    return syn::Error::new_spanned(
                        &sig,
                        "free function export entries cannot declare a receiver",
                    )
                    .to_compile_error();
                }
            }
            syn::Item::Struct(type_decl) => {
                let vis = &type_decl.vis;
                let ident = &type_decl.ident;
                let generics = &type_decl.generics;
                let opaque_impls = derive_opaque_item(ident, generics);
                generated_items.push(quote! {
                    #vis struct #ident #generics;
                    #opaque_impls
                });
            }
            other => {
                return syn::Error::new_spanned(other, "item not supported").to_compile_error();
            }
        }
    }

    let generated = quote! {
        #(#generated_items)*
        #(
        const _: () = {
            #selected_method_exports
        };)*
    };

    generated
}

pub(crate) fn expand_extern_import_decls(
    abi: &syn::Abi,
    decls: Vec<syn::Item>,
    module_attrs: &[syn::Attribute],
    link_crate: Option<&syn::LitStr>,
) -> TokenStream {
    fn has_bare_dispatch_marker(attrs: &[syn::Attribute]) -> bool {
        attrs.iter().any(|attr| {
            if !attr.path().is_ident("dispatch") {
                return false;
            }
            match &attr.meta {
                syn::Meta::Path(_) => true,
                syn::Meta::List(list) => list.tokens.is_empty(),
                _ => false,
            }
        })
    }
    fn add_dispatch_attr_to_receiver(receiver: &mut syn::Receiver) {
        if receiver
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("dispatch"))
        {
            return;
        }
        receiver.attrs.push(syn::parse_quote!(#[dispatch]));
    }
    fn add_dispatch_attr_to_arg(arg: &mut syn::PatType) {
        if arg
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("dispatch"))
        {
            return;
        }
        arg.attrs.push(syn::parse_quote!(#[dispatch]));
    }
    fn base_type(ty: &syn::Type) -> &syn::Type {
        if let syn::Type::Reference(reference) = ty {
            return &reference.elem;
        }
        ty
    }
    fn is_handle_id_type(ty: &syn::Type) -> bool {
        let syn::Type::Path(type_path) = base_type(ty) else {
            return false;
        };
        let segments = type_path.path.segments.iter().collect::<Vec<_>>();
        match segments.as_slice() {
            [seg] => seg.ident == "Id",
            [first, second] => first.ident == "handle" && second.ident == "Id",
            [first, second, third] => {
                first.ident == "co3" && second.ident == "handle" && third.ident == "Id"
            }
            _ => false,
        }
    }
    fn is_self_type(ty: &syn::Type) -> bool {
        matches!(
            ty,
            syn::Type::Path(syn::TypePath { qself: None, path }) if path.is_ident("Self")
        )
    }
    fn dispatch_type_keys_from_map(
        handle_map: &BTreeMap<String, Vec<syn::Type>>,
    ) -> BTreeSet<String> {
        handle_map
            .values()
            .flat_map(|types| types.iter())
            .map(|ty| quote!(#ty).to_string())
            .collect()
    }
    fn annotate_dispatch_args(
        item_impl: &mut syn::ItemImpl,
        handle_map: Option<&BTreeMap<String, Vec<syn::Type>>>,
    ) {
        let trait_is_drop = item_impl
            .trait_
            .as_ref()
            .is_some_and(|(_, path, _)| path_symbol_name(path) == "Drop");
        let dispatch_type_keys = handle_map.map(dispatch_type_keys_from_map);

        for item in &mut item_impl.items {
            let syn::ImplItem::Fn(method) = item else {
                continue;
            };

            for input in &mut method.sig.inputs {
                match input {
                    syn::FnArg::Receiver(receiver) => {
                        let is_drop_receiver = trait_is_drop
                            && method.sig.ident == "drop"
                            && receiver.reference.is_some()
                            && receiver.mutability.is_some();
                        if handle_map.is_some() || is_drop_receiver {
                            add_dispatch_attr_to_receiver(receiver);
                        }
                    }
                    syn::FnArg::Typed(arg) => {
                        let ty = base_type(arg.ty.as_ref());
                        let should_dispatch = if let Some(keys) = &dispatch_type_keys {
                            is_self_type(ty) || keys.contains(&quote!(#ty).to_string())
                        } else {
                            false
                        };
                        if should_dispatch {
                            add_dispatch_attr_to_arg(arg);
                        }
                    }
                }
            }
        }
    }
    fn annotate_free_fn_handle_args(sig: &mut syn::Signature) {
        let typed_indices = sig
            .inputs
            .iter()
            .enumerate()
            .filter_map(|(idx, input)| matches!(input, syn::FnArg::Typed(_)).then_some(idx))
            .collect::<Vec<_>>();

        for &idx in &typed_indices {
            let syn::FnArg::Typed(arg) = &mut sig.inputs[idx] else {
                continue;
            };
            if is_handle_id_type(arg.ty.as_ref())
                && !arg.attrs.iter().any(|attr| attr.path().is_ident("by_val"))
            {
                arg.attrs.push(syn::parse_quote!(#[by_val]));
            }
        }

        for window in typed_indices.windows(2) {
            let [arg_idx, id_idx] = *window else {
                continue;
            };
            let should_dispatch = match (&sig.inputs[arg_idx], &sig.inputs[id_idx]) {
                (syn::FnArg::Typed(arg), syn::FnArg::Typed(id_arg)) => {
                    matches!(arg.ty.as_ref(), syn::Type::Reference(_))
                        && is_handle_id_type(id_arg.ty.as_ref())
                }
                _ => false,
            };
            if !should_dispatch {
                continue;
            }
            let syn::FnArg::Typed(arg) = &mut sig.inputs[arg_idx] else {
                continue;
            };
            if !arg
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("dispatch"))
            {
                arg.attrs.push(syn::parse_quote!(#[dispatch]));
            }
        }
    }
    fn type_ctor_ident(ty: &syn::Type) -> Option<&syn::Ident> {
        let syn::Type::Path(type_path) = ty else {
            return None;
        };
        type_path.path.segments.last().map(|seg| &seg.ident)
    }
    fn has_non_lifetime_generics(generics: &syn::Generics) -> bool {
        generics
            .params
            .iter()
            .any(|p| !matches!(p, syn::GenericParam::Lifetime(_)))
    }
    fn add_handle_bound_for_named_type(name: &syn::Ident, generics: &mut syn::Generics) {
        let cloned_generics = generics.clone();
        let (_, ty_generics, _) = cloned_generics.split_for_impl();
        generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#name #ty_generics: co3::handle::Handle));
    }
    fn impl_desc_needs_drop_self_handle_bound(
        impl_desc: &ImplDescriptor<'_>,
        impl_has_bare_dispatch: bool,
    ) -> bool {
        if !impl_has_bare_dispatch {
            return false;
        }
        if impl_desc
            .trait_name
            .is_none_or(|trait_name| path_symbol_name(trait_name) != "Drop")
        {
            return false;
        }
        has_non_lifetime_generics(impl_desc.generics)
    }
    fn parse_unique_link_name(attrs: &[syn::Attribute]) -> Option<syn::LitStr> {
        let mut out = None;
        for attr in attrs {
            if let Some(link_name) =
                parse_link_name_attr(attr).expect("link_name attrs were prevalidated")
                && out.is_none()
            {
                out = Some(link_name);
            }
        }
        out
    }
    fn type_is_ident(ty: &syn::Type, ident: &syn::Ident) -> bool {
        let syn::Type::Path(type_path) = ty else {
            return false;
        };
        type_path.qself.is_none()
            && type_path.path.segments.len() == 1
            && type_path.path.segments[0].ident == *ident
    }
    fn monomorphize_foreign_impl(
        item_impl: &syn::ItemImpl,
        handle_map: &BTreeMap<String, Vec<syn::Type>>,
        emitter: &mut Emitter,
    ) -> Option<Vec<syn::ItemImpl>> {
        fn infer_param_variants_from_self_map(
            self_ty: &syn::Type,
            param_ident: &syn::Ident,
            self_variants: &[syn::Type],
        ) -> Option<Vec<syn::Type>> {
            let syn::Type::Path(self_path) = self_ty else {
                return None;
            };
            let self_seg = self_path.path.segments.last()?;
            let syn::PathArguments::AngleBracketed(self_args) = &self_seg.arguments else {
                return None;
            };
            let param_pos = self_args.args.iter().position(|arg| {
                matches!(
                    arg,
                    syn::GenericArgument::Type(syn::Type::Path(type_path))
                        if type_path.qself.is_none()
                            && type_path.path.segments.len() == 1
                            && type_path.path.segments[0].ident == *param_ident
                )
            })?;

            let mut out = Vec::with_capacity(self_variants.len());
            for variant in self_variants {
                let syn::Type::Path(variant_path) = variant else {
                    return None;
                };
                let variant_seg = variant_path.path.segments.last()?;
                if variant_seg.ident != self_seg.ident {
                    return None;
                }
                let syn::PathArguments::AngleBracketed(variant_args) = &variant_seg.arguments
                else {
                    return None;
                };
                let arg = variant_args.args.iter().nth(param_pos)?;
                let syn::GenericArgument::Type(ty) = arg else {
                    return None;
                };
                out.push(ty.clone());
            }
            Some(out)
        }

        let mut type_params = Vec::<syn::Ident>::new();
        for param in &item_impl.generics.params {
            match param {
                syn::GenericParam::Type(type_param) => type_params.push(type_param.ident.clone()),
                syn::GenericParam::Lifetime(_) => {}
                syn::GenericParam::Const(const_param) => {
                    emit!(
                        emitter,
                        const_param,
                        "Const generics on extern impl blocks are not supported"
                    );
                    return None;
                }
            }
        }
        if type_params.is_empty() {
            return Some(vec![item_impl.clone()]);
        }

        let mut param_variants = BTreeMap::<String, Vec<syn::Type>>::new();
        for ident in &type_params {
            let key = ident.to_string();
            if let Some(types) = handle_map.get(&key) {
                param_variants.insert(key, types.clone());
                continue;
            }
            if let Some(types) = handle_map.get("Self")
                && type_is_ident(item_impl.self_ty.as_ref(), ident)
            {
                param_variants.insert(key, types.clone());
                continue;
            }
            if let Some(self_types) = handle_map.get("Self")
                && let Some(inferred) = infer_param_variants_from_self_map(
                    item_impl.self_ty.as_ref(),
                    ident,
                    self_types,
                )
            {
                param_variants.insert(key, inferred);
                continue;
            }
            emit!(
                emitter,
                ident,
                "generic extern impl parameter `{}` requires `#[dispatch({} = [Type, ...])]` mapping or `Self` mapping when used as the self type",
                ident,
                ident
            );
            return None;
        }

        let mut arms = 1usize;
        for types in param_variants.values() {
            if types.is_empty() {
                emit!(
                    emitter,
                    item_impl,
                    "handle mapping lists for generic extern impls may not be empty"
                );
                return None;
            }
            arms = arms.max(types.len());
        }
        for (param, types) in &param_variants {
            if types.len() != 1 && types.len() != arms {
                emit!(
                    emitter,
                    item_impl,
                    "generic extern impl mapping for `{}` must have either 1 entry or {} entries",
                    param,
                    arms
                );
                return None;
            }
        }

        let mut mono_impls = Vec::<syn::ItemImpl>::new();
        for idx in 0..arms {
            let mut subst = BTreeMap::<String, syn::Type>::new();
            for (param, types) in &param_variants {
                let selected = if types.len() == 1 {
                    types[0].clone()
                } else {
                    types[idx].clone()
                };
                subst.insert(param.clone(), selected);
            }

            struct Rewriter<'a> {
                subst: &'a BTreeMap<String, syn::Type>,
            }
            impl syn::visit_mut::VisitMut for Rewriter<'_> {
                fn visit_type_mut(&mut self, node: &mut syn::Type) {
                    if let syn::Type::Path(type_path) = node
                        && type_path.qself.is_none()
                        && type_path.path.segments.len() == 1
                        && matches!(
                            type_path.path.segments[0].arguments,
                            syn::PathArguments::None
                        )
                    {
                        let key = type_path.path.segments[0].ident.to_string();
                        if let Some(replacement) = self.subst.get(&key) {
                            *node = replacement.clone();
                            return;
                        }
                    }
                    syn::visit_mut::visit_type_mut(self, node);
                }
            }

            let mut monomorphized = item_impl.clone();
            let mut rewriter = Rewriter { subst: &subst };
            syn::visit_mut::VisitMut::visit_item_impl_mut(&mut rewriter, &mut monomorphized);
            monomorphized.generics.params = monomorphized
                .generics
                .params
                .into_iter()
                .filter(|param| matches!(param, syn::GenericParam::Lifetime(_)))
                .collect();
            if monomorphized.generics.params.is_empty() {
                monomorphized.generics.lt_token = None;
                monomorphized.generics.gt_token = None;
            }
            mono_impls.push(monomorphized);
        }
        Some(mono_impls)
    }

    let mut emitter = Emitter::new();
    let import_prefix = link_crate.map(|link_prefix| quote!(concat!(#link_prefix, "_")));
    let mut out = Vec::new();
    let mut type_names_with_drop_handle_bound = BTreeSet::<String>::new();
    for decl in &decls {
        let syn::Item::Impl(impl_decl) = decl else {
            continue;
        };
        let Some((_, trait_path, _)) = &impl_decl.trait_ else {
            continue;
        };
        if path_symbol_name(trait_path) != "Drop" {
            continue;
        }
        if !has_non_lifetime_generics(&impl_decl.generics) {
            continue;
        }
        let Some(type_ident) = type_ctor_ident(impl_decl.self_ty.as_ref()) else {
            continue;
        };
        if has_bare_dispatch_marker(&impl_decl.attrs) {
            type_names_with_drop_handle_bound.insert(type_ident.to_string());
        }
    }

    for decl in decls {
        match decl {
            syn::Item::Impl(decl) => {
                let impl_dispatch_map = dispatch::parse_impl_dispatch_attrs(&decl.attrs)
                    .expect("extern impl attrs were prevalidated");
                let impl_has_bare_dispatch = has_bare_dispatch_marker(&decl.attrs);
                let method_sigs = decl
                    .items
                    .iter()
                    .filter_map(|item| match item {
                        syn::ImplItem::Fn(method) => {
                            Some((method.sig.ident.to_string(), method.sig.clone()))
                        }
                        _ => None,
                    })
                    .collect::<BTreeMap<_, _>>();
                let items = decl.items.iter().map(|item| match item {
                    syn::ImplItem::Fn(m) => {
                        let attrs: Vec<_> = m
                            .attrs
                            .iter()
                            .filter(|attr| parse_link_attr(attr).ok().flatten().is_none())
                            .collect();
                        let vis = &m.vis;
                        let mut sig = m.sig.clone();
                        strip_internal_sig_attrs(&mut sig);
                        quote! {
                            #(#module_attrs)*
                            #(#attrs)*
                            #vis #sig {
                                unreachable!("replaced by extern_")
                            }
                        }
                    }
                    syn::ImplItem::Type(assoc) => quote! {
                        #(#module_attrs)*
                        #assoc
                    },
                    syn::ImplItem::Const(assoc) => quote! {
                        #(#module_attrs)*
                        #assoc
                    },
                    other => quote! {
                        #(#module_attrs)*
                        #other
                    },
                });

                let attrs = &decl.attrs;
                let generics = &decl.generics;
                let self_ty = &decl.self_ty;
                let defaultness = &decl.defaultness;
                let unsafety = &decl.unsafety;
                let impl_item_tokens = if let Some((bang, trait_path, for_token)) = &decl.trait_ {
                    quote! {
                        #(#attrs)* #defaultness #unsafety impl #generics #bang #trait_path #for_token #self_ty {
                            #(#items)*
                        }
                    }
                } else {
                    quote! {
                        #(#attrs)* #defaultness #unsafety impl #generics #self_ty {
                            #(#items)*
                        }
                    }
                };
                let Some(item_impl) =
                    emitter.handle(syn::parse2::<syn::ItemImpl>(impl_item_tokens))
                else {
                    continue;
                };

                let has_non_lifetime_generics = item_impl
                    .generics
                    .params
                    .iter()
                    .any(|p| !matches!(p, syn::GenericParam::Lifetime(_)));
                let item_impls = if has_non_lifetime_generics {
                    if let Some(handle_map) = impl_dispatch_map.clone() {
                        let Some(monomorphized) =
                            monomorphize_foreign_impl(&item_impl, &handle_map, &mut emitter)
                        else {
                            continue;
                        };
                        monomorphized
                    } else {
                        vec![item_impl]
                    }
                } else {
                    vec![item_impl]
                };

                for mut item_impl in item_impls {
                    annotate_dispatch_args(&mut item_impl, impl_dispatch_map.as_ref());
                    let Some(mut impl_desc) =
                        ImplDescriptor::from_foreign_impl(&mut emitter, &item_impl)
                    else {
                        continue;
                    };
                    for fn_ in &mut impl_desc.fns {
                        if let Some(sig) = method_sigs.get(&fn_.sig.ident.to_string()) {
                            apply_sig_attrs_to_fn_descriptor(fn_, sig);
                        }
                    }
                    let mut invalid_handle_id_specs = false;
                    for fn_ in &impl_desc.fns {
                        let skip_defaults = impl_desc
                            .trait_name
                            .is_some_and(|trait_name| path_symbol_name(trait_name) == "Drop")
                            && fn_.sig.ident == "drop"
                            && !impl_has_bare_dispatch;
                        let required_selectors = if has_non_lifetime_generics {
                            Vec::new()
                        } else {
                            impl_dispatch_map
                                .as_ref()
                                .map(|map| map.keys().cloned().collect())
                                .unwrap_or_default()
                        };
                        if let Err(err) = dispatch::resolve_default_handle_id_specs_for_fn(
                            fn_,
                            skip_defaults,
                            &[],
                            required_selectors,
                        ) {
                            emit!(emitter, err.span(), "{}", err);
                            invalid_handle_id_specs = true;
                        }
                    }
                    if invalid_handle_id_specs {
                        continue;
                    }

                    let wrapped_methods = impl_desc
                        .fns
                        .iter()
                        .map(|fn_| {
                            let method_attrs = fn_
                                .attrs
                                .iter()
                                .map(|attr| (*attr).clone())
                                .collect::<Vec<_>>();
                            let method_link_name = parse_unique_link_name(&method_attrs);
                            let method_import_name = method_link_name.as_ref();
                            let skip_defaults = impl_desc
                                .trait_name
                                .is_some_and(|trait_name| path_symbol_name(trait_name) == "Drop")
                                && fn_.sig.ident == "drop"
                                && !impl_has_bare_dispatch;
                            let required_selectors = if has_non_lifetime_generics {
                                Vec::new()
                            } else {
                                impl_dispatch_map
                                    .as_ref()
                                    .map(|map| map.keys().cloned().collect())
                                    .unwrap_or_default()
                            };
                            let method_handle_id_specs =
                                dispatch::resolve_default_handle_id_specs_for_fn(
                                    fn_,
                                    skip_defaults,
                                    &[],
                                    required_selectors,
                                )
                                .expect("extern method handle ids were prevalidated");
                            let method_prefix = if method_import_name.is_some() {
                                None
                            } else {
                                import_prefix.as_ref()
                            };
                            wrapper::wrap_method_with_import(
                                fn_,
                                impl_desc.trait_name,
                                method_prefix,
                                method_import_name,
                                Some(abi),
                                &method_handle_id_specs,
                            )
                        })
                        .collect::<Vec<_>>();

                    let self_ty = &item_impl.self_ty;
                    let impl_trait_for = impl_desc
                        .trait_name
                        .map(|trait_name| quote! { #trait_name for });
                    let (associated_names, associated_types) =
                        impl_desc.associated_types.iter().fold(
                            (Vec::new(), Vec::new()),
                            |(mut names, mut types), (name, ty)| {
                                names.push(name);
                                types.push(ty);
                                (names, types)
                            },
                        );
                    let mut associated_const_names = Vec::new();
                    let mut associated_const_types = Vec::new();
                    let mut associated_const_values = Vec::new();
                    for (name, ty, value) in &impl_desc.associated_consts {
                        associated_const_names.push(name);
                        associated_const_types.push(ty);
                        associated_const_values.push(value);
                    }
                    let mut generics = impl_desc.generics.clone();
                    if impl_desc_needs_drop_self_handle_bound(&impl_desc, impl_has_bare_dispatch) {
                        let self_ty = item_impl.self_ty.as_ref().clone();
                        generics
                            .make_where_clause()
                            .predicates
                            .push(syn::parse_quote!(#self_ty: co3::handle::Handle));
                    }
                    let (impl_generics, _, where_clause) = generics.split_for_impl();

                    out.push(quote! {
                        impl #impl_generics #impl_trait_for #self_ty #where_clause {
                            #(type #associated_names = #associated_types;)*
                            #(const #associated_const_names: #associated_const_types = #associated_const_values;)*
                            #(#wrapped_methods)*
                        }
                    });
                }
            }
            syn::Item::Struct(decl) => {
                let type_attrs = decl.attrs.clone();
                let vis = &decl.vis;
                let ident = &decl.ident;
                let mut generics = decl.generics.clone();
                if type_names_with_drop_handle_bound.contains(&ident.to_string()) {
                    add_handle_bound_for_named_type(ident, &mut generics);
                }
                let (decl_generics, _, decl_where_clause) = generics.split_for_impl();

                let Some(item) = emitter.handle(syn::parse2::<syn::DeriveInput>(quote! {
                    #(#module_attrs)*
                    #(#type_attrs)*
                    #vis struct #ident #decl_generics #decl_where_clause;
                })) else {
                    continue;
                };
                let Some(input) =
                    emitter.handle(crate::repr::FfiTypeInput::from_derive_input(&item))
                else {
                    continue;
                };

                out.push(wrapper::wrap_as_opaque(input));
            }
            syn::Item::Fn(decl) => {
                let fn_attrs: Vec<_> = decl
                    .attrs
                    .iter()
                    .filter(|attr| parse_link_attr(attr).ok().flatten().is_none())
                    .collect();
                let vis = &decl.vis;
                let mut sig = decl.sig.clone();
                strip_internal_sig_attrs(&mut sig);
                let mut descriptor_sig = sig.clone();
                annotate_free_fn_handle_args(&mut descriptor_sig);
                let Some(item_fn) = emitter.handle(syn::parse2::<syn::ItemFn>(quote! {
                    #(#module_attrs)*
                    #(#fn_attrs)*
                    #vis #descriptor_sig {
                        unreachable!("replaced by extern_")
                    }
                })) else {
                    continue;
                };

                let item_link_name = parse_unique_link_name(&item_fn.attrs);

                let Some(mut fn_descriptor) = FnDescriptor::from_fn(&mut emitter, &item_fn) else {
                    continue;
                };
                apply_sig_attrs_to_fn_descriptor(&mut fn_descriptor, &descriptor_sig);
                let import_name = item_link_name.as_ref();
                let method_prefix = if import_name.is_some() {
                    None
                } else {
                    import_prefix.as_ref()
                };
                out.push(wrapper::wrap_method_with_import(
                    &fn_descriptor,
                    None,
                    method_prefix,
                    import_name,
                    Some(abi),
                    &[],
                ));
            }
            syn::Item::Trait(_) => {
                unreachable!("extern trait declarations are rejected in validation")
            }
            other => {
                emit!(emitter, other, "item not supported");
            }
        }
    }

    emitter.finish_token_stream_with(quote!(#(#out)*))
}

fn derive_opaque_item(name: &syn::Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let params = &generics.params;

    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    // TODO: Implement ?Sized Opaque types
    let sized_impls = quote! {
        impl #impl_generics co3::ir::SizeFamily for #name #ty_generics where Self: Sized, #predicates {
            type Kind = co3::ir::Sized_;
        }

        impl #impl_generics co3::borrow::Borrow for #name #ty_generics where Self: Sized, #predicates {
            type Borrowed<'itm>
                = Self
            where
                Self: 'itm;

            type Store = ();

            #[inline(always)]
            fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
            where
                Self: 'itm,
            {
                self
            }
        }

        impl<'_ršč, #params> co3::borrow::ToOwned<'_ršč> for #name #ty_generics where Self: Sized + '_ršč, #predicates {
            #[inline(always)]
            fn to_owned(borrowed: Self::Borrowed<'_ršč>) -> Self {
                borrowed
            }
        }

        impl #impl_generics co3::niche::Niche for #name #ty_generics where Self: Sized, #predicates {
            const NICHE_VALUE: co3::boxed::CBox<Self> = co3::boxed::CBox::none();
        }
    };

    quote! {
        impl #impl_generics co3::ir::ReprFamily for #name #ty_generics #where_clause {
            type Kind = co3::ir::Opaque;
        }

        impl #impl_generics co3::borrow::DropFamily for #name #ty_generics #where_clause {
            type Kind = co3::borrow::NoDrop;
        }

        impl #impl_generics co3::niche::NicheFamily for #name #ty_generics #where_clause {
            type Kind = co3::niche::WithCustomNiche;
        }

        #sized_impls
    }
}
