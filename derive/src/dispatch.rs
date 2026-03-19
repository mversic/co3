use std::collections::BTreeSet;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    GenericArgument, GenericParam, Result, parse::Parser, parse_quote, punctuated::Punctuated,
    visit_mut::VisitMut,
};

use crate::{
    DispatchItem,
    ffi_fn::{
        emit_extern_definition, gen_definition_body, gen_extern_fn_signature,
        gen_signature_input_conversion_stmts, gen_signature_input_init_stmts,
        gen_signature_store_sync_stmts, normalize_fn_signature, ownership_mode_for_arg,
    },
    generate::OwnershipMode,
    utils::{
        DispatchMonomorphizer, dyn_dispatch_repr, is_drop_impl, is_type_erased, unwrap_result_type,
    },
};

pub(crate) fn find_dispatch_attr(attrs: &[syn::Attribute]) -> Option<&syn::Attribute> {
    attrs.iter().find(|&attr| attr.path().is_ident("dispatch"))
}

pub(crate) fn parse_dispatch_attr(
    impl_: &syn::ItemImpl,
) -> Result<Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]>> {
    let Some(attr) = find_dispatch_attr(&impl_.attrs) else {
        return Ok(Punctuated::default());
    };

    let params = &impl_
        .generics
        .params
        .iter()
        .filter(|arg| !matches!(arg, GenericParam::Lifetime(_)))
        .collect::<Vec<_>>();

    let err_msg = format!(
        "dispatch must provide {} generic argument{}",
        params.len(),
        if params.len() == 1 { "" } else { "s" }
    );

    let syn::Meta::List(list) = &attr.meta else {
        if !params.is_empty() {
            return Err(syn::Error::new_spanned(attr, err_msg));
        }

        return Ok(Punctuated::default());
    };

    let mut generic_args: Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]> =
        Punctuated::parse_terminated.parse2(list.tokens.clone())?;

    if generic_args.is_empty() && !params.is_empty() {
        return Err(syn::Error::new_spanned(attr, err_msg));
    }

    for entry in &generic_args {
        for arg in &entry.args {
            if matches!(arg, GenericArgument::Lifetime(_)) {
                let err_msg = "lifetime arguments not required in dispatch";
                return Err(syn::Error::new_spanned(arg, err_msg));
            }
        }

        if entry.args.len() != params.len() {
            return Err(syn::Error::new_spanned(entry, err_msg));
        }
    }

    let mut errors = None::<syn::Error>;
    for entry in &mut generic_args {
        let err_msg = "argument kind must match declared parameter kind";

        for (param, arg) in params.iter().zip(&mut entry.args) {
            if let (GenericParam::Const(_), GenericArgument::Type(ty)) = (param, &arg)
                && matches!(ty, syn::Type::Path(_))
            {
                *arg = parse_quote!({ #ty });
            }
        }

        for (param, arg) in params.iter().zip(&entry.args) {
            let mismatch = match param {
                GenericParam::Lifetime(_) => false,
                GenericParam::Type(_) => !matches!(arg, GenericArgument::Type(_)),
                GenericParam::Const(_) => !matches!(arg, GenericArgument::Const(_)),
            };

            if mismatch {
                let err = syn::Error::new_spanned(arg, err_msg);

                if let Some(errors) = &mut errors {
                    errors.combine(err);
                } else {
                    errors = Some(err);
                }
            }
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(generic_args)
}

pub(crate) fn gen_dispatch_export(abi: &syn::Abi, dispatch: DispatchItem) -> TokenStream {
    let DispatchItem { impl_, args, .. } = dispatch;

    let trait_ = impl_.trait_.as_ref().map(|(_, path, _)| path);
    let self_ty = &impl_.self_ty;
    let generics = &impl_.generics;

    let drop_impl = is_drop_impl(&impl_);
    let items = impl_.items.into_iter().filter_map(|item| {
        let syn::ImplItem::Fn(item) = item else {
            return None;
        };

        Some(item)
    });

    let definitions = items.map(|mut item| {
        normalize_fn_signature(&mut item.sig, Some(self_ty));
        let id_arg_indices = item
            .sig
            .inputs
            .iter()
            .enumerate()
            .filter_map(|input| {
                let (idx, input) = input;
                let syn::FnArg::Typed(syn::PatType { ty, .. }) = input else {
                    return None;
                };

                extract_dispatch_id(generics, self_ty, ty).map(|_| idx)
            })
            .collect::<Vec<_>>();
        let id_arg_names = id_arg_indices
            .iter()
            .filter_map(|idx| match item.sig.inputs.get(*idx) {
                Some(syn::FnArg::Typed(syn::PatType { pat, .. })) => Some((**pat).clone()),
                _ => None,
            })
            .collect::<Vec<_>>();

        let dispatch_arms =
            gen_dispatch_arms(generics, &item.sig, self_ty, trait_, drop_impl, &args)
                .collect::<Vec<_>>();

        erase_handle_types(generics, self_ty, &mut item.sig, &args);
        let id_inputs = id_arg_indices
            .iter()
            .filter_map(|idx| item.sig.inputs.get(*idx).cloned())
            .collect::<Vec<_>>();

        let id_initialization_stmts = gen_signature_input_init_stmts(&id_inputs);
        let id_input_conversions = gen_signature_input_conversion_stmts(&id_inputs);
        let id_store_sync = gen_signature_store_sync_stmts(id_inputs.len());

        let fn_body = quote! {{
            #id_initialization_stmts
            #id_input_conversions

            let (#(Some(#id_arg_names),)*) = __co3_input_values else {
                let __co3_sync_errors = #id_store_sync;
                return Err(co3::FfiReturn::TrapRepresentation);
            };

            let __co3_dispatch_result: Result<(), co3::FfiReturn> = match (#(#id_arg_names,)*) {
                #(#dispatch_arms,)*
                _ => Err(co3::FfiReturn::UnknownHandle),
            };

            let __co3_sync_errors = #id_store_sync;
            let mut __co3_sync_errors_iter = core::iter::IntoIterator::into_iter(__co3_sync_errors);
            if core::iter::Iterator::any(&mut __co3_sync_errors_iter, core::convert::identity) {
                return Err(co3::FfiReturn::TrapRepresentation);
            }

            __co3_dispatch_result
        }};

        let sig = gen_extern_fn_signature(Some(generics), item.sig);
        emit_extern_definition(abi, &item.attrs, sig, fn_body)
    });

    quote! {
        #(#definitions)*
    }
}

fn gen_dispatch_arms(
    generics: &syn::Generics,
    sig: &syn::Signature,
    self_ty: &syn::Type,
    trait_: Option<&syn::Path>,
    drop_impl: bool,
    args: &Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]>,
) -> impl Iterator<Item = TokenStream> {
    let derase_handle_stmts = gen_derase_handle_stmts(generics, self_ty, sig);

    let dispatch_id_params = sig
        .inputs
        .iter()
        .filter_map(|input| {
            let syn::FnArg::Typed(syn::PatType { ty, .. }) = input else {
                return None;
            };

            extract_dispatch_id(generics, self_ty, ty).map(|p| p.ident.clone())
        })
        .collect::<Vec<_>>();

    args.iter().map(move |entry| {
        let mut monomorphizer = DispatchMonomorphizer::new(generics, entry);

        let mut arm_sig = sig.clone();
        let fn_name = &arm_sig.ident;
        let callee = if drop_impl {
            quote!(|__co3_self: &mut #self_ty| unsafe { core::ptr::drop_in_place(__co3_self as *mut _) })
        } else if let Some(trait_) = trait_ {
            quote!(<#self_ty as #trait_>::#fn_name)
        } else {
            quote!(<#self_ty>::#fn_name)
        };

        let mut patterns = dispatch_id_params
            .iter()
            .map(|ident| parse_quote!(<#ident as co3::handle::Handle>::ID))
            .collect::<Vec<syn::Expr>>();

        arm_sig.inputs = arm_sig
            .inputs
            .into_iter()
            .filter(|input| !is_dispatch_id_arg(generics, self_ty, input))
            .collect();

        let arm_body = gen_definition_body(arm_sig, callee);
        let mut arm_body: syn::Block = parse_quote! {{
            (|| -> Result<(), co3::FfiReturn> {
                #(#derase_handle_stmts)*
                #arm_body
            })()
        }};

        patterns.iter_mut().for_each(|pat| monomorphizer.visit_expr_mut(pat));
        monomorphizer.visit_block_mut(&mut arm_body);
        quote! { (#(#patterns,)*) => { #arm_body }}
    })
}

pub(crate) fn is_dispatch_id_arg(
    generics: &syn::Generics,
    self_ty: &syn::Type,
    input: &syn::FnArg,
) -> bool {
    let syn::FnArg::Typed(arg) = input else {
        return false;
    };

    extract_dispatch_id(generics, self_ty, &arg.ty).is_some()
}

fn gen_derase_handle_stmts(
    generics: &syn::Generics,
    self_ty: &syn::Type,
    sig: &syn::Signature,
) -> Vec<TokenStream> {
    let mut stmts = Vec::new();

    let handles = sig
        .inputs
        .iter()
        .filter(|input| !is_dispatch_id_arg(generics, self_ty, input));

    for input in handles {
        let (attrs, arg_name, arg_ty) = match input {
            syn::FnArg::Receiver(receiver) => (&receiver.attrs, quote!(__co3_self), &*receiver.ty),
            syn::FnArg::Typed(syn::PatType { attrs, pat, ty, .. }) => (attrs, quote!(#pat), &**ty),
        };

        let Some(erased_ty) = erase_handle(generics, self_ty, arg_ty) else {
            continue;
        };

        let (ty, erased_ty) = if ownership_mode_for_arg(attrs, arg_ty) == OwnershipMode::Borrow {
            (
                quote! { <#arg_ty as co3::borrow::Borrow>::Borrowed<'_> },
                quote! { <#erased_ty as co3::borrow::Borrow>::Borrowed<'_> },
            )
        } else {
            (quote! { #arg_ty }, quote! { #erased_ty })
        };

        stmts.push(quote! {
            // FIXME: THIS IS EXTREMELY DANGEROUS!!! but I don't have time atm
            // Replace transmute with proper conversion or define an unsafe trait
            let #arg_name = unsafe { core::mem::transmute::<
                <#erased_ty as co3::ExternC>::CType,
                <#ty as co3::ExternC>::CType
            >(#arg_name) };
        });
    }

    let syn::ReturnType::Type(_, output_ty) = &sig.output else {
        return stmts;
    };

    let output_ty = unwrap_result_type(output_ty)
        .map(|(ok, _)| ok)
        .unwrap_or(output_ty);

    let Some(_) = erase_handle(generics, self_ty, output_ty) else {
        return stmts;
    };

    stmts.push(quote! {
        // FIXME: this is not as extremly dangerous but still very dubious
        let __co3_out_ptr = __co3_out_ptr.cast::<<#output_ty as co3::out_ptr::OutPtr>::OutPtr>();
    });

    stmts
}

pub(crate) fn extract_dispatch_id<'a>(
    generics: &'a syn::Generics,
    self_ty: &syn::Type,
    ty: &'a syn::Type,
) -> Option<&'a syn::TypeParam> {
    let syn::Type::Path(syn::TypePath { path, qself }) = ty else {
        return None;
    };
    if path.segments.len() > 2 || qself.is_some() {
        return None;
    }
    if path.segments.last()?.ident != "ID" {
        return None;
    }

    let first_seg = &path.segments.first()?.ident;
    if let Some(x) = generics.type_params().find(|p| p.ident == *first_seg) {
        return Some(x);
    }
    let self_ty = self_ty_ident(generics, self_ty)?;
    generics.type_params().find(|p| p.ident == *self_ty)
}

pub(crate) fn erase_handle_types(
    generics: &syn::Generics,
    self_ty: &syn::Type,
    sig: &mut syn::Signature,
    args: &Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]>,
) {
    let id_types = sig
        .inputs
        .iter()
        .enumerate()
        .filter_map(|(idx, input)| {
            let syn::FnArg::Typed(syn::PatType { ty, .. }) = input else {
                return None;
            };

            extract_dispatch_id(generics, self_ty, ty).and_then(|param| {
                param
                    .attrs
                    .iter()
                    .find(|attr| is_type_erased(attr))
                    .and_then(|attr| dyn_dispatch_repr(attr).ok())
                    .map(|repr| (idx, repr))
            })
        })
        .collect::<Vec<_>>();

    DynHandleEraser::new(generics, self_ty).visit_signature_mut(sig);
    for (idx, lowered_ty) in id_types {
        let syn::FnArg::Typed(syn::PatType { ty, .. }) = &mut sig.inputs[idx] else {
            continue;
        };

        **ty = lowered_ty;
    }

    if let Some(entry) = args.first() {
        // FIXME: This monomorphizes everything, not just handles and handle ids, is that ok?
        // Likely that it isn't
        let mut monomorphizer = DispatchMonomorphizer::new(generics, entry);
        monomorphizer.visit_signature_mut(sig);
    }
}

pub(crate) fn gen_handle_erase_stmts(
    generics: &syn::Generics,
    self_ty: &syn::Type,
    sig: &syn::Signature,
) -> Vec<TokenStream> {
    let mut stmts = Vec::new();

    let handles = sig
        .inputs
        .iter()
        .filter(|input| !is_dispatch_id_arg(generics, self_ty, input));

    for input in handles {
        let (attrs, arg_name, ty) = match input {
            syn::FnArg::Receiver(receiver) => (&receiver.attrs, quote!(__co3_self), &*receiver.ty),
            syn::FnArg::Typed(syn::PatType { attrs, pat, ty, .. }) => (attrs, quote!(#pat), &**ty),
        };

        let Some(erased_ty) = erase_handle(generics, self_ty, ty) else {
            continue;
        };

        let (ty, erased_ty) = if ownership_mode_for_arg(attrs, ty) == OwnershipMode::Borrow {
            (
                quote! { <#ty as co3::borrow::Borrow>::Borrowed<'_> },
                quote! { <#erased_ty as co3::borrow::Borrow>::Borrowed<'_> },
            )
        } else {
            (quote! { #ty }, quote! { #erased_ty })
        };

        stmts.push(quote! {
            // FIXME: As commented elsewhere this is EXTREMELY DANGEROUS!!!
            let #arg_name = unsafe { core::mem::transmute::<
                <#ty as co3::ExternC>::CType,
                <#erased_ty as co3::ExternC>::CType
            >(#arg_name) };
        });
    }

    let syn::ReturnType::Type(_, output_ty) = &sig.output else {
        return stmts;
    };

    let output_ty = unwrap_result_type(output_ty)
        .map(|(ok, _)| ok)
        .unwrap_or(output_ty);

    let Some(erased_output_ty) = erase_handle(generics, self_ty, output_ty) else {
        return stmts;
    };

    stmts.push(quote! {
        // FIXME: this is not as extremly dangerous but still very dubious
        let __co3_out_ptr = __co3_out_ptr.cast::<<#erased_output_ty as co3::out_ptr::OutPtr>::OutPtr>();
    });

    stmts
}

struct DynHandleEraser {
    erased_params: BTreeSet<syn::Ident>,
    was_erased: bool,
}

impl DynHandleEraser {
    fn new(generics: &syn::Generics, self_ty: &syn::Type) -> Self {
        let mut erased_params = generics
            .type_params()
            .filter_map(|p| {
                if p.attrs.iter().any(is_type_erased) {
                    return Some(p.ident.clone());
                }

                None
            })
            .collect::<BTreeSet<_>>();

        if self_ty_ident(generics, self_ty).is_some() {
            erased_params.insert(format_ident!("Self"));
        }

        Self {
            was_erased: false,
            erased_params,
        }
    }
}

fn erase_handle(
    generics: &syn::Generics,
    self_ty: &syn::Type,
    ty: &syn::Type,
) -> Option<syn::Type> {
    let mut eraser = DynHandleEraser::new(generics, self_ty);

    let mut new_ty = ty.clone();
    eraser.visit_type_mut(&mut new_ty);
    eraser.was_erased.then_some(new_ty)
}

impl VisitMut for DynHandleEraser {
    fn visit_type_mut(&mut self, node: &mut syn::Type) {
        syn::visit_mut::visit_type_mut(self, node);

        let syn::Type::Path(syn::TypePath { path, qself }) = node else {
            return;
        };
        if qself.is_some() {
            return;
        }
        let Some(syn::PathSegment { ident, .. }) = path.segments.first() else {
            return;
        };

        if self.erased_params.contains(ident) {
            *node = parse_quote!(co3::handle::Erased<#ident>);
            self.was_erased = true;
        }
    }
}

pub(crate) fn parse_handle_id_attr(attrs: &mut Vec<syn::Attribute>) -> Result<Option<syn::Type>> {
    let mut kept = Vec::with_capacity(attrs.len());

    let mut id_ty = None;
    for attr in attrs.drain(..) {
        if !attr.path().is_ident("id") {
            kept.push(attr);
            continue;
        }

        let syn::Meta::List(list) = &attr.meta else {
            return Err(syn::Error::new_spanned(attr, "expected `#[id(repr)]`"));
        };

        let ty = list
            .parse_args::<syn::Type>()
            .map_err(|_| syn::Error::new_spanned(&attr, "expected `#[id(repr)]`"))?;

        if id_ty.replace(ty).is_some() {
            return Err(syn::Error::new_spanned(attr, "duplicate `#[id(...)]`"));
        }
    }

    *attrs = kept;
    Ok(id_ty)
}

fn self_ty_ident<'a>(generics: &'a syn::Generics, self_ty: &syn::Type) -> Option<&'a syn::Ident> {
    let syn::Type::Path(syn::TypePath { path, qself: None }) = self_ty else {
        return None;
    };

    let ident = path.get_ident()?;
    generics
        .type_params()
        .find_map(|p| (p.ident == *ident).then_some(&p.ident))
}
