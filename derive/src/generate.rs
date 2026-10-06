use std::collections::BTreeSet;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ImplItem, ImplItemFn, ItemImpl, punctuated::Punctuated, visit_mut::VisitMut};

use crate::{
    Co3Fn, Co3Impl, DispatchGroups, ForeignItem, ForeignItemType, co3_path,
    dispatch::{
        StaticLifetimeNormalizer, erase_dispatch_signature, gen_dispatch_erased_layout_checks,
        gen_dispatch_export, gen_dispatch_fn_export, gen_retype, gen_tag_erase_stmts,
        gen_tag_id_type_checks,
    },
    ffi_fn::{
        self, gen_extern_fn_signature, merge_generics, normalize_fn_signature,
        strip_dispatch_params,
    },
    parse::{FailureMode, MacroFeatures},
    trait_object_single_trait_bound,
    utils::{
        DispatchMonomorphizer, ParamUseDetector, cfg_attrs, erased_id_repr,
        has_non_lifetime_generics, has_runtime_dispatch, is_payload_erased, is_type_erased,
        soft_for_arg, strip_internal_generic_param,
    },
    wrapper::{
        gen_decl_abi_assertions, gen_extern_decl, gen_owned_drop_wrapper_body, gen_wrapper_body,
        gen_wrapper_body_with_callee, strip_internal_arg_attrs, wrap_fn_definition,
        wrap_impl_definition,
    },
};

fn lift_dispatch_method(mut dispatch: Co3Impl) -> Co3Impl {
    let syn::ImplItem::Fn(method) = dispatch.items.first_mut().unwrap() else {
        unreachable!()
    };
    let method_generics = core::mem::take(&mut method.sig.generics);
    dispatch.generics = combine_dispatch_generics(Some(&dispatch.generics), &method_generics);

    dispatch
}

/// Flattens the nested impl and method generic scopes for generated dispatch machinery.
///
/// Rust requires lifetime parameters to precede type and const parameters. Within those
/// categories, keep the outer (impl) scope before the inner (method) scope and preserve each
/// declaration's source order.
fn combine_dispatch_generics(
    impl_generics: Option<&syn::Generics>,
    method_generics: &syn::Generics,
) -> syn::Generics {
    let mut combined = impl_generics.cloned().unwrap_or_default();
    merge_generics(method_generics.clone(), &mut combined);

    let (lifetimes, non_lifetimes): (Vec<_>, Vec<_>) = combined
        .params
        .into_iter()
        .partition(|param| matches!(param, syn::GenericParam::Lifetime(_)));
    combined.params = lifetimes.into_iter().chain(non_lifetimes).collect();
    combined
}

/// Separate methods with their own dispatch from methods using only impl-level dispatch.
fn partition_method_dispatch(mut impl_: Co3Impl) -> (Co3Impl, Vec<Co3Impl>) {
    let impl_dispatch_args = impl_.dispatch_args.clone();
    let mut plain_items = Vec::new();
    let mut dispatched = Vec::new();
    for item in core::mem::take(&mut impl_.item.items) {
        let syn::ImplItem::Fn(method) = item else {
            plain_items.push(item);
            continue;
        };
        let method_dispatch_args = impl_.method_dispatch_args.remove(&method.sig.ident);
        if method_dispatch_args.is_none() && !has_runtime_dispatch(&method.sig.generics) {
            plain_items.push(syn::ImplItem::Fn(method));
            continue;
        }
        let method_dispatch_args = method_dispatch_args.unwrap_or_default();
        let mut method_impl = impl_.item.clone();
        method_impl.items = vec![syn::ImplItem::Fn(method)];
        dispatched.push(Co3Impl {
            item: method_impl,
            import_mode: impl_.import_mode,
            dispatch_args: impl_dispatch_args.combined_with(&method_dispatch_args),
            method_dispatch_args: Default::default(),
        });
    }
    impl_.item.items = plain_items;
    (impl_, dispatched)
}

fn split_impl_methods(mut impl_: Co3Impl) -> Vec<Co3Impl> {
    if impl_.items.len() <= 1
        || impl_
            .items
            .iter()
            .any(|item| !matches!(item, syn::ImplItem::Fn(_)))
    {
        return vec![impl_];
    }

    core::mem::take(&mut impl_.item.items)
        .into_iter()
        .map(|item| {
            let mut method_impl = impl_.clone();
            method_impl.item.items = vec![item];
            method_impl
        })
        .collect()
}

