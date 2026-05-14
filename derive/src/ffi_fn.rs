use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{
    Ident, Path, Type, parse_quote,
    visit::{Visit, visit_type},
    visit_mut::VisitMut,
};

use crate::{
    dispatch::handle_id,
    generate::OwnershipMode,
    utils::{TypeImplTraitResolver, gen_normalization_stmts, soft_for_arg, unwrap_result_type},
};

pub(crate) fn emit_extern_definition(
    abi: &syn::Abi,
    attrs: &[syn::Attribute],
    fn_signature: TokenStream,
    ffi_fn_body: TokenStream,
) -> TokenStream {
    quote! {
        #(#attrs)*
        unsafe #abi #fn_signature {
            let fn_ = || {
                let fn_body = || #ffi_fn_body;

                if let Err(err) = fn_body() {
                    return err;
                }

                co3::FfiReturn::Ok
            };

            match std::panic::catch_unwind(fn_) {
                Ok(res) => res,
                Err(_) => {
                    // TODO: Implement error handling (https://github.com/hyperledger/iroha/issues/2252)
                    co3::FfiReturn::UnrecoverableError
                },
            }
        }
    }
}

pub(crate) fn gen_definition_body(sig: syn::Signature, callee: TokenStream) -> TokenStream {
    let fn_ty = signature_fn_pointer_type(&sig);

    let inputs = &sig.inputs;
    let output = &sig.output;

    let decode_input_stmts = gen_input_decode_stmts(inputs);
    let output_assignment = gen_output_encode_stmt(output);
    let store_sync_stmts = gen_store_sync_stmts(inputs.len());

    let arg_names = inputs
        .iter()
        .map(|arg| match arg {
            syn::FnArg::Typed(arg) => item_fn_input_ident(&arg.pat).clone(),
            syn::FnArg::Receiver(_) => format_ident!("__co3_self"),
        })
        .collect::<Vec<_>>();

    quote! {{
        #decode_input_stmts

        let mut __co3_call_error = false;

        // NOTE: Avoids signature drift
        let __co3_fn: #fn_ty = #callee;

        let __co3_output = __co3_fn(
            #(#arg_names),*
        );

        #output_assignment

        let __co3_sync_errors = #store_sync_stmts;
        let mut __co3_sync_errors_iter = core::iter::IntoIterator::into_iter(__co3_sync_errors);
        if core::iter::Iterator::any(&mut __co3_sync_errors_iter, core::convert::identity) {
            return Err(co3::FfiReturn::TrapRepresentation);
        }

        if __co3_call_error {
            return Err(co3::FfiReturn::ExecutionFail);
        }

        Ok(())
    }}
}

pub(crate) fn item_fn_input_ident(input: &syn::Pat) -> &Ident {
    let syn::Pat::Ident(syn::PatIdent { ident, .. }) = input else {
        unreachable!()
    };

    ident
}

