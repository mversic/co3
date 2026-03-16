use std::collections::BTreeMap;

use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::visit_mut::VisitMut;

use crate::{
    OwnershipMode, handle_id_positions,
    impl_visitor::{FnDescriptor, path_symbol_name},
    input_ownerships, ownership_mode_for_receiver, ownership_mode_for_type, receiver_ownership,
    utils::{SignatureLifetimeBuilder, unwrap_result_type},
    wrapper::HandleIdSpec,
};

#[derive(Clone)]
pub(crate) struct ExportPolySpec {
    pub(crate) trait_path: Option<syn::Path>,
    pub(crate) trait_generics: syn::Generics,
    pub(crate) method: syn::Ident,
    pub(crate) decl_sig: syn::Signature,
    pub(crate) body: TokenStream,
    pub(crate) key_types: BTreeMap<String, Vec<syn::Type>>,
    pub(crate) handle_id_specs: Vec<HandleIdSpec>,
    pub(crate) unsafe_name: Option<syn::LitStr>,
    pub(crate) unsafe_no_mangle: bool,
}

fn gen_poly_export_name(spec: &ExportPolySpec) -> TokenStream {
    if let Some(name) = &spec.unsafe_name {
        quote!(#name)
    } else if spec.unsafe_no_mangle {
        let name = syn::LitStr::new(&spec.method.to_string(), proc_macro2::Span::call_site());
        quote!(#name)
    } else {
        let owner = if let Some(trait_path) = &spec.trait_path {
            path_symbol_name(trait_path)
        } else {
            spec.key_types
                .get("Self")
                .and_then(|tys| tys.first())
                .and_then(|ty| match ty {
                    syn::Type::Path(type_path) if type_path.qself.is_none() => {
                        Some(path_symbol_name(&type_path.path))
                    }
                    _ => None,
                })
                .unwrap_or_else(|| "Self".to_owned())
        };
        let owner_lit = syn::LitStr::new(&owner, proc_macro2::Span::call_site());
        let method_lit = syn::LitStr::new(&spec.method.to_string(), proc_macro2::Span::call_site());
        quote!(concat!(env!("CARGO_CRATE_NAME"), "_", #owner_lit, "_", #method_lit))
    }
}

pub(crate) fn gen_poly_export(spec: &ExportPolySpec, export_abi: Option<&syn::Abi>) -> TokenStream {
    fn has_dispatch_attr(attrs: &[syn::Attribute]) -> bool {
        attrs.iter().any(|attr| attr.path().is_ident("dispatch"))
    }
    fn selector_from_arg_type(ty: &syn::Type) -> syn::Type {
        if let syn::Type::Reference(reference) = ty {
            return (*reference.elem).clone();
        }
        ty.clone()
    }
    fn contains_self_type(ty: &syn::Type) -> bool {
        struct Finder {
            found: bool,
        }
        impl syn::visit::Visit<'_> for Finder {
            fn visit_type_path(&mut self, node: &syn::TypePath) {
                if node.qself.is_none() && node.path.is_ident("Self") {
                    self.found = true;
                    return;
                }
                syn::visit::visit_type_path(self, node);
            }
        }
        let mut finder = Finder { found: false };
        syn::visit::Visit::visit_type(&mut finder, ty);
        finder.found
    }
    fn rewrite_selectors_in_type(
        ty: &syn::Type,
        selector_map: &BTreeMap<String, syn::Type>,
    ) -> syn::Type {
        struct Rewriter<'a> {
            selector_map: &'a BTreeMap<String, syn::Type>,
        }

        impl VisitMut for Rewriter<'_> {
            fn visit_type_mut(&mut self, node: &mut syn::Type) {
                if let syn::Type::Path(type_path) = node
                    && type_path.qself.is_none()
                {
                    let key = type_path.path.to_token_stream().to_string();
                    if let Some(replacement) = self.selector_map.get(&key) {
                        *node = replacement.clone();
                        return;
                    }
                }
                syn::visit_mut::visit_type_mut(self, node);
            }
        }

        let mut out = ty.clone();
        Rewriter { selector_map }.visit_type_mut(&mut out);
        out
    }
    fn rewrite_dispatch_body(
        body: &TokenStream,
        value_names: &std::collections::BTreeSet<String>,
        selector_map: &BTreeMap<String, syn::Type>,
    ) -> syn::Result<syn::Block> {
        fn recurse(
            tokens: TokenStream,
            value_names: &std::collections::BTreeSet<String>,
            selector_map: &BTreeMap<String, syn::Type>,
        ) -> TokenStream {
            let items = tokens.into_iter().collect::<Vec<_>>();
            let mut out = TokenStream::new();
            let mut idx = 0usize;
            while idx < items.len() {
                if idx + 4 < items.len()
                    && let proc_macro2::TokenTree::Ident(base_ident) = &items[idx]
                    && matches!(&items[idx + 1], proc_macro2::TokenTree::Punct(p) if p.as_char() == ':')
                    && matches!(&items[idx + 2], proc_macro2::TokenTree::Punct(p) if p.as_char() == ':')
                    && matches!(&items[idx + 3], proc_macro2::TokenTree::Punct(p) if p.as_char() == '<')
                    && matches!(&items[idx + 4], proc_macro2::TokenTree::Ident(_))
                {
                    let mut end = idx + 5;
                    let mut depth = 1usize;
                    while end < items.len() {
                        match &items[end] {
                            proc_macro2::TokenTree::Punct(p) if p.as_char() == '<' => depth += 1,
                            proc_macro2::TokenTree::Punct(p) if p.as_char() == '>' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        end += 1;
                    }
                    let base = base_ident.to_string();
                    if end == idx + 5 {
                        if value_names.contains(&base) {
                            out.extend(core::iter::once(proc_macro2::TokenTree::Ident(
                                base_ident.clone(),
                            )));
                            idx = end + 1;
                            continue;
                        }
                        if let Some(replacement) = selector_map.get(&base) {
                            out.extend(quote!(#replacement));
                            idx = end + 1;
                            continue;
                        }
                    }
                }

                let tt = match &items[idx] {
                    proc_macro2::TokenTree::Group(group) => {
                        let mut new_group = proc_macro2::Group::new(
                            group.delimiter(),
                            recurse(group.stream(), value_names, selector_map),
                        );
                        new_group.set_span(group.span());
                        proc_macro2::TokenTree::Group(new_group)
                    }
                    other => other.clone(),
                };
                out.extend(core::iter::once(tt));
                idx += 1;
            }
            out
        }

        let rewritten = recurse(body.clone(), value_names, selector_map);
        syn::parse2(quote!({ #rewritten }))
    }
    fn handle_ffi_ty_from_rust(ty: &syn::Type, ownership_mode: OwnershipMode) -> syn::Type {
        if ownership_mode == OwnershipMode::Borrow {
            if let syn::Type::Reference(reference) = ty {
                if reference.mutability.is_some() {
                    syn::parse_quote!(*mut core::ffi::c_void)
                } else {
                    syn::parse_quote!(*const core::ffi::c_void)
                }
            } else {
                syn::parse_quote!(*const core::ffi::c_void)
            }
        } else if let syn::Type::Reference(reference) = ty {
            if reference.mutability.is_some() {
                syn::parse_quote!(*mut core::ffi::c_void)
            } else {
                syn::parse_quote!(*const core::ffi::c_void)
            }
        } else {
            syn::parse_quote!(*mut core::ffi::c_void)
        }
    }
    fn inject_missing_lifetimes(ty: &mut syn::Type, lifetime_name: &str) {
        struct LifetimeInjector<'a> {
            lifetime: &'a str,
        }

        impl VisitMut for LifetimeInjector<'_> {
            fn visit_type_reference_mut(&mut self, node: &mut syn::TypeReference) {
                if node.lifetime.is_none() {
                    node.lifetime = Some(syn::Lifetime::new(
                        &format!("'{}", self.lifetime),
                        proc_macro2::Span::call_site(),
                    ));
                }
                syn::visit_mut::visit_type_reference_mut(self, node);
            }
        }

        VisitMut::visit_type_mut(
            &mut LifetimeInjector {
                lifetime: lifetime_name,
            },
            ty,
        );
    }
    fn borrowed_body_src_type(mut ty: syn::Type) -> TokenStream {
        inject_missing_lifetimes(&mut ty, "_");
        quote!(<#ty as co3::borrow::Borrow>::Borrowed<'_>)
    }
    fn inject_handle_id_decl_args(handle_id_specs: &[HandleIdSpec], args: &mut Vec<TokenStream>) {
        let mut inserts: Vec<(usize, usize, TokenStream)> = handle_id_specs
            .iter()
            .enumerate()
            .map(|(order, spec)| {
                let arg_name = &spec.arg_name;
                (spec.at, order, quote!(#arg_name: co3::handle::Id))
            })
            .collect();
        inserts.sort_by(|(a_at, a_order, _), (b_at, b_order, _)| {
            a_at.cmp(b_at).then(a_order.cmp(b_order))
        });

        if inserts.is_empty() {
            return;
        }

        let real_args = core::mem::take(args);
        let final_len = real_args.len() + inserts.len();
        let mut slots: Vec<Option<TokenStream>> = vec![None; final_len];

        for (at, _order, decl) in inserts {
            let desired = core::cmp::min(at, final_len.saturating_sub(1));
            let mut pos = desired;
            while pos < final_len && slots[pos].is_some() {
                pos += 1;
            }
            if pos == final_len {
                pos = 0;
                while pos < desired && slots[pos].is_some() {
                    pos += 1;
                }
            }
            if pos < final_len {
                slots[pos] = Some(decl);
            }
        }

        let mut real_iter = real_args.into_iter();
        for slot in &mut slots {
            if slot.is_none() {
                *slot = real_iter.next();
            }
        }
        *args = slots.into_iter().flatten().collect();
    }

    let abi = spec
        .decl_sig
        .abi
        .clone()
        .or_else(|| export_abi.cloned())
        .unwrap_or_else(|| syn::parse_quote!(extern "Rust"));
    let fn_name = spec.method.clone();
    let export_name = gen_poly_export_name(spec);
    let Some(handle_types) = spec.key_types.get("Self") else {
        return syn::Error::new_spanned(
            &spec.decl_sig,
            "poly selector export requires `#[dispatch(Self = [Type, ...])]`",
        )
        .to_compile_error();
    };
    if handle_types.is_empty() {
        return syn::Error::new_spanned(
            &spec.decl_sig,
            "poly selector handle list may not be empty",
        )
        .to_compile_error();
    }
    let handle_id_specs = spec.handle_id_specs.clone();
    let mut seen_handle_id_names = std::collections::BTreeSet::<String>::new();
    let handle_id_decode_stmts = handle_id_specs
        .iter()
        .filter_map(|spec| {
            let key = spec.arg_name.to_string();
            if !seen_handle_id_names.insert(key) {
                return None;
            }
            let arg_name = &spec.arg_name;
            Some(quote! {
                let #arg_name: co3::handle::Id = co3::Decode::decode(#arg_name, &mut ())
                    .ok_or(co3::FfiReturn::TrapRepresentation)?;
            })
        })
        .collect::<Vec<_>>();
    #[derive(Clone)]
    struct ArgInfo {
        name: syn::Ident,
        ty: syn::Type,
        is_handle: bool,
        ownership_mode: OwnershipMode,
        is_receiver: bool,
        receiver_is_mut: bool,
        selector_key: Option<String>,
    }

    fn is_handle_id_arg(name: &syn::Ident, handle_id_specs: &[HandleIdSpec]) -> bool {
        handle_id_specs.iter().any(|spec| spec.arg_name == *name)
    }

    let mut args = Vec::<ArgInfo>::new();
    let mut typed_input_idx = 0usize;
    for input in &spec.decl_sig.inputs {
        match input {
            syn::FnArg::Receiver(receiver) => {
                let rust_ty: syn::Type = if receiver.reference.is_none() {
                    syn::parse_quote!(Self)
                } else if receiver.mutability.is_some() {
                    syn::parse_quote!(&mut Self)
                } else {
                    syn::parse_quote!(&Self)
                };
                args.push(ArgInfo {
                    name: format_ident!("self_"),
                    ty: rust_ty,
                    is_handle: has_dispatch_attr(&receiver.attrs)
                        || spec.key_types.contains_key("Self"),
                    ownership_mode: receiver_ownership(&spec.decl_sig)
                        .unwrap_or_else(|| ownership_mode_for_receiver(receiver)),
                    is_receiver: true,
                    receiver_is_mut: receiver.mutability.is_some(),
                    selector_key: Some("Self".to_string()),
                });
            }
            syn::FnArg::Typed(arg) => {
                let syn::Pat::Ident(pat_ident) = arg.pat.as_ref() else {
                    return syn::Error::new_spanned(
                        &arg.pat,
                        "poly selector arguments must use identifier patterns",
                    )
                    .to_compile_error();
                };
                let selector = selector_from_arg_type(&arg.ty);
                let selector_key = selector_key_name(&selector);
                args.push(ArgInfo {
                    name: pat_ident.ident.clone(),
                    ty: arg.ty.as_ref().clone(),
                    is_handle: has_dispatch_attr(&arg.attrs)
                        || spec.key_types.contains_key(&selector_key),
                    ownership_mode: input_ownerships(&spec.decl_sig)
                        .get(typed_input_idx)
                        .copied()
                        .unwrap_or_else(|| ownership_mode_for_type(arg.ty.as_ref())),
                    is_receiver: false,
                    receiver_is_mut: false,
                    selector_key: if has_dispatch_attr(&arg.attrs)
                        || spec.key_types.contains_key(&selector_key)
                    {
                        Some(selector_key)
                    } else {
                        None
                    },
                });
                typed_input_idx += 1;
            }
        }
    }

    let mut expr_selector_map = BTreeMap::<String, String>::new();
    let mut value_names = std::collections::BTreeSet::<String>::new();
    if let Some(receiver) = args.iter().find(|arg| arg.is_receiver) {
        expr_selector_map.insert("self".to_string(), receiver.selector_key.clone().unwrap());
        value_names.insert("self".to_string());
    }
    for arg in &args {
        if !arg.is_receiver
            && let Some(selector_key) = &arg.selector_key
        {
            expr_selector_map.insert(arg.name.to_string(), selector_key.clone());
            value_names.insert(arg.name.to_string());
        }
    }
    let mut id_selector_map = BTreeMap::<String, String>::new();
    let mut selector_id_map = BTreeMap::<String, String>::new();
    for handle_id_spec in &handle_id_specs {
        let selector_key = selector_key_name(&handle_id_spec.selector);
        id_selector_map.insert(handle_id_spec.arg_name.to_string(), selector_key.clone());
        selector_id_map.insert(selector_key.clone(), handle_id_spec.arg_name.to_string());
    }
    let method = &spec.method;
    let is_drop_poly = spec
        .trait_path
        .as_ref()
        .is_some_and(|trait_path| path_symbol_name(trait_path) == "Drop")
        && method == "drop";
    let handle_id_spec_map = handle_id_specs
        .iter()
        .map(|spec| (spec.arg_name.to_string(), spec.arg_name.clone()))
        .collect::<BTreeMap<_, _>>();
    let id_idents = handle_id_specs
        .iter()
        .map(|spec| spec.arg_name.to_string())
        .filter(|name| id_selector_map.contains_key(name))
        .collect::<Vec<_>>();

    let mut decl_params = Vec::<TokenStream>::new();
    let mut lifetime_builder =
        SignatureLifetimeBuilder::new(&[&spec.trait_generics, &spec.decl_sig.generics]);
    for arg in &args {
        if is_handle_id_arg(&arg.name, &handle_id_specs) {
            continue;
        }
        let arg_name = &arg.name;
        let ffi_ty: syn::Type = if arg.is_handle {
            handle_ffi_ty_from_rust(&arg.ty, arg.ownership_mode)
        } else if arg.ownership_mode == OwnershipMode::Borrow {
            let borrowed_ty = lifetime_builder.borrowed_src_type(arg.ty.clone());
            syn::parse_quote!(<#borrowed_ty as co3::ExternC>::CType)
        } else {
            let ty = &arg.ty;
            syn::parse_quote!(<#ty as co3::ExternC>::CType)
        };
        decl_params.push(quote!(#arg_name: #ffi_ty));
    }
    inject_handle_id_decl_args(&handle_id_specs, &mut decl_params);
    let (export_generics, export_where_clause) =
        lifetime_builder.split_for_signature(&[&spec.trait_generics, &spec.decl_sig.generics]);
    let raw_output_ty = match &spec.decl_sig.output {
        syn::ReturnType::Default => None,
        syn::ReturnType::Type(_, ty) => Some((**ty).clone()),
    };
    let output_ty = raw_output_ty
        .as_ref()
        .and_then(|ty| unwrap_result_type(ty).map(|(ok, _)| ok.clone()))
        .or(raw_output_ty.clone());
    let output_is_result = raw_output_ty
        .as_ref()
        .is_some_and(|ty| unwrap_result_type(ty).is_some());
    let output_contains_self = output_ty.as_ref().is_some_and(contains_self_type);
    if let Some(out_ty) = &output_ty {
        if output_contains_self {
            decl_params.push(quote!(out_ptr: *mut *mut core::ffi::c_void));
        } else {
            decl_params.push(quote!(out_ptr: *mut <#out_ty as co3::out_ptr::OutPtr>::OutPtr));
        }
    }

    let used_ids = id_idents
        .iter()
        .filter_map(|name| handle_id_spec_map.get(name).cloned())
        .collect::<Vec<_>>();
    fn enumerate_selector_maps(
        ids: &[String],
        id_selector_map: &BTreeMap<String, String>,
        key_types: &BTreeMap<String, Vec<syn::Type>>,
        idx: usize,
        current: &mut BTreeMap<String, syn::Type>,
        out: &mut Vec<BTreeMap<String, syn::Type>>,
    ) -> syn::Result<()> {
        if idx == ids.len() {
            out.push(current.clone());
            return Ok(());
        }
        let id = &ids[idx];
        let selector = id_selector_map
            .get(id)
            .expect("collected from the same map");
        let types = key_types.get(selector).ok_or_else(|| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                format!("missing dispatch mapping for selector `{selector}`"),
            )
        })?;
        if types.is_empty() {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                format!("selector `{selector}` has an empty handle list"),
            ));
        }
        for ty in types {
            current.insert(selector.clone(), ty.clone());
            enumerate_selector_maps(ids, id_selector_map, key_types, idx + 1, current, out)?;
        }
        current.remove(selector);
        Ok(())
    }
    let mut selector_maps = Vec::new();
    if let Err(err) = enumerate_selector_maps(
        &id_idents,
        &id_selector_map,
        &spec.key_types,
        0,
        &mut BTreeMap::new(),
        &mut selector_maps,
    ) {
        return err.to_compile_error();
    }

    let match_arms = selector_maps.into_iter().map(|selector_map| {
        let handle_ty = selector_map.get("Self").cloned().unwrap_or_else(|| handle_types[0].clone());

        let mut decode_stmts = Vec::<TokenStream>::new();
        let mut sync_stmts = Vec::<TokenStream>::new();

        for arg in &args {
            if is_drop_poly {
                continue;
            }
            if is_handle_id_arg(&arg.name, &handle_id_specs) {
                continue;
            }
            let arg_name = &arg.name;
            let rust_ty = rewrite_selectors_in_type(&arg.ty, &selector_map);
            let store = format_ident!("{arg_name}_store");
            let decode_input = if arg.is_handle {
                quote!(#arg_name as <#rust_ty as co3::ExternC>::CType)
            } else {
                quote!(#arg_name)
            };
            if !arg.is_handle && arg.ownership_mode == OwnershipMode::Borrow {
                let borrowed_ty = borrowed_body_src_type(rust_ty.clone());
                decode_stmts.push(quote! {
                    let mut #store = Default::default();
                    let #arg_name: #borrowed_ty = unsafe { co3::Decode::decode(#decode_input, &mut #store) }
                        .ok_or(co3::FfiReturn::TrapRepresentation)?;
                    let #arg_name: #rust_ty = co3::borrow::ToOwned::to_owned(#arg_name);
                });
            } else {
                decode_stmts.push(quote! {
                    let mut #store = Default::default();
                    let #arg_name: #rust_ty = unsafe { co3::Decode::decode(#decode_input, &mut #store) }
                        .ok_or(co3::FfiReturn::TrapRepresentation)?;
                });
            }
            if !arg.is_handle {
                sync_stmts.push(quote! {
                    co3::Store::sync(#store).ok_or(co3::FfiReturn::TrapRepresentation)?;
                });
            }
        }
        let body = match rewrite_dispatch_body(
            &spec.body,
            &value_names,
            &selector_map,
        ) {
            Ok(body) => body,
            Err(err) => return err.to_compile_error(),
        };

        let output_write = if is_drop_poly {
            let call_args_only = args
                .iter()
                .filter(|arg| !is_handle_id_arg(&arg.name, &handle_id_specs))
                .collect::<Vec<_>>();
            if call_args_only.len() != 1
                || !call_args_only[0].is_receiver
                || !call_args_only[0].is_handle
                || !call_args_only[0].receiver_is_mut
            {
                return syn::Error::new_spanned(
                    &spec.decl_sig,
                    "Drop poly selector must be `Drop::drop(&mut self)`",
                )
                .to_compile_error();
            }
            let arg_name = &call_args_only[0].name;
            quote! {
                let __self_ptr = #arg_name as *mut #handle_ty;
                unsafe {
                    let __owned: Box<#handle_ty> = Box::from_raw(__self_ptr);
                    core::mem::drop(__owned);
                }
            }
        } else if let Some(out_ty) = &output_ty {
            let rewritten = rewrite_selectors_in_type(out_ty, &selector_map);
            let rewritten_raw_output = raw_output_ty
                .as_ref()
                .map(|raw| rewrite_selectors_in_type(raw, &selector_map));
            if output_contains_self {
                let output_capture = if output_is_result {
                    let rewritten_raw_output =
                        rewritten_raw_output.expect("present when output_is_result");
                    quote! {
                        let output: #rewritten_raw_output = #body;
                        let output = output.map_err(|_| co3::FfiReturn::ExecutionFail)?;
                    }
                } else {
                    quote! {
                        let output: #rewritten = #body;
                    }
                };
                quote! {
                    #output_capture
                    let out_ptr = out_ptr.cast::<<#rewritten as co3::ExternC>::CType>();
                    unsafe { <#rewritten as co3::out_ptr::OutPtrWrite>::write_out(output, out_ptr) };
                }
            } else {
                let output_capture = if output_is_result {
                    let rewritten_raw_output =
                        rewritten_raw_output.expect("present when output_is_result");
                    quote! {
                        let output: #rewritten_raw_output = #body;
                        let output = output.map_err(|_| co3::FfiReturn::ExecutionFail)?;
                    }
                } else {
                    quote! {
                        let output: #rewritten = #body;
                    }
                };
                quote! {
                    #output_capture
                    unsafe { <#rewritten as co3::out_ptr::OutPtrWrite>::write_out(output, out_ptr) };
                }
            }
        } else {
            quote! {
                #body
            }
        };

        let match_pattern = used_ids.iter().map(|ident| {
            let selector_key = id_selector_map
                .get(&ident.to_string())
                .expect("collected from same map");
            let selector_ty = selector_map.get(selector_key).expect("enumerated selector map");
            quote!(<#selector_ty as co3::handle::Handle>::ID)
        });
        quote! {
            (#(#match_pattern),*) => {
                #(#decode_stmts)*
                #output_write
                #(#sync_stmts)*
            }
        }
    });

    quote! {
        #[unsafe(export_name = #export_name)]
        unsafe #abi fn #fn_name #export_generics (
            #(#decl_params),*
        ) -> co3::FfiReturn #export_where_clause {
            let fn_ = || {
                let fn_body = || -> Result<(), co3::FfiReturn> {
                    #(#handle_id_decode_stmts)*
                    match (#(#used_ids),*) {
                        #(#match_arms,)*
                        _ => return Err(co3::FfiReturn::UnknownHandle),
                    }
                    Ok(())
                };
                if let Err(err) = fn_body() {
                    return err;
                }
                co3::FfiReturn::Ok
            };

            match std::panic::catch_unwind(fn_) {
                Ok(res) => res,
                Err(_) => co3::FfiReturn::UnrecoverableError,
            }
        }
    }
}

fn selector_from_handle_id_type(ty: &syn::Type, span: proc_macro2::Span) -> syn::Result<syn::Type> {
    let syn::Type::Path(type_path) = ty else {
        return Err(syn::Error::new(
            span,
            "arguments with `_id` suffix must use `<Selector>::ID`",
        ));
    };
    let Some(last) = type_path.path.segments.last() else {
        return Err(syn::Error::new(
            span,
            "arguments with `_id` suffix must use `<Selector>::ID`",
        ));
    };
    if last.ident != "ID" {
        return Err(syn::Error::new(
            span,
            "arguments with `_id` suffix must use `<Selector>::ID`",
        ));
    }

    if let Some(qself) = &type_path.qself {
        if let syn::Type::Path(selector_path) = qself.ty.as_ref() {
            return Ok(syn::Type::Path(syn::TypePath {
                qself: None,
                path: selector_path.path.clone(),
            }));
        }
        return Ok((*qself.ty).clone());
    }

    let mut selector_path = type_path.path.clone();
    selector_path.segments.pop();
    if selector_path.segments.is_empty() {
        return Err(syn::Error::new(
            span,
            "arguments with `_id` suffix must use `<Selector>::ID`",
        ));
    }

    Ok(syn::Type::Path(syn::TypePath {
        qself: None,
        path: selector_path,
    }))
}

fn collect_explicit_handle_id_specs_from_sig(
    sig: &syn::Signature,
    _allowed_selectors: &std::collections::BTreeSet<String>,
    handle_id_positions: Option<&std::collections::BTreeMap<String, usize>>,
) -> syn::Result<Vec<HandleIdSpec>> {
    let mut out = Vec::new();
    let handle_id_positions = handle_id_positions.cloned().unwrap_or_default();
    let marked_positions = handle_id_positions
        .values()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();

    for (at, input) in sig.inputs.iter().enumerate() {
        let syn::FnArg::Typed(arg) = input else {
            continue;
        };
        let syn::Pat::Ident(pat_ident) = arg.pat.as_ref() else {
            continue;
        };
        if !marked_positions.contains(&at) {
            continue;
        }
        let selector = selector_from_handle_id_type(arg.ty.as_ref(), pat_ident.ident.span())?;

        out.push(HandleIdSpec {
            arg_name: pat_ident.ident.clone(),
            selector,
            at: handle_id_positions
                .get(&pat_ident.ident.to_string())
                .copied()
                .unwrap_or(at),
        });
    }

    Ok(out)
}

fn available_default_handle_selectors(fn_descriptor: &FnDescriptor) -> Vec<syn::Type> {
    let mut seen = std::collections::BTreeSet::<String>::new();
    let mut selectors = Vec::<syn::Type>::new();
    fn handle_selector_from_arg_type(ty: &syn::Type) -> syn::Type {
        match ty {
            syn::Type::Reference(reference) => (*reference.elem).clone(),
            syn::Type::Path(type_path) => {
                let Some(last) = type_path.path.segments.last() else {
                    return ty.clone();
                };
                if (last.ident == "ExternRef" || last.ident == "ExternRefMut")
                    && let syn::PathArguments::AngleBracketed(args) = &last.arguments
                    && args.args.len() >= 2
                    && let syn::GenericArgument::Type(inner) = &args.args[1]
                {
                    return inner.clone();
                }
                ty.clone()
            }
            _ => ty.clone(),
        }
    }

    if let Some(receiver) = &fn_descriptor.receiver
        && receiver.is_handle()
    {
        let ty = handle_selector_from_arg_type(receiver.src_type());
        let key = ty.to_token_stream().to_string();
        if seen.insert(key) {
            selectors.push(ty);
        }
    }
    for arg in &fn_descriptor.input_args {
        if !arg.is_handle() {
            continue;
        }
        let ty = handle_selector_from_arg_type(arg.src_type());
        let key = ty.to_token_stream().to_string();
        if seen.insert(key) {
            selectors.push(ty);
        }
    }

    selectors
}

pub(crate) fn validate_handle_id_positions_for_sig(
    sig: &syn::Signature,
    handle_id_specs: &[HandleIdSpec],
) -> Result<(), syn::Error> {
    if handle_id_specs.is_empty() {
        return Ok(());
    }

    let base_args = sig.inputs.len();
    let total_slots = base_args + handle_id_specs.len();
    let max_index = total_slots.saturating_sub(1);

    for spec in handle_id_specs {
        if spec.at >= total_slots {
            return Err(syn::Error::new_spanned(
                &spec.selector,
                format!("position {} is out of bounds(max: {max_index})", spec.at),
            ));
        }
    }
    Ok(())
}

pub(crate) mod dispatch {
    use super::{
        FnDescriptor, HandleIdSpec, available_default_handle_selectors,
        collect_explicit_handle_id_specs_from_sig, parse_entry_handle_map_attr,
        validate_handle_id_positions_for_sig,
    };
    use std::collections::{BTreeMap, BTreeSet};

    pub(crate) fn parse_impl_dispatch_attrs(
        attrs: &[syn::Attribute],
    ) -> syn::Result<Option<BTreeMap<String, Vec<syn::Type>>>> {
        let mut out = None;
        for attr in attrs {
            if let Some(map) = parse_entry_handle_map_attr(attr)?
                && out.replace(map).is_some()
            {
                return Err(syn::Error::new_spanned(
                    attr,
                    "`dispatch` mapping can only be provided once per entry",
                ));
            }
        }
        Ok(out)
    }

    pub(crate) fn collect_explicit_handle_id_specs(
        sig: &syn::Signature,
        allowed_selectors: &BTreeSet<String>,
        handle_id_positions: Option<&BTreeMap<String, usize>>,
    ) -> syn::Result<Vec<HandleIdSpec>> {
        collect_explicit_handle_id_specs_from_sig(sig, allowed_selectors, handle_id_positions)
    }

    pub(crate) fn resolve_poly_handle_id_specs_for_sig(
        sig: &syn::Signature,
        _attrs: &[syn::Attribute],
        entry_explicit_handle_id_specs: &[HandleIdSpec],
        key_types: &BTreeMap<String, Vec<syn::Type>>,
    ) -> syn::Result<Vec<HandleIdSpec>> {
        fn collect_poly_handle_id_specs(
            sig: &syn::Signature,
            handle_id_positions: &BTreeMap<String, usize>,
            allowed_selectors: &BTreeSet<String>,
        ) -> Vec<HandleIdSpec> {
            let mut out = Vec::new();
            let marked_positions = handle_id_positions
                .values()
                .copied()
                .collect::<BTreeSet<_>>();
            for (at, input) in sig.inputs.iter().enumerate() {
                let syn::FnArg::Typed(arg) = input else {
                    continue;
                };
                let syn::Pat::Ident(pat_ident) = arg.pat.as_ref() else {
                    continue;
                };
                if !marked_positions.contains(&at) {
                    continue;
                }
                let Ok(selector) =
                    super::selector_from_handle_id_type(arg.ty.as_ref(), pat_ident.ident.span())
                else {
                    continue;
                };
                if !allowed_selectors.contains(&super::selector_key_name(&selector)) {
                    continue;
                }
                out.push(HandleIdSpec {
                    arg_name: pat_ident.ident.clone(),
                    selector,
                    at: handle_id_positions
                        .get(&pat_ident.ident.to_string())
                        .copied()
                        .unwrap_or(at),
                });
            }
            out
        }
        let mut explicit_handle_id_specs = entry_explicit_handle_id_specs.to_vec();
        let allowed_selectors = key_types.keys().cloned().collect::<BTreeSet<_>>();
        explicit_handle_id_specs.extend(collect_poly_handle_id_specs(
            sig,
            &super::handle_id_positions(sig),
            &allowed_selectors,
        ));
        validate_handle_id_positions_for_sig(sig, &explicit_handle_id_specs)?;
        Ok(explicit_handle_id_specs)
    }

    fn validate_default_handle_id_selectors(
        available_selectors: &BTreeSet<String>,
        selectors: impl IntoIterator<Item = String>,
    ) -> syn::Result<()> {
        for key in selectors {
            if !available_selectors.contains(&key) {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    format!("selector `{key}` does not refer to a handle in this signature"),
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn resolve_default_handle_id_specs_for_fn(
        fn_: &FnDescriptor<'_>,
        skip_defaults: bool,
        impl_explicit_handle_id_specs: &[HandleIdSpec],
        required_selectors: impl IntoIterator<Item = String>,
    ) -> syn::Result<Vec<HandleIdSpec>> {
        if skip_defaults {
            return Ok(Vec::new());
        }
        let mut explicit_handle_id_specs = impl_explicit_handle_id_specs.to_vec();
        let available_selectors = available_default_handle_selectors(fn_)
            .into_iter()
            .map(|selector| super::selector_key_name(&selector))
            .collect::<BTreeSet<_>>();
        explicit_handle_id_specs.extend(collect_explicit_handle_id_specs(
            &fn_.sig,
            &available_selectors,
            Some(&super::handle_id_positions(&fn_.sig)),
        )?);
        validate_default_handle_id_selectors(&available_selectors, required_selectors)?;
        validate_handle_id_positions_for_sig(&fn_.sig, &explicit_handle_id_specs)?;
        Ok(explicit_handle_id_specs)
    }
}

fn selector_key_name(selector: &syn::Type) -> String {
    selector
        .to_token_stream()
        .to_string()
        .trim()
        .trim_end_matches(':')
        .trim()
        .to_string()
}

pub(crate) fn parse_entry_handle_map_attr(
    attr: &syn::Attribute,
) -> Result<Option<BTreeMap<String, Vec<syn::Type>>>, syn::Error> {
    if !attr.path().is_ident("dispatch") {
        return Ok(None);
    }
    if matches!(attr.meta, syn::Meta::Path(_)) {
        // Bare `#[dispatch]` acts as a marker and does not define a selector map.
        return Ok(None);
    }

    struct HandleMapEntry {
        selector: syn::Type,
        types: Vec<syn::Type>,
    }
    impl syn::parse::Parse for HandleMapEntry {
        fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
            let selector: syn::Type = input.parse()?;
            input.parse::<syn::Token![=]>()?;
            let content;
            syn::bracketed!(content in input);
            let tys = content.parse_terminated(|i| i.parse::<syn::Type>(), syn::Token![,])?;
            Ok(Self {
                selector,
                types: tys.into_iter().collect(),
            })
        }
    }

    let entries = attr.parse_args_with(
        syn::punctuated::Punctuated::<HandleMapEntry, syn::Token![,]>::parse_terminated,
    )?;
    if entries.is_empty() {
        // `#[dispatch()]` acts like a bare marker.
        return Ok(None);
    }

    let mut out = BTreeMap::new();
    for entry in entries {
        let key = selector_key_name(&entry.selector);
        if out.contains_key(&key) {
            return Err(syn::Error::new_spanned(
                &entry.selector,
                format!("selector `{key}` can only be specified once in `#[dispatch(...)]`"),
            ));
        }
        out.insert(key, entry.types);
    }
    Ok(Some(out))
}
