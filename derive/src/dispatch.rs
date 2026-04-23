use std::collections::BTreeSet;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    GenericArgument, GenericParam, Result, parse::Parser, parse_quote, punctuated::Punctuated,
    visit::Visit, visit_mut::VisitMut,
};

use crate::{
    DynImpl,
    ffi_fn::{
        emit_extern_definition, gen_definition_body, gen_extern_fn_signature,
        gen_signature_input_conversion_stmts, gen_signature_input_init_stmts,
        gen_signature_store_sync_stmts, merge_generics, normalize_fn_signature,
        ownership_mode_for_arg,
    },
    generate::OwnershipMode,
    utils::{DispatchMonomorphizer, is_drop_impl, is_type_erased, unwrap_result_type},
};

#[derive(Clone, Copy)]
enum RetypeDirection {
    Erase,
    Derase,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum HandleId<'a> {
    DynType(&'a syn::Ident),
    DynSelf,
}

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

pub(crate) fn gen_dispatch_export(
    abi: &syn::Abi,
    dispatch: DynImpl,
    self_id: Option<&syn::Type>,
) -> TokenStream {
    let DynImpl { impl_, args } = dispatch;

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
        merge_generics(impl_.generics.clone(), &mut item.sig.generics);

        normalize_fn_signature(&mut item.sig, Some(self_ty));
        synthesize_dispatch_handle_ids(self_id, &mut item.sig);
        monomorphize_predicates(&mut item.sig.generics, &args);

        let (id_arg_names, handle_ids): (Vec<_>, Vec<_>) = item
            .sig
            .inputs
            .iter()
            .filter_map(|input| {
                let syn::FnArg::Typed(syn::PatType { pat, ty, .. }) = input else {
                    return None;
                };

                let handle_id = match handle_id(ty)? {
                    HandleId::DynSelf => self_id.cloned(),
                    HandleId::DynType(ident) => generics
                        .type_params()
                        .find(|param| param.ident == *ident)?
                        .attrs
                        .iter()
                        .find(|attr| is_type_erased(attr))
                        .and_then(|attr| attr.parse_args().ok()),
                };

                Some((pat, parse_quote!(#pat: #handle_id)))
            })
            .unzip();

        let dispatch_arms = gen_dispatch_arms(
            generics, trait_, self_id, self_ty, &item.sig, drop_impl, &args,
        )
        .collect::<Vec<_>>();

        let id_initialization_stmts = gen_signature_input_init_stmts(&handle_ids);
        let id_input_conversions = gen_signature_input_conversion_stmts(&handle_ids);
        let id_store_sync = gen_signature_store_sync_stmts(handle_ids.len());

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

        erase_handle_types(generics, self_id, self_ty, &mut item.sig, &args);
        let sig = gen_extern_fn_signature(item.sig);
        emit_extern_definition(abi, &item.attrs, sig, fn_body)
    });

    quote! { #(#definitions)* }
}

fn synthesize_dispatch_handle_ids(self_id: Option<&syn::Type>, sig: &mut syn::Signature) {
    let mut synthesized = Punctuated::<syn::FnArg, syn::Token![,]>::new();

    let erased_params = sig
        .generics
        .type_params()
        .filter(|p| p.attrs.iter().any(is_type_erased))
        .map(|p| &p.ident)
        .collect::<BTreeSet<_>>();

    if erased_params.is_empty() && self_id.is_some() {
        synthesized.push(parse_quote!(__co3_self_id: <dyn Self>::ID));
    }

    for ident in erased_params {
        let pat = format_ident!("{ident}_id");
        synthesized.push(parse_quote!(#pat: <dyn #ident>::ID));
    }

    synthesized.extend(core::mem::take(&mut sig.inputs));
    sig.inputs = synthesized;
}

fn gen_dispatch_arms(
    generics: &syn::Generics,
    trait_: Option<&syn::Path>,
    self_id: Option<&syn::Type>,
    self_ty: &syn::Type,
    sig: &syn::Signature,
    drop_impl: bool,
    args: &Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]>,
) -> impl Iterator<Item = TokenStream> {
    let derase_handle_stmts = gen_handle_derase_stmts(generics, self_id, self_ty, sig);

    let dispatch_id_params = sig
        .inputs
        .iter()
        .filter_map(|input| {
            let syn::FnArg::Typed(syn::PatType { ty, .. }) = input else {
                return None;
            };

            handle_id(ty)
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
            .map(|handle_id| {
                let handle_ty = match handle_id {
                    HandleId::DynSelf => quote!(#self_ty),
                    HandleId::DynType(ident) => quote!(#ident),
                };

                parse_quote! { <#handle_ty as co3::handle::Handle>::ID }
            })
            .collect::<Vec<syn::Expr>>();

        arm_sig.inputs = arm_sig
            .inputs
            .into_iter()
            .filter(|input| !is_handle_id_arg(input))
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

pub(crate) fn erase_handle_types(
    generics: &syn::Generics,
    self_id: Option<&syn::Type>,
    self_ty: &syn::Type,
    sig: &mut syn::Signature,
    args: &Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]>,
) {
    let mut eraser = DynHandleEraser::new(generics, self_id, self_ty);

    let handle_ids = sig
        .inputs
        .iter()
        .enumerate()
        .filter_map(|(idx, input)| {
            let syn::FnArg::Typed(syn::PatType { ty, .. }) = input else {
                return None;
            };

            let handle_ty = match handle_id(ty)? {
                HandleId::DynSelf => self_id.cloned()?,
                HandleId::DynType(ident) => generics
                    .type_params()
                    .find(|p| p.ident == *ident)?
                    .attrs
                    .iter()
                    .find(|attr| is_type_erased(attr))
                    .and_then(|attr| attr.parse_args().ok())?,
            };

            Some((idx, handle_ty))
        })
        .collect::<Vec<_>>();

    for input in &mut sig.inputs {
        match input {
            syn::FnArg::Receiver(receiver) => eraser.visit_type_mut(&mut receiver.ty),
            syn::FnArg::Typed(syn::PatType { ty, .. }) => eraser.visit_type_mut(ty),
        }
    }

    if let syn::ReturnType::Type(_, ty) = &mut sig.output {
        eraser.visit_type_mut(ty);
    }

    for (idx, lowered_ty) in handle_ids {
        let syn::FnArg::Typed(syn::PatType { ty, .. }) = &mut sig.inputs[idx] else {
            continue;
        };

        **ty = lowered_ty;
    }

    if let Some(entry) = args.first() {
        DispatchMonomorphizer::new(generics, entry).visit_signature_mut(sig);
    }
}

pub(crate) fn gen_handle_erase_stmts(
    generics: &syn::Generics,
    self_id: Option<&syn::Type>,
    self_ty: &syn::Type,
    sig: &syn::Signature,
) -> Vec<TokenStream> {
    let mut sig = sig.clone();

    // TODO: I don't like to clone sig and normalize
    normalize_fn_signature(&mut sig, Some(self_ty));
    gen_handle_retype_stmts(RetypeDirection::Erase, generics, self_id, self_ty, &sig)
}

fn gen_handle_derase_stmts(
    generics: &syn::Generics,
    self_id: Option<&syn::Type>,
    self_ty: &syn::Type,
    sig: &syn::Signature,
) -> Vec<TokenStream> {
    gen_handle_retype_stmts(RetypeDirection::Derase, generics, self_id, self_ty, sig)
}

fn gen_handle_retype_stmts(
    direction: RetypeDirection,
    generics: &syn::Generics,
    self_id: Option<&syn::Type>,
    self_ty: &syn::Type,
    sig: &syn::Signature,
) -> Vec<TokenStream> {
    let handles = sig.inputs.iter().filter(|input| !is_handle_id_arg(input));

    let mut stmts = vec![];
    for input in handles {
        let (attrs, arg_name, ty) = match input {
            syn::FnArg::Receiver(receiver) => (&receiver.attrs, quote!(__co3_self), &*receiver.ty),
            syn::FnArg::Typed(syn::PatType { attrs, pat, ty, .. }) => (attrs, quote!(#pat), &**ty),
        };

        let Some(erased_ty) = erase_handle(generics, self_id, self_ty, ty) else {
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
        let (src_ty, dst_ty) = match direction {
            RetypeDirection::Erase => (
                quote! { <#ty as co3::ExternC>::CType },
                quote! { <#erased_ty as co3::ExternC>::CType },
            ),
            RetypeDirection::Derase => (
                quote! { <#erased_ty as co3::ExternC>::CType },
                quote! { <#ty as co3::ExternC>::CType },
            ),
        };

        stmts.push(quote! {
            // FIXME: THIS IS EXTREMELY DANGEROUS!!! but I don't have time atm
            // Replace transmute with proper conversion or define an unsafe trait
            let #arg_name = unsafe { core::mem::transmute::<
                #src_ty,
                #dst_ty
            >(#arg_name) };
        });
    }

    let syn::ReturnType::Type(_, output_ty) = &sig.output else {
        return stmts;
    };

    let output_ty = unwrap_result_type(output_ty)
        .map(|(ok, _)| ok)
        .unwrap_or(output_ty);

    let Some(erased_output_ty) = erase_handle(generics, self_id, self_ty, output_ty) else {
        return stmts;
    };

    let out_ptr_ty = match direction {
        RetypeDirection::Erase => quote! { <#erased_output_ty as co3::out_ptr::OutPtr>::OutPtr },
        RetypeDirection::Derase => quote! { <#output_ty as co3::out_ptr::OutPtr>::OutPtr },
    };

    stmts.push(quote! {
        // FIXME: this is not as extremly dangerous but still very dubious
        let __co3_out_ptr = __co3_out_ptr.cast::<#out_ptr_ty>();
    });

    stmts
}

struct DynHandleEraser {
    erased: BTreeSet<syn::Ident>,
    was_erased: bool,
}

impl DynHandleEraser {
    fn new(generics: &syn::Generics, self_id: Option<&syn::Type>, self_ty: &syn::Type) -> Self {
        let mut erased: BTreeSet<syn::Ident> = generics
            .type_params()
            .filter_map(|p| {
                if p.attrs.iter().any(is_type_erased) {
                    return Some(p.ident.clone());
                }

                None
            })
            .collect();

        if erased.is_empty()
            && self_id.is_some()
            && let syn::Type::Path(syn::TypePath { qself: None, path }) = self_ty
            && let Some(self_ty) = path.segments.first()
        {
            erased.insert(self_ty.ident.clone());
        }

        Self {
            was_erased: false,
            erased,
        }
    }
}

impl VisitMut for DynHandleEraser {
    fn visit_type_mut(&mut self, node: &mut syn::Type) {
        syn::visit_mut::visit_type_mut(self, node);

        let syn::Type::Path(syn::TypePath { qself: None, path }) = node else {
            return;
        };
        let Some(syn::PathSegment { ident, .. }) = path.segments.first() else {
            return;
        };

        if self.erased.contains(ident) {
            self.was_erased = true;

            *node = parse_quote! {
                co3::handle::Erased<#node>
            };
        }
    }
}

fn erase_handle(
    generics: &syn::Generics,
    self_id: Option<&syn::Type>,
    self_ty: &syn::Type,
    ty: &syn::Type,
) -> Option<syn::Type> {
    let mut eraser = DynHandleEraser::new(generics, self_id, self_ty);

    let mut new_ty = ty.clone();
    eraser.visit_type_mut(&mut new_ty);
    eraser.was_erased.then_some(new_ty)
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

pub(crate) fn handle_id(ty: &syn::Type) -> Option<HandleId<'_>> {
    let syn::Type::Path(syn::TypePath {
        qself:
            Some(syn::QSelf {
                ty,
                position: 0,
                as_token: None,
                ..
            }),
        path,
    }) = ty
    else {
        return None;
    };
    if path.segments.len() != 1 {
        return None;
    }

    let first_seg = path.segments.first()?;
    if first_seg.ident != "ID" || !first_seg.arguments.is_none() {
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
    if arg_ty.modifier != syn::TraitBoundModifier::None || arg_ty.lifetimes.is_some() {
        return None;
    }
    if arg_ty.path.segments.len() > 1 {
        return None;
    }
    if let Some(arg_path) = arg_ty.path.get_ident()
        && arg_path == "Self"
    {
        return Some(HandleId::DynSelf);
    }

    Some(HandleId::DynType(&arg_ty.path.segments.first()?.ident))
}

pub(crate) fn is_handle_id_arg(input: &syn::FnArg) -> bool {
    let syn::FnArg::Typed(arg) = input else {
        return false;
    };

    handle_id(&arg.ty).is_some()
}

fn monomorphize_predicates(
    generics: &mut syn::Generics,
    args: &Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]>,
) {
    struct ParamUseDetector<'a> {
        params: BTreeSet<&'a syn::Ident>,
        found: bool,
    }

    impl<'a> ParamUseDetector<'a> {
        fn new(params: impl IntoIterator<Item = &'a syn::GenericParam>) -> Self {
            Self {
                params: params
                    .into_iter()
                    .filter_map(|param| {
                        if let syn::GenericParam::Type(param) = param {
                            return Some(&param.ident);
                        }

                        None
                    })
                    .collect(),
                found: false,
            }
        }
    }

    impl Visit<'_> for ParamUseDetector<'_> {
        fn visit_path(&mut self, node: &syn::Path) {
            if node.leading_colon.is_none()
                && let Some(first) = node.segments.first()
                && self.params.contains(&first.ident)
            {
                self.found = true;
                return;
            }

            syn::visit::visit_path(self, node);
        }
    }

    let mut predicates = Punctuated::<_, syn::Token![,]>::new();
    let (erased_params, params): (Vec<_>, _) = core::mem::take(&mut generics.params)
        .into_iter()
        .partition(|param| matches!(param, syn::GenericParam::Type(_)));

    generics.params = params.into_iter().collect();
    if let Some(where_clause) = &mut generics.where_clause {
        let mut detector = ParamUseDetector::new(&erased_params);

        for predicate in core::mem::take(&mut where_clause.predicates) {
            detector.visit_where_predicate(&predicate);

            if !detector.found {
                predicates.push(predicate);
                continue;
            }

            for entry in args {
                let mut monomorphizer = DispatchMonomorphizer::new(generics, entry);
                let mut predicate = predicate.clone();
                monomorphizer.visit_where_predicate_mut(&mut predicate);
                predicates.push(predicate);
            }
        }
    }

    generics.make_where_clause().predicates = predicates;
}
