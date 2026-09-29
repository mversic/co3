use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::{Span, TokenStream, TokenTree};
use quote::{format_ident, quote};
use syn::{
    GenericParam, ReturnType, parse_quote, punctuated::Punctuated, spanned::Spanned, visit::Visit,
    visit_mut::VisitMut,
};

use crate::{
    Co3Fn, Co3Impl, DispatchGroups,
    ffi_fn::{
        self, emit_extern_definition, gen_definition_body, gen_drop_definition_body,
        gen_failure_panic, gen_input_decode_stmts, gen_static_fn_signature_drift_check,
        gen_store_sync_stmts, gen_sync_check, gen_sync_error, gen_unknown_tag_error, is_unpack_arg,
        item_fn_input_arg_type, item_fn_output_type, merge_generics, normalize_fn_signature,
        strip_dispatch_params,
    },
    parse::FailureMode,
    utils::{
        DispatchMonomorphizer, ParamUseDetector, erased_id_repr, is_drop_impl, is_payload_erased,
        is_type_erased,
    },
};

#[derive(Clone, Copy)]
pub(crate) enum DispatchReceiver<'a> {
    None,
    Impl {
        ty: &'a syn::Type,
        id: Option<&'a syn::Type>,
    },
    DynSelf {
        ty: &'a syn::Type,
        id: Option<&'a syn::Type>,
    },
}

impl<'a> DispatchReceiver<'a> {
    pub(crate) fn for_impl(
        original_ty: &'a syn::Type,
        ty: &'a syn::Type,
        id: Option<&'a syn::Type>,
    ) -> Self {
        if crate::trait_object_single_trait_bound(original_ty).is_some() {
            Self::DynSelf { ty, id }
        } else {
            Self::Impl { ty, id }
        }
    }

    fn ty(self) -> Option<&'a syn::Type> {
        match self {
            Self::None => None,
            Self::Impl { ty, .. } | Self::DynSelf { ty, .. } => Some(ty),
        }
    }

    fn id(self) -> Option<&'a syn::Type> {
        match self {
            Self::None => None,
            Self::Impl { id, .. } | Self::DynSelf { id, .. } => id,
        }
    }

    fn is_dyn_self(self) -> bool {
        matches!(self, Self::DynSelf { .. })
    }
}

pub(crate) fn gen_dispatch_fn_export(
    abi: &syn::Abi,
    failure_mode: FailureMode,
    Co3Fn {
        item,
        dispatch_args,
        ..
    }: Co3Fn,
    callee: syn::Expr,
) -> TokenStream {
    let generics = item.sig.generics.clone();

    let definition = synthesize_dispatch_export_fn(
        abi,
        failure_mode,
        &generics,
        &[],
        DispatchReceiver::None,
        &dispatch_args,
        item.sig,
        &item.attrs,
        &callee,
        false,
        DispatchEmission::Export,
    );

    quote! {
        const _: () = {
            #definition
        };
    }
}

#[derive(Clone, Copy)]
enum DispatchEmission<'a> {
    Export,
    Companion(&'a syn::Visibility),
}

pub(crate) fn gen_raw_dispatch_companion(
    failure_mode: FailureMode,
    raw_decl: &crate::parse::RawFnDecl,
) -> TokenStream {
    let attrs = &raw_decl.attrs;
    let vis = &raw_decl.vis;
    let mut sig = raw_decl.sig.clone();
    let owner = raw_decl.owner.as_ref();
    let callee: syn::Expr = match syn::parse2(raw_decl.callee.clone()) {
        Ok(callee) => callee,
        Err(error) => return error.to_compile_error(),
    };
    let method_params = sig
        .generics
        .type_params()
        .map(|param| param.ident.clone())
        .collect::<Vec<_>>();
    let owner_ty = owner.map(|owner| owner.self_ty.as_ref());
    if let Some(owner) = owner {
        ffi_fn::merge_generics(owner.generics.clone(), &mut sig.generics);
    }
    let generics = sig.generics.clone();
    ffi_fn::normalize_fn_signature(&mut sig, owner_ty);
    synthesize_dispatch_tag_ids(None, &generics, &mut sig.inputs);
    sig.ident = ffi_fn::raw_definition_name(&sig.ident);
    let abi: syn::Abi = parse_quote!(extern "C");
    let definition = synthesize_dispatch_export_fn(
        &abi,
        failure_mode,
        &generics,
        &method_params,
        owner_ty.map_or(DispatchReceiver::None, |ty| DispatchReceiver::Impl {
            ty,
            id: None,
        }),
        &raw_decl.dispatch_args,
        sig,
        attrs,
        &callee,
        false,
        DispatchEmission::Companion(vis),
    );
    definition
}

#[derive(Clone, Copy)]
enum RetypeDirection {
    Erase,
    Derase,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TagId<'a> {
    DynType(&'a syn::Ident),
    DynSelf,
}

struct ErasedParamReplacer {
    erased_params: BTreeMap<syn::Ident, Option<syn::Type>>,
    payloadless_params: BTreeSet<syn::Ident>,
}

/// Searches through an otherwise opaque type path for indirection boundaries without erasing
/// by-value occurrences of dynamic parameters.
struct NestedIndirectionEraser<'a>(&'a mut ErasedParamReplacer);

impl VisitMut for NestedIndirectionEraser<'_> {
    fn visit_type_reference_mut(&mut self, node: &mut syn::TypeReference) {
        self.0.visit_type_reference_mut(node);
    }

    fn visit_type_ptr_mut(&mut self, node: &mut syn::TypePtr) {
        self.0.visit_type_ptr_mut(node);
    }

    fn visit_type_fn_ptr_mut(&mut self, _: &mut syn::TypeFnPtr) {
        // Function pointer parameters are not pointees of the surrounding value.
    }
}

impl ErasedParamReplacer {
    fn new(generics: &syn::Generics) -> Self {
        let erased_params = generics
            .type_params()
            .filter(|p| p.attrs.iter().any(is_type_erased))
            .map(|p| (p.ident.clone(), p.default.clone()))
            .collect::<BTreeMap<_, _>>();
        let payloadless_params = erased_params
            .iter()
            .filter_map(|(ident, payload)| payload.is_none().then_some(ident.clone()))
            .collect();
        Self {
            erased_params: erased_params
                .into_iter()
                .map(|(ident, ty)| (ident, ty.map(|(_, ty)| ty)))
                .collect(),
            payloadless_params,
        }
    }

    fn abi_repr(payload: Option<&syn::Type>) -> syn::Type {
        payload
            .cloned()
            .unwrap_or_else(|| syn::parse_quote!(core::ffi::c_void))
    }

    fn replace(&mut self, mut ty: syn::Type) -> syn::Type {
        self.visit_type_mut(&mut ty);
        ty
    }

    fn replace_with_change(&mut self, mut ty: syn::Type) -> (syn::Type, bool) {
        let original = ty.clone();
        self.visit_type_mut(&mut ty);
        let changed = original != ty;
        (ty, changed)
    }