pub(crate) fn gen_input_decode_stmts<'a>(
    inputs: impl IntoIterator<Item = &'a syn::FnArg>,
) -> TokenStream {
    let inputs = inputs.into_iter().collect::<Vec<_>>();

    let mut stmts = quote! {};
    let arg_names = inputs
        .iter()
        .map(|arg| match arg {
            syn::FnArg::Typed(arg) => item_fn_input_ident(&arg.pat).clone(),
            syn::FnArg::Receiver(_) => format_ident!("__co3_self"),
        })
        .collect::<Vec<_>>();

    let store_sync_stmts = gen_store_sync_stmts(inputs.len());

    let mut value_tys = Vec::with_capacity(inputs.len());
    let mut store_tys = Vec::with_capacity(inputs.len());

    let mut value_present = Vec::new();
    for (idx, input) in inputs.iter().enumerate() {
        let idx = syn::Index::from(idx);

        let (attrs, arg_name, arg_ty) = match input {
            syn::FnArg::Receiver(receiver) => (
                &receiver.attrs,
                format_ident!("__co3_self"),
                (*receiver.ty).clone(),
            ),
            syn::FnArg::Typed(arg) => {
                let arg_name = item_fn_input_ident(&arg.pat);
                let arg_ty = (*arg.ty).clone();
                (&arg.attrs, arg_name.clone(), arg_ty)
            }
        };

        let decode_ty = match ownership_mode_for_arg(attrs) {
            OwnershipMode::ByValue => quote!(#arg_ty),
            OwnershipMode::Borrow => quote! {
                <#arg_ty as co3::borrow::Borrow>::Borrowed<'_>
            },
        };

        let decode_call = if soft_for_arg(attrs) {
            quote! { co3::SoftDecode::decode(#arg_name, &mut __co3_input_stores.#idx) }
        } else {
            quote! { co3::Decode::decode(#arg_name) }
        };

        let to_owned = match ownership_mode_for_arg(attrs) {
            OwnershipMode::ByValue => quote!(#arg_name),
            OwnershipMode::Borrow => quote! {
                #arg_name.map(co3::borrow::ToOwned::to_owned)
            },
        };

        stmts.extend(quote! {
            let #arg_name: Option<#decode_ty> = unsafe { #decode_call };
            let #arg_name: Option<#arg_ty> = #to_owned;

            if let Some(#arg_name) = #arg_name {
                __co3_input_values.#idx = Some(#arg_name);
            }
        });

        value_tys.push(arg_ty);
        store_tys.push(if soft_for_arg(attrs) {
            quote!(<#decode_ty as co3::stored::SoftDecodeOwned<'_>>::Store)
        } else {
            quote!(())
        });

        value_present.push(quote! { __co3_input_values.#idx.is_some() });
    }

    quote! {
        let mut __co3_input_values = (#(<Option<#value_tys> as core::default::Default>::default(),)*);
        let mut __co3_input_stores = (#(<#store_tys as core::default::Default>::default(),)*);

        #stmts

        let __co3_input_present: [bool; _] = [
            #(#value_present),*
        ];

        let (#(Some(#arg_names),)*) = __co3_input_values else {
            let __co3_sync_errors = #store_sync_stmts;
            return Err(co3::FfiReturn::TrapRepresentation);
        };
    }
}

fn signature_fn_pointer_type(sig: &syn::Signature) -> TokenStream {
    let syn::Signature {
        unsafety,
        abi,
        output,
        inputs,
        ..
    } = sig;

    let arg_tys = inputs.iter().map(|input| match input {
        syn::FnArg::Receiver(syn::Receiver { ty, .. }) => ty,
        syn::FnArg::Typed(syn::PatType { ty, .. }) => ty,
    });

    quote! { #unsafety #abi fn(#(#arg_tys),*) #output }
}

fn gen_output_encode_stmt(ret_ty: &syn::ReturnType) -> TokenStream {
    let output = format_ident!("__co3_output");

    let syn::ReturnType::Type(_, ret_ty) = &ret_ty else {
        return quote! {};
    };

    if let Some((ok, _)) = unwrap_result_type(ret_ty) {
        let normalize_output = gen_normalization_stmts(&output, ok);

        quote! {
            match __co3_output {
                Ok(__co3_output) => {
                    #normalize_output

                    unsafe {
                        <#ok as co3::out_ptr::OutPtrWrite>::write_out(
                            #output,
                            __co3_out_ptr,
                        );
                    }
                }
                Err(_) => {
                    __co3_call_error = true;
                }
            }
        }
    } else {
        let normalize_output = gen_normalization_stmts(&output, ret_ty);

        quote! {
            #normalize_output

            unsafe {
                <#ret_ty as co3::out_ptr::OutPtrWrite>::write_out(
                    #output,
                    __co3_out_ptr
                );
            }
        }
    }
}

pub(crate) fn gen_store_sync_stmts(len: usize) -> TokenStream {
    let idxs = (0..len).map(syn::Index::from);

    quote! {{
        let mut __co3_sync_errors = [false; #len]; #(

        if __co3_input_present[#idxs] && co3::stored::Store::sync(__co3_input_stores.#idxs).is_none() {
            __co3_sync_errors[#idxs] = true;
        })*

        __co3_sync_errors
    }}
}

fn gen_fn_definition_body(item: &syn::ItemFn) -> TokenStream {
    let fn_name = &item.sig.ident;
    gen_definition_body(item.sig.clone(), quote! { self::#fn_name })
}

pub fn gen_impl_definition(abi: &syn::Abi, mut impl_: syn::ItemImpl) -> TokenStream {
    let trait_ = impl_.trait_.as_ref().map(|(_, path, _)| path);

    let self_ty = &impl_.self_ty;
    impl_.items.iter_mut().for_each(|item| {
        let syn::ImplItem::Fn(item) = item else {
            return;
        };

        normalize_fn_signature(&mut item.sig, Some(self_ty));
    });

    let definitions = impl_.items.into_iter().filter_map(|item| {
        let syn::ImplItem::Fn(mut item) = item else {
            return None;
        };

        let fn_name = &item.sig.ident;
        let callee = if let Some(trait_) = trait_ {
            quote!(<#self_ty as #trait_>::#fn_name)
        } else {
            quote!(<#self_ty>::#fn_name)
        };

        merge_generics(impl_.generics.clone(), &mut item.sig.generics);
        let ffi_fn_body = gen_definition_body(item.sig.clone(), callee);
        let fn_signature = gen_extern_fn_signature(item.sig.clone());

        Some(emit_extern_definition(
            abi,
            &item.attrs,
            fn_signature,
            ffi_fn_body,
        ))
    });

    quote! { #(#definitions)* }
}

pub(crate) fn merge_generics(impl_generics: syn::Generics, fn_generics: &mut syn::Generics) {
    fn_generics.params.extend(impl_generics.params);

    if let Some(impl_where_clause) = impl_generics.where_clause {
        fn_generics
            .make_where_clause()
            .predicates
            .extend(impl_where_clause.predicates);
    }
}

pub fn gen_fn_definition(abi: &syn::Abi, mut item: syn::ItemFn) -> TokenStream {
    normalize_fn_signature(&mut item.sig, None);

    let ffi_fn_body = gen_fn_definition_body(&item);
    let fn_signature = gen_extern_fn_signature(item.sig);

    emit_extern_definition(abi, &item.attrs, fn_signature, ffi_fn_body)
}

pub(crate) fn gen_extern_fn_signature(mut sig: syn::Signature) -> TokenStream {
    explicitize_signature_lifetimes(&mut sig);
    synthesize_lifetime_bounds(&mut sig);

    lower_signature_inputs(&mut sig);
    lower_signature_output(&mut sig);

    sig.generics.params = core::mem::take(&mut sig.generics.params)
        .into_iter()
        .filter(|p| matches!(p, syn::GenericParam::Lifetime(_)))
        .collect();

    sig.constness = None;
    sig.asyncness = None;
    sig.unsafety = None;
    sig.abi = None;

    quote! { #sig }
}

fn lower_signature_inputs(sig: &mut syn::Signature) {
    sig.inputs = core::mem::take(&mut sig.inputs)
        .into_iter()
        .map(|input| lower_signature_input(&mut sig.generics, input))
        .collect();
}

fn lower_signature_input(generics: &mut syn::Generics, input: syn::FnArg) -> syn::FnArg {
    let (pat, attrs, arg_ty) = match input {
        syn::FnArg::Receiver(receiver) => {
            let arg_ty = *receiver.ty;
            (quote!(__co3_self), receiver.attrs, arg_ty)
        }
        syn::FnArg::Typed(arg) => {
            let syn::PatType { attrs, pat, ty, .. } = arg;
            (quote!(#pat), attrs, *ty)
        }
    };

    let ffi_ty = item_fn_input_arg_type(&attrs, &arg_ty);

    generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#ffi_ty: co3::CFnArg));

    parse_quote!(#pat: #ffi_ty)
}

fn lower_signature_output(sig: &mut syn::Signature) {
    let return_type = core::mem::replace(&mut sig.output, parse_quote!(-> co3::FfiReturn));

    let syn::ReturnType::Type(_, return_type) = return_type else {
        return;
    };

    let ret_ty = unwrap_result_type(&return_type).map_or(&*return_type, |(ok, _)| ok);
    let output_ty = quote! { <#ret_ty as co3::out_ptr::OutPtr>::OutPtr };

    sig.generics
        .make_where_clause()
        .predicates
        // TODO: Should look for &mut instead of raw ptr?
        .push(parse_quote!(*mut #output_ty: co3::CFnReturn));

    sig.inputs
        .push(parse_quote! { __co3_out_ptr: *mut #output_ty });
}

fn explicitize_signature_lifetimes(sig: &mut syn::Signature) {
    struct InputLifetimeCollector<'a> {
        out_lifetime: Option<syn::Lifetime>,
        generics: &'a mut syn::Generics,
        multiple_lifetimes: bool,
        is_self: bool,
        next_idx: usize,
    }

    impl<'a> InputLifetimeCollector<'a> {
        fn new(generics: &'a mut syn::Generics) -> Self {
            Self {
                generics,
                next_idx: 0,
                out_lifetime: None,
                is_self: false,
                multiple_lifetimes: false,
            }
        }

        fn explicitize_lifetime(&mut self, lifetime: &mut Option<syn::Lifetime>) -> syn::Lifetime {
            if let Some(lifetime) = lifetime
                && lifetime.ident != "_"
            {
                return lifetime.clone();
            }

            let new_lifetime =
                syn::Lifetime::new(&format!("'__co3_{}", self.next_idx), Span::call_site());

            self.generics
                .params
                .push(syn::GenericParam::Lifetime(syn::LifetimeParam::new(
                    new_lifetime.clone(),
                )));

            self.next_idx += 1;
            lifetime.insert(new_lifetime).clone()
        }

        fn record_lifetime<const IS_SELF: bool>(&mut self, lifetime: syn::Lifetime) {
            if self.out_lifetime.is_none() {
                self.out_lifetime = Some(lifetime);
                self.is_self = IS_SELF;
            } else {
                self.multiple_lifetimes = true;

                if IS_SELF {
                    self.out_lifetime = Some(lifetime);
                    self.is_self = true;
                } else if !self.is_self {
                    self.out_lifetime = None;
                }
            }
        }
    }

    impl VisitMut for InputLifetimeCollector<'_> {
        fn visit_receiver_mut(&mut self, node: &mut syn::Receiver) {
            syn::visit_mut::visit_receiver_mut(self, node);

            if let Some((_, lifetime)) = &mut node.reference {
                let l = self.explicitize_lifetime(lifetime);

                self.record_lifetime::<true>(l);
            }
        }

        fn visit_type_reference_mut(&mut self, node: &mut syn::TypeReference) {
            syn::visit_mut::visit_type_reference_mut(self, node);
            let l = self.explicitize_lifetime(&mut node.lifetime);

            self.record_lifetime::<false>(l);
        }
    }

    struct OutputLifetimeExplicator<'a> {
        lifetime: &'a syn::Lifetime,
    }

    impl VisitMut for OutputLifetimeExplicator<'_> {
        fn visit_type_reference_mut(&mut self, node: &mut syn::TypeReference) {
            syn::visit_mut::visit_type_reference_mut(self, node);

            if node.lifetime.is_none() || matches!(&node.lifetime, Some(l) if l.ident == "_") {
                node.lifetime = Some(self.lifetime.clone());
            }
        }
    }

    let mut input_collector = InputLifetimeCollector::new(&mut sig.generics);

    for input in &mut sig.inputs {
        input_collector.visit_fn_arg_mut(input);
    }

    let output_lifetime = (!input_collector.multiple_lifetimes || input_collector.is_self)
        .then_some(input_collector.out_lifetime)
        .flatten();

    if let syn::ReturnType::Type(_, output_ty) = &mut sig.output
        && let Some(lifetime) = &output_lifetime
    {
        OutputLifetimeExplicator { lifetime }.visit_type_mut(output_ty);
    }
}
fn synthesize_lifetime_bounds(sig: &mut syn::Signature) {
    #[derive(Default)]
    struct LifetimeUseCollector<'a> {
        bounds: BTreeMap<&'a syn::Lifetime, BTreeSet<&'a syn::Lifetime>>,
        parent_lifetimes: Vec<&'a syn::Lifetime>,
    }

    impl<'a> syn::visit::Visit<'a> for LifetimeUseCollector<'a> {
        fn visit_type_reference(&mut self, node: &'a syn::TypeReference) {
            if let Some(lifetime) = &node.lifetime {
                let parents = &self.parent_lifetimes;
                self.bounds.entry(lifetime).or_default().extend(parents);

                self.parent_lifetimes.push(lifetime);
                visit_type(self, &node.elem);
                self.parent_lifetimes.pop();
            } else {
                visit_type(self, &node.elem);
            }
        }

        fn visit_lifetime(&mut self, node: &'a syn::Lifetime) {
            let parents = &self.parent_lifetimes;
            self.bounds.entry(node).or_default().extend(parents);
        }
    }

    let mut lifetime_collector = LifetimeUseCollector::default();
    lifetime_collector.visit_signature(sig);

    let bounds = lifetime_collector
        .bounds
        .into_iter()
        .map(|(k, v)| (k.clone(), v.into_iter().cloned().collect::<Vec<_>>()))
        .collect::<Vec<_>>();

    for (lhs, rhs) in bounds {
        sig.generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#lhs: #(#rhs)+*));
    }
}

fn item_fn_input_arg_type(attrs: &[syn::Attribute], arg_ty: &Type) -> TokenStream {
    let c_type = quote! { <#arg_ty as co3::ExternC>::CType };

    match ownership_mode_for_arg(attrs) {
        OwnershipMode::ByValue => quote! { #c_type },
        OwnershipMode::Borrow => quote! { <#c_type as co3::borrow::BorrowCast>::AsConst },
    }
}

pub(crate) fn normalize_fn_signature(sig: &mut syn::Signature, self_ty: Option<&Type>) {
    TypeImplTraitResolver.visit_signature_mut(sig);

    for input in &mut sig.inputs {
        if let syn::FnArg::Receiver(syn::Receiver { attrs, ty, .. }) = input {
            *input = parse_quote! { #(#attrs)* __co3_self: #ty }
        }
    }

    if let Some(self_ty) = self_ty {
        SelfConcretizer { self_ty }.visit_signature_mut(sig);
    }
}

pub(crate) fn ownership_mode_for_arg(attrs: &[syn::Attribute]) -> OwnershipMode {
    if attrs.iter().any(|attr| attr.path().is_ident("by_val")) {
        return OwnershipMode::ByValue;
    }

    OwnershipMode::Borrow
}

pub(crate) struct SelfConcretizer<'a> {
    pub(crate) self_ty: &'a Type,
}

fn qualify_self_path(self_ty: &Type, rest: &Path) -> Type {
    if rest.segments.is_empty() {
        return self_ty.clone();
    }

    if matches!(self_ty, Type::Path(type_path) if type_path.qself.is_none()) {
        return parse_quote!(#self_ty::#rest);
    }

    parse_quote!(<#self_ty>::#rest)
}

impl VisitMut for SelfConcretizer<'_> {
    fn visit_type_mut(&mut self, node: &mut Type) {
        syn::visit_mut::visit_type_mut(self, node);

        if handle_id(node).is_some() {
            return;
        }

        let Type::Path(syn::TypePath { qself: None, path }) = node else {
            return;
        };
        let Some(first) = path.segments.first() else {
            return;
        };
        if first.ident != "Self" {
            return;
        }

        let mut rest = Path {
            leading_colon: None,
            segments: Default::default(),
        };

        rest.segments.extend(path.segments.iter().skip(1).cloned());
        *node = qualify_self_path(self.self_ty, &rest);
    }
}