fn move_method_only_impl_params(
    impl_: &mut ItemImpl,
    method: &mut ImplItemFn,
    retained_impl_params: &BTreeSet<syn::Ident>,
) {
    let appears_in_impl_identity = |ident: &syn::Ident| {
        let detector = ParamUseDetector::new([ident]);
        detector.type_mentions_param(&impl_.self_ty)
            || impl_
                .trait_
                .as_ref()
                .is_some_and(|(path, _)| detector.path_mentions_param(path))
    };
    let moved_params = impl_
        .generics
        .params
        .iter()
        .filter_map(|param| match param {
            syn::GenericParam::Type(param)
                if !appears_in_impl_identity(&param.ident)
                    && !retained_impl_params.contains(&param.ident) =>
            {
                Some(param.ident.clone())
            }
            syn::GenericParam::Const(param)
                if !appears_in_impl_identity(&param.ident)
                    && !retained_impl_params.contains(&param.ident) =>
            {
                Some(param.ident.clone())
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    if moved_params.is_empty() {
        return;
    }

    impl_.generics.params = core::mem::take(&mut impl_.generics.params)
        .into_iter()
        .filter_map(|param| {
            let move_to_method = match &param {
                syn::GenericParam::Type(param) => moved_params.contains(&param.ident),
                syn::GenericParam::Const(param) => moved_params.contains(&param.ident),
                syn::GenericParam::Lifetime(_) => false,
            };
            if move_to_method {
                method.sig.generics.params.push(param);
                None
            } else {
                Some(param)
            }
        })
        .collect();

    let moved = ParamUseDetector::new(&moved_params);
    let Some(where_clause) = impl_.generics.where_clause.as_mut() else {
        return;
    };
    where_clause.predicates = core::mem::take(&mut where_clause.predicates)
        .into_iter()
        .filter_map(|predicate| {
            if moved.predicate_mentions_param(&predicate) {
                method
                    .sig
                    .generics
                    .make_where_clause()
                    .predicates
                    .push(predicate);
                None
            } else {
                Some(predicate)
            }
        })
        .collect();
    if where_clause.predicates.is_empty() {
        impl_.generics.where_clause = None;
    }
}

fn monomorphize_static_impl_bindings(
    dispatch: Co3Impl,
    symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
    declared_types: &BTreeSet<syn::Ident>,
) -> Vec<Co3Impl> {
    let generics = dispatch.generics.clone();
    let static_binding_params = dispatch
        .items
        .iter()
        .filter_map(|item| match item {
            syn::ImplItem::Fn(method) => Some(crate::validate::impl_method_static_binding_params(
                &dispatch,
                method,
                declared_types,
            )),
            _ => None,
        })
        .flatten()
        .collect::<BTreeSet<_>>();
    dispatch
        .dispatch_args
        .static_bindings(&generics, &static_binding_params)
        .into_iter()
        .map(|(static_args, dynamic_args)| {
            let mut binding = dispatch.clone();
            let mut dynamic_args = dynamic_args;
            static_args.for_each_combination(|selections| {
                let mut monomorphizer =
                    DispatchMonomorphizer::for_static_dispatch_group(&generics, selections);
                monomorphizer.visit_item_impl_mut(&mut binding.item);
                dynamic_args.visit_targets_mut(|arg| monomorphizer.visit_generic_argument_mut(arg));
                for groups in binding.method_dispatch_args.values_mut() {
                    groups.visit_targets_mut(|arg| monomorphizer.visit_generic_argument_mut(arg));
                }
                for item in &mut binding.item.items {
                    let syn::ImplItem::Fn(method) = item else {
                        continue;
                    };
                    monomorphizer.interpolate_symbol_attrs(&mut method.attrs, symbol_fragments);
                }
            });
            binding.item.generics.params = core::mem::take(&mut binding.item.generics.params)
                .into_iter()
                .filter(|param| match param {
                    syn::GenericParam::Lifetime(_) => true,
                    syn::GenericParam::Type(param) => !static_args.contains_param(&param.ident),
                    syn::GenericParam::Const(param) => !static_args.contains_param(&param.ident),
                })
                .collect();
            binding.dispatch_args = dynamic_args;
            binding
        })
        .collect()
}

fn monomorphize_static_fn_bindings(
    dispatch: Co3Fn,
    symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
    declared_types: &BTreeSet<syn::Ident>,
) -> Vec<(Co3Fn, syn::Expr)> {
    let generics = dispatch.sig.generics.clone();
    let fn_name = &dispatch.sig.ident;
    let generic_args = generics
        .params
        .iter()
        .filter_map(|param| match param {
            syn::GenericParam::Lifetime(_) => None,
            syn::GenericParam::Type(param) => {
                let ident = &param.ident;
                Some(syn::GenericArgument::Type(syn::parse_quote!(#ident)))
            }
            syn::GenericParam::Const(param) => {
                let ident = &param.ident;
                Some(syn::GenericArgument::Const(syn::parse_quote!(#ident)))
            }
        })
        .collect::<Punctuated<_, syn::Token![,]>>();
    let mut callee: syn::Expr = syn::parse_quote!(self::#fn_name);
    if !generic_args.is_empty() {
        let syn::Expr::Path(path) = &mut callee else {
            unreachable!()
        };
        path.path.segments.last_mut().unwrap().arguments =
            syn::PathArguments::AngleBracketed(syn::AngleBracketedGenericArguments {
                colon2_token: Some(Default::default()),
                lt_token: Default::default(),
                args: generic_args,
                gt_token: Default::default(),
            });
    }

    let static_binding_params =
        crate::validate::fn_static_binding_params(&dispatch, declared_types)
            .into_iter()
            .collect::<BTreeSet<_>>();
    dispatch
        .dispatch_args
        .static_bindings(&generics, &static_binding_params)
        .into_iter()
        .map(|(static_args, dynamic_args)| {
            let mut binding = dispatch.clone();
            let mut callee = callee.clone();
            static_args.for_each_combination(|selections| {
                let mut monomorphizer =
                    DispatchMonomorphizer::for_static_dispatch_group(&generics, selections);
                monomorphizer.visit_item_fn_mut(&mut binding.item);
                monomorphizer.visit_expr_mut(&mut callee);
                monomorphizer.interpolate_symbol_attrs(&mut binding.attrs, symbol_fragments);
            });
            binding.sig.generics.params = core::mem::take(&mut binding.sig.generics.params)
                .into_iter()
                .filter(|param| match param {
                    syn::GenericParam::Lifetime(_) => true,
                    syn::GenericParam::Type(param) => !static_args.contains_param(&param.ident),
                    syn::GenericParam::Const(param) => !static_args.contains_param(&param.ident),
                })
                .collect();
            binding.dispatch_args = dynamic_args;
            (binding, callee)
        })
        .collect()
}

fn materialize_dyn_self_receiver(impl_: &mut ItemImpl) {
    let Some(bound) = crate::trait_object_single_trait_bound(&impl_.self_ty) else {
        return;
    };
    let path = &bound.path;
    impl_.self_ty = syn::parse_quote!(#path);
}

fn materialize_declared_dyn_self(impl_: &mut ItemImpl, declared_self: bool) -> bool {
    let dyn_self = declared_self && trait_object_single_trait_bound(&impl_.self_ty).is_some();
    if dyn_self {
        materialize_dyn_self_receiver(impl_);
    }
    dyn_self
}

struct ExportAssociatedSelfConcretizer<'a> {
    self_ty: &'a syn::Type,
    trait_path: Option<&'a syn::Path>,
}

impl VisitMut for ExportAssociatedSelfConcretizer<'_> {
    fn visit_type_mut(&mut self, ty: &mut syn::Type) {
        syn::visit_mut::visit_type_mut(self, ty);
        let syn::Type::Path(syn::TypePath {
            qself: None, path, ..
        }) = ty
        else {
            return;
        };
        if !path
            .segments
            .first()
            .is_some_and(|segment| segment.ident == "Self")
        {
            return;
        }
        if path.segments.len() == 1 {
            *ty = self.self_ty.clone();
            return;
        }
        let mut rest = syn::Path {
            leading_colon: None,
            segments: Default::default(),
        };
        rest.segments.extend(path.segments.iter().skip(1).cloned());
        let self_ty = self.self_ty;
        *ty = if let Some(trait_path) = self.trait_path {
            syn::parse_quote!(<#self_ty as #trait_path>::#rest)
        } else {
            syn::parse_quote!(<#self_ty>::#rest)
        };
    }

    fn visit_expr_path_mut(&mut self, expr: &mut syn::ExprPath) {
        syn::visit_mut::visit_expr_path_mut(self, expr);
        if expr.qself.is_some()
            || !expr
                .path
                .segments
                .first()
                .is_some_and(|segment| segment.ident == "Self")
        {
            return;
        }
        let mut rest = syn::Path {
            leading_colon: None,
            segments: Default::default(),
        };
        rest.segments
            .extend(expr.path.segments.iter().skip(1).cloned());
        let self_ty = self.self_ty;
        *expr = if let Some(trait_path) = self.trait_path {
            syn::parse_quote!(<#self_ty as #trait_path>::#rest)
        } else {
            syn::parse_quote!(<#self_ty>::#rest)
        };
    }
}

fn gen_export_associated_item_checks(dispatch: &Co3Impl) -> TokenStream {
    if !dispatch
        .items
        .iter()
        .any(|item| matches!(item, ImplItem::Type(_) | ImplItem::Const(_)))
    {
        return TokenStream::new();
    }

    let mut checks = Vec::new();
    dispatch.dispatch_args.for_each_combination(|selections| {
        let mut selected = dispatch.item.clone();
        concretize_selected_impl(
            &dispatch.generics,
            &dispatch.dispatch_args,
            selections,
            &mut selected,
        );
        let self_ty = &selected.self_ty;
        let trait_path = selected.trait_.as_ref().map(|(path, _)| path);
        let impl_attrs = cfg_attrs(&selected.attrs).collect::<Vec<_>>();
        for item in &selected.items {
            let mut concretizer = ExportAssociatedSelfConcretizer {
                self_ty,
                trait_path,
            };
            match item {
                ImplItem::Type(item) => {
                    let ident = &item.ident;
                    let mut declared = item.ty.clone();
                    concretizer.visit_type_mut(&mut declared);
                    let mut generics =
                        combine_dispatch_generics(Some(&selected.generics), &item.generics);
                    concretizer.visit_generics_mut(&mut generics);
                    generics
                        .type_params_mut()
                        .for_each(strip_internal_generic_param);
                    let (fn_generics, _, where_clause) = generics.split_for_impl();
                    let assoc_args = dispatch_trait_args(&item.generics);
                    let assoc_args = (!assoc_args.is_empty()).then(|| quote!(<#assoc_args>));
                    let actual = if let Some(trait_path) = trait_path {
                        quote!(<#self_ty as #trait_path>::#ident #assoc_args)
                    } else {
                        quote!(<#self_ty>::#ident #assoc_args)
                    };
                    let item_attrs = cfg_attrs(&item.attrs);
                    checks.push(quote! {
                        #(#impl_attrs)*
                        #(#item_attrs)*
                        const _: () = {
                            fn __co3_check_associated_type #fn_generics () #where_clause {
                                let _: ::core::marker::PhantomData<*mut #declared> =
                                    ::core::marker::PhantomData::<*mut #actual>;
                            }
                        };
                    });
                }
                ImplItem::Const(item) => {
                    let ident = &item.ident;
                    let mut ty = item.ty.clone();
                    let mut declared = item.expr.clone();
                    concretizer.visit_type_mut(&mut ty);
                    concretizer.visit_expr_mut(&mut declared);
                    let actual = if let Some(trait_path) = trait_path {
                        quote!(<#self_ty as #trait_path>::#ident)
                    } else {
                        quote!(<#self_ty>::#ident)
                    };
                    let item_attrs = cfg_attrs(&item.attrs);
                    checks.push(quote! {
                        #(#impl_attrs)*
                        #(#item_attrs)*
                        const _: () = {
                            let declared: #ty = #declared;
                            let actual: #ty = #actual;
                            assert!(actual == declared, "exported associated constant differs from its declaration");
                        };
                    });
                }
                _ => {}
            }
        }
    });

    quote!(#(#checks)*)
}

fn gen_export_impl(
    abi: &syn::Abi,
    failure_mode: FailureMode,
    impl_: Co3Impl,
    type_id: Option<&syn::Type>,
    declared_self: bool,
    symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
    declared_types: &BTreeSet<syn::Ident>,
) -> TokenStream {
    let (plain, methods) = partition_method_dispatch(impl_);
    let descriptors = core::iter::once(plain)
        .chain(methods.into_iter().map(lift_dispatch_method))
        .flat_map(split_impl_methods)
        .flat_map(|mut descriptor| {
            let dyn_self = materialize_declared_dyn_self(&mut descriptor.item, declared_self);
            monomorphize_static_impl_bindings(descriptor, symbol_fragments, declared_types)
                .into_iter()
                .map(move |descriptor| (descriptor, dyn_self))
        });
    let definitions = descriptors.map(|(descriptor, dyn_self)| {
        let associated_item_checks = gen_export_associated_item_checks(&descriptor);
        if descriptor.items.is_empty() {
            TokenStream::new()
        } else if !descriptor.dispatch_args.is_empty() || dyn_self {
            let self_id = dyn_self.then_some(type_id).flatten();
            let export = gen_dispatch_export(abi, failure_mode, descriptor, self_id, dyn_self);
            quote!(#associated_item_checks #export)
        } else {
            let export = ffi_fn::gen_impl_definition(abi, failure_mode, descriptor.item);
            quote!(#associated_item_checks #export)
        }
    });

    quote!(#(#definitions)*)
}

fn dispatch_type_idents(generics: &syn::Generics) -> Vec<syn::Ident> {
    generics
        .type_params()
        .filter(|param| param.attrs.iter().any(is_type_erased))
        .map(|param| param.ident.clone())
        .collect()
}

/// Builds the dispatch-set generic arguments in source declaration order.
fn dispatch_trait_args(
    generics: &syn::Generics,
) -> Punctuated<syn::GenericArgument, syn::Token![,]> {
    generics
        .params
        .iter()
        .map(|param| match param {
            syn::GenericParam::Lifetime(param) => {
                syn::GenericArgument::Lifetime(param.lifetime.clone())
            }
            syn::GenericParam::Type(param) => {
                let ident = &param.ident;
                syn::parse_quote!(#ident)
            }
            syn::GenericParam::Const(param) => {
                let ident = &param.ident;
                syn::GenericArgument::Const(syn::parse_quote!(#ident))
            }
        })
        .collect()
}

fn dispatch_trait_generics(generics: &syn::Generics, _args: &DispatchGroups) -> syn::Generics {
    let mut trait_generics = generics.clone();
    trait_generics
        .type_params_mut()
        .for_each(strip_internal_generic_param);
    trait_generics
}

fn dispatch_membership_bound(
    dispatch_set: &TokenStream,
    trait_args: &Punctuated<syn::GenericArgument, syn::Token![,]>,
) -> syn::WherePredicate {
    syn::parse_quote!((): #dispatch_set<#trait_args>)
}

fn dispatch_set_name() -> syn::Ident {
    format_ident!("DispatchSet")
}

fn dispatch_module_name(self_ty: &syn::Type, method: &syn::Ident) -> syn::Ident {
    let receiver = match self_ty {
        syn::Type::Path(syn::TypePath {
            qself: None, path, ..
        }) => path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            .unwrap_or_else(|| "dispatch".to_owned()),
        _ => "dispatch".to_owned(),
    };
    format_ident!("{}_{}", receiver.to_lowercase(), method)
}

struct DispatchSetDelegate<'a> {
    raw: bool,
    abi: &'a syn::Abi,
    block_attrs: &'a [syn::Attribute],
    fn_attrs: &'a [syn::Attribute],
    failure_mode: FailureMode,
    source_sig: &'a syn::Signature,
    wrapper_sig: &'a syn::Signature,
    raw_sig: &'a syn::Signature,
    self_ty: Option<&'a syn::Type>,
    dispatch_generics: &'a syn::Generics,
    declared_self: bool,
    symbol_fragments: &'a std::collections::BTreeMap<String, syn::LitStr>,
}

fn raw_dispatch_wrapper_sig(
    sig: &syn::Signature,
    dispatch_generics: &syn::Generics,
    failure_mode: FailureMode,
    fn_by_val: bool,
    abi: &syn::Abi,
) -> syn::Signature {
    let co3 = co3_path();
    let mut bounded = prepare_dispatch_forwarding_sig(sig);
    for ident in dispatch_type_idents(dispatch_generics) {
        let id_repr = dispatch_generics
            .type_params()
            .find(|param| param.ident == ident)
            .and_then(erased_id_repr)
            .expect("dispatch type has a tag representation");
        let where_clause = bounded.generics.make_where_clause();
        where_clause
            .predicates
            .push(syn::parse_quote!(#ident: #co3::tag::Tagged));
        where_clause.predicates.push(syn::parse_quote!(
            <#ident as #co3::tag::TagFamily>::Kind:
                #co3::Encode<CType = #id_repr, Store: #co3::stored::EmptyStore>
        ));
    }
    let mut source = sig.clone();
    source.inputs = source
        .inputs
        .into_iter()
        .filter(|input| !crate::dispatch::is_tag_id_arg(input))
        .collect();
    let mut raw = ffi_fn::lower_abi_fn_signature(source, failure_mode, fn_by_val);
    raw.generics.where_clause = bounded.generics.where_clause;
    raw.generics
        .type_params_mut()
        .for_each(strip_internal_generic_param);
    let generic_idents = dispatch_generics
        .type_params()
        .map(|param| param.ident.clone())
        .collect::<Vec<_>>();
    let detector = ParamUseDetector::new(generic_idents.iter());
    let where_clause = raw.generics.make_where_clause();
    for ident in &generic_idents {
        let used = raw.inputs.iter().any(|input| match input {
            FnArg::Typed(arg) => ParamUseDetector::new([ident]).type_mentions_param(&arg.ty),
            FnArg::Receiver(_) => false,
        }) || matches!(&raw.output, syn::ReturnType::Type(_, ty) if ParamUseDetector::new([ident]).type_mentions_param(ty));
        if used {
            where_clause
                .predicates
                .push(syn::parse_quote!(#ident: #co3::ReprC));
        }
    }
    for input in &raw.inputs {
        let FnArg::Typed(arg) = input else { continue };
        if detector.type_mentions_param(&arg.ty) {
            let ty = &arg.ty;
            where_clause.predicates.push(syn::parse_quote!(#ty: Sized));
        }
    }
    if let syn::ReturnType::Type(_, ty) = &raw.output
        && detector.type_mentions_param(ty)
    {
        where_clause.predicates.push(syn::parse_quote!(#ty: Sized));
    }
    raw.safety = syn::Safety::Unsafe(Default::default());
    raw.abi = Some(sig.abi.clone().unwrap_or_else(|| abi.clone()));
    raw
}

fn raw_dispatch_call(
    source_sig: &syn::Signature,
    wrapper_sig: &syn::Signature,
    extern_sig: &syn::Signature,
    dispatch_generics: &syn::Generics,
    self_ty: Option<&syn::Type>,
    callee: TokenStream,
) -> TokenStream {
    let dummy_self: syn::Type = syn::parse_quote!(());
    let encoded_tags = source_sig.inputs.iter().filter_map(|input| {
        let FnArg::Typed(arg) = input else {
            return None;
        };
        if !crate::dispatch::is_tag_id_arg(input) {
            return None;
        }
        let ident = ffi_fn::item_fn_input_ident(&arg.pat);
        Some(quote!(let #ident = co3::encode(#ident);))
    });
    let erased = gen_tag_erase_stmts(
        self_ty.unwrap_or(&dummy_self),
        dispatch_generics,
        false,
        source_sig,
    );
    let args = extern_sig.inputs.iter().map(|input| match input {
        FnArg::Typed(arg) => {
            let ident = ffi_fn::item_fn_input_ident(&arg.pat);
            quote!(#ident)
        }
        FnArg::Receiver(_) => quote!(__co3_self),
    });
    let call = quote!(unsafe { #callee(#(#args),*) });
    let output = match (&extern_sig.output, &wrapper_sig.output) {
        (syn::ReturnType::Type(_, source), syn::ReturnType::Type(_, target)) => {
            gen_retype(&quote!(__co3_result), source, target)
        }
        _ => quote!(__co3_result),
    };
    quote! {
        #(#encoded_tags)*
        #(#erased)*
        let __co3_result = #call;
        #output
    }
}

fn dispatch_delegate_sig(
    wrapper_sig: &syn::Signature,
    self_ty: Option<&syn::Type>,
    delegate_name: &syn::Ident,
) -> syn::Signature {
    let mut sig = wrapper_sig.clone();
    normalize_fn_signature(&mut sig, self_ty);
    sig.ident = delegate_name.clone();
    sig.generics.params.clear();
    sig
}

fn dispatch_source_arg_names(sig: &syn::Signature) -> Vec<TokenStream> {
    sig.inputs
        .iter()
        .filter(|input| !crate::dispatch::is_tag_id_arg(input))
        .map(|input| match input {
            FnArg::Typed(input) => {
                let ident = ffi_fn::item_fn_input_ident(&input.pat);
                quote!(#ident)
            }
            FnArg::Receiver(_) => quote!(__co3_self),
        })
        .collect()
}

fn gen_dispatch_set(
    trait_name: &syn::Ident,
    sealed_module: &syn::Ident,
    generics: &syn::Generics,
    args: &DispatchGroups,
    delegate: Option<&DispatchSetDelegate<'_>>,
) -> TokenStream {
    let trait_generics = dispatch_trait_generics(generics, args);
    let trait_args = dispatch_trait_args(&trait_generics);
    let trait_where_clause = &trait_generics.where_clause;
    let trait_params = &trait_generics.params;
    let trait_decl_generics = (!trait_params.is_empty()).then(|| quote!(<#trait_params>));
    let dispatch_trait_items = delegate
        .map(|delegate| {
            let sig = dispatch_delegate_sig(
                delegate.wrapper_sig,
                delegate.self_ty,
                &delegate.source_sig.ident,
            );
            quote!(#sig;)
        })
        .unwrap_or_default();
    let mut impls = Vec::new();
    args.for_each_combination(|selections| {
        let mut selection_args = dispatch_trait_args(generics);
        let mut monomorphizer = DispatchMonomorphizer::for_dispatch_group(generics, selections);
        selection_args
            .iter_mut()
            .for_each(|arg| monomorphizer.visit_generic_argument_mut(arg));
        if let Some(self_ty) = delegate.and_then(|delegate| delegate.self_ty) {
            selection_args.iter_mut().for_each(|arg| {
                ffi_fn::SelfConcretizer { self_ty }.visit_generic_argument_mut(arg);
                monomorphizer.visit_generic_argument_mut(arg);
            });
        }
        let lifetimes = generics.lifetimes().collect::<Vec<_>>();
        let mut auxiliary_params = generics
            .params
            .iter()
            .filter_map(|param| match param {
                syn::GenericParam::Type(param) if !args.contains_param(&param.ident) => {
                    let mut param = param.clone();
                    strip_internal_generic_param(&mut param);
                    Some(syn::GenericParam::Type(param))
                }
                syn::GenericParam::Const(param) if !args.contains_param(&param.ident) => {
                    let mut param = param.clone();
                    param.default = None;
                    Some(syn::GenericParam::Const(param))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        auxiliary_params
            .iter_mut()
            .for_each(|param| monomorphizer.visit_generic_param_mut(param));
        let mut impl_where_clause = trait_generics.where_clause.clone();
        if let Some(where_clause) = &mut impl_where_clause {
            monomorphizer.visit_where_clause_mut(where_clause);
        }
        if let Some(self_ty) = delegate.and_then(|delegate| delegate.self_ty) {
            auxiliary_params.iter_mut().for_each(|param| {
                ffi_fn::SelfConcretizer { self_ty }.visit_generic_param_mut(param);
                monomorphizer.visit_generic_param_mut(param);
            });
            if let Some(where_clause) = &mut impl_where_clause {
                ffi_fn::SelfConcretizer { self_ty }.visit_where_clause_mut(where_clause);
                monomorphizer.visit_where_clause_mut(where_clause);
            }
        }
        if let Some(where_clause) = &mut impl_where_clause {
            let auxiliary_idents = auxiliary_params.iter().filter_map(|param| match param {
                syn::GenericParam::Type(param) => Some(&param.ident),
                syn::GenericParam::Const(param) => Some(&param.ident),
                syn::GenericParam::Lifetime(_) => None,
            });
            let auxiliary_detector = ParamUseDetector::new(auxiliary_idents);
            where_clause.predicates = core::mem::take(&mut where_clause.predicates)
                .into_iter()
                .filter_map(|mut predicate| {
                    if let syn::WherePredicate::Type(predicate) = &mut predicate
                        && !auxiliary_detector.type_mentions_param(&predicate.bounded_ty)
                    {
                        predicate.bounds = core::mem::take(&mut predicate.bounds)
                            .into_iter()
                            .filter(|bound| {
                                !matches!(bound, syn::TypeParamBound::Trait(bound)
                                    if bound.maybe.is_some() && bound.path.is_ident("Sized"))
                            })
                            .collect();
                        if predicate.bounds.is_empty() {
                            return None;
                        }
                    }
                    Some(predicate)
                })
                .collect();
        }
        let impl_generics = (!lifetimes.is_empty() || !auxiliary_params.is_empty())
            .then(|| quote!(<#(#lifetimes,)* #(#auxiliary_params),*>));

        let dispatch_impl_items = delegate
            .map(|delegate| {
                let mut raw_sig = delegate.raw_sig.clone();
                monomorphizer.visit_signature_mut(&mut raw_sig);
                if let Some(self_ty) = delegate.self_ty {
                    ffi_fn::SelfConcretizer { self_ty }.visit_signature_mut(&mut raw_sig);
                    monomorphizer.visit_signature_mut(&mut raw_sig);
                }
                raw_sig.ident = format_ident!("__co3_raw");
                raw_sig.safety = syn::Safety::Default;
                let mut attrs = delegate.fn_attrs.to_vec();
                monomorphizer.interpolate_symbol_attrs(&mut attrs, delegate.symbol_fragments);
                let extern_decl =
                    gen_extern_decl(delegate.abi, delegate.block_attrs, &attrs, quote!(#raw_sig));
                let abi_assertions = gen_decl_abi_assertions(&quote!(#raw_sig));

                // Generate conversion and erasure while the dispatch
                // parameters are still visible, then monomorphize the whole
                // method. This preserves the casts that dynamic dispatch needs
                // while making static parameters concrete inside each impl.
                let sig = dispatch_delegate_sig(
                    delegate.wrapper_sig,
                    delegate.self_ty,
                    &delegate.source_sig.ident,
                );
                let id_assignments = dispatch_id_assignments(delegate.source_sig, delegate.self_ty);
                let dummy_self_ty = syn::parse_quote!(());
                let wrapper_body = if delegate.raw {
                    raw_dispatch_call(
                        delegate.source_sig,
                        delegate.wrapper_sig,
                        delegate.raw_sig,
                        delegate.dispatch_generics,
                        delegate.self_ty,
                        quote!(__co3_raw),
                    )
                } else {
                    gen_wrapper_body_with_callee::<true>(
                        delegate.failure_mode,
                        delegate.fn_attrs.iter().any(ffi_fn::is_by_val_attr),
                        Some(delegate.self_ty.unwrap_or(&dummy_self_ty)),
                        Some(delegate.dispatch_generics),
                        delegate.declared_self,
                        delegate.source_sig,
                        quote!(__co3_raw),
                    )
                };
                let co3 = co3_path();
                let method_tokens = quote! {
                    #sig {
                        use #co3 as co3;
                        #abi_assertions
                        #extern_decl
                        #(#id_assignments)*
                        #wrapper_body
                    }
                };
                let mut method: syn::ImplItemFn = syn::parse2(method_tokens.clone())
                    .unwrap_or_else(|error| {
                        panic!("invalid dispatch delegate `{method_tokens}`: {error}")
                    });
                monomorphizer.visit_impl_item_fn_mut(&mut method);
                if let Some(self_ty) = delegate.self_ty {
                    ffi_fn::SelfConcretizer { self_ty }.visit_impl_item_fn_mut(&mut method);
                    monomorphizer.visit_impl_item_fn_mut(&mut method);
                }
                quote!(#method)
            })
            .unwrap_or_default();

        impls.push(quote! {
            impl #impl_generics #sealed_module::Sealed<#selection_args> for () #impl_where_clause {}
            impl #impl_generics #trait_name<#selection_args> for () #impl_where_clause {
                #dispatch_impl_items
            }
        });
    });

    quote! {
        mod #sealed_module {
            use super::*;

            pub trait Sealed #trait_decl_generics #trait_where_clause {}
        }
        pub(super) trait #trait_name #trait_decl_generics:
            #sealed_module::Sealed<#trait_args> #trait_where_clause {
            #dispatch_trait_items
        }
        #(#impls)*
    }
}

fn concretize_selected_impl(
    source_generics: &syn::Generics,
    args: &DispatchGroups,
    selections: &[crate::DispatchSelection<'_>],
    wrapper: &mut ItemImpl,
) {
    DispatchMonomorphizer::for_dispatch_group(source_generics, selections)
        .visit_item_impl_mut(wrapper);
    wrapper.generics.params = core::mem::take(&mut wrapper.generics.params)
        .into_iter()
        .filter(|param| match param {
            syn::GenericParam::Lifetime(_) => true,
            syn::GenericParam::Type(param) => !args.contains_param(&param.ident),
            syn::GenericParam::Const(param) => !args.contains_param(&param.ident),
        })
        .collect();
}

fn prepare_dispatch_wrapper_sig(
    sig: &syn::Signature,
    dispatch_generics: &syn::Generics,
    static_dispatch: Option<&DispatchGroups>,
    co3: &TokenStream,
) -> syn::Signature {
    let mut wrapper_sig = prepare_dispatch_forwarding_sig(sig);

    let dispatch_tys = dispatch_type_idents(dispatch_generics);
    let erased_tys = dispatch_generics
        .type_params()
        .filter(|param| is_payload_erased(param))
        .map(|param| param.ident.clone())
        .collect::<Vec<_>>();
    let parameter_idents = dispatch_generics
        .type_params()
        .filter(|param| {
            !static_dispatch.is_some_and(|dispatch| dispatch.contains_param(&param.ident))
        })
        .map(|param| param.ident.clone())
        .collect::<Vec<_>>();
    let parameter_detector = ParamUseDetector::new(parameter_idents.iter());
    let where_clause = wrapper_sig.generics.make_where_clause();

    for ident in &dispatch_tys {
        let id_repr = dispatch_generics
            .type_params()
            .find(|param| param.ident == *ident)
            .and_then(erased_id_repr)
            .expect("dispatch type has a tag representation");
        where_clause.predicates.push(syn::parse_quote!(
            #ident: #co3::tag::Tagged
        ));
        where_clause.predicates.push(syn::parse_quote!(
            <#ident as #co3::tag::TagFamily>::Kind:
                #co3::Encode<CType = #id_repr, Store: #co3::stored::EmptyStore>
        ));
    }

    let detector = ParamUseDetector::new(dispatch_tys.iter().chain(erased_tys.iter()));
    for input in &sig.inputs {
        let FnArg::Typed(input) = input else { continue };
        let (attrs, ty) = (&input.attrs[..], &*input.ty);
        if let Some(target_ty) =
            ffi_fn::single_unpack_part(attrs, ty).expect("validated one-part unpack attribute")
        {
            let target_part = quote!(<#target_ty as #co3::ReprC>::CType);
            let unpack_trait = quote!(#co3::slice::Unpack<#target_part>);
            let unpack_bound =
                if ffi_fn::ownership_mode_for_arg(attrs, ty) == OwnershipMode::ByValue {
                    syn::parse_quote!(#ty: #unpack_trait)
                } else {
                    syn::parse_quote!(
                        for<'__co3_unpack> <#ty as #co3::borrow::Borrow>::Borrowed<'__co3_unpack>:
                            #unpack_trait
                    )
                };
            where_clause.predicates.push(unpack_bound);
            where_clause
                .predicates
                .push(syn::parse_quote!(#target_ty: #co3::ReprC));
            where_clause
                .predicates
                .push(syn::parse_quote!(#target_part: #co3::CType + Sized));
        }
        let parameterized_unpack =
            ffi_fn::is_unpack_arg(attrs) && parameter_detector.type_mentions_param(ty);
        if parameterized_unpack {
            let (part1, part2) = ffi_fn::unpack_types(attrs)
                .expect("validated #[unpack] attribute")
                .expect("unpack attribute was found");
            let (source_part1, source_part2) =
                ffi_fn::unpack_logical_parts(attrs, ty).expect("validated #[unpack] attribute");
            let (abi_part1, abi_part2) =
                ffi_fn::unpack_abi_parts(attrs, ty).expect("validated #[unpack] attribute");
            if matches!(part1, syn::Type::Infer(_)) {
                where_clause.predicates.push(syn::parse_quote!(
                    #source_part1: #co3::CType + Sized
                ));
            } else {
                where_clause
                    .predicates
                    .push(syn::parse_quote!(#part1: #co3::ReprC));
                where_clause.predicates.push(syn::parse_quote!(
                    <#part1 as #co3::ReprC>::CType: #co3::CType + Sized
                ));
            }
            if matches!(part2, syn::Type::Infer(_)) {
                where_clause.predicates.push(syn::parse_quote!(
                    #source_part2: #co3::CType + Sized
                ));
            } else {
                where_clause
                    .predicates
                    .push(syn::parse_quote!(#part2: #co3::ReprC));
                where_clause.predicates.push(syn::parse_quote!(
                    <#part2 as #co3::ReprC>::CType: #co3::CType + Sized
                ));
            }
            let unpack_trait = quote!(#co3::slice::Unpack2<#source_part1, #source_part2>);
            let unpack_bound =
                if ffi_fn::ownership_mode_for_arg(attrs, ty) == OwnershipMode::ByValue {
                    syn::parse_quote!(
                        #ty: #unpack_trait
                    )
                } else {
                    syn::parse_quote!(
                        for<'__co3_unpack> <#ty as #co3::borrow::Borrow>::Borrowed<'__co3_unpack>:
                            #unpack_trait
                    )
                };
            where_clause.predicates.push(unpack_bound);
            where_clause
                .predicates
                .push(syn::parse_quote!(#abi_part1: #co3::CType + Sized));
            where_clause
                .predicates
                .push(syn::parse_quote!(#abi_part2: #co3::CType + Sized));
        }
        if crate::dispatch::tag_id(ty).is_none()
            && (detector.type_mentions_param(ty) || parameterized_unpack)
        {
            if ffi_fn::ownership_mode_for_arg(attrs, ty) == OwnershipMode::ByValue {
                let bound = if soft_for_arg(attrs) {
                    syn::parse_quote!(#ty: #co3::Encode)
                } else {
                    syn::parse_quote!(#ty: #co3::Encode<Store: #co3::stored::EmptyStore>)
                };
                where_clause.predicates.push(bound);
            } else {
                where_clause
                    .predicates
                    .push(syn::parse_quote!(#ty: #co3::ReprC + #co3::borrow::Borrow));
                where_clause.predicates.push(syn::parse_quote!(
                    <#ty as #co3::ReprC>::CType: #co3::borrow::BorrowCast<AsConst: Sized>
                ));
                let bound = if soft_for_arg(attrs) {
                    syn::parse_quote!(
                        for<'__co3_borrow> <#ty as #co3::borrow::Borrow>::Borrowed<'__co3_borrow>:
                            #co3::Encode<
                                CType = <<#ty as #co3::ReprC>::CType as #co3::borrow::BorrowCast>::AsConst,
                            >
                    )
                } else {
                    syn::parse_quote!(
                        for<'__co3_borrow> <#ty as #co3::borrow::Borrow>::Borrowed<'__co3_borrow>:
                            #co3::Encode<
                                CType = <<#ty as #co3::ReprC>::CType as #co3::borrow::BorrowCast>::AsConst,
                                Store: #co3::stored::EmptyStore,
                            >
                    )
                };
                where_clause.predicates.push(bound);
            }
        }
    }

    if let syn::ReturnType::Type(_, output_ty) = &sig.output
        && detector.type_mentions_param(output_ty)
    {
        where_clause.predicates.push(syn::parse_quote!(
            for<'a> #output_ty: #co3::Decode<'a, Store: #co3::stored::EmptyStore>
        ));
    }

    wrapper_sig
}

fn prepare_dispatch_forwarding_sig(sig: &syn::Signature) -> syn::Signature {
    let mut wrapper_sig = sig.clone();
    wrapper_sig.inputs = wrapper_sig
        .inputs
        .into_iter()
        .filter(|input| !crate::dispatch::is_tag_id_arg(input))
        .collect();
    strip_internal_arg_attrs(&mut wrapper_sig);
    wrapper_sig
        .generics
        .type_params_mut()
        .for_each(strip_internal_generic_param);
    wrapper_sig
}

fn dispatch_id_assignments(sig: &syn::Signature, self_ty: Option<&syn::Type>) -> Vec<TokenStream> {
    sig.inputs
        .iter()
        .filter_map(|input| {
            let FnArg::Typed(syn::PatType { pat, ty, .. }) = input else {
                return None;
            };
            match crate::dispatch::tag_id(ty)? {
                crate::dispatch::TagId::DynType(ident) => {
                    Some(quote! { let #pat = <#ident as co3::tag::Tagged>::TAG; })
                }
                crate::dispatch::TagId::DynSelf => {
                    let self_ty = self_ty?;
                    Some(quote! { let #pat = <#self_ty as co3::tag::Tagged>::TAG; })
                }
            }
        })
        .collect()
}

struct DispatchImportParts {
    set: TokenStream,
    id_checks: TokenStream,
    layout_checks: TokenStream,
    wrapper_sig: syn::Signature,
    id_assignments: Vec<TokenStream>,
    wrapper_body: TokenStream,
    extern_decl: TokenStream,
    abi_assertions: TokenStream,
}

struct DispatchImportContext<'a> {
    abi: &'a syn::Abi,
    failure_mode: FailureMode,
    block_attrs: &'a [syn::Attribute],
    fn_attrs: &'a [syn::Attribute],
    sig: &'a syn::Signature,
    args: &'a DispatchGroups,
    self_ty: Option<&'a syn::Type>,
    impl_generics: Option<&'a syn::Generics>,
    declared_self: bool,
    self_id: Option<&'a syn::Type>,
    raw: bool,
}

#[derive(Clone, Copy)]
enum DispatchImportMode<'a> {
    Static(&'a std::collections::BTreeMap<String, syn::LitStr>),
    Dynamic,
}

fn emit_import_dispatch_items(
    module_name: &syn::Ident,
    fn_attrs: &[syn::Attribute],
    vis: &syn::Visibility,
    wrapper_sig: &syn::Signature,
    parts: &DispatchImportParts,
    self_binding: TokenStream,
    module_cfg: bool,
) -> (TokenStream, TokenStream) {
    let wrapper_attrs = fn_attrs
        .iter()
        .filter(|attr| !crate::is_symbol_name_attr(attr) && !ffi_fn::is_by_val_attr(attr));
    let wrapper_body = gen_dispatch_import_wrapper_body(parts, self_binding);
    let module = gen_dispatch_import_module(module_name, parts, TokenStream::new());
    let module_attrs = (module_cfg && !module.is_empty())
        .then(|| cfg_attrs(fn_attrs))
        .into_iter()
        .flatten();
    (
        quote! {
            #(#module_attrs)*
            #module
        },
        quote! {
            #(#wrapper_attrs)*
            #vis #wrapper_sig {
                #wrapper_body
            }
        },
    )
}

fn gen_dispatch_import_wrapper_body(
    DispatchImportParts {
        id_checks,
        layout_checks,
        id_assignments,
        wrapper_body,
        extern_decl,
        abi_assertions,
        ..
    }: &DispatchImportParts,
    self_binding: TokenStream,
) -> TokenStream {
    let co3 = co3_path();

    quote! {
        use #co3 as co3;
        #id_checks
        #layout_checks
        #extern_decl
        #abi_assertions
        #(#id_assignments)*
        #self_binding
        #wrapper_body
    }
}

fn gen_dispatch_import_module(
    module_name: &syn::Ident,
    DispatchImportParts { set, .. }: &DispatchImportParts,
    definitions: TokenStream,
) -> TokenStream {
    if set.is_empty() && definitions.is_empty() {
        return TokenStream::new();
    }

    quote! {
        #[allow(unused_braces)]
        mod #module_name {
            use super::*;

            #set
            #definitions
        }
    }
}

fn prepare_dispatch_import(
    context: DispatchImportContext<'_>,
    set_name: &syn::Ident,
    module_name: &syn::Ident,
    mode: DispatchImportMode<'_>,
) -> DispatchImportParts {
    let mut wrapper_source_sig = context.sig.clone();
    ffi_fn::explicitize_signature_lifetimes(&mut wrapper_source_sig);
    let mut args = context.args.clone();
    if matches!(mode, DispatchImportMode::Dynamic) {
        args.inject_unnamed_lifetimes(&mut wrapper_source_sig.generics);
    }
    let dispatch_generics =
        combine_dispatch_generics(context.impl_generics, &wrapper_source_sig.generics);
    let co3 = co3_path();
    let delegate_sig = if context.raw {
        raw_dispatch_wrapper_sig(
            &wrapper_source_sig,
            &dispatch_generics,
            context.failure_mode,
            context.fn_attrs.iter().any(ffi_fn::is_by_val_attr),
            context.abi,
        )
    } else {
        prepare_dispatch_wrapper_sig(
            &wrapper_source_sig,
            &dispatch_generics,
            matches!(mode, DispatchImportMode::Static(_)).then_some(context.args),
            &co3,
        )
    };
    let dispatch_set_path = quote!(#module_name::#set_name);
    let trait_args = dispatch_trait_args(&dispatch_generics);
    let mut wrapper_sig = delegate_sig.clone();
    wrapper_sig.generics.make_where_clause().predicates.insert(
        0,
        dispatch_membership_bound(&dispatch_set_path, &trait_args),
    );

    let mut extern_sig = if matches!(mode, DispatchImportMode::Static(_)) {
        context.sig.clone()
    } else {
        wrapper_source_sig.clone()
    };
    normalize_fn_signature(&mut extern_sig, context.self_ty);
    let extern_generics = if matches!(mode, DispatchImportMode::Static(_)) {
        let mut extern_args = context.args.clone();
        extern_args.inject_unnamed_lifetimes(&mut extern_sig.generics);
        let mut generics = extern_sig.generics.clone();
        if let Some(impl_generics) = context.impl_generics {
            merge_generics(impl_generics.clone(), &mut generics);
        }
        generics
    } else {
        dispatch_generics.clone()
    };
    if let Some(impl_generics) = context.impl_generics {
        // A nested foreign declaration retains outer lifetime binders after
        // its receiver is normalized and its type parameters are erased.
        ffi_fn::merge_impl_generics_for_raw_decl(
            impl_generics.clone(),
            false,
            &mut extern_sig.generics,
        );
    }
    strip_erased_param_predicates(&extern_generics, &mut extern_sig.generics);
    let receiver = context
        .self_ty
        .map_or(crate::dispatch::DispatchReceiver::None, |ty| {
            if context.declared_self {
                crate::dispatch::DispatchReceiver::DynSelf {
                    ty,
                    id: context.self_id,
                }
            } else {
                crate::dispatch::DispatchReceiver::Impl { ty, id: None }
            }
        });
    erase_dispatch_signature(&extern_generics, receiver, &mut extern_sig);
    strip_dispatch_params(&mut extern_sig.generics);

    let DispatchImportContext {
        abi,
        failure_mode,
        block_attrs,
        fn_attrs,
        sig,
        args: dispatch_args,
        self_ty,
        declared_self,
        raw,
        ..
    } = context;
    let sealed_module = format_ident!("sealed");
    match mode {
        DispatchImportMode::Static(symbol_fragments) => {
            let trait_args = dispatch_trait_args(&dispatch_generics);
            let raw_sig = ffi_fn::lower_abi_fn_signature(
                extern_sig,
                failure_mode,
                fn_attrs.iter().any(ffi_fn::is_by_val_attr),
            );
            let delegate_name = &wrapper_source_sig.ident;
            let delegate_callee = quote!(<() as #dispatch_set_path<#trait_args>>::#delegate_name);
            let wrapper_args = dispatch_source_arg_names(&wrapper_source_sig);
            let wrapper_body = quote!(#delegate_callee(#(#wrapper_args),*));
            let delegate = DispatchSetDelegate {
                raw,
                abi,
                block_attrs,
                fn_attrs,
                failure_mode,
                source_sig: &wrapper_source_sig,
                wrapper_sig: &delegate_sig,
                raw_sig: &raw_sig,
                self_ty,
                dispatch_generics: &dispatch_generics,
                declared_self,
                symbol_fragments,
            };

            DispatchImportParts {
                set: gen_dispatch_set(
                    set_name,
                    &sealed_module,
                    &dispatch_generics,
                    dispatch_args,
                    Some(&delegate),
                ),
                id_checks: gen_tag_id_type_checks(&dispatch_generics, dispatch_args),
                layout_checks: gen_dispatch_erased_layout_checks(
                    &dispatch_generics,
                    sig,
                    dispatch_args,
                ),
                wrapper_sig,
                id_assignments: Vec::new(),
                wrapper_body,
                extern_decl: TokenStream::new(),
                abi_assertions: TokenStream::new(),
            }
        }
        DispatchImportMode::Dynamic => {
            let set = gen_dispatch_set(set_name, &sealed_module, &dispatch_generics, &args, None);
            let id_assignments = dispatch_id_assignments(&wrapper_source_sig, self_ty);
            let dummy_self_ty = syn::parse_quote!(());
            let wrapper_body = (!raw).then(|| {
                gen_wrapper_body::<true>(
                    failure_mode,
                    fn_attrs.iter().any(ffi_fn::is_by_val_attr),
                    Some(self_ty.unwrap_or(&dummy_self_ty)),
                    Some(&dispatch_generics),
                    declared_self,
                    &wrapper_source_sig,
                )
            });
            let mut decl = ffi_fn::lower_abi_fn_signature(
                extern_sig,
                failure_mode,
                fn_attrs.iter().any(ffi_fn::is_by_val_attr),
            );
            if raw {
                decl.ident = format_ident!("__co3_raw");
            }
            let abi_assertions = gen_decl_abi_assertions(&quote!(#decl));
            let wrapper_body = if raw {
                raw_dispatch_call(
                    &wrapper_source_sig,
                    &wrapper_sig,
                    &decl,
                    &dispatch_generics,
                    self_ty,
                    quote!(__co3_raw),
                )
            } else {
                wrapper_body.expect("ordinary dispatch has a wrapper body")
            };

            DispatchImportParts {
                set,
                id_checks: gen_tag_id_type_checks(&dispatch_generics, &args),
                layout_checks: gen_dispatch_erased_layout_checks(
                    &dispatch_generics,
                    &wrapper_source_sig,
                    &args,
                ),
                wrapper_sig,
                id_assignments,
                wrapper_body,
                extern_decl: gen_extern_decl(abi, block_attrs, fn_attrs, quote!(#decl)),
                abi_assertions,
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OwnershipMode {
    #[default]
    Borrow,
    ByValue,
}

pub(crate) fn expand_export_decls(
    abi: syn::Abi,
    _features: MacroFeatures,
    failure_mode: FailureMode,
    decls: Vec<ForeignItem>,
    symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
) -> TokenStream {
    let co3 = co3_path();
    let declared_types = decls
        .iter()
        .filter_map(|decl| match decl {
            ForeignItem::Type(item) => Some(item.ty.ident.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let owned_type_aliases = if cfg!(feature = "alloc") {
        decls
            .iter()
            .filter_map(|decl| match decl {
                ForeignItem::Type(item) => Some(gen_export_owned_type_alias(&item.ty)),
                _ => None,
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let exports = decls.into_iter().map(|decl| {
        let export = match decl {
        ForeignItem::Static(item) => crate::statics::gen_export_static(item),
        ForeignItem::Type(ForeignItemType {
            ty,
            id,
            id_value,
            covariant_lifetimes,
            self_impls,
            drop,
        }) => {
            let type_cfg_attrs = cfg_attrs(&ty.attrs).map(|attr| quote!(#attr)).collect::<Vec<_>>();
            let (impl_generics, ty_generics, where_clause) = ty.generics.split_for_impl();
            let mut decode_generics = ty.generics.clone();
            decode_generics.params.insert(0, syn::parse_quote!('_dšč));
            let (decode_impl_generics, _, _) = decode_generics.split_for_impl();

            let ident = &ty.ident;
            let dispatch = self_impls.into_iter().map(|impl_| {
                gen_export_impl(
                    &abi,
                    failure_mode,
                    impl_,
                    id.as_deref(),
                    true,
                    symbol_fragments,
                    &declared_types,
                )
            });

            let drop_impl = drop.as_ref().map(|drop| {
                let mut item = drop.item.clone();
                if trait_object_single_trait_bound(&drop.self_ty).is_some() {
                    materialize_dyn_self_receiver(&mut item);
                }
                item
            });

            let drop_check = drop_impl
                .as_ref()
                .map(|drop_impl| gen_drop_impl_check(&ty, drop_impl));
            let drop = drop.map(|item| {
                gen_export_impl(
                    &abi,
                    failure_mode,
                    item,
                    id.as_deref(),
                    true,
                    symbol_fragments,
                    &declared_types,
                )
            });

            let opaque = derive_opaque_item(
                id.as_deref(),
                ident,
                &ty.generics,
                // TODO: This is not correct, but I don't think it matters whether it's ZST or not
                quote! { #co3::rust_spec::size::Sized<#co3::rust_spec::Gt<#co3::rust_spec::Zero>> },
                quote! { #co3::rust_spec::niche::WithoutNiche },
            );
            let tag_impl = id_value
                .as_deref()
                .map(|value| gen_tag_impl(ident, &ty.generics, value));
            let size_check = gen_non_zst_sized_check(ident, &ty.generics);
            let covariance_checks = gen_export_covariance_checks(ident, &ty.generics, &covariant_lifetimes);

            quote! {
                #(#type_cfg_attrs)*
                const _: () = {
                    #opaque
                    #tag_impl

                    #drop
                    #size_check
                    #covariance_checks
                    #drop_check

                    unsafe impl #impl_generics co3::stored::EncodeOwned for #ident #ty_generics #where_clause {
                        type Store = ();

                        #[inline(always)]
                        fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
                        where
                            Self: 'itm
                        {
                            self
                        }
                    }
                    unsafe impl #decode_impl_generics co3::stored::DecodeOwned<'_dšč> for #ident #ty_generics #where_clause {
                        type Store = ();

                        #[inline(always)]
                        unsafe fn soft_decode<'_išč: '_dšč>(source: Self::CType, (): &mut ()) -> Option<Self> {
                            Some(source)
                        }
                    }

                    impl #impl_generics co3::Encode for #ident #ty_generics #where_clause {}
                    impl #impl_generics co3::Decode<'_> for #ident #ty_generics #where_clause {}

                    unsafe impl #impl_generics co3::borrow::BorrowCast for #ident #ty_generics #where_clause {
                        type AsConst = Self;
                    }
                    unsafe impl #impl_generics co3::borrow::BorrowCastMut for #ident #ty_generics #where_clause {
                        type AsMut = Self;
                    }

                    #(#dispatch)*
                };
            }
        }
        ForeignItem::Fn(mut item) => {
            normalize_fn_signature(&mut item.sig, None);
            let bindings =
                monomorphize_static_fn_bindings(item, symbol_fragments, &declared_types);
            let definitions = bindings
                .into_iter()
                .map(|(binding, callee)| {
                    let definition = if binding.dispatch_args.is_empty() {
                        ffi_fn::gen_fn_definition(&abi, failure_mode, binding.item, callee)
                    } else {
                        gen_dispatch_fn_export(&abi, failure_mode, binding, callee)
                    };
                    quote!(const _: () = { #definition };)
                });
            quote!(#(#definitions)*)
        }
        ForeignItem::Impl(impl_) => {
            gen_export_impl(
                &abi,
                failure_mode,
                impl_,
                None,
                false,
                symbol_fragments,
                &declared_types,
            )
        }
    };

        quote! { const _: () = { use #co3 as co3; #export }; }
    });

    quote! { #(#owned_type_aliases)* #(#exports)* }
}

fn gen_export_owned_type_alias(ty: &syn::ForeignItemType) -> TokenStream {
    let co3 = co3_path();
    let type_cfg_attrs = cfg_attrs(&ty.attrs)
        .map(|attr| quote!(#attr))
        .collect::<Vec<_>>();
    let owned_ident = gen_owned_extern_type_name(&ty.ident);
    let owned_doc = gen_owned_extern_type_doc(&ty.ident);
    let ident = &ty.ident;
    let vis = &ty.vis;
    let generics = &ty.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        #(#type_cfg_attrs)*
        #[allow(type_alias_bounds)]
        #[doc = #owned_doc]
        #vis type #owned_ident #impl_generics #where_clause = #co3::boxed::Box<#ident #ty_generics>;
    }
}

pub(crate) fn gen_raw_companions(
    failure_mode: FailureMode,
    raw_decls: Vec<crate::parse::RawFnDecl>,
) -> Vec<TokenStream> {
    raw_decls
        .into_iter()
        .map(|raw_decl| {
            let dispatched = !raw_decl.dispatch_args.is_empty()
                || crate::utils::has_runtime_dispatch(&raw_decl.sig.generics)
                || raw_decl
                    .owner
                    .as_ref()
                    .is_some_and(|owner| crate::utils::has_runtime_dispatch(&owner.generics));
            let companion = if dispatched {
                crate::dispatch::gen_raw_dispatch_companion(failure_mode, &raw_decl)
            } else {
                let item = syn::ItemFn {
                    attrs: raw_decl.attrs.clone(),
                    vis: raw_decl.vis.clone(),
                    modifiers: Default::default(),
                    sig: raw_decl.sig.clone(),
                    block: Box::new(syn::parse_quote!({})),
                };
                match crate::callback::expand_companion(
                    failure_mode,
                    &item,
                    raw_decl.callee.clone(),
                    raw_decl.owner.as_ref().map(|owner| &owner.generics),
                ) {
                    Ok(companion) => companion,
                    Err(error) => return error.to_compile_error(),
                }
            };
            if let Some(owner) = raw_decl.owner {
                let attrs = owner.attrs;
                let self_ty = owner.self_ty;
                let trait_impl = owner.trait_path.map(|trait_path| quote!(#trait_path for));
                let mut owner_generics = owner.generics;
                owner_generics
                    .type_params_mut()
                    .for_each(strip_internal_generic_param);
                let (impl_generics, _, where_clause) = owner_generics.split_for_impl();
                quote! {
                    #(#attrs)*
                    impl #impl_generics #trait_impl #self_ty #where_clause {
                        #companion
                    }
                }
            } else {
                companion
            }
        })
        .collect()
}

#[expect(clippy::too_many_arguments)]
fn synthesize_impl_extern_decls(
    abi: &syn::Abi,
    failure_mode: FailureMode,
    import_mode: crate::ImportMode,
    attrs: &[syn::Attribute],
    impl_: ItemImpl,
    self_id: Option<&syn::Type>,
    args: Option<&DispatchGroups>,
    declared_self: bool,
    selection: Option<&[crate::DispatchSelection<'_>]>,
    symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
) -> Vec<(syn::Ident, TokenStream, TokenStream)> {
    let dispatch_generics = impl_.generics.clone();
    let receiver =
        crate::dispatch::DispatchReceiver::for_impl(&impl_.self_ty, &impl_.self_ty, self_id);
    impl_
        .items
        .into_iter()
        .filter_map(|item| {
            let syn::ImplItem::Fn(mut item) = item else {
                return None;
            };

            let has_receiver = item
                .sig
                .inputs
                .iter()
                .any(|input| matches!(input, FnArg::Receiver(_)));
            if import_mode == crate::ImportMode::Regular {
                normalize_fn_signature(&mut item.sig, Some(&impl_.self_ty));
            }
            if declared_self {
                erase_dispatch_signature(
                    &syn::Generics::default(),
                    crate::dispatch::DispatchReceiver::DynSelf {
                        ty: &impl_.self_ty,
                        id: None,
                    },
                    &mut item.sig,
                );
            }
            if args.is_some() {
                merge_generics(impl_.generics.clone(), &mut item.sig.generics);
            } else {
                ffi_fn::merge_impl_generics_for_raw_decl(
                    impl_.generics.clone(),
                    has_receiver && !declared_self,
                    &mut item.sig.generics,
                );
            }

            let erased_layout_checks = args
                .map(|args| gen_dispatch_erased_layout_checks(&impl_.generics, &item.sig, args));

            if args.is_some() {
                strip_erased_param_predicates(&impl_.generics, &mut item.sig.generics);
                erase_dispatch_signature(&impl_.generics, receiver, &mut item.sig);
                strip_dispatch_params(&mut item.sig.generics);
            }
            if let Some(selection) = selection {
                // Direct dynamic parameters have already been erased. This
                // substitution resolves the concrete wrapper/self types and
                // projections which deliberately survive erasure.
                DispatchMonomorphizer::for_dispatch_group(&dispatch_generics, selection)
                    .visit_signature_mut(&mut item.sig);
                DispatchMonomorphizer::for_static_dispatch_group(&dispatch_generics, selection)
                    .interpolate_symbol_attrs(&mut item.attrs, symbol_fragments);
            }
            let method_name = item.sig.ident.clone();
            let decl = if import_mode == crate::ImportMode::Raw {
                item.sig.abi = None;
                item.sig.safety = syn::Safety::Default;
                let sig = item.sig;
                quote!(#sig)
            } else {
                gen_extern_fn_signature(
                    item.sig,
                    failure_mode,
                    item.attrs.iter().any(ffi_fn::is_by_val_attr),
                )
            };
            let abi_assertions = gen_decl_abi_assertions(&decl);
            let extern_decl = gen_extern_decl(abi, attrs, &item.attrs, decl);

            Some((
                method_name,
                quote! {
                    #erased_layout_checks
                    #extern_decl
                },
                abi_assertions,
            ))
        })
        .collect()
}

fn attach_impl_abi_assertions(
    wrapper: &mut ItemImpl,
    extern_decls: &[(syn::Ident, TokenStream, TokenStream)],
) {
    for item in &mut wrapper.items {
        let ImplItem::Fn(method) = item else { continue };
        for (name, _, assertions) in extern_decls {
            if method.sig.ident == *name {
                let block: syn::Block = syn::parse_quote!({ #assertions });
                method.block.stmts.splice(0..0, block.stmts);
            }
        }
    }
}

pub(crate) fn expand_extern_decls(
    abi: syn::Abi,
    features: MacroFeatures,
    failure_mode: FailureMode,
    attrs: &[syn::Attribute],
    decls: Vec<ForeignItem>,
    symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
) -> TokenStream {
    fn expand_impl_import(
        abi: &syn::Abi,
        failure_mode: FailureMode,
        import_mode: crate::ImportMode,
        attrs: &[syn::Attribute],
        mut impl_: ItemImpl,
        declared_self: bool,
        symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
    ) -> (TokenStream, ItemImpl) {
        if import_mode == crate::ImportMode::Raw {
            // The forwarding method and the foreign declaration both consume this
            // lowered signature. Keep ABI and safety on the method only.
            let self_ty = impl_.self_ty.clone();
            for item in &mut impl_.items {
                let syn::ImplItem::Fn(method) = item else {
                    continue;
                };
                let wrapper_abi = method.sig.abi.clone().unwrap_or_else(|| abi.clone());
                normalize_fn_signature(&mut method.sig, Some(&self_ty));
                method.sig = ffi_fn::lower_abi_fn_signature(
                    method.sig.clone(),
                    failure_mode,
                    method.attrs.iter().any(ffi_fn::is_by_val_attr),
                );
                method.sig.abi = Some(wrapper_abi);
                method.sig.safety = syn::Safety::Unsafe(Default::default());
            }
        }
        let mut import =
            wrap_impl_definition::<false>(failure_mode, import_mode, &impl_, declared_self);
        let extern_decl = synthesize_impl_extern_decls(
            abi,
            failure_mode,
            import_mode,
            attrs,
            impl_,
            None,
            None,
            declared_self,
            None,
            symbol_fragments,
        );
        attach_impl_abi_assertions(&mut import, &extern_decl);
        let extern_decl = extern_decl.iter().map(|(_, decl, _)| decl);

        (quote!(#(#extern_decl)*), import)
    }

    #[expect(clippy::too_many_arguments)]
    fn synthesize_dispatched_impl_imports(
        abi: &syn::Abi,
        failure_mode: FailureMode,
        import_mode: crate::ImportMode,
        attrs: &[syn::Attribute],
        source_impl: &ItemImpl,
        args: &DispatchGroups,
        self_id: Option<&syn::Type>,
        declared_self: bool,
        symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
    ) -> Vec<(TokenStream, ItemImpl)> {
        let mut wrapper_source = source_impl.clone();
        if declared_self {
            materialize_dyn_self_receiver(&mut wrapper_source);
        }
        let wrapper_impl = (import_mode == crate::ImportMode::Regular).then(|| {
            wrap_impl_definition::<true>(
                failure_mode,
                crate::ImportMode::Regular,
                &wrapper_source,
                declared_self,
            )
        });

        let mut imports = Vec::new();
        args.for_each_combination(|selections| {
            let (mut concrete_wrapper, extern_decl_tokens) = if import_mode
                == crate::ImportMode::Raw
            {
                let mut concrete_wrapper = wrapper_source.clone();
                let self_ty = source_impl.self_ty.clone();
                let impl_generics = source_impl.generics.clone();
                for item in &mut concrete_wrapper.items {
                    let syn::ImplItem::Fn(method) = item else {
                        continue;
                    };
                    DispatchMonomorphizer::for_static_dispatch_group(
                        &source_impl.generics,
                        selections,
                    )
                    .interpolate_symbol_attrs(&mut method.attrs, symbol_fragments);
                    let module_name = dispatch_module_name(&self_ty, &method.sig.ident);
                    let set_name = dispatch_set_name();
                    let parts = prepare_dispatch_import(
                        DispatchImportContext {
                            abi,
                            failure_mode,
                            block_attrs: attrs,
                            fn_attrs: &method.attrs,
                            sig: &method.sig,
                            args,
                            self_ty: Some(&self_ty),
                            impl_generics: Some(&impl_generics),
                            declared_self,
                            self_id,
                            raw: true,
                        },
                        &set_name,
                        &module_name,
                        DispatchImportMode::Dynamic,
                    );
                    let mut wrapper_sig = parts.wrapper_sig.clone();
                    // Impl-level selection creates a concrete trait impl. A
                    // dispatch-set bound on its method would make that impl
                    // stricter than the trait declaration.
                    wrapper_sig.generics.where_clause = method.sig.generics.where_clause.clone();
                    let self_binding = if method
                        .sig
                        .inputs
                        .iter()
                        .any(|input| matches!(input, FnArg::Receiver(_)))
                    {
                        quote!(let __co3_self = self;)
                    } else {
                        Default::default()
                    };
                    let (_, wrapper_fn) = emit_import_dispatch_items(
                        &module_name,
                        &method.attrs,
                        &method.vis,
                        &wrapper_sig,
                        &parts,
                        self_binding,
                        false,
                    );
                    *method = syn::parse2(wrapper_fn)
                        .expect("generated dispatch wrapper must be a valid method");
                }
                (concrete_wrapper, Vec::new())
            } else {
                let extern_decls = synthesize_impl_extern_decls(
                    abi,
                    failure_mode,
                    crate::ImportMode::Regular,
                    attrs,
                    source_impl.clone(),
                    self_id,
                    Some(args),
                    false,
                    Some(selections),
                    symbol_fragments,
                );
                let mut concrete_wrapper = wrapper_impl
                    .as_ref()
                    .expect("regular imports have a wrapper")
                    .clone();
                attach_impl_abi_assertions(&mut concrete_wrapper, &extern_decls);
                let extern_decl_tokens = extern_decls
                    .into_iter()
                    .map(|(_, decl, _)| decl)
                    .collect::<Vec<_>>();
                (concrete_wrapper, extern_decl_tokens)
            };
            concretize_selected_impl(
                &source_impl.generics,
                args,
                selections,
                &mut concrete_wrapper,
            );
            imports.push((quote!(#(#extern_decl_tokens)*), concrete_wrapper));
        });

        imports
    }

    fn synthesize_impl_dispatch_import(
        abi: &syn::Abi,
        failure_mode: FailureMode,
        attrs: &[syn::Attribute],
        self_id: Option<&syn::Type>,
        dispatch: Co3Impl,
        declared_self: bool,
        symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
    ) -> (TokenStream, Vec<(TokenStream, ItemImpl)>) {
        let Co3Impl {
            item: mut impl_,
            import_mode,
            dispatch_args,
            ..
        } = dispatch;
        let mut args = dispatch_args;
        args.inject_unnamed_lifetimes(&mut impl_.generics);

        let dispatch_helper = gen_dispatch_helper(&impl_.generics, &args);
        let id_checks = gen_tag_id_type_checks(&impl_.generics, &args);
        let mut check_generics = impl_.generics.clone();
        strip_erased_param_predicates(&impl_.generics, &mut check_generics);
        check_generics.params = core::mem::take(&mut check_generics.params)
            .into_iter()
            .filter(|param| match param {
                syn::GenericParam::Lifetime(_) => true,
                syn::GenericParam::Type(param) => !args.contains_param(&param.ident),
                syn::GenericParam::Const(param) => !args.contains_param(&param.ident),
            })
            .map(|mut param| {
                match &mut param {
                    syn::GenericParam::Type(param) => param.default = None,
                    syn::GenericParam::Const(param) => param.default = None,
                    syn::GenericParam::Lifetime(_) => {}
                }
                param
            })
            .collect();
        let checks = if has_non_lifetime_generics(&check_generics) {
            let (check_impl_generics, _, check_where_clause) = check_generics.split_for_impl();
            quote! {
                fn __co3_check_dispatch #check_impl_generics () #check_where_clause {
                    #dispatch_helper
                    #id_checks
                }
            }
        } else {
            quote! {
                #dispatch_helper
                #id_checks
            }
        };
        let imports = synthesize_dispatched_impl_imports(
            abi,
            failure_mode,
            import_mode,
            attrs,
            &impl_,
            &args,
            self_id,
            declared_self,
            symbol_fragments,
        );
        (checks, imports)
    }

    fn expand_dispatch_fn_import(
        abi: &syn::Abi,
        failure_mode: FailureMode,
        attrs: &[syn::Attribute],
        mut dispatch: Co3Fn,
        symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
        declared_types: &BTreeSet<syn::Ident>,
    ) -> TokenStream {
        let raw = dispatch.import_mode == crate::ImportMode::Raw;
        let has_static_bindings =
            !crate::validate::fn_static_binding_params(&dispatch, declared_types).is_empty();
        let args = core::mem::take(&mut dispatch.dispatch_args);
        let syn::ItemFn {
            attrs: fn_attrs,
            sig,
            vis,
            ..
        } = &mut *dispatch;
        let module_name = sig.ident.clone();
        let context = DispatchImportContext {
            abi,
            failure_mode,
            block_attrs: attrs,
            fn_attrs,
            sig,
            args: &args,
            self_ty: None,
            impl_generics: None,
            declared_self: false,
            self_id: None,
            raw,
        };
        let dispatch_set = dispatch_set_name();
        let mode = if has_static_bindings {
            DispatchImportMode::Static(symbol_fragments)
        } else {
            DispatchImportMode::Dynamic
        };
        let parts = prepare_dispatch_import(context, &dispatch_set, &module_name, mode);
        let (module, wrapper) = emit_import_dispatch_items(
            &module_name,
            fn_attrs,
            vis,
            &parts.wrapper_sig,
            &parts,
            TokenStream::new(),
            true,
        );

        quote! {
            #module
            #wrapper
        }
    }

    #[expect(clippy::too_many_arguments)]
    fn expand_dispatch_method_import(
        abi: &syn::Abi,
        failure_mode: FailureMode,
        attrs: &[syn::Attribute],
        mut dispatch: Co3Impl,
        impl_dispatch_args: &DispatchGroups,
        declared_self: bool,
        self_id: Option<&syn::Type>,
        symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
        declared_types: &BTreeSet<syn::Ident>,
    ) -> (TokenStream, Vec<ItemImpl>) {
        let raw = dispatch.import_mode == crate::ImportMode::Raw;
        let syn::ImplItem::Fn(mut method) = dispatch.items.pop().unwrap() else {
            unreachable!()
        };
        let has_static_bindings =
            !crate::validate::impl_method_static_binding_params(&dispatch, &method, declared_types)
                .is_empty();
        let args = core::mem::take(&mut dispatch.dispatch_args);

        // A selected impl parameter can introduce another parameter into the
        // concrete self or trait type. Keep that dependency on the impl.
        let mut retained_impl_params = BTreeSet::new();
        for param in &dispatch.generics.params {
            let selected = match param {
                syn::GenericParam::Type(param) => &param.ident,
                syn::GenericParam::Const(param) => &param.ident,
                syn::GenericParam::Lifetime(_) => continue,
            };
            let detector = ParamUseDetector::new([selected]);
            let in_identity = detector.type_mentions_param(&dispatch.self_ty)
                || dispatch
                    .trait_
                    .as_ref()
                    .is_some_and(|(path, _)| detector.path_mentions_param(path));
            if !in_identity || !impl_dispatch_args.contains_param(selected) {
                continue;
            }
            let bindings = impl_dispatch_args.bindings_for(selected);
            for candidate in &dispatch.generics.params {
                let ident = match candidate {
                    syn::GenericParam::Type(param) => &param.ident,
                    syn::GenericParam::Const(param) => &param.ident,
                    syn::GenericParam::Lifetime(_) => continue,
                };
                let detector = ParamUseDetector::new([ident]);
                if bindings
                    .groups()
                    .flat_map(|(_, targets)| targets)
                    .flat_map(|target| &target.args)
                    .any(|arg| detector.generic_arg_mentions_param(arg))
                {
                    retained_impl_params.insert(ident.clone());
                }
            }
        }
        move_method_only_impl_params(&mut dispatch.item, &mut method, &retained_impl_params);
        if declared_self {
            materialize_dyn_self_receiver(&mut dispatch.item);
        }
        let self_ty = &dispatch.self_ty;
        let module_name = dispatch_module_name(self_ty, &method.sig.ident);
        let context = DispatchImportContext {
            abi,
            failure_mode,
            block_attrs: attrs,
            fn_attrs: &method.attrs,
            sig: &method.sig,
            args: &args,
            self_ty: Some(self_ty),
            impl_generics: Some(&dispatch.generics),
            declared_self,
            self_id,
            raw,
        };
        let dispatch_set = dispatch_set_name();
        let mode = if has_static_bindings {
            DispatchImportMode::Static(symbol_fragments)
        } else {
            DispatchImportMode::Dynamic
        };
        let parts = prepare_dispatch_import(context, &dispatch_set, &module_name, mode);
        let self_binding = method
            .sig
            .inputs
            .iter()
            .any(|input| matches!(input, FnArg::Receiver(_)))
            .then(|| quote!(let __co3_self = self;));
        let ItemImpl {
            attrs: impl_attrs,
            modifiers,
            unsafety,
            trait_,
            self_ty,
            ..
        } = &*dispatch;
        let trait_ = trait_
            .as_ref()
            .map(|(path, _)| quote!(#path for))
            .unwrap_or_default();
        let defaultness = &modifiers.defaultness;
        let mut wrapper_impl_generics = dispatch.generics.clone();
        wrapper_impl_generics
            .type_params_mut()
            .for_each(strip_internal_generic_param);
        let (impl_generics, _, where_clause) = wrapper_impl_generics.split_for_impl();
        let mut wrapper_sig = parts.wrapper_sig.clone();
        if let Some(position) = wrapper_sig
            .inputs
            .iter()
            .position(|input| matches!(input, FnArg::Receiver(_)))
            && position != 0
        {
            let mut inputs = core::mem::take(&mut wrapper_sig.inputs)
                .into_iter()
                .collect::<Vec<_>>();
            let receiver = inputs.remove(position);
            wrapper_sig.inputs = core::iter::once(receiver).chain(inputs).collect();
        }
        let (module, wrapper_fn) = emit_import_dispatch_items(
            &module_name,
            &method.attrs,
            &method.vis,
            &wrapper_sig,
            &parts,
            self_binding.unwrap_or_default(),
            false,
        );

        let wrapper: ItemImpl = syn::parse2(quote! {
            #(#impl_attrs)*
            #[allow(unused_braces)]
            #defaultness #unsafety impl #impl_generics #trait_ #self_ty #where_clause {
                #wrapper_fn
            }
        })
        .expect("generated dispatch wrapper must be a valid impl");
        let mut wrappers = Vec::new();
        impl_dispatch_args.for_each_combination(|selections| {
            let mut selected = wrapper.clone();
            concretize_selected_impl(
                &dispatch.generics,
                impl_dispatch_args,
                selections,
                &mut selected,
            );
            wrappers.push(selected);
        });

        (module, wrappers)
    }

    #[expect(clippy::too_many_arguments)]
    fn expand_import_impl(
        abi: &syn::Abi,
        failure_mode: FailureMode,
        attrs: &[syn::Attribute],
        impl_: Co3Impl,
        type_id: Option<&syn::Type>,
        declared_self: bool,
        symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
        declared_types: &BTreeSet<syn::Ident>,
    ) -> TokenStream {
        let source_generic_count = impl_.generics.params.len();
        let descriptors =
            monomorphize_static_impl_bindings(impl_, symbol_fragments, declared_types);
        let imports = descriptors.into_iter().map(|descriptor| {
            let selected_static_impl = descriptor.generics.params.len() < source_generic_count;
            let (plain, methods) = partition_method_dispatch(descriptor);
            let dyn_self =
                declared_self && trait_object_single_trait_bound(&plain.self_ty).is_some();
            let impl_dispatch_args = plain.dispatch_args.clone();
            let self_id = dyn_self.then_some(type_id).flatten();
            let dispatched_impl =
                !impl_dispatch_args.is_empty() || has_runtime_dispatch(&plain.generics) || dyn_self;
            let (checks, mut selected_imports) = if plain.items.is_empty() {
                (TokenStream::new(), Vec::new())
            } else if dispatched_impl {
                synthesize_impl_dispatch_import(
                    abi,
                    failure_mode,
                    attrs,
                    self_id,
                    plain,
                    declared_self,
                    symbol_fragments,
                )
            } else {
                (
                    TokenStream::new(),
                    vec![expand_impl_import(
                        abi,
                        failure_mode,
                        plain.import_mode,
                        attrs,
                        plain.item,
                        declared_self,
                        symbol_fragments,
                    )],
                )
            };

            let mut modules = Vec::new();
            for method in methods {
                let (module, wrappers) = expand_dispatch_method_import(
                    abi,
                    failure_mode,
                    attrs,
                    method,
                    &impl_dispatch_args,
                    declared_self,
                    self_id,
                    symbol_fragments,
                    declared_types,
                );
                modules.push(module);
                if selected_imports.is_empty() {
                    selected_imports = wrappers
                        .into_iter()
                        .map(|wrapper| (TokenStream::new(), wrapper))
                        .collect();
                } else {
                    assert_eq!(
                        selected_imports.len(),
                        wrappers.len(),
                        "each selected impl must receive every dispatched method"
                    );
                    for ((_, selected_wrapper), mut wrapper) in
                        selected_imports.iter_mut().zip(wrappers)
                    {
                        selected_wrapper.items.append(&mut wrapper.items);
                    }
                }
            }
            let co3 = co3_path();
            let groups = selected_imports.into_iter().map(|(extern_decls, wrapper)| {
                quote! { const _: () = { use #co3 as co3; #extern_decls #wrapper }; }
            });
            let output = if dispatched_impl {
                quote! {
                    #(#modules)*
                    const _: () = { use #co3 as co3; #checks #(#groups)* };
                }
            } else {
                quote!(#(#modules)* #(#groups)*)
            };
            if selected_static_impl && !modules.is_empty() {
                quote!(const _: () = { #output };)
            } else {
                output
            }
        });

        quote!(#(#imports)*)
    }

    let declared_types = decls
        .iter()
        .filter_map(|decl| match decl {
            ForeignItem::Type(item) => Some(item.ty.ident.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let imports = decls.into_iter().map(|decl| match decl {
        ForeignItem::Type(ForeignItemType {
            ty,
            id,
            id_value,
            covariant_lifetimes,
            self_impls,
            drop,
        }) => {
            let owned_ident = gen_owned_extern_type_name(&ty.ident);
            let declared_generics = ty.generics.clone();
            let type_cfg_attrs = cfg_attrs(&ty.attrs)
                .map(|attr| quote!(#attr))
                .collect::<Vec<_>>();
            let ty = wrap_extern_type_decl(
                &abi,
                features,
                attrs,
                OpaqueTypeAttrs {
                    id: id.as_deref(),
                    id_value: id_value.as_deref(),
                    covariant_lifetimes: &covariant_lifetimes,
                },
                drop.as_ref()
                    .is_some_and(|item| trait_object_single_trait_bound(&item.self_ty).is_some()),
                ty,
            );

            let dispatch = self_impls.into_iter().map(|impl_| {
                expand_import_impl(
                    &abi,
                    failure_mode,
                    attrs,
                    impl_,
                    id.as_deref(),
                    true,
                    symbol_fragments,
                    &declared_types,
                )
            });

            let drop = drop.map(|item| {
                if item.dispatch_args.is_empty()
                    && trait_object_single_trait_bound(&item.self_ty).is_none()
                {
                    expand_plain_drop_import(
                        &abi,
                        failure_mode,
                        attrs,
                        item.item,
                        &owned_ident,
                        &declared_generics,
                        symbol_fragments,
                    )
                } else {
                    expand_dispatch_drop_import(
                        &abi,
                        failure_mode,
                        attrs,
                        item,
                        id.as_deref(),
                        &owned_ident,
                        &declared_generics,
                        symbol_fragments,
                    )
                }
            });

            quote! {
                #ty

                #(#type_cfg_attrs)*
                #(#dispatch)*
                #drop
            }
        }
        ForeignItem::Fn(item) => {
            if !item.dispatch_args.is_empty() || has_runtime_dispatch(&item.sig.generics) {
                expand_dispatch_fn_import(
                    &abi,
                    failure_mode,
                    attrs,
                    item,
                    symbol_fragments,
                    &declared_types,
                )
            } else {
                wrap_fn_definition(&abi, failure_mode, attrs, item.import_mode, item.item)
            }
        }
        ForeignItem::Impl(impl_) => expand_import_impl(
            &abi,
            failure_mode,
            attrs,
            impl_,
            None,
            false,
            symbol_fragments,
            &declared_types,
        ),
        ForeignItem::Static(item) => crate::statics::gen_extern_static(&abi, attrs, item),
    });

    quote! { #(#imports)* }
}

pub(crate) fn gen_tag_family_impl(
    ident: &syn::Ident,
    generics: &syn::Generics,
    id: &syn::Type,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics co3::tag::TagFamily for #ident #ty_generics #where_clause {
            type Kind = #id;
        }
    }
}

fn gen_tag_impl(ident: &syn::Ident, generics: &syn::Generics, value: &syn::Expr) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        unsafe impl #impl_generics co3::tag::Tagged for #ident #ty_generics #where_clause {
            const TAG: Self::Kind = #value;
        }
    }
}

fn gen_non_zst_sized_check(ident: &syn::Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let static_ty_generics = (!generics.params.is_empty() && !has_non_lifetime_generics(generics))
        .then(|| {
            let args = generics.params.iter().map(|param| match param {
                syn::GenericParam::Lifetime(_) => quote! { 'static },
                syn::GenericParam::Type(_) | syn::GenericParam::Const(_) => unreachable!(),
            });

            quote! { <#(#args),*> }
        });

    let non_zst_check = (!has_non_lifetime_generics(generics)).then(|| {
        quote! {
            const _: () = assert!(
                core::mem::size_of::<#ident #static_ty_generics>() != 0,
                concat!(stringify!(#ident), " must not be zero-sized")
            );
        }
    });

    quote! {
        const _: () = {
            trait __Co3AssertSized: core::marker::Sized {}

            impl #impl_generics __Co3AssertSized for #ident #ty_generics #where_clause {}

            #non_zst_check
        };
    }
}

fn gen_export_covariance_checks(
    ident: &syn::Ident,
    generics: &syn::Generics,
    covariant_lifetimes: &[syn::Lifetime],
) -> TokenStream {
    let checks = covariant_lifetimes.iter().map(|long| {
        let mut name = String::from("__co3_covariant_short");
        while generics
            .lifetimes()
            .any(|param| param.lifetime.ident == name)
        {
            name.push('_');
        }
        let short = syn::Lifetime::new(&format!("'{name}"), long.span());
        let mut check_generics = generics.clone();
        check_generics.params.insert(0, syn::parse_quote!(#short));
        check_generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote!(#long: #short));
        let mut target_bounds = generics
            .where_clause
            .iter()
            .flat_map(|clause| clause.predicates.iter().cloned())
            .collect::<Vec<_>>();
        for param in &generics.params {
            match param {
                syn::GenericParam::Lifetime(param) if !param.bounds.is_empty() => {
                    let lifetime = &param.lifetime;
                    let bounds = &param.bounds;
                    target_bounds.push(syn::parse_quote!(#lifetime: #bounds));
                }
                syn::GenericParam::Type(param) if !param.bounds.is_empty() => {
                    let ident = &param.ident;
                    let bounds = &param.bounds;
                    target_bounds.push(syn::parse_quote!(#ident: #bounds));
                }
                _ => {}
            }
        }
        for mut bound in target_bounds {
            ReplaceLifetime {
                from: long,
                to: &short,
            }
            .visit_where_predicate_mut(&mut bound);
            check_generics.make_where_clause().predicates.push(bound);
        }
        let (fn_generics, _, where_clause) = check_generics.split_for_impl();
        let source_args = generics
            .params
            .iter()
            .map(|param| match param {
                syn::GenericParam::Lifetime(param) => {
                    let lifetime = &param.lifetime;
                    quote!(#lifetime)
                }
                syn::GenericParam::Type(param) => {
                    let ident = &param.ident;
                    quote!(#ident)
                }
                syn::GenericParam::Const(param) => {
                    let ident = &param.ident;
                    quote!(#ident)
                }
            })
            .collect::<Vec<_>>();
        let target_args =
            generics
                .params
                .iter()
                .zip(&source_args)
                .map(|(param, arg)| match param {
                    syn::GenericParam::Lifetime(param) if param.lifetime.ident == long.ident => {
                        quote!(#short)
                    }
                    _ => arg.clone(),
                });
        let source_ty = quote!(#ident <#(#source_args),*>);
        let target_ty = quote!(#ident <#(#target_args),*>);

        quote! {
            const _: () = {
                fn __co3_check_covariance #fn_generics (
                    value: &#short #source_ty,
                ) -> &#short #target_ty #where_clause {
                    value
                }
            };
        }
    });

    quote! { #(#checks)* }
}

struct ReplaceLifetime<'a> {
    from: &'a syn::Lifetime,
    to: &'a syn::Lifetime,
}

impl VisitMut for ReplaceLifetime<'_> {
    fn visit_lifetime_mut(&mut self, lifetime: &mut syn::Lifetime) {
        if lifetime.ident == self.from.ident {
            *lifetime = self.to.clone();
        }
    }
}

fn gen_dispatch_helper(generics: &syn::Generics, args: &DispatchGroups) -> Option<TokenStream> {
    if args.is_empty() {
        return None;
    }

    let erased_params = generics
        .type_params()
        .filter(|p| p.attrs.iter().any(is_type_erased))
        .collect::<Vec<_>>();

    let erased_tys = erased_params.iter().map(|p| &p.ident);
    let erased_generics = erased_params.iter().map(|param| {
        let mut param = (*param).clone();
        strip_internal_generic_param(&mut param);
        quote!(#param)
    });

    let mut dispatch_checks = Vec::new();
    args.for_each_combination(|selections| {
        let erased_args = selections
            .iter()
            .flat_map(|selection| selection.params.iter().zip(&selection.target.args))
            .filter_map(|(param, arg)| {
                generics
                    .type_params()
                    .find(|generic| generic.ident == *param)
                    .filter(|generic| generic.attrs.iter().any(is_type_erased))?;
                let mut arg = arg.clone();
                StaticLifetimeNormalizer::default().visit_generic_argument_mut(&mut arg);
                Some(quote!(#arg))
            });

        dispatch_checks.push(quote! {
            let _: __Co3DispatchParams::<#(#erased_args),*> =
                __Co3DispatchParams(core::marker::PhantomData);
        });
    });
    Some(quote! {
        // NOTE: Verifies `?Sized` bounds on erased params
        struct __Co3DispatchParams<#(#erased_generics),*>(
            core::marker::PhantomData<(#(*const #erased_tys),*)>
        );

        #(#dispatch_checks)*
    })
}

#[expect(clippy::too_many_arguments)]
fn expand_dispatch_drop_import(
    abi: &syn::Abi,
    failure_mode: FailureMode,
    attrs: &[syn::Attribute],
    item: Co3Impl,
    declared_id_ty: Option<&syn::Type>,
    owned_ident: &syn::Ident,
    declared_generics: &syn::Generics,
    symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
) -> TokenStream {
    let Co3Impl {
        item: source_impl,
        dispatch_args,
        ..
    } = item;
    let extern_decls = synthesize_impl_extern_decls(
        abi,
        failure_mode,
        crate::ImportMode::Regular,
        attrs,
        source_impl.clone(),
        declared_id_ty,
        Some(&dispatch_args),
        false,
        None,
        symbol_fragments,
    );

    let declared_self = trait_object_single_trait_bound(&source_impl.self_ty).is_some();
    let mut impl_ = source_impl;
    materialize_dyn_self_receiver(&mut impl_);

    let ItemImpl {
        attrs: impl_attrs,
        generics,
        self_ty,
        items,
        ..
    } = &impl_;
    let declared_self_ty = declared_extern_self_ty(self_ty, declared_generics);
    let owned_self_ty = owned_extern_self_ty(owned_ident, declared_generics);

    let mut wrapper_generics = generics.clone();
    add_missing_decl_lifetimes(&mut wrapper_generics, declared_generics);
    ffi_fn::SelfConcretizer {
        self_ty: &declared_self_ty,
    }
    .visit_generics_mut(&mut wrapper_generics);
    wrapper_generics
        .type_params_mut()
        .for_each(strip_internal_generic_param);
    let (impl_generics, _, _) = wrapper_generics.split_for_impl();
    let predicates = wrapper_generics
        .where_clause
        .as_ref()
        .map(|w| &w.predicates);

    let ImplItem::Fn(method) = items.iter().next().unwrap() else {
        unreachable!()
    };
    let wrapper_attrs = method
        .attrs
        .iter()
        .filter(|attr| !crate::is_symbol_name_attr(attr) && !ffi_fn::is_by_val_attr(attr));
    let self_tag_bound = method.sig.inputs.iter().any(|input| {
        matches!(input, FnArg::Typed(input)
            if matches!(crate::dispatch::tag_id(&input.ty), Some(crate::dispatch::TagId::DynSelf)))
    });
    let self_tag_bound = self_tag_bound.then(|| quote!(#declared_self_ty: co3::tag::Tagged,));

    let mut lowered_method = method.clone();
    lowered_method.sig.output = syn::ReturnType::Default;
    let selector_assignments = lowered_method
        .sig
        .inputs
        .iter_mut()
        .filter_map(|input| {
            let FnArg::Typed(syn::PatType { pat, ty, .. }) = input else {
                return None;
            };
            let (tag_ty, id_ty) = match crate::dispatch::tag_id(ty)? {
                crate::dispatch::TagId::DynType(ident) => {
                    let param = generics.type_params().find(|param| param.ident == *ident)?;
                    (quote!(#ident), erased_id_repr(param)?.clone())
                }
                crate::dispatch::TagId::DynSelf => {
                    (quote!(#declared_self_ty), declared_id_ty?.clone())
                }
            };
            **ty = id_ty.clone();
            Some(quote! {
                let #pat: #id_ty = {
                    // FIXME: https://github.com/mversic/co3/issues/93
                    let __co3_tag_id = <#tag_ty as co3::tag::Tagged>::TAG;
                    unsafe { core::mem::transmute_copy(&__co3_tag_id) }
                };
            })
        })
        .collect::<Vec<_>>();
    let body = gen_owned_drop_wrapper_body::<true>(
        failure_mode,
        &lowered_method,
        &declared_self_ty,
        generics,
        declared_self,
    );
    let abi_assertions = extern_decls.iter().map(|(_, _, assertions)| assertions);
    let extern_decl_tokens = extern_decls.iter().map(|(_, decl, _)| decl);
    let co3 = co3_path();

    quote! {
        const _: () = {
            use #co3 as co3;
            #(#extern_decl_tokens)*

            #(#impl_attrs)*
            impl #impl_generics Drop for #owned_self_ty where
                #self_tag_bound
                #predicates
            {
                #(#wrapper_attrs)*
                fn drop(&mut self) {
                    #(#abi_assertions)*
                    #(#selector_assignments)*
                    #body
                }
            }
        };
    }
}

fn expand_plain_drop_import(
    abi: &syn::Abi,
    failure_mode: FailureMode,
    attrs: &[syn::Attribute],
    mut impl_: ItemImpl,
    owned_ident: &syn::Ident,
    declared_generics: &syn::Generics,
    symbol_fragments: &std::collections::BTreeMap<String, syn::LitStr>,
) -> TokenStream {
    let extern_decls = synthesize_impl_extern_decls(
        abi,
        failure_mode,
        crate::ImportMode::Regular,
        attrs,
        impl_.clone(),
        None,
        None,
        true,
        None,
        symbol_fragments,
    );
    materialize_dyn_self_receiver(&mut impl_);

    let ItemImpl {
        attrs: impl_attrs,
        generics,
        self_ty,
        items,
        ..
    } = &impl_;
    let declared_self_ty = declared_extern_self_ty(self_ty, declared_generics);
    let owned_self_ty = owned_extern_self_ty(owned_ident, declared_generics);
    let ImplItem::Fn(method) = items.iter().next().unwrap() else {
        unreachable!()
    };
    let wrapper_attrs = method
        .attrs
        .iter()
        .filter(|attr| !crate::is_symbol_name_attr(attr) && !ffi_fn::is_by_val_attr(attr));
    let mut lowered_method = method.clone();
    lowered_method.sig.output = syn::ReturnType::Default;
    let body = gen_owned_drop_wrapper_body::<false>(
        failure_mode,
        &lowered_method,
        &declared_self_ty,
        generics,
        true,
    );
    let mut wrapper_generics = generics.clone();
    add_missing_decl_lifetimes(&mut wrapper_generics, declared_generics);
    ffi_fn::SelfConcretizer {
        self_ty: &declared_self_ty,
    }
    .visit_generics_mut(&mut wrapper_generics);
    let (impl_generics, _, where_clause) = wrapper_generics.split_for_impl();
    let abi_assertions = extern_decls.iter().map(|(_, _, assertions)| assertions);
    let extern_decl_tokens = extern_decls.iter().map(|(_, decl, _)| decl);
    let co3 = co3_path();

    quote! {
        const _: () = {
            use #co3 as co3;
            #(#extern_decl_tokens)*

            #(#impl_attrs)*
            impl #impl_generics Drop for #owned_self_ty #where_clause {
                #(#wrapper_attrs)*
                fn drop(&mut self) {
                    #(#abi_assertions)*
                    #body
                }
            }
        };
    }
}

fn owned_extern_self_ty(owned_ident: &syn::Ident, declared_generics: &syn::Generics) -> syn::Type {
    let (_, ty_generics, _) = declared_generics.split_for_impl();
    syn::parse_quote!(#owned_ident #ty_generics)
}

fn declared_extern_self_ty(self_ty: &syn::Type, declared_generics: &syn::Generics) -> syn::Type {
    let syn::Type::Path(path) = self_ty else {
        unreachable!("materialized opaque type is a path");
    };
    let ident = &path
        .path
        .segments
        .last()
        .expect("opaque type path is non-empty")
        .ident;
    let (_, ty_generics, _) = declared_generics.split_for_impl();
    syn::parse_quote!(#ident #ty_generics)
}

fn add_missing_decl_lifetimes(
    impl_generics: &mut syn::Generics,
    declared_generics: &syn::Generics,
) {
    let existing = impl_generics
        .lifetimes()
        .map(|param| param.lifetime.ident.clone())
        .collect::<BTreeSet<_>>();
    let missing = declared_generics
        .lifetimes()
        .filter(|param| !existing.contains(&param.lifetime.ident))
        .cloned()
        .map(syn::GenericParam::Lifetime)
        .collect::<Vec<_>>();
    for param in missing.into_iter().rev() {
        impl_generics.params.insert(0, param);
    }
}

fn gen_drop_impl_check(item: &syn::ForeignItemType, impl_: &ItemImpl) -> TokenStream {
    let ident = &item.ident;
    let self_ty = &impl_.self_ty;

    let mut impl_generics = impl_.generics.clone();
    impl_generics.type_params_mut().for_each(|param| {
        strip_internal_generic_param(param);
    });

    let item_attrs = impl_
        .attrs
        .iter()
        .filter(|attr| !attr.path().is_ident("erased"));

    let marker_fields = item.generics.params.iter().map(|param| match param {
        syn::GenericParam::Lifetime(param) => {
            let lifetime = &param.lifetime;
            quote!(core::marker::PhantomData<&#lifetime ()>)
        }
        syn::GenericParam::Type(param) => {
            let ident = &param.ident;
            quote!(core::marker::PhantomData<#ident>)
        }
        syn::GenericParam::Const(param) => {
            let ident = &param.ident;
            quote!([(); #ident])
        }
    });

    let (decl_generics, _, item_where_clause) = item.generics.split_for_impl();
    let (impl_generics, _, where_clause) = impl_generics.split_for_impl();

    quote! {{
        #(#item_attrs)*
        struct #ident #decl_generics (#(#marker_fields),*) #item_where_clause;

        impl #impl_generics Drop for #self_ty #where_clause {
            fn drop(&mut self) {}
        }
    }}
}

struct OpaqueTypeAttrs<'a> {
    id: Option<&'a syn::Type>,
    id_value: Option<&'a syn::Expr>,
    covariant_lifetimes: &'a [syn::Lifetime],
}

fn wrap_extern_type_decl(
    abi: &syn::Abi,
    features: MacroFeatures,
    attrs: &[syn::Attribute],
    type_attrs: OpaqueTypeAttrs<'_>,
    has_dyn_self_drop: bool,
    mut type_: syn::ForeignItemType,
) -> TokenStream {
    let OpaqueTypeAttrs {
        id,
        id_value,
        covariant_lifetimes,
    } = type_attrs;
    // A `dyn Self` drop dispatches through the opaque type's `Tagged` implementation.
    // implementation, so generic opaque types need the bound on their
    // declaration. Other drops, including the generated Owned wrapper drop,
    // do not require it.
    if has_non_lifetime_generics(&type_.generics) && has_dyn_self_drop && id_value.is_none() {
        let co3 = co3_path();
        let ident = &type_.ident;
        let generics = &type_.generics;
        let (_, ty_generics, _) = generics.split_for_impl();
        let extern_type = quote! { #ident #ty_generics };
        type_
            .generics
            .make_where_clause()
            .predicates
            .push(syn::parse_quote! { #extern_type: #co3::tag::Tagged });
    }

    let syn::ForeignItemType {
        attrs: type_attrs,
        generics,
        vis,
        ident,
        ..
    } = type_;
    let type_cfg_attrs = cfg_attrs(&type_attrs)
        .map(|attr| quote!(#attr))
        .collect::<Vec<_>>();

    let owned_ident = gen_owned_extern_type_name(&ident);
    let owned_doc = gen_owned_extern_type_doc(&ident);
    let owned_repr_c_name = gen_owned_repr_c_name(&ident);
    let owned_repr_c_doc = format!("FFI-safe representation of `{owned_ident}`");
    let ident_impls = gen_extern_type_impls(id, id_value, &ident, &generics);
    let owned_impls = gen_owned_extern_type_impls(id, id_value, &ident, &generics);
    let owned_repr_c_impls = gen_owned_repr_c_impls(&ident, &generics);

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    use syn::GenericParam::*;
    let phantom_data_fields = generics.params.iter().filter_map(|param| match param {
        Lifetime(param) => {
            let lifetime = &param.lifetime;
            let pointer = if covariant_lifetimes
                .iter()
                .any(|covariant| covariant.ident == lifetime.ident)
            {
                quote! { *const &#lifetime () }
            } else {
                quote! { *mut &#lifetime () }
            };
            Some(quote! {
                (
                    core::marker::PhantomData<&#lifetime ()>,
                    core::marker::PhantomData<#pointer>,
                )
            })
        }
        Type(param) => {
            let ident = &param.ident;
            Some(quote! {
                (
                    core::marker::PhantomData<#ident>,
                    core::marker::PhantomData<*mut #ident>,
                )
            })
        }
        Const(_) => None,
    });

    let type_decl = if features.extern_types {
        quote! {
            unsafe #abi {
                #(#attrs)*

                #(#type_attrs)*
                #vis type #ident #impl_generics #where_clause;
            }
        }
    } else {
        quote! {
            #(#type_attrs)*
            #[repr(C)]
            #vis struct #ident #impl_generics #where_clause {
                // FIXME: Is this the correct way to declare extern type
                // https://doc.rust-lang.org/nomicon/ffi.html#representing-opaque-structs
                // FIXME: How do data fields affect alignment here?
                data: core::marker::PhantomData<(#(#phantom_data_fields),*)>,
                __marker: core::marker::PhantomData<(*mut u8, core::marker::PhantomPinned)>,
            }
        }
    };

    let co3 = co3_path();

    quote! {
        #type_decl

        #(#type_cfg_attrs)*
        #[doc = #owned_doc]
        #[repr(transparent)]
        #vis struct #owned_ident #impl_generics (*mut #ident #ty_generics) #where_clause;

        #(#type_cfg_attrs)*
        #[doc(hidden)]
        #[repr(transparent)]
        #[doc = #owned_repr_c_doc]
        #vis struct #owned_repr_c_name #impl_generics (*mut #ident #ty_generics) #where_clause;

        #(#type_cfg_attrs)*
        const _: () = {
            use #co3 as co3;

            #ident_impls
            #owned_impls
            #owned_repr_c_impls
        };
    }
}

fn gen_owned_extern_type_name(ident: &syn::Ident) -> syn::Ident {
    format_ident!("Owned{ident}")
}

fn gen_owned_repr_c_name(ident: &syn::Ident) -> syn::Ident {
    format_ident!("C{}", gen_owned_extern_type_name(ident))
}

fn gen_owned_extern_type_doc(ident: &syn::Ident) -> String {
    format!("Owned representation of `{ident}`")
}

fn gen_extern_type_impls(
    id: Option<&syn::Type>,
    id_value: Option<&syn::Expr>,
    ident: &syn::Ident,
    generics: &syn::Generics,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let opaque_impls = derive_opaque_item(
        id,
        ident,
        generics,
        quote! { co3::rust_spec::size::ExternTypeLike },
        quote! { co3::rust_spec::niche::WithoutNiche },
    );
    let tag_impl = id_value.map(|value| gen_tag_impl(ident, generics, value));

    quote! {
        #opaque_impls
        #tag_impl

        unsafe impl #impl_generics co3::borrow::BorrowCast for #ident #ty_generics #where_clause {
            type AsConst = Self;
        }
        unsafe impl #impl_generics co3::borrow::BorrowCastMut for #ident #ty_generics #where_clause {
            type AsMut = Self;
        }
    }
}

fn gen_owned_repr_c_impls(ident: &syn::Ident, generics: &syn::Generics) -> TokenStream {
    let co3 = co3_path();
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let mut decode_generics = generics.clone();
    decode_generics.params.insert(0, syn::parse_quote!('d));
    let (decode_impl_generics, _, _) = decode_generics.split_for_impl();
    let owned_repr_c_name = gen_owned_repr_c_name(ident);

    quote! {
        impl #impl_generics #owned_repr_c_name #ty_generics #where_clause {
            fn is_none(&self) -> bool {
                self.0.is_null()
            }
        }

        impl #impl_generics Clone for #owned_repr_c_name #ty_generics #where_clause {
            fn clone(&self) -> Self { *self }
        }
        impl #impl_generics Copy for #owned_repr_c_name #ty_generics #where_clause {}

        unsafe impl #impl_generics #co3::rust_spec::RustSpec for #owned_repr_c_name #ty_generics #where_clause {
            type Layout = #co3::rust_spec::Stable;
            type Size = #co3::rust_spec::size::Sized<#co3::rust_spec::Gt<#co3::rust_spec::Zero>>;
            type Alignment = <usize as #co3::rust_spec::RustSpec>::Alignment;
            type Trap = #co3::rust_spec::layout::Robust;
            type Niche = #co3::rust_spec::niche::WithoutNiche;
            type Mutability = #co3::rust_spec::mutability::Exclusive;
            type __IndirectTrap = #co3::rust_spec::layout::Robust;
        }

        impl #impl_generics #co3::ReprC for #owned_repr_c_name #ty_generics #where_clause {
            type CType = Self;
        }
        unsafe impl #impl_generics #co3::stored::EncodeOwned for #owned_repr_c_name #ty_generics #where_clause {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                self
            }
        }
        unsafe impl #decode_impl_generics #co3::stored::DecodeOwned<'d> for #owned_repr_c_name #ty_generics #where_clause {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Some(source)
            }
        }

        impl #impl_generics #co3::Encode for #owned_repr_c_name #ty_generics #where_clause {}
        impl #impl_generics #co3::Decode<'_> for #owned_repr_c_name #ty_generics #where_clause {}

        unsafe impl #impl_generics #co3::transmute::CheckedTransmute for #owned_repr_c_name #ty_generics #where_clause {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        unsafe impl #impl_generics #co3::CType for #owned_repr_c_name #ty_generics #where_clause {}
        unsafe impl #impl_generics #co3::CFnArg for #owned_repr_c_name #ty_generics #where_clause {}
        unsafe impl #impl_generics #co3::CFnReturn for #owned_repr_c_name #ty_generics #where_clause {}

        unsafe impl #impl_generics #co3::borrow::BorrowCast for #owned_repr_c_name #ty_generics #where_clause {
            type AsConst = *const #ident #ty_generics;
        }
        unsafe impl #impl_generics #co3::borrow::BorrowCastMut for #owned_repr_c_name #ty_generics #where_clause {
            type AsMut = #co3::reference::CRefMut<#ident #ty_generics>;
        }
    }
}

fn gen_owned_extern_type_impls(
    id: Option<&syn::Type>,
    id_value: Option<&syn::Expr>,
    ident: &syn::Ident,
    generics: &syn::Generics,
) -> TokenStream {
    let co3 = co3_path();
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let mut decode_generics = generics.clone();
    decode_generics.params.insert(0, syn::parse_quote!('d));
    let (decode_impl_generics, _, _) = decode_generics.split_for_impl();

    let owned_ident = gen_owned_extern_type_name(ident);
    let owned_repr_c_name = gen_owned_repr_c_name(ident);
    let tag_family_impl = id
        .map(|id| gen_tag_family_impl(&owned_ident, generics, id))
        .unwrap_or_default();
    let tag_impl = id_value.map(|value| gen_tag_impl(&owned_ident, generics, value));

    quote! {
        unsafe impl #impl_generics #co3::rust_spec::RustSpec for #owned_ident #ty_generics #where_clause {
            type Layout = #co3::rust_spec::Stable;
            type Size = #co3::rust_spec::size::Sized<#co3::rust_spec::Gt<#co3::rust_spec::Zero>>;
            type Alignment = <usize as #co3::rust_spec::RustSpec>::Alignment;
            type Trap = #co3::rust_spec::layout::NonRobust;
            type Niche = #co3::rust_spec::niche::WithNiche<#co3::rust_spec::Stable>;
            type Mutability = #co3::rust_spec::mutability::Exclusive;
            type __IndirectTrap = #co3::rust_spec::layout::Robust;
        }

        #tag_family_impl
        #tag_impl

        unsafe impl #impl_generics #co3::transmute::CheckedTransmute for #owned_ident #ty_generics #where_clause {
            #[inline(always)]
            unsafe fn is_valid(target: &Self::CType) -> bool {
                // NOTE: Null pointer is validated although it's not strictly required
                // Opaque pointers should never be dereferenced, this catches mistakes
                // TODO: Just return true?
                !target.is_none()
            }
        }

        impl #impl_generics #co3::ReprC for #owned_ident #ty_generics #where_clause {
            type CType = #owned_repr_c_name #ty_generics;
        }
        impl #impl_generics core::cmp::PartialEq for #owned_repr_c_name #ty_generics #where_clause {
            fn eq(&self, other: &Self) -> bool {
                self.0 == other.0
            }
        }
        impl #impl_generics #co3::niche::Niche for #owned_ident #ty_generics #where_clause {
            const NICHE: Self::CType = #owned_repr_c_name(core::ptr::null_mut());
        }

        unsafe impl #impl_generics #co3::stored::EncodeOwned for #owned_ident #ty_generics #where_clause {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                #owned_repr_c_name(core::mem::ManuallyDrop::new(self).0)
            }
        }
        unsafe impl #decode_impl_generics #co3::stored::DecodeOwned<'d> for #owned_ident #ty_generics #where_clause {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                unsafe { <Self as #co3::transmute::CheckedTransmute>::is_valid(&source) }.then_some(Self(source.0))
            }

            #[inline(always)]
            unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, (): &mut ()) -> Self {
                Self(source.0)
            }
        }

        impl #impl_generics #co3::Encode for #owned_ident #ty_generics #where_clause {}
        impl #impl_generics #co3::Decode<'_> for #owned_ident #ty_generics #where_clause {}

        impl #impl_generics core::ops::Deref for #owned_ident #ty_generics #where_clause {
            type Target = #ident #ty_generics;

            fn deref(&self) -> &Self::Target {
                unsafe { &*self.0 }
            }
        }
        impl #impl_generics core::ops::DerefMut for #owned_ident #ty_generics #where_clause {
            fn deref_mut(&mut self) -> &mut Self::Target {
                unsafe { &mut *self.0 }
            }
        }

        impl #impl_generics core::convert::AsRef<#ident #ty_generics> for #owned_ident #ty_generics #where_clause {
            fn as_ref(&self) -> &#ident #ty_generics {
                self
            }
        }
        impl #impl_generics core::convert::AsMut<#ident #ty_generics> for #owned_ident #ty_generics #where_clause {
            fn as_mut(&mut self) -> &mut #ident #ty_generics {
                self
            }
        }

        impl #impl_generics core::borrow::Borrow<#ident #ty_generics> for #owned_ident #ty_generics #where_clause {
            fn borrow(&self) -> &#ident #ty_generics {
                self
            }
        }
        impl #impl_generics core::borrow::BorrowMut<#ident #ty_generics> for #owned_ident #ty_generics #where_clause {
            fn borrow_mut(&mut self) -> &mut #ident #ty_generics {
                self
            }
        }
    }
}

fn derive_opaque_item(
    id: Option<&syn::Type>,
    ident: &syn::Ident,
    generics: &syn::Generics,
    size_kind: TokenStream,
    niche_kind: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let tag_family_impl = id.map(|id| gen_tag_family_impl(ident, generics, id));

    quote! {
        #tag_family_impl

        unsafe impl #impl_generics co3::rust_spec::RustSpec for #ident #ty_generics #where_clause {
            type Layout = co3::rust_spec::Stable;
            type Size = #size_kind;
            // TODO: This is just useless IMO
            type Alignment = co3::rust_spec::One;
            type Trap = co3::rust_spec::layout::Robust;
            type Niche = #niche_kind;
            type Mutability = co3::rust_spec::mutability::Exclusive;
            type __IndirectTrap = co3::rust_spec::layout::Robust;
        }

        unsafe impl #impl_generics co3::CType for #ident #ty_generics #where_clause {}

        unsafe impl #impl_generics co3::transmute::CheckedTransmute for #ident #ty_generics #where_clause {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        impl #impl_generics co3::ReprC for #ident #ty_generics #where_clause {
            type CType = Self;
        }
    }
}

fn strip_erased_param_predicates(impl_generics: &syn::Generics, sig_generics: &mut syn::Generics) {
    let erased_params = impl_generics
        .type_params()
        .filter(|param| param.attrs.iter().any(is_type_erased))
        .map(|param| &param.ident)
        .collect::<BTreeSet<_>>();

    let Some(where_clause) = &mut sig_generics.where_clause else {
        return;
    };

    let old_predicates = core::mem::take(&mut where_clause.predicates);
    let detector = ParamUseDetector::new(erased_params.iter().copied());
    let mut new_predicates = Punctuated::new();

    for predicate in old_predicates {
        if !detector.predicate_mentions_param(&predicate) {
            new_predicates.push(predicate);
        }
    }

    where_clause.predicates = new_predicates;
}
