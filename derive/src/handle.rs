use std::collections::BTreeMap;

use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::visit_mut::VisitMut;

use crate::{
    OwnershipMode, effective_ownership_mode,
    impl_visitor::{FnDescriptor, path_symbol_name},
    utils::{SignatureLifetimeBuilder, unwrap_result_type},
    wrapper::HandleIdSpec,
};

#[derive(Clone)]
pub(crate) struct ExportPolySpec {
    pub(crate) trait_path: Option<syn::Path>,
    pub(crate) method: syn::Ident,
    pub(crate) decl_sig: syn::Signature,
    pub(crate) ownership_mode: OwnershipMode,
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
    fn handle_ffi_ty_from_rust(ty: &syn::Type) -> syn::Type {
        if let syn::Type::Reference(reference) = ty {
            if reference.mutability.is_some() {
                syn::parse_quote!(*mut core::ffi::c_void)
            } else {
                syn::parse_quote!(*const core::ffi::c_void)
            }
        } else {
            syn::parse_quote!(*mut core::ffi::c_void)
        }
    }
    fn selector_base_name(selector: &syn::Type) -> String {
        if let syn::Type::Path(type_path) = selector {
            if type_path.qself.is_none() && type_path.path.is_ident("Self") {
                return "self".to_string();
            }
            if let Some(seg) = type_path.path.segments.last() {
                return seg.ident.to_string().to_lowercase();
            }
        }
        "handle".to_string()
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
                let base = selector_base_name(&spec.selector);
                let arg_name = syn::Ident::new(
                    &format!("__{base}_handle_id"),
                    proc_macro2::Span::call_site(),
                );
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
    let fn_unsafety = spec.decl_sig.unsafety;
    let target_fn_prefix = if let Some(method_abi) = &spec.decl_sig.abi {
        if fn_unsafety.is_some() {
            quote!(unsafe #method_abi fn)
        } else {
            quote!(#method_abi fn)
        }
    } else if fn_unsafety.is_some() {
        quote!(unsafe fn)
    } else {
        quote!(fn)
    };
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
    let handle_id_specs = if spec.handle_id_specs.is_empty() {
        vec![HandleIdSpec {
            selector: syn::parse_quote!(Self),
            at: 0,
        }]
    } else {
        spec.handle_id_specs.clone()
    };
    let self_handle_id_ident = {
        let self_ty: syn::Type = syn::parse_quote!(Self);
        let self_key = selector_key_name(&self_ty);
        let mut found = None;
        for id_spec in &handle_id_specs {
            if selector_key_name(&id_spec.selector) == self_key {
                let base = selector_base_name(&id_spec.selector);
                found = Some(syn::Ident::new(
                    &format!("__{base}_handle_id"),
                    proc_macro2::Span::call_site(),
                ));
                break;
            }
        }
        let Some(ident) = found else {
            return syn::Error::new_spanned(
                &spec.decl_sig,
                "poly selector export requires `#[id_pos(Self: index)]` or an implicit Self handle id",
            )
            .to_compile_error();
        };
        ident
    };

    #[derive(Clone)]
    struct ArgInfo {
        name: syn::Ident,
        ty: syn::Type,
        is_handle: bool,
        ownership_mode: OwnershipMode,
        is_receiver: bool,
        receiver_is_mut: bool,
    }

    let mut args = Vec::<ArgInfo>::new();
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
                    ownership_mode: effective_ownership_mode(&receiver.attrs, spec.ownership_mode),
                    is_receiver: true,
                    receiver_is_mut: receiver.mutability.is_some(),
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
                    ownership_mode: effective_ownership_mode(&arg.attrs, spec.ownership_mode),
                    is_receiver: false,
                    receiver_is_mut: false,
                });
            }
        }
    }

    let mut decl_params = Vec::<TokenStream>::new();
    let mut lifetime_builder = SignatureLifetimeBuilder::new(&[&spec.decl_sig.generics]);
    for arg in &args {
        let arg_name = &arg.name;
        let ffi_ty: syn::Type = if arg.is_handle {
            handle_ffi_ty_from_rust(&arg.ty)
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
        lifetime_builder.split_for_signature(&[&spec.decl_sig.generics]);
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

    let method = &spec.method;
    let is_drop_poly = spec
        .trait_path
        .as_ref()
        .is_some_and(|trait_path| path_symbol_name(trait_path) == "Drop")
        && method == "drop";

    let match_arms = handle_types.iter().enumerate().map(|(arm_idx, handle_ty)| {
        let mut selector_map = BTreeMap::<String, syn::Type>::new();
        selector_map.insert("Self".to_string(), handle_ty.clone());
        for (selector, types) in &spec.key_types {
            if selector == "Self" {
                continue;
            }
            if types.is_empty() {
                return syn::Error::new_spanned(
                    &spec.decl_sig,
                    format!("selector `{selector}` has an empty handle list"),
                )
                .to_compile_error();
            }
            let selected = if types.len() == 1 {
                types[0].clone()
            } else if arm_idx < types.len() {
                types[arm_idx].clone()
            } else {
                return syn::Error::new_spanned(
                    &spec.decl_sig,
                    format!(
                        "selector `{selector}` provides {} entries but `Self` arm {} needs a matching entry",
                        types.len(),
                        arm_idx
                    ),
                )
                .to_compile_error();
            };
            selector_map.insert(selector.clone(), selected);
        }

        let mut decode_stmts = Vec::<TokenStream>::new();
        let mut sync_stmts = Vec::<TokenStream>::new();
        let mut call_args = Vec::<TokenStream>::new();
        let mut target_arg_tys = Vec::<TokenStream>::new();

        for arg in &args {
            if is_drop_poly {
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
            target_arg_tys.push(quote!(#rust_ty));
            call_args.push(quote!(#arg_name));
        }

        let target_ret_ty = if let Some(out_ty) = &raw_output_ty {
            let rewritten = rewrite_selectors_in_type(out_ty, &selector_map);
            quote!(#rewritten)
        } else {
            quote!(())
        };
        let target_ptr_ty = quote!(#target_fn_prefix (#(#target_arg_tys),*) -> #target_ret_ty);
        let target_path = if let Some(trait_path) = &spec.trait_path {
            let rewritten_trait_ty: syn::Type = rewrite_selectors_in_type(
                &syn::parse_quote!(#trait_path),
                &selector_map,
            );
            let rewritten_trait_path = if let syn::Type::Path(type_path) = rewritten_trait_ty {
                type_path.path
            } else {
                return syn::Error::new_spanned(
                    trait_path,
                    "failed to rewrite trait selector path",
                )
                .to_compile_error();
            };
            quote!(<#handle_ty as #rewritten_trait_path>::#method)
        } else {
            quote!(<#handle_ty>::#method)
        };

        let output_write = if is_drop_poly {
            if args.len() != 1 || !args[0].is_receiver || !args[0].is_handle || !args[0].receiver_is_mut {
                return syn::Error::new_spanned(
                    &spec.decl_sig,
                    "Drop poly selector must be `Drop::drop(&mut self)`",
                )
                .to_compile_error();
            }
            let arg_name = &args[0].name;
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
                        let output: #rewritten_raw_output = target(#(#call_args),*);
                        let output = output.map_err(|_| co3::FfiReturn::ExecutionFail)?;
                    }
                } else {
                    quote! {
                        let output: #rewritten = target(#(#call_args),*);
                    }
                };
                quote! {
                    let target: #target_ptr_ty = #target_path;
                    #output_capture
                    let out_ptr = out_ptr.cast::<<#rewritten as co3::ExternC>::CType>();
                    unsafe { <#rewritten as co3::out_ptr::OutPtrWrite>::write_out(output, out_ptr) };
                }
            } else {
                let output_capture = if output_is_result {
                    let rewritten_raw_output =
                        rewritten_raw_output.expect("present when output_is_result");
                    quote! {
                        let output: #rewritten_raw_output = target(#(#call_args),*);
                        let output = output.map_err(|_| co3::FfiReturn::ExecutionFail)?;
                    }
                } else {
                    quote! {
                        let output: #rewritten = target(#(#call_args),*);
                    }
                };
                quote! {
                    let target: #target_ptr_ty = #target_path;
                    #output_capture
                    unsafe { <#rewritten as co3::out_ptr::OutPtrWrite>::write_out(output, out_ptr) };
                }
            }
        } else {
            quote! {
                let target: #target_ptr_ty = #target_path;
                target(#(#call_args),*);
            }
        };

        quote! {
            <#handle_ty as co3::handle::Handle>::ID => {
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
                    let #self_handle_id_ident: co3::handle::Id = co3::Decode::decode(#self_handle_id_ident, &mut ())
                        .ok_or(co3::FfiReturn::TrapRepresentation)?;
                    match #self_handle_id_ident {
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

pub(crate) fn parse_handle_id_attr(
    attr: &syn::Attribute,
) -> Result<Option<Vec<HandleIdSpec>>, syn::Error> {
    if !attr.path().is_ident("id_pos") {
        return Ok(None);
    }

    struct HandleIdEntry {
        selector: syn::Type,
        at: usize,
    }
    impl syn::parse::Parse for HandleIdEntry {
        fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
            let selector = input.parse::<syn::Type>()?;
            input.parse::<syn::Token![:]>()?;
            let at = input.parse::<syn::LitInt>()?.base10_parse::<usize>()?;
            Ok(Self { selector, at })
        }
    }

    let entries = attr.parse_args_with(
        syn::punctuated::Punctuated::<HandleIdEntry, syn::Token![,]>::parse_terminated,
    )?;
    if entries.is_empty() {
        return Err(syn::Error::new_spanned(
            attr,
            "expected at least one `Type: index` mapping",
        ));
    }

    Ok(Some(
        entries
            .into_iter()
            .map(|entry| HandleIdSpec {
                selector: entry.selector,
                at: entry.at,
            })
            .collect(),
    ))
}

pub(crate) fn infer_default_handle_id_specs(
    fn_descriptor: &FnDescriptor,
    explicit: &[HandleIdSpec],
) -> Vec<HandleIdSpec> {
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

    let inferred: Vec<HandleIdSpec> = selectors
        .into_iter()
        .enumerate()
        .map(|(idx, selector)| HandleIdSpec { selector, at: idx })
        .collect();

    if explicit.is_empty() {
        return inferred;
    }

    let mut merged = explicit.to_vec();
    let explicit_keys = explicit
        .iter()
        .map(|spec| selector_key_name(&spec.selector))
        .collect::<std::collections::BTreeSet<_>>();
    for spec in inferred {
        if !explicit_keys.contains(&selector_key_name(&spec.selector)) {
            merged.push(spec);
        }
    }
    merged
}

pub(crate) fn infer_poly_handle_id_specs(
    sig: &syn::Signature,
    key_types: &BTreeMap<String, Vec<syn::Type>>,
    explicit: &[HandleIdSpec],
) -> Vec<HandleIdSpec> {
    fn has_dispatch_attr(attrs: &[syn::Attribute]) -> bool {
        attrs.iter().any(|attr| attr.path().is_ident("dispatch"))
    }
    fn selector_from_arg_type(ty: &syn::Type) -> syn::Type {
        if let syn::Type::Reference(reference) = ty {
            return (*reference.elem).clone();
        }
        ty.clone()
    }

    let mut seen = std::collections::BTreeSet::<String>::new();
    let mut selectors = Vec::<syn::Type>::new();

    if let Some(receiver) = sig.receiver()
        && (has_dispatch_attr(&receiver.attrs) || key_types.contains_key("Self"))
    {
        let self_ty: syn::Type = syn::parse_quote!(Self);
        let key = self_ty.to_token_stream().to_string();
        if seen.insert(key) {
            selectors.push(self_ty);
        }
    }

    for input in &sig.inputs {
        let syn::FnArg::Typed(arg) = input else {
            continue;
        };
        let selector = selector_from_arg_type(&arg.ty);
        let selector_key = selector_key_name(&selector);
        if !has_dispatch_attr(&arg.attrs) && !key_types.contains_key(&selector_key) {
            continue;
        }
        let key = selector.to_token_stream().to_string();
        if seen.insert(key) {
            selectors.push(selector);
        }
    }

    let inferred: Vec<HandleIdSpec> = selectors
        .into_iter()
        .enumerate()
        .map(|(idx, selector)| HandleIdSpec { selector, at: idx })
        .collect();

    if explicit.is_empty() {
        return inferred;
    }

    let mut merged = explicit.to_vec();
    let explicit_keys = explicit
        .iter()
        .map(|spec| selector_key_name(&spec.selector))
        .collect::<std::collections::BTreeSet<_>>();
    for spec in inferred {
        if !explicit_keys.contains(&selector_key_name(&spec.selector)) {
            merged.push(spec);
        }
    }
    merged
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
                format!(
                    "id_pos position {} is out of bounds for this signature; available positions are 0..={}",
                    spec.at, max_index
                ),
            ));
        }
    }
    Ok(())
}

fn selector_key_name(selector: &syn::Type) -> String {
    if let syn::Type::Path(type_path) = selector
        && type_path.qself.is_none()
        && type_path.path.is_ident("Self")
    {
        return "Self".to_string();
    }
    selector.to_token_stream().to_string()
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
        return Err(syn::Error::new_spanned(
            attr,
            "expected at least one `Selector = [Type, ...]` mapping",
        ));
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