    fn has_payloadless_tail(&self, ty: &syn::Type) -> bool {
        match ty {
            syn::Type::Group(group) => self.has_payloadless_tail(&group.elem),
            syn::Type::Paren(paren) => self.has_payloadless_tail(&paren.elem),
            syn::Type::Tuple(tuple) => tuple
                .elems
                .last()
                .is_some_and(|ty| self.has_payloadless_tail(ty)),
            syn::Type::Path(path) if path.qself.is_none() && path.path.segments.len() == 1 => {
                let segment = &path.path.segments[0];
                segment.arguments.is_none() && self.payloadless_params.contains(&segment.ident)
            }
            _ => false,
        }
    }

    fn path_contains_payloadless(&self, path: &syn::TypePath) -> bool {
        let detector = ParamUseDetector::new(&self.payloadless_params);
        path.qself
            .as_ref()
            .is_some_and(|qself| detector.type_mentions_param(&qself.ty))
            || detector.path_mentions_param(&path.path)
    }
}

impl VisitMut for ErasedParamReplacer {
    fn visit_type_reference_mut(&mut self, node: &mut syn::TypeReference) {
        if let syn::Type::Path(path) = node.elem.as_ref()
            && self.path_contains_payloadless(path)
        {
            *node.elem = Self::abi_repr(None);
            return;
        }

        syn::visit_mut::visit_type_reference_mut(self, node);
    }

    fn visit_type_ptr_mut(&mut self, node: &mut syn::TypePtr) {
        if let syn::Type::Path(path) = node.elem.as_ref()
            && self.path_contains_payloadless(path)
        {
            *node.elem = Self::abi_repr(None);
            return;
        }

        syn::visit_mut::visit_type_ptr_mut(self, node);
    }

    fn visit_type_mut(&mut self, node: &mut syn::Type) {
        if let syn::Type::Path(syn::TypePath {
            qself: None, path, ..
        }) = node
            && path.segments.len() == 1
            && path.segments[0].arguments.is_none()
            && let Some(payload) = self.erased_params.get(&path.segments[0].ident)
        {
            *node = Self::abi_repr(payload.as_ref());
            return;
        }

        if let syn::Type::Path(path) = node
            && self.path_contains_payloadless(path)
        {
            NestedIndirectionEraser(self).visit_type_path_mut(path);
            return;
        }

        let opaque_tuple_element = if let syn::Type::Tuple(tuple) = node {
            tuple
                .elems
                .iter()
                .position(|ty| self.has_payloadless_tail(ty))
        } else {
            None
        };

        syn::visit_mut::visit_type_mut(self, node);

        let syn::Type::Tuple(tuple) = node else {
            return;
        };
        let Some(opaque) = opaque_tuple_element else {
            return;
        };
        if opaque == 0 {
            *node = tuple.elems[0].clone();
        } else {
            tuple.elems = core::mem::take(&mut tuple.elems)
                .into_iter()
                .take(opaque + 1)
                .collect();
        }
    }
}

pub(crate) fn gen_dispatch_export(
    abi: &syn::Abi,
    failure_mode: FailureMode,
    Co3Impl {
        item: impl_,
        dispatch_args,
        ..
    }: Co3Impl,
    self_id: Option<&syn::Type>,
    dyn_self: bool,
) -> TokenStream {
    let receiver = if dyn_self {
        DispatchReceiver::DynSelf {
            ty: &impl_.self_ty,
            id: self_id,
        }
    } else {
        DispatchReceiver::Impl {
            ty: &impl_.self_ty,
            id: self_id,
        }
    };

    let impl_attrs = &impl_.attrs;
    let self_ty = &impl_.self_ty;

    let drop_impl = is_drop_impl(&impl_);
    let items = impl_.items.into_iter().filter_map(|item| {
        let syn::ImplItem::Fn(item) = item else {
            return None;
        };

        Some(item)
    });

    let definitions = items.map(|mut item| {
        merge_generics(impl_.generics.clone(), &mut item.sig.generics);
        let dispatch_params = dispatch_args
            .groups()
            .flat_map(|(params, _)| params)
            .collect::<std::collections::BTreeSet<_>>();
        let callee_type_params = item
            .sig
            .generics
            .type_params()
            .filter(|param| !dispatch_params.contains(&param.ident))
            .map(|param| param.ident.clone())
            .collect::<Vec<_>>();
        normalize_fn_signature(&mut item.sig, Some(self_ty));

        let trait_ = impl_.trait_.as_ref().map(|(path, _)| path);
        let callee = ffi_fn::impl_method_callee(self_ty, trait_, &item.sig.ident);

        let definition = synthesize_dispatch_export_fn(
            abi,
            failure_mode,
            &impl_.generics,
            &callee_type_params,
            receiver,
            &dispatch_args,
            item.sig,
            &item.attrs,
            &callee,
            drop_impl,
            DispatchEmission::Export,
        );

        quote! { #definition }
    });

    quote! {
        #(#impl_attrs)*
        const _: () = {
            #(#definitions)*
        };
    }
}

#[expect(clippy::too_many_arguments)]
fn synthesize_dispatch_export_fn(
    abi: &syn::Abi,
    failure_mode: FailureMode,
    generics: &syn::Generics,
    callee_type_params: &[syn::Ident],
    receiver: DispatchReceiver,
    dispatch_args: &DispatchGroups,
    mut sig: syn::Signature,
    attrs: &[syn::Attribute],
    callee: &syn::Expr,
    drop_impl: bool,
    emission: DispatchEmission<'_>,
) -> TokenStream {
    let layout_checks = gen_dispatch_erased_layout_checks(generics, &sig, dispatch_args);

    // Static parameters have already been substituted by the caller. What
    // remains is the generic signature used to synthesize dynamic match arms
    // and the erased signature exposed at the ABI boundary.
    monomorphize_predicates(&mut sig.generics, dispatch_args);
    let arm_generics = sig.generics.clone();
    strip_dispatch_params(&mut sig.generics);

    let fn_by_val = attrs.iter().any(crate::ffi_fn::is_by_val_attr);
    let selector_inputs = dispatch_selector_inputs(&sig.inputs)
        .filter_map(|(_, pat, tag_id)| {
            let tag_id_ty = resolve_tag_id_type(generics, receiver, tag_id)?;
            Some((pat, tag_id_ty))
        })
        .collect::<Vec<_>>();
    let selector_names = selector_inputs
        .iter()
        .map(|(pat, _)| pat)
        .collect::<Vec<_>>();
    let deny_unreachable =
        (!selector_names.is_empty()).then(|| quote!(#[deny(unreachable_patterns)]));
    let selector_inputs = selector_inputs
        .iter()
        .map(|(pat, id_ty)| parse_quote!(#pat: #id_ty))
        .collect::<Vec<_>>();

    let dispatch_arms = synthesize_dispatch_arms(
        &arm_generics,
        callee_type_params,
        receiver,
        &sig,
        fn_by_val,
        callee,
        dispatch_args,
        failure_mode,
        drop_impl,
    );

    let decode_selector_stmts = gen_input_decode_stmts(&selector_inputs, failure_mode);
    let sync_selector_stores = gen_store_sync_stmts(selector_inputs.len());
    let unknown_tag = gen_unknown_tag_error(failure_mode);
    let sync_error = gen_sync_error(failure_mode);
    let selector_sync_check = gen_sync_check(sync_selector_stores, sync_error);

    let fn_body = quote! {{
        #decode_selector_stmts

        let __co3_dispatch_result: core::result::Result<_, _> = {
            #deny_unreachable
            match (#(#selector_names,)*) {
                #(#dispatch_arms,)*
                _ => #unknown_tag,
            }
        };

        #selector_sync_check
        __co3_dispatch_result
    }};

    erase_dispatch_signature(generics, receiver, &mut sig);
    let lowered_sig = ffi_fn::lower_abi_fn_signature(sig, failure_mode, fn_by_val);
    let definition = match emission {
        DispatchEmission::Export => {
            emit_extern_definition(abi, attrs, failure_mode, quote!(#lowered_sig), fn_body)
        }
        DispatchEmission::Companion(vis) => {
            let mut sig = lowered_sig;
            sig.safety = syn::Safety::Unsafe(Default::default());
            sig.abi = Some(abi.clone());
            let checks = ffi_fn::gen_abi_assertions(&sig);
            crate::callback::emit_companion(
                failure_mode,
                attrs,
                vis,
                sig,
                quote!(#layout_checks #checks),
                fn_body,
                fn_by_val,
            )
        }
    };

    match emission {
        DispatchEmission::Export => quote! { #layout_checks #definition },
        DispatchEmission::Companion(_) => definition,
    }
}

/// Imports permit repeated IDs; only their tag representation is asserted.
pub(crate) fn gen_tag_id_type_checks(
    generics: &syn::Generics,
    args: &DispatchGroups,
) -> TokenStream {
    let mut checks = Vec::new();
    let auxiliary_params = generics
        .params
        .iter()
        .filter_map(|param| match param {
            syn::GenericParam::Type(param) if !args.contains_param(&param.ident) => {
                Some(&param.ident)
            }
            syn::GenericParam::Const(param) if !args.contains_param(&param.ident) => {
                Some(&param.ident)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let auxiliary_detector = ParamUseDetector::new(auxiliary_params);
    for param in generics.type_params() {
        let Some(repr) = erased_id_repr(param) else {
            continue;
        };
        args.for_each_combination(|selections| {
            let Some(selection) = selections
                .iter()
                .find(|selection| selection.params.contains(&param.ident))
            else {
                return;
            };
            let Some(index) = selection
                .params
                .iter()
                .position(|ident| *ident == param.ident)
            else {
                return;
            };
            let Some(mut ty) = selection.target.args.get(index).cloned() else {
                return;
            };
            DispatchMonomorphizer::for_dispatch_group(generics, selections)
                .visit_generic_argument_mut(&mut ty);
            StaticLifetimeNormalizer::default().visit_generic_argument_mut(&mut ty);
            if auxiliary_detector.generic_arg_mentions_param(&ty) {
                return;
            }
            checks.push(quote!(let _: #repr = <#ty as co3::tag::Tagged>::TAG;));
        });
    }
    quote!(#(#checks)*)
}

pub(crate) fn gen_dispatch_erased_layout_checks(
    generics: &syn::Generics,
    sig: &syn::Signature,
    args: &DispatchGroups,
) -> TokenStream {
    let layout_param_detector = ParamUseDetector::new(
        generics
            .type_params()
            .filter(|p| is_payload_erased(p))
            .map(|p| &p.ident),
    );
    let mut checks = Vec::new();
    args.for_each_combination(|selections| {
        for input in &sig.inputs {
            let receiver_ty;
            let (attrs, ty) = match input {
                syn::FnArg::Receiver(receiver) => {
                    receiver_ty = crate::utils::receiver_ty(receiver);
                    (&receiver.attrs[..], &receiver_ty)
                }
                syn::FnArg::Typed(syn::PatType { attrs, ty, .. }) => (&attrs[..], &**ty),
            };

            if tag_id(ty).is_some()
                || is_unpack_arg(attrs)
                || crate::ffi_fn::is_single_unpack_arg(attrs)
            {
                continue;
            }

            for layout_ty in dispatch_layout_check_tys(ty, &layout_param_detector) {
                let span = layout_ty.span();
                let mut concrete_ty = layout_ty.clone();
                DispatchMonomorphizer::for_dispatch_group(generics, selections)
                    .visit_type_mut(&mut concrete_ty);
                let concrete_tys = input_abi_tys(attrs, &concrete_ty);
                let erased_ty = ErasedParamReplacer::new(generics).replace(layout_ty);
                let erased_tys = input_abi_tys(attrs, &erased_ty);

                checks.extend(dispatch_layout_checks(
                    concrete_tys
                        .into_iter()
                        .zip(erased_tys)
                        .map(|pair| (pair, span)),
                ));
            }
        }

        let ReturnType::Type(_, output_ty) = &sig.output else {
            return;
        };

        for layout_ty in dispatch_layout_check_tys(output_ty, &layout_param_detector) {
            let span = layout_ty.span();
            let mut concrete_ty = layout_ty.clone();
            DispatchMonomorphizer::for_dispatch_group(generics, selections)
                .visit_type_mut(&mut concrete_ty);
            let concrete_ty = item_fn_output_type(&concrete_ty);
            let erased_ty =
                ErasedParamReplacer::new(generics).replace(item_fn_output_type(&layout_ty));

            checks.extend(dispatch_layout_checks([((concrete_ty, erased_ty), span)]));
        }
    });

    quote! { #(#checks)* }
}

/// Returns the root type and each pointee reached through a reference or raw pointer.
fn dispatch_layout_check_tys(
    ty: &syn::Type,
    param_detector: &ParamUseDetector<'_>,
) -> Vec<syn::Type> {
    struct PointeeCollector {
        tys: Vec<syn::Type>,
    }

    impl PointeeCollector {
        fn visit_pointee(&mut self, pointee: &syn::Type) {
            self.tys.push(pointee.clone());
            self.visit_type(pointee);
        }
    }

    impl Visit<'_> for PointeeCollector {
        fn visit_type_reference(&mut self, node: &syn::TypeReference) {
            self.visit_pointee(&node.elem);
        }

        fn visit_type_ptr(&mut self, node: &syn::TypePtr) {
            self.visit_pointee(&node.elem);
        }

        fn visit_type_fn_ptr(&mut self, _: &syn::TypeFnPtr) {
            // Function pointer arguments do not describe reinterpreted pointee layouts.
        }
    }

    let mut collector = PointeeCollector {
        tys: vec![ty.clone()],
    };
    collector.visit_type(ty);
    collector
        .tys
        .into_iter()
        .filter(|ty| param_detector.type_mentions_param(ty))
        .collect()
}

fn dispatch_layout_checks(
    pairs: impl IntoIterator<Item = ((syn::Type, syn::Type), Span)>,
) -> Vec<TokenStream> {
    pairs
        .into_iter()
        .map(|((mut concrete_ty, mut erased_ty), span)| {
            StaticLifetimeNormalizer::default().visit_type_mut(&mut concrete_ty);
            StaticLifetimeNormalizer::default().visit_type_mut(&mut erased_ty);

            crate::abi_retype::gen_forced_assertion(&concrete_ty, &erased_ty, span)
        })
        .collect()
}

/// Replaces free lifetimes with `'static` while preserving higher-ranked binders.
#[derive(Default)]
pub(crate) struct StaticLifetimeNormalizer {
    bound: Vec<syn::Ident>,
}

impl StaticLifetimeNormalizer {
    fn with_binders(
        &mut self,
        lifetimes: Option<&syn::BoundLifetimes>,
        visit: impl FnOnce(&mut Self),
    ) {
        let previous_len = self.bound.len();
        self.bound
            .extend(lifetimes.into_iter().flat_map(|lifetimes| {
                lifetimes.lifetimes.iter().filter_map(|param| match param {
                    syn::GenericParam::Lifetime(param) => Some(param.lifetime.ident.clone()),
                    _ => None,
                })
            }));
        visit(self);
        self.bound.truncate(previous_len);
    }
}

impl VisitMut for StaticLifetimeNormalizer {
    // The enclosing visitor records these names before traversing their scope.
    fn visit_bound_lifetimes_mut(&mut self, _: &mut syn::BoundLifetimes) {}

    fn visit_lifetime_mut(&mut self, node: &mut syn::Lifetime) {
        if !self.bound.contains(&node.ident) {
            *node = syn::Lifetime::new("'static", node.span());
        }
    }

    fn visit_type_fn_ptr_mut(&mut self, node: &mut syn::TypeFnPtr) {
        let lifetimes = node.lifetimes.clone();
        self.with_binders(lifetimes.as_ref(), |this| {
            syn::visit_mut::visit_type_fn_ptr_mut(this, node);
        });
    }

    fn visit_trait_bound_mut(&mut self, node: &mut syn::TraitBound) {
        let lifetimes = node.lifetimes.clone();
        self.with_binders(lifetimes.as_ref(), |this| {
            syn::visit_mut::visit_trait_bound_mut(this, node);
        });
    }

    fn visit_predicate_type_mut(&mut self, node: &mut syn::PredicateType) {
        let lifetimes = node.lifetimes.clone();
        self.with_binders(lifetimes.as_ref(), |this| {
            syn::visit_mut::visit_predicate_type_mut(this, node);
        });
    }
}

fn input_abi_tys(attrs: &[syn::Attribute], ty: &syn::Type) -> Vec<syn::Type> {
    if is_unpack_arg(attrs) {
        let (part1, part2) =
            crate::ffi_fn::unpack_abi_parts(attrs, ty).expect("validated #[unpack] attribute");
        return vec![part1, part2];
    }

    let abi_ty = item_fn_input_arg_type(attrs, ty);
    vec![parse_quote!(#abi_ty)]
}

pub(crate) fn synthesize_dispatch_tag_ids(
    self_id: Option<&syn::Type>,
    impl_generics: &syn::Generics,
    inputs: &mut Punctuated<syn::FnArg, syn::Token![,]>,
) {
    let mut synthesized = Punctuated::<syn::FnArg, syn::Token![,]>::new();

    let erased_params = impl_generics
        .type_params()
        .filter(|p| p.attrs.iter().any(is_type_erased))
        .map(|p| &p.ident)
        .collect::<BTreeSet<_>>();

    let explicit_ids = dispatch_selector_inputs(inputs)
        .map(|(_, _, tag_id)| tag_id)
        .collect::<BTreeSet<_>>();
    let has_explicit_self_id = inputs.iter().any(|input| {
        let syn::FnArg::Typed(input) = input else {
            return false;
        };
        let Some(self_id) = self_id else {
            return false;
        };
        matches!(
            input.ty.as_ref(),
            syn::Type::Path(syn::TypePath {
                qself: Some(syn::QSelf { ty, .. }),
                path,
                ..
            }) if path.is_ident("TAG") && ty.as_ref() == self_id
        )
    });

    if erased_params.is_empty()
        && self_id.is_some()
        && !has_explicit_self_id
        && !explicit_ids.contains(&TagId::DynSelf)
    {
        synthesized.push(parse_quote!(__co3_self_id: <dyn Self>::TAG));
    }

    for ident in erased_params {
        if explicit_ids.contains(&TagId::DynType(ident)) {
            continue;
        }

        let pat = format_ident!("{ident}_id");
        synthesized.push(parse_quote!(#pat: <dyn #ident>::TAG));
    }

    synthesized.extend(core::mem::take(inputs));
    *inputs = synthesized.into_iter().collect();
}

#[expect(clippy::too_many_arguments)]
fn synthesize_dispatch_arms(
    generics: &syn::Generics,
    callee_type_params: &[syn::Ident],
    receiver: DispatchReceiver,
    sig: &syn::Signature,
    fn_by_val: bool,
    callee: &syn::Expr,
    args: &DispatchGroups,
    failure_mode: FailureMode,
    drop_impl: bool,
) -> Vec<TokenStream> {
    let derase_tag_stmts = gen_tag_retype_stmts(RetypeDirection::Derase, generics, receiver, sig);

    let dispatch_selectors = dispatch_selector_inputs(&sig.inputs)
        .map(|(_, _, tag_id)| tag_id)
        .collect::<Vec<_>>();

    let mut arms = Vec::new();
    args.for_each_combination(|selections| {
        let mut monomorphizer = DispatchMonomorphizer::for_dispatch_group(generics, selections);
        let mut arm_sig = sig.clone();
        let mut patterns = dispatch_selectors
            .iter()
            .map(|tag_id| {
                let tag_ty = match tag_id {
                    TagId::DynSelf => {
                        let ty = receiver
                            .ty()
                            .expect("`dyn Self` selector requires an impl self type");
                        quote!(#ty)
                    }
                    TagId::DynType(ident) => quote!(#ident),
                };

                parse_quote! { <#tag_ty as co3::tag::Tagged>::TAG }
            })
            .collect::<Vec<syn::Expr>>();

        arm_sig.inputs = arm_sig
            .inputs
            .into_iter()
            .filter(|input| !is_tag_id_arg(input))
            .collect();

        let retype_sig = arm_sig.clone();
        monomorphizer.visit_signature_mut(&mut arm_sig);
        let mut check_callee = instantiate_dispatch_callee(callee, callee_type_params, selections);

        monomorphizer.visit_expr_mut(&mut check_callee);
        let signature_check = (!drop_impl)
            .then(|| gen_static_fn_signature_drift_check(arm_sig.clone(), check_callee.clone()));
        let arm_body = if drop_impl {
            gen_drop_definition_body(&arm_sig, failure_mode)
        } else {
            gen_definition_body(arm_sig, quote!(#check_callee), fn_by_val, failure_mode)
        };
        let mut arm_body: syn::Block = if let ReturnType::Type(_, output_ty) = &retype_sig.output {
            let mut concrete_ty = item_fn_output_type(output_ty);
            monomorphizer.visit_type_mut(&mut concrete_ty);

            let erased_ty =
                ErasedParamReplacer::new(generics).replace(item_fn_output_type(output_ty));
            let (concrete_ty, erased_ty) = if fn_by_val {
                (concrete_ty, erased_ty)
            } else {
                (
                    parse_quote!(<#concrete_ty as co3::borrow::BorrowCast>::AsConst),
                    parse_quote!(<#erased_ty as co3::borrow::BorrowCast>::AsConst),
                )
            };
            let erased_out = gen_retype(&quote!(__co3_arm_out), &concrete_ty, &erased_ty);

            let erased_err = match failure_mode {
                FailureMode::Panic => {
                    let failure_panic = gen_failure_panic(quote!(__co3_arm_err));
                    quote! { Err(__co3_arm_err) => #failure_panic, }
                }
                FailureMode::Error => {
                    let erased_err = gen_retype(&quote!(__co3_arm_err), &concrete_ty, &erased_ty);
                    let encoded_err =
                        ffi_fn::gen_abi_return_encode(quote!(__co3_arm_err), !fn_by_val);

                    quote! {
                        Err(__co3_arm_err) => {
                            let __co3_arm_err = #encoded_err;
                            Ok(#erased_err)
                        },
                    }
                }
            };

            parse_quote! {{
                match (|| -> Result<_, _> {
                    #(#derase_tag_stmts)*
                    #signature_check
                    #arm_body
                })() {
                    Ok(__co3_arm_out) => Ok::<_, ()>(#erased_out),
                    #erased_err
                }
            }}
        } else {
            parse_quote! {{
                (|| -> Result<_, _> {
                    #(#derase_tag_stmts)*
                    #signature_check
                    #arm_body
                })()
            }}
        };

        patterns
            .iter_mut()
            .for_each(|pat| monomorphizer.visit_expr_mut(pat));

        monomorphizer.visit_block_mut(&mut arm_body);
        arms.push(quote! { (#(#patterns,)*) => { #arm_body }});
    });
    arms
}

fn instantiate_dispatch_callee(
    callee: &syn::Expr,
    callee_type_params: &[syn::Ident],
    selections: &[crate::DispatchSelection<'_>],
) -> syn::Expr {
    let mut args = callee_type_params
        .iter()
        .filter_map(|param| {
            selections.iter().find_map(|selection| {
                let index = selection
                    .params
                    .iter()
                    .position(|candidate| candidate == param)?;
                selection.target.args.get(index).cloned()
            })
        })
        .collect::<Punctuated<syn::GenericArgument, syn::Token![,]>>();

    if args.is_empty() {
        return callee.clone();
    }

    let mut callee = callee.clone();
    let syn::Expr::Path(path) = &mut callee else {
        return callee;
    };
    let Some(segment) = path.path.segments.last_mut() else {
        return callee;
    };
    segment.arguments = syn::PathArguments::AngleBracketed(syn::AngleBracketedGenericArguments {
        colon2_token: Some(Default::default()),
        lt_token: Default::default(),
        args: core::mem::take(&mut args),
        gt_token: Default::default(),
    });
    callee
}

pub(crate) fn erase_dispatch_signature(
    generics: &syn::Generics,
    receiver: DispatchReceiver,
    sig: &mut syn::Signature,
) {
    let mut erased_params = ErasedParamReplacer::new(generics);

    let tag_ids = dispatch_selector_inputs(&sig.inputs)
        .filter_map(|(idx, _, tag_id)| {
            Some((idx, resolve_tag_id_type(generics, receiver, tag_id)?))
        })
        .collect::<Vec<_>>();

    for input in &mut sig.inputs {
        match input {
            syn::FnArg::Receiver(rec) => {
                if let syn::ReceiverKind::Typed(_, ty) = &mut rec.kind {
                    **ty = erased_params.replace((**ty).clone());
                }
            }
            syn::FnArg::Typed(syn::PatType { attrs, pat, ty, .. }) => {
                if let Some(target_ty) = crate::ffi_fn::single_unpack_part(attrs, ty)
                    .expect("validated one-part unpack attribute")
                {
                    **ty = target_ty;
                } else if !is_unpack_arg(attrs) {
                    **ty = erased_params.replace((**ty).clone());
                }

                if receiver.is_dyn_self()
                    && (is_synthetic_receiver(pat)
                        || is_declared_self_type(receiver.ty().unwrap(), ty))
                {
                    **ty = erase_dyn_self_type(receiver.ty().unwrap(), ty);
                }
            }
        }
    }

    if let ReturnType::Type(_, ty) = &mut sig.output {
        **ty = erased_params.replace((**ty).clone());
    }

    for (idx, lowered_ty) in tag_ids {
        let syn::FnArg::Typed(syn::PatType { ty, .. }) = &mut sig.inputs[idx] else {
            continue;
        };

        **ty = lowered_ty;
    }
}

pub(crate) fn gen_tag_erase_stmts(
    self_ty: &syn::Type,
    generics: &syn::Generics,
    erase_declared_self: bool,
    sig: &syn::Signature,
) -> Vec<TokenStream> {
    let mut sig = sig.clone();

    normalize_fn_signature(&mut sig, Some(self_ty));
    gen_tag_retype_stmts(
        RetypeDirection::Erase,
        generics,
        if erase_declared_self {
            DispatchReceiver::DynSelf {
                ty: self_ty,
                id: None,
            }
        } else {
            DispatchReceiver::Impl {
                ty: self_ty,
                id: None,
            }
        },
        &sig,
    )
}

pub(crate) fn gen_dyn_self_erase_stmts(
    self_ty: &syn::Type,
    sig: &syn::Signature,
) -> Vec<TokenStream> {
    let mut sig = sig.clone();
    normalize_fn_signature(&mut sig, Some(self_ty));
    gen_tag_retype_stmts(
        RetypeDirection::Erase,
        &syn::Generics::default(),
        DispatchReceiver::DynSelf {
            ty: self_ty,
            id: None,
        },
        &sig,
    )
}

fn is_synthetic_receiver(pat: &syn::Pat) -> bool {
    matches!(pat, syn::Pat::Ident(ident) if ident.ident == "__co3_self")
}

pub(crate) fn is_declared_self_type(self_ty: &syn::Type, ty: &syn::Type) -> bool {
    fn is_self(ty: &syn::Type) -> bool {
        matches!(ty, syn::Type::Path(path) if path.qself.is_none() && path.path.is_ident("Self"))
    }

    ty == self_ty
        || is_self(ty)
        || matches!(ty,
            syn::Type::Reference(reference)
                if reference.elem.as_ref() == self_ty || is_self(&reference.elem)
        )
        || matches!(ty,
            syn::Type::Ptr(pointer)
                if pointer.elem.as_ref() == self_ty || is_self(&pointer.elem)
        )
}

pub(crate) fn has_declared_self_input(self_ty: &syn::Type, sig: &syn::Signature) -> bool {
    sig.inputs.iter().any(|input| match input {
        syn::FnArg::Receiver(_) => true,
        syn::FnArg::Typed(arg) => is_declared_self_type(self_ty, &arg.ty),
    })
}

fn erase_dyn_self_type(self_ty: &syn::Type, ty: &syn::Type) -> syn::Type {
    struct Replacer<'a> {
        self_ty: &'a syn::Type,
        replaced: bool,
    }

    impl VisitMut for Replacer<'_> {
        fn visit_type_mut(&mut self, node: &mut syn::Type) {
            if node == self.self_ty {
                *node = parse_quote!(core::ffi::c_void);
                self.replaced = true;
                return;
            }

            syn::visit_mut::visit_type_mut(self, node);
        }
    }

    let mut erased = ty.clone();
    if let syn::Type::Reference(reference) = &mut erased {
        *reference.elem = parse_quote!(core::ffi::c_void);
    } else {
        let mut replacer = Replacer {
            self_ty,
            replaced: false,
        };

        replacer.visit_type_mut(&mut erased);
    }

    erased
}

pub(crate) fn gen_retype(
    arg_name: &TokenStream,
    source_ty: &syn::Type,
    target_ty: &syn::Type,
) -> TokenStream {
    crate::abi_retype::gen_retype_after_check(arg_name.clone(), source_ty, target_ty)
}

pub(crate) fn set_token_stream_span(tokens: TokenStream, span: Span) -> TokenStream {
    tokens
        .into_iter()
        .map(|token| match token {
            TokenTree::Group(group) => {
                let mut replacement = proc_macro2::Group::new(
                    group.delimiter(),
                    set_token_stream_span(group.stream(), span),
                );
                replacement.set_span(span);
                TokenTree::Group(replacement)
            }
            mut token => {
                token.set_span(span);
                token
            }
        })
        .collect()
}

pub(crate) fn gen_return_derase_expr(
    generics: &syn::Generics,
    output_ty: &syn::Type,
    value: TokenStream,
    fn_by_val: bool,
) -> TokenStream {
    let concrete_ty = item_fn_output_type(output_ty);
    let erased_output_ty = ErasedParamReplacer::new(generics).replace(output_ty.clone());
    let erased_ty = item_fn_output_type(&erased_output_ty);

    let (concrete_ty, erased_ty) = if fn_by_val {
        (concrete_ty, erased_ty)
    } else {
        (
            parse_quote!(<#concrete_ty as co3::borrow::BorrowCast>::AsConst),
            parse_quote!(<#erased_ty as co3::borrow::BorrowCast>::AsConst),
        )
    };

    gen_retype(&value, &erased_ty, &concrete_ty)
}

fn gen_tag_retype_stmts(
    direction: RetypeDirection,
    generics: &syn::Generics,
    receiver: DispatchReceiver,
    sig: &syn::Signature,
) -> Vec<TokenStream> {
    let args = sig.inputs.iter().filter(|input| !is_tag_id_arg(input));
    let mut erased_params = ErasedParamReplacer::new(generics);
    let erased_idents = generics
        .type_params()
        .filter(|param| param.attrs.iter().any(is_type_erased))
        .map(|param| &param.ident)
        .collect::<Vec<_>>();
    let detector = ParamUseDetector::new(erased_idents);

    let mut stmts = vec![];
    for input in args {
        let receiver_ty;
        let (attrs, arg_name, ty, is_self) = match input {
            syn::FnArg::Receiver(receiver) => {
                receiver_ty = crate::utils::receiver_ty(receiver);
                (&receiver.attrs, quote!(__co3_self), &receiver_ty, true)
            }
            syn::FnArg::Typed(syn::PatType { attrs, pat, ty, .. }) => (
                attrs,
                quote!(#pat),
                &**ty,
                is_synthetic_receiver(pat)
                    || (receiver.is_dyn_self()
                        && is_declared_self_type(receiver.ty().unwrap(), ty)),
            ),
        };

        if is_unpack_arg(attrs) || crate::ffi_fn::is_single_unpack_arg(attrs) {
            continue;
        }

        if !is_self && !detector.type_mentions_param(ty) {
            continue;
        }

        if !is_self {
            let (_, changed) = erased_params.replace_with_change(ty.clone());
            if !changed {
                continue;
            }
        }

        let c_ty = item_fn_input_arg_type(attrs, ty);
        let c_ty: syn::Type = parse_quote! { #c_ty };
        let erased_ty = if receiver.is_dyn_self() && is_self {
            let erased_self_ty = erase_dyn_self_type(receiver.ty().unwrap(), ty);
            let erased_c_ty = item_fn_input_arg_type(attrs, &erased_self_ty);
            parse_quote!(#erased_c_ty)
        } else {
            let erased_input_ty = erased_params.replace(ty.clone());
            let erased_c_ty = item_fn_input_arg_type(attrs, &erased_input_ty);
            parse_quote!(#erased_c_ty)
        };

        let retype = match direction {
            RetypeDirection::Erase => gen_retype(&arg_name, &c_ty, &erased_ty),
            RetypeDirection::Derase => gen_retype(&arg_name, &erased_ty, &c_ty),
        };

        let retype = set_token_stream_span(quote! { let #arg_name = #retype; }, sig.span());
        stmts.push(retype);
    }

    stmts
}

pub(crate) fn tag_id(ty: &syn::Type) -> Option<TagId<'_>> {
    let syn::Type::Path(syn::TypePath {
        qself:
            Some(syn::QSelf {
                ty,
                position: 0,
                as_token: None,
                ..
            }),
        path,
        ..
    }) = ty
    else {
        return None;
    };
    if path.segments.len() != 1 {
        return None;
    }

    let first_seg = path.segments.first()?;
    if first_seg.ident != "TAG" || !first_seg.arguments.is_none() {
        return None;
    }

    let syn::Type::TraitObject(arg_ty) = ty.as_ref() else {
        return None;
    };
    if arg_ty.bounds.len() != 1 {
        return None;
    }

    let syn::TypeParamBound::Trait(arg_ty) = arg_ty.bounds.first()? else {
        return None;
    };
    if arg_ty.maybe.is_some() || arg_ty.lifetimes.is_some() {
        return None;
    }
    if arg_ty.path.segments.len() > 1 {
        return None;
    }
    if let Some(arg_path) = arg_ty.path.get_ident()
        && arg_path == "Self"
    {
        return Some(TagId::DynSelf);
    }

    Some(TagId::DynType(&arg_ty.path.segments.first()?.ident))
}

pub(crate) fn is_tag_id_arg(input: &syn::FnArg) -> bool {
    let syn::FnArg::Typed(arg) = input else {
        return false;
    };

    tag_id(&arg.ty).is_some()
}

fn dispatch_selector_inputs(
    inputs: &Punctuated<syn::FnArg, syn::Token![,]>,
) -> impl Iterator<Item = (usize, &syn::Pat, TagId<'_>)> {
    inputs.iter().enumerate().filter_map(|(idx, input)| {
        let syn::FnArg::Typed(syn::PatType { pat, ty, .. }) = input else {
            return None;
        };

        Some((idx, pat.as_ref(), tag_id(ty)?))
    })
}

fn resolve_tag_id_type(
    generics: &syn::Generics,
    receiver: DispatchReceiver<'_>,
    tag_id: TagId<'_>,
) -> Option<syn::Type> {
    match tag_id {
        TagId::DynSelf => receiver.id().cloned(),
        TagId::DynType(ident) => generics
            .type_params()
            .find(|param| param.ident == *ident)
            .and_then(erased_id_repr),
    }
}

fn monomorphize_predicates(generics: &mut syn::Generics, args: &DispatchGroups) {
    fn strip_relaxed_sized_bounds(predicate: &mut syn::WherePredicate) -> bool {
        let syn::WherePredicate::Type(predicate) = predicate else {
            return true;
        };

        predicate.bounds = core::mem::take(&mut predicate.bounds)
            .into_iter()
            .filter(|bound| {
                !matches!(
                    bound,
                    syn::TypeParamBound::Trait(bound)
                        if bound.maybe.is_some()
                )
            })
            .collect();

        !predicate.bounds.is_empty()
    }

    let params = generics
        .params
        .iter()
        .filter_map(|param| {
            if let syn::GenericParam::Type(param) = param {
                return Some(param.ident.clone());
            }

            None
        })
        .collect::<Vec<_>>();

    let mut monomorphized_predicates = Punctuated::new();
    let param_detector = ParamUseDetector::new(&params);

    let old_predicates = generics
        .where_clause
        .as_mut()
        .map(|w| core::mem::take(&mut w.predicates))
        .unwrap_or_default();

    for generic_predicate in old_predicates {
        if !param_detector.predicate_mentions_param(&generic_predicate) {
            monomorphized_predicates.push(generic_predicate);
            continue;
        }

        args.for_each_combination(|selections| {
            let mut concrete_predicate = generic_predicate.clone();

            let mut concrete_args: syn::AngleBracketedGenericArguments = syn::parse_quote!(<>);
            concrete_args.args.extend(
                selections
                    .iter()
                    .flat_map(|selection| selection.target.args.iter().cloned()),
            );

            let args = inject_predicate_unnamed_lifetimes(
                &mut generics.params,
                &mut concrete_predicate,
                concrete_args,
            );

            let mut monomorphizer = DispatchMonomorphizer::for_substitutions(
                generics,
                selections
                    .iter()
                    .flat_map(|selection| selection.params.iter())
                    .zip(&args.args),
            );
            monomorphizer.visit_where_predicate_mut(&mut concrete_predicate);
            if strip_relaxed_sized_bounds(&mut concrete_predicate) {
                monomorphized_predicates.push(concrete_predicate);
            }
        });
    }

    generics.make_where_clause().predicates = monomorphized_predicates;
}

struct NamedLifetime {
    lifetime: syn::Lifetime,
    universal: bool,
}

struct NamedDispatchEntry {
    entry: syn::AngleBracketedGenericArguments,
    concrete_lifetimes: Vec<syn::Lifetime>,
    universal_lifetimes: Vec<syn::Lifetime>,
}

struct EntryLifetimeNamer {
    prefix: String,
    span: proc_macro2::Span,
    next_lifetime: usize,
    universal_depth: usize,
    lifetimes: Vec<NamedLifetime>,
}

impl EntryLifetimeNamer {
    fn next_lifetime(&mut self) -> syn::Lifetime {
        let lifetime = syn::Lifetime::new(
            &format!("'{}_{}", self.prefix, self.next_lifetime),
            self.span,
        );
        self.next_lifetime += 1;
        lifetime
    }

    fn bind_lifetime(&mut self) -> syn::Lifetime {
        let lifetime = self.next_lifetime();
        self.lifetimes.push(NamedLifetime {
            lifetime: lifetime.clone(),
            universal: self.universal_depth > 0,
        });
        lifetime
    }

    fn visit_universal(&mut self, f: impl FnOnce(&mut Self)) {
        self.universal_depth += 1;
        f(self);
        self.universal_depth -= 1;
    }
}

impl VisitMut for EntryLifetimeNamer {
    fn visit_type_path_mut(&mut self, node: &mut syn::TypePath) {
        if let Some(qself) = &mut node.qself {
            self.visit_type_mut(&mut qself.ty);
            self.visit_universal(|this| {
                for segment in &mut node.path.segments {
                    this.visit_path_arguments_mut(&mut segment.arguments);
                }
            });
            return;
        }

        syn::visit_mut::visit_type_path_mut(self, node);
    }

    fn visit_type_reference_mut(&mut self, node: &mut syn::TypeReference) {
        if node.lifetime.as_ref().is_none_or(|l| l.ident == "_") {
            node.lifetime = Some(self.bind_lifetime());
        }

        syn::visit_mut::visit_type_reference_mut(self, node);
    }

    fn visit_lifetime_mut(&mut self, node: &mut syn::Lifetime) {
        if node.ident == "_" {
            *node = self.bind_lifetime();
        }
    }

    fn visit_type_fn_ptr_mut(&mut self, _: &mut syn::TypeFnPtr) {}
}

fn name_unnamed_lifetimes(
    generics: &Punctuated<GenericParam, syn::Token![,]>,
    mut entry: syn::AngleBracketedGenericArguments,
    mut include_param: impl FnMut(&syn::TypeParam) -> bool,
) -> NamedDispatchEntry {
    let mut concrete_lifetimes = Vec::new();
    let mut universal_lifetimes = Vec::new();

    for (param_idx, (param, arg)) in generics
        .iter()
        .filter(|param| !matches!(param, syn::GenericParam::Lifetime(_)))
        .zip(&mut entry.args)
        .enumerate()
    {
        let syn::GenericParam::Type(param) = param else {
            continue;
        };

        if !include_param(param) {
            continue;
        }

        let mut namer = EntryLifetimeNamer {
            prefix: format!("__co3_dispatch_{param_idx}"),
            span: param.ident.span(),
            next_lifetime: 0,
            universal_depth: 0,
            lifetimes: Vec::new(),
        };
        namer.visit_generic_argument_mut(arg);

        for named in namer.lifetimes {
            if named.universal {
                universal_lifetimes.push(named.lifetime);
            } else {
                concrete_lifetimes.push(named.lifetime);
            }
        }
    }

    NamedDispatchEntry {
        entry,
        concrete_lifetimes,
        universal_lifetimes,
    }
}

fn push_lifetime_param(
    generics: &mut Punctuated<GenericParam, syn::Token![,]>,
    lifetime: &syn::Lifetime,
) {
    if generics.iter().any(
        |param| matches!(param, syn::GenericParam::Lifetime(param) if param.lifetime == *lifetime),
    ) {
        return;
    }

    generics.push(parse_quote!(#lifetime));
}

fn push_lifetime_params(
    generics: &mut Punctuated<GenericParam, syn::Token![,]>,
    lifetimes: &[syn::Lifetime],
) {
    for lifetime in lifetimes {
        push_lifetime_param(generics, lifetime);
    }
}

pub(crate) fn inject_unnamed_lifetimes(
    generics: &mut Punctuated<GenericParam, syn::Token![,]>,
    entry: syn::AngleBracketedGenericArguments,
) -> syn::AngleBracketedGenericArguments {
    let named = name_unnamed_lifetimes(generics, entry, |_| true);
    push_lifetime_params(generics, &named.concrete_lifetimes);
    named.entry
}

fn inject_predicate_unnamed_lifetimes(
    generics: &mut Punctuated<GenericParam, syn::Token![,]>,
    predicate: &mut syn::WherePredicate,
    entry: syn::AngleBracketedGenericArguments,
) -> syn::AngleBracketedGenericArguments {
    struct PredicateLifetimeNamer {
        next_lifetime: usize,
        lifetimes: Vec<syn::Lifetime>,
    }

    impl PredicateLifetimeNamer {
        fn bind_lifetime(&mut self, span: proc_macro2::Span) -> syn::Lifetime {
            let lifetime = syn::Lifetime::new(
                &format!("'__co3_dispatch_predicate_{}", self.next_lifetime),
                span,
            );
            self.next_lifetime += 1;
            self.lifetimes.push(lifetime.clone());
            lifetime
        }
    }

    impl VisitMut for PredicateLifetimeNamer {
        fn visit_type_reference_mut(&mut self, node: &mut syn::TypeReference) {
            if node.lifetime.as_ref().is_none_or(|l| l.ident == "_") {
                node.lifetime = Some(self.bind_lifetime(node.span()));
            }

            syn::visit_mut::visit_type_reference_mut(self, node);
        }

        fn visit_lifetime_mut(&mut self, node: &mut syn::Lifetime) {
            if node.ident == "_" {
                *node = self.bind_lifetime(node.span());
            }
        }

        fn visit_type_fn_ptr_mut(&mut self, _: &mut syn::TypeFnPtr) {}
    }

    fn push_universal_lifetimes(
        predicate: &mut syn::WherePredicate,
        lifetimes: impl IntoIterator<Item = syn::Lifetime>,
    ) {
        let syn::WherePredicate::Type(predicate) = predicate else {
            return;
        };

        let bound_lifetimes = &mut predicate
            .lifetimes
            .get_or_insert_with(|| parse_quote!(for<>))
            .lifetimes;

        for lifetime in lifetimes {
            if bound_lifetimes.iter().any(|param| {
                matches!(param, syn::GenericParam::Lifetime(param) if param.lifetime == lifetime)
            }) {
                continue;
            }

            bound_lifetimes.push(syn::GenericParam::Lifetime(parse_quote!(#lifetime)));
        }
    }

    let mut named = name_unnamed_lifetimes(generics, entry, |param| {
        ParamUseDetector::new([&param.ident]).predicate_mentions_param(predicate)
    });
    push_lifetime_params(generics, &named.concrete_lifetimes);

    let mut predicate_lifetime_namer = PredicateLifetimeNamer {
        next_lifetime: 0,
        lifetimes: Vec::new(),
    };
    predicate_lifetime_namer.visit_where_predicate_mut(predicate);

    named
        .universal_lifetimes
        .extend(predicate_lifetime_namer.lifetimes);
    push_universal_lifetimes(predicate, named.universal_lifetimes);

    named.entry
}

#[cfg(test)]
mod static_lifetime_normalizer_tests {
    use super::*;

    #[test]
    fn preserves_higher_ranked_lifetimes_and_normalizes_free_lifetimes() {
        let mut fn_ptr: syn::Type = syn::parse_quote!(
            for<'a> fn(&'a u8, &'outer u8, for<'b> fn(&'b u8, &'a u8, &'outer u8))
        );
        StaticLifetimeNormalizer::default().visit_type_mut(&mut fn_ptr);
        assert_eq!(
            quote!(#fn_ptr).to_string(),
            quote!(for<'a> fn(&'a u8, &'static u8, for<'b> fn(&'b u8, &'a u8, &'static u8)))
                .to_string()
        );

        let mut trait_object: syn::Type = syn::parse_quote!(dyn for<'a> Trait<&'a u8, &'outer u8>);
        StaticLifetimeNormalizer::default().visit_type_mut(&mut trait_object);
        assert_eq!(
            quote!(#trait_object).to_string(),
            quote!(dyn for<'a> Trait<&'a u8, &'static u8>).to_string()
        );

        let mut predicate: syn::WherePredicate =
            syn::parse_quote!(for<'a> &'a u8: Trait<&'outer u8>);
        StaticLifetimeNormalizer::default().visit_where_predicate_mut(&mut predicate);
        assert_eq!(
            quote!(#predicate).to_string(),
            quote!(for<'a> &'a u8: Trait<&'static u8>).to_string()
        );
    }
}

#[cfg(test)]
mod erased_param_replacer_tests {
    use super::*;

    fn erase(ty: syn::Type) -> String {
        let generics = syn::parse_quote!(<#[erased(u8)] T>);
        let erased = ErasedParamReplacer::new(&generics).replace(ty);
        quote!(#erased).to_string()
    }

    fn erase_receiver(ty: syn::Type) -> String {
        let self_ty = syn::parse_quote!(Self);
        let erased = erase_dyn_self_type(&self_ty, &ty);
        quote!(#erased).to_string()
    }

    #[test]
    fn no_payload_before_suffix_erases_the_root() {
        assert_eq!(erase(syn::parse_quote!((T, u32))), "core :: ffi :: c_void");
        assert_eq!(
            erase(syn::parse_quote!(&(T, u32))),
            "& core :: ffi :: c_void"
        );
    }

    #[test]
    fn no_payload_at_tail_preserves_the_prefix() {
        assert_eq!(
            erase(syn::parse_quote!((u32, T))),
            "(u32 , core :: ffi :: c_void)"
        );
        assert_eq!(
            erase(syn::parse_quote!(&(u32, T))),
            "& (u32 , core :: ffi :: c_void)"
        );
    }

    #[test]
    fn no_payload_truncates_only_the_suffix() {
        assert_eq!(
            erase(syn::parse_quote!((u32, T, u32))),
            "(u32 , core :: ffi :: c_void)"
        );
        assert_eq!(
            erase(syn::parse_quote!((u32, u32, T))),
            "(u32 , u32 , core :: ffi :: c_void)"
        );
        assert_eq!(
            erase(syn::parse_quote!((u32, core::ffi::c_void, u32))),
            "(u32 , core :: ffi :: c_void , u32)"
        );
        assert_eq!(
            erase(syn::parse_quote!((u32, u32, core::ffi::c_void))),
            "(u32 , u32 , core :: ffi :: c_void)"
        );
        assert_eq!(
            erase(syn::parse_quote!((u32, user::c_void, u32))),
            "(u32 , user :: c_void , u32)"
        );
    }

    #[test]
    fn no_payload_is_not_erased_through_paths_or_projections() {
        assert_eq!(erase(syn::parse_quote!(Wrapper<T>)), "Wrapper < T >");
        assert_eq!(erase(syn::parse_quote!(T::Assoc)), "T :: Assoc");
        assert_eq!(
            erase(syn::parse_quote!(<T as Trait>::Assoc)),
            "< T as Trait > :: Assoc"
        );
    }

    #[test]
    fn indirect_wrapper_or_projection_erases_the_whole_pointee() {
        assert_eq!(
            erase(syn::parse_quote!(&Wrapper<T>)),
            "& core :: ffi :: c_void"
        );
        assert_eq!(
            erase(syn::parse_quote!(&mut MaybeUninit<T::Assoc>)),
            "& mut core :: ffi :: c_void"
        );
        assert_eq!(
            erase(syn::parse_quote!(*mut T::Assoc)),
            "* mut core :: ffi :: c_void"
        );
        assert_eq!(
            erase(syn::parse_quote!(&<T as Trait>::Assoc)),
            "& core :: ffi :: c_void"
        );
    }

    #[test]
    fn finds_indirections_nested_inside_type_paths() {
        assert_eq!(
            erase(syn::parse_quote!(Option<&T::Assoc>)),
            "Option < & core :: ffi :: c_void >"
        );
        assert_eq!(
            erase(syn::parse_quote!(Option<&mut MaybeUninit<T::Assoc>>)),
            "Option < & mut core :: ffi :: c_void >"
        );
        assert_eq!(
            erase(syn::parse_quote!(Wrapper<fn(T::Assoc)>)),
            "Wrapper < fn (T :: Assoc) >"
        );
    }

    #[test]
    fn receiver_pointee_is_always_erased() {
        assert_eq!(
            erase_receiver(syn::parse_quote!(&Wrapper<Self>)),
            "& core :: ffi :: c_void"
        );
        assert_eq!(
            erase_receiver(syn::parse_quote!(&<Self as Trait>::Assoc)),
            "& core :: ffi :: c_void"
        );
        assert_eq!(
            erase_receiver(syn::parse_quote!(Box<Self>)),
            "Box < core :: ffi :: c_void >"
        );
    }
}
