use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{
    Ident, Meta, Path, Type,
    parse::{Parse, ParseStream},
    parse_quote,
    punctuated::Punctuated,
    spanned::Spanned,
    token::Comma,
    visit::{Visit, visit_type},
    visit_mut::VisitMut,
};

use crate::{
    dispatch::{StaticLifetimeNormalizer, set_token_stream_span, tag_id},
    generate::OwnershipMode,
    parse::FailureMode,
    symbol_name_value,
    utils::{cfg_attrs, co3_path, is_drop_impl, soft_for_arg},
};

fn export_definition_attrs(attrs: &[syn::Attribute]) -> TokenStream {
    let attrs = attrs.iter().filter_map(|attr| {
        if is_by_val_attr(attr) {
            return None;
        }
        let Some(value) = symbol_name_value(attr) else {
            return Some(quote!(#attr));
        };

        Some(quote!(#[unsafe(export_name = #value)]))
    });

    quote!(#(#attrs)*)
}

pub(crate) fn is_by_val_attr(attr: &syn::Attribute) -> bool {
    attr.path().is_ident("by_val")
}

pub(crate) fn fn_return_ty(sig: &syn::Signature) -> Option<&syn::Type> {
    match &sig.output {
        syn::ReturnType::Default => None,
        syn::ReturnType::Type(_, ty) => Some(ty),
    }
}

pub(crate) fn gen_failure_panic(err: TokenStream) -> TokenStream {
    quote! {
        panic!(
            "co3 generated FFI failure with error type `{}`",
            core::any::type_name_of_val(&#err),
        )
    }
}

pub(crate) fn gen_trap_value() -> TokenStream {
    quote! { co3::Error::trap_value() }
}

pub(crate) fn gen_soft_sync_error_value() -> TokenStream {
    quote! { co3::Error::soft_sync_error() }
}

pub(crate) fn gen_decode_error(failure_mode: FailureMode) -> TokenStream {
    match failure_mode {
        FailureMode::Panic => quote! { panic!("co3 generated FFI decode failure"); },
        FailureMode::Error => {
            let error = gen_trap_value();
            quote! { return Err(#error); }
        }
    }
}

pub(crate) fn gen_sync_error(failure_mode: FailureMode) -> TokenStream {
    match failure_mode {
        FailureMode::Panic => quote! { panic!("co3 generated FFI sync failure"); },
        FailureMode::Error => {
            let error = gen_soft_sync_error_value();
            quote! { return Err(#error); }
        }
    }
}

pub(crate) fn gen_sync_check(
    store_sync_stmts: TokenStream,
    sync_error: TokenStream,
) -> TokenStream {
    quote! {
        let __co3_sync_errors = #store_sync_stmts;
        let mut __co3_sync_errors_iter = core::iter::IntoIterator::into_iter(__co3_sync_errors);
        if core::iter::Iterator::any(&mut __co3_sync_errors_iter, core::convert::identity) {
            #sync_error
        }
    }
}

pub(crate) fn gen_unknown_tag_error(failure_mode: FailureMode) -> TokenStream {
    match failure_mode {
        FailureMode::Panic => quote! { panic!("co3 generated FFI unknown tag") },
        FailureMode::Error => quote! { Err(co3::Error::unknown_tag()) },
    }
}

pub(crate) fn emit_extern_definition(
    abi: &syn::Abi,
    attrs: &[syn::Attribute],
    failure_mode: FailureMode,
    fn_signature: TokenStream,
    ffi_fn_body: TokenStream,
) -> TokenStream {
    let attrs = export_definition_attrs(attrs);
    let signature: syn::Signature =
        syn::parse2(fn_signature.clone()).expect("generated FFI signature must parse");
    let abi_assertions = gen_abi_assertions(&signature);

    let error_handler = match failure_mode {
        FailureMode::Panic => gen_failure_panic(quote!(err)),
        FailureMode::Error => quote! { co3::encode(err) },
    };

    quote! {
        #attrs
        unsafe #abi #fn_signature {
            #abi_assertions
            let fn_body = || #ffi_fn_body;

            match fn_body() {
                Ok(value) => value,
                Err(err) => #error_handler,
            }
        }
    }
}

pub(crate) fn gen_abi_assertions(sig: &syn::Signature) -> TokenStream {
    let arguments = sig.inputs.iter().map(|input| {
        let (attrs, mut ty) = match input {
            syn::FnArg::Typed(arg) => (&arg.attrs, arg.ty.as_ref().clone()),
            syn::FnArg::Receiver(receiver) => {
                (&receiver.attrs, crate::utils::receiver_ty(receiver))
            }
        };
        StaticLifetimeNormalizer.visit_type_mut(&mut ty);
        let cfg = cfg_attrs(attrs);
        quote! {
            #(#cfg)*
            const {
                assert!(co3::impls!(#ty: co3::CFnArg), "co3 FFI argument must implement CFnArg");
            };
        }
    });
    let mut return_ty: syn::Type = fn_return_ty(sig)
        .cloned()
        .unwrap_or_else(|| parse_quote!(()));
    StaticLifetimeNormalizer.visit_type_mut(&mut return_ty);
    quote! {
        #(#arguments)*
        const {
            assert!(co3::impls!(#return_ty: co3::CFnReturn), "co3 FFI return must implement CFnReturn");
        };
    }
}

pub(crate) fn gen_definition_body(
    sig: syn::Signature,
    callee: TokenStream,
    fn_by_val: bool,
    failure_mode: FailureMode,
) -> TokenStream {
    gen_definition_body_with_output(sig, callee, fn_by_val, failure_mode, false)
}

pub(crate) fn gen_raw_definition_body(
    sig: syn::Signature,
    callee: TokenStream,
    fn_by_val: bool,
    failure_mode: FailureMode,
) -> TokenStream {
    gen_definition_body_with_output(sig, callee, fn_by_val, failure_mode, !fn_by_val)
}

pub(crate) fn gen_abi_return_encode(value: TokenStream, borrow_output: bool) -> TokenStream {
    if borrow_output {
        quote!(co3::borrow::borrow_cast(co3::encode(#value)))
    } else {
        quote!(co3::encode(#value))
    }
}

fn gen_definition_body_with_output(
    sig: syn::Signature,
    callee: TokenStream,
    fn_by_val: bool,
    failure_mode: FailureMode,
    borrow_output: bool,
) -> TokenStream {
    let inputs = &sig.inputs;
    let return_ty = fn_return_ty(&sig);

    let decode_input_stmts = gen_input_decode_stmts(inputs, failure_mode);
    let store_sync_stmts = gen_store_sync_stmts(inputs.len());
    let sync_error = gen_sync_error(failure_mode);
    let sync_check = gen_sync_check(store_sync_stmts, sync_error);

    let arg_names = inputs.iter().map(|arg| {
        let (attrs, name) = match arg {
            syn::FnArg::Typed(arg) => (&arg.attrs, item_fn_input_ident(&arg.pat).clone()),
            syn::FnArg::Receiver(arg) => (&arg.attrs, format_ident!("__co3_self")),
        };
        let cfg = cfg_attrs(attrs);
        quote!(#(#cfg)* #name)
    });

    let return_borrow_check = if let Some(return_ty) = &return_ty {
        gen_return_borrow_check(return_ty, fn_by_val)
    } else {
        quote! {}
    };
    let encoded_output = gen_abi_return_encode(quote!(__co3_output), borrow_output);
    let output = match failure_mode {
        FailureMode::Panic => quote! { Ok::<_, ()>(#encoded_output) },
        FailureMode::Error => match return_ty {
            Some(return_ty) => quote! { Ok::<_, #return_ty>(#encoded_output) },
            None => quote! { Ok(#encoded_output) },
        },
    };

    quote! {{
        #return_borrow_check

        #decode_input_stmts
        let __co3_output = (#callee)(
            #(#arg_names),*
        );

        #sync_check

        #output
    }}
}

pub(crate) fn gen_drop_definition_body(
    sig: &syn::Signature,
    failure_mode: FailureMode,
) -> TokenStream {
    debug_assert!(sig.inputs.iter().any(|input| matches!(
        input,
        syn::FnArg::Typed(arg) if item_fn_input_ident(&arg.pat) == "__co3_self"
    )));

    let decode_error = gen_decode_error(failure_mode);
    let output = match failure_mode {
        FailureMode::Panic => quote! { Ok::<_, ()>(co3::encode(())) },
        FailureMode::Error => quote! { Ok(co3::encode(())) },
    };

    quote! {{
        if __co3_self.is_null() {
            #decode_error
        }

        unsafe {
            core::mem::drop(co3::boxed::Box::from_raw(__co3_self));
        }

        #output
    }}
}

pub(crate) fn gen_return_borrow_check(return_ty: &syn::Type, fn_by_val: bool) -> TokenStream {
    if fn_by_val {
        return quote! {};
    }

    quote! {
        const {
            assert!(
                co3::impls!(#return_ty: co3::borrow::Borrow<Owner: co3::stored::EmptyStore>),
                "Mark the return type with `move` to transfer ownership",
            );
        }
    }
}

pub(crate) fn item_fn_input_ident(input: &syn::Pat) -> &Ident {
    let syn::Pat::Ident(syn::PatIdent { ident, .. }) = input else {
        unreachable!()
    };

    ident
}

pub(crate) fn is_unpack_attr(attr: &syn::Attribute) -> bool {
    attr.path().is_ident("unpack")
}

pub(crate) fn unpack_attr_name(_attr: &syn::Attribute) -> &'static str {
    "#[unpack]"
}

pub(crate) fn is_unpack_arg(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        if !is_unpack_attr(attr) {
            return false;
        }
        let Meta::List(_) = &attr.meta else {
            return true;
        };
        attr.parse_args_with(Punctuated::<UnpackPart, Comma>::parse_terminated)
            .is_ok_and(|parts| parts.len() != 1)
    })
}

#[derive(Clone)]
pub(crate) struct UnpackPart {
    pub(crate) logical: Type,
    pub(crate) abi: Option<Type>,
}

impl Parse for UnpackPart {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let logical = input.parse()?;
        let abi = if input.peek(syn::Token![=>]) {
            input.parse::<syn::Token![=>]>()?;
            let abi: Type = input.parse()?;
            if matches!(abi, Type::Infer(_)) {
                return Err(syn::Error::new_spanned(
                    abi,
                    "the ABI type after `=>` cannot be `_`",
                ));
            }
            Some(abi)
        } else {
            None
        };
        Ok(Self { logical, abi })
    }
}

pub(crate) fn unpack_parts(
    attrs: &[syn::Attribute],
) -> syn::Result<Option<(UnpackPart, UnpackPart)>> {
    let Some(attr) = attrs.iter().find(|attr| is_unpack_attr(attr)) else {
        return Ok(None);
    };
    let syntax_err = || format!("{} expects exactly two parts", unpack_attr_name(attr));
    let parts = match &attr.meta {
        Meta::List(_) => attr.parse_args_with(Punctuated::<UnpackPart, Comma>::parse_terminated)?,
        Meta::NameValue(_) | Meta::Path(_) => {
            return Err(syn::Error::new_spanned(attr, syntax_err()));
        }
    };
    if parts.len() != 2 {
        return Err(syn::Error::new_spanned(attr, syntax_err()));
    }
    Ok(Some((parts[0].clone(), parts[1].clone())))
}

pub(crate) fn validate_single_unpack(attrs: &[syn::Attribute]) -> syn::Result<bool> {
    let Some(attr) = attrs.iter().find(|attr| is_unpack_attr(attr)) else {
        return Ok(false);
    };
    let Meta::List(_) = &attr.meta else {
        return Ok(false);
    };
    let parts = attr.parse_args_with(Punctuated::<UnpackPart, Comma>::parse_terminated)?;
    if parts.len() != 1 {
        return Ok(false);
    }
    let part = &parts[0];
    if part.abi.is_some() {
        return Err(syn::Error::new_spanned(
            attr,
            format!(
                "the one-part {} form does not support ABI erasure",
                unpack_attr_name(attr)
            ),
        ));
    }
    Ok(true)
}

pub(crate) fn single_unpack_part(
    attrs: &[syn::Attribute],
    arg_ty: &Type,
) -> syn::Result<Option<Type>> {
    if !validate_single_unpack(attrs)? {
        return Ok(None);
    }
    let attr = attrs.iter().find(|attr| is_unpack_attr(attr)).unwrap();
    let parts = attr.parse_args_with(Punctuated::<UnpackPart, Comma>::parse_terminated)?;
    let part = &parts[0];
    Ok(Some(if matches!(part.logical, Type::Infer(_)) {
        arg_ty.clone()
    } else {
        part.logical.clone()
    }))
}

pub(crate) fn is_single_unpack_arg(attrs: &[syn::Attribute]) -> bool {
    validate_single_unpack(attrs).unwrap_or(false)
}

pub(crate) fn unpack_types(attrs: &[syn::Attribute]) -> syn::Result<Option<(Type, Type)>> {
    Ok(unpack_parts(attrs)?.map(|(part1, part2)| (part1.logical, part2.logical)))
}

/// Returns the C representation of the value being split.
pub(crate) fn unpack_abi_parts(
    attrs: &[syn::Attribute],
    arg_ty: &Type,
) -> syn::Result<(Type, Type)> {
    let (part1, part2) = unpack_parts(attrs)?.expect("unpack attribute was validated");
    Ok((
        abi_unpack_part(arg_ty, part1, 1)?,
        abi_unpack_part(arg_ty, part2, 2)?,
    ))
}

pub(crate) fn unpack_logical_parts(
    attrs: &[syn::Attribute],
    arg_ty: &Type,
) -> syn::Result<(Type, Type)> {
    let (part1, part2) = unpack_parts(attrs)?.expect("unpack attribute was validated");
    Ok((
        logical_unpack_part(arg_ty, &part1.logical, 1)?,
        logical_unpack_part(arg_ty, &part2.logical, 2)?,
    ))
}

fn abi_unpack_part(arg_ty: &Type, part: UnpackPart, position: u8) -> syn::Result<Type> {
    match part.abi {
        Some(abi) => Ok(parse_quote!(<#abi as co3::ExternC>::CType)),
        None => logical_unpack_part(arg_ty, &part.logical, position),
    }
}

fn logical_unpack_part(arg_ty: &Type, part: &Type, position: u8) -> syn::Result<Type> {
    if matches!(part, Type::Infer(_)) {
        inferred_unpack_part(arg_ty, position)
    } else {
        Ok(parse_quote!(<#part as co3::ExternC>::CType))
    }
}

fn inferred_unpack_part(arg_ty: &Type, part: u8) -> syn::Result<Type> {
    let arg_ty = peel_grouped_type(arg_ty);

    if let Some(inner_ty) = option_inner_type(arg_ty) {
        return inferred_non_option_unpack_part(inner_ty, part);
    }

    inferred_non_option_unpack_part(arg_ty, part)
}

fn inferred_non_option_unpack_part(arg_ty: &Type, part: u8) -> syn::Result<Type> {
    match arg_ty {
        Type::Paren(paren) => return inferred_non_option_unpack_part(&paren.elem, part),
        Type::Group(group) => return inferred_non_option_unpack_part(&group.elem, part),
        _ => {}
    }

    if let Some((part1, part2)) = tuple_parts(arg_ty) {
        return Ok(if part == 1 {
            parse_quote!(<#part1 as co3::ExternC>::CType)
        } else {
            parse_quote!(<#part2 as co3::ExternC>::CType)
        });
    }

    if let Some(wide_ty) = boxed_wide_type(arg_ty) {
        return Ok(if part == 1 {
            parse_quote!(co3::boxed::CBox<<<#wide_ty as co3::wide::Wide>::Data as co3::ExternC>::CType>)
        } else {
            parse_quote!(<#wide_ty as co3::wide::Wide>::Metadata)
        });
    }

    if let Type::Reference(reference) = arg_ty {
        let wide_ty = &reference.elem;
        return Ok(if part == 1 {
            if reference.mutability.is_some() {
                parse_quote!(*mut <<#wide_ty as co3::wide::Wide>::Data as co3::ExternC>::CType)
            } else {
                parse_quote!(*const <<#wide_ty as co3::wide::Wide>::Data as co3::ExternC>::CType)
            }
        } else {
            parse_quote!(<#wide_ty as co3::wide::Wide>::Metadata)
        });
    }

    let position = if part == 1 { "first" } else { "second" };
    Err(syn::Error::new_spanned(
        arg_ty,
        format!(
            "the {position} `_` unpack argument is only supported for two-element tuples, references to `Wide` types, and `Box`es of `Wide` types"
        ),
    ))
}

pub(crate) fn inferred_unpack_option_inner<'a>(
    attrs: &[syn::Attribute],
    arg_ty: &'a Type,
) -> syn::Result<Option<&'a Type>> {
    if validate_single_unpack(attrs)? {
        return Ok(None);
    }
    let Some((part1, part2)) = unpack_parts(attrs)? else {
        return Ok(None);
    };
    if !matches!(part1.logical, Type::Infer(_)) && !matches!(part2.logical, Type::Infer(_)) {
        return Ok(None);
    }

    Ok(option_inner_type(peel_grouped_type(arg_ty)))
}

fn peel_grouped_type(mut ty: &Type) -> &Type {
    loop {
        ty = match ty {
            Type::Paren(paren) => &paren.elem,
            Type::Group(group) => &group.elem,
            _ => return ty,
        };
    }
}

fn option_inner_type(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != "Option" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    if arguments.args.len() != 1 {
        return None;
    }
    match arguments.args.first()? {
        syn::GenericArgument::Type(inner) => Some(inner),
        _ => None,
    }
}

fn boxed_wide_type(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    if segment.ident != "Box" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    let mut types = arguments.args.iter().filter_map(|argument| match argument {
        syn::GenericArgument::Type(ty) => Some(ty),
        _ => None,
    });
    let wide = types.next()?;
    types.next().is_none().then_some(wide)
}

fn tuple_parts(ty: &Type) -> Option<(&Type, &Type)> {
    match ty {
        Type::Tuple(tuple) if tuple.elems.len() == 2 => Some((&tuple.elems[0], &tuple.elems[1])),
        _ => None,
    }
}

fn borrowed_arg_ty(attrs: &[syn::Attribute], arg_ty: &Type) -> TokenStream {
    match ownership_mode_for_arg(attrs, arg_ty) {
        OwnershipMode::ByValue => quote!(#arg_ty),
        OwnershipMode::Borrow => quote! {
            <#arg_ty as co3::borrow::Borrow>::Borrowed<'_>
        },
    }
}

pub(crate) fn unpack_arg_names(arg_name: &Ident) -> (Ident, Ident) {
    (
        format_ident!("__co3_{arg_name}_data"),
        format_ident!("__co3_{arg_name}_metadata"),
    )
}

fn parameter_cfg(attrs: &[syn::Attribute]) -> Option<(TokenStream, TokenStream)> {
    let predicates = attrs
        .iter()
        .filter(|attr| attr.path().is_ident("cfg"))
        .map(|attr| attr.parse_args::<syn::Meta>().expect("valid cfg predicate"))
        .collect::<Vec<_>>();
    (!predicates.is_empty()).then(|| {
        (
            quote!(#[cfg(all(#(#predicates),*))]),
            quote!(#[cfg(not(all(#(#predicates),*)))]),
        )
    })
}

pub(crate) fn gen_input_decode_stmts<'a>(
    inputs: impl IntoIterator<Item = &'a syn::FnArg>,
    failure_mode: FailureMode,
) -> TokenStream {
    let inputs = inputs.into_iter().collect::<Vec<_>>();

    let mut stmts = quote! {};
    let mut value_initializers = Vec::with_capacity(inputs.len());
    let mut store_initializers = Vec::with_capacity(inputs.len());
    let mut bindings = Vec::with_capacity(inputs.len());

    let store_sync_stmts = gen_store_sync_stmts(inputs.len());
    let decode_error = gen_decode_error(failure_mode);

    let mut value_present = Vec::new();
    for (idx, input) in inputs.iter().enumerate() {
        let idx = syn::Index::from(idx);

        let (attrs, arg_name, arg_ty) = match input {
            syn::FnArg::Receiver(receiver) => (
                &receiver.attrs,
                format_ident!("__co3_self"),
                crate::utils::receiver_ty(receiver),
            ),
            syn::FnArg::Typed(arg) => {
                let arg_name = item_fn_input_ident(&arg.pat);
                let arg_ty = (*arg.ty).clone();
                (&arg.attrs, arg_name.clone(), arg_ty)
            }
        };

        let cfg = parameter_cfg(attrs);
        let enabled = cfg.as_ref().map(|(enabled, _)| enabled);
        let disabled = cfg.as_ref().map(|(_, disabled)| disabled);
        // Keep an inactive slot so every generated tuple index remains stable.
        let inactive_value = disabled.map(|disabled| quote!(#disabled Some(()),));
        let inactive_store = disabled.map(|disabled| quote!(#disabled (),));

        let decode_ty = borrowed_arg_ty(attrs, &arg_ty);
        let decode_arg = quote! { #arg_name };

        let decode_call = if soft_for_arg(attrs) {
            quote! { co3::soft_decode(#decode_arg, &mut __co3_input_stores.#idx) }
        } else {
            quote! { co3::decode(#decode_arg) }
        };

        let from_borrow = match ownership_mode_for_arg(attrs, &arg_ty) {
            OwnershipMode::ByValue => quote!(#arg_name),
            OwnershipMode::Borrow => quote! {
                #arg_name.map(co3::borrow::FromBorrow::from_borrow)
            },
        };

        stmts.extend(quote! {
            #enabled
            let #arg_name: Option<#decode_ty> = unsafe { #decode_call };
            #enabled
            let #arg_name: Option<#arg_ty> = #from_borrow;

            #enabled
            if let Some(#arg_name) = #arg_name {
                __co3_input_values.#idx = Some(#arg_name);
            }
        });

        value_initializers.push(quote!(#enabled <Option<#arg_ty> as core::default::Default>::default(), #inactive_value));
        let store_ty = if soft_for_arg(attrs) {
            quote!(<#decode_ty as co3::stored::DecodeOwned<'_>>::Store)
        } else {
            quote!(())
        };
        store_initializers.push(
            quote!(#enabled <#store_ty as core::default::Default>::default(), #inactive_store),
        );

        bindings.push(quote! {
            #enabled
            let Some(#arg_name) = __co3_input_values.#idx else {
                let __co3_sync_errors = #store_sync_stmts;
                #decode_error
            };
        });

        value_present.push(quote! { __co3_input_values.#idx.is_some() });
    }

    quote! {
        let mut __co3_input_values = (#(#value_initializers)*);
        let mut __co3_input_stores = (#(#store_initializers)*);

        #stmts

        let __co3_input_present: [bool; _] = [
            #(#value_present),*
        ];

        #(#bindings)*
    }
}

pub(crate) fn gen_fn_signature_drift_check(
    mut sig: syn::Signature,
    mut callee: syn::Expr,
) -> TokenStream {
    StaticLifetimeNormalizer.visit_signature_mut(&mut sig);
    StaticLifetimeNormalizer.visit_expr_mut(&mut callee);

    let syn::Signature {
        safety,
        abi,
        output,
        inputs,
        ..
    } = &sig;

    let arg_tys = inputs.iter().map(|input| match input {
        syn::FnArg::Receiver(receiver) => {
            let ty = crate::utils::receiver_ty(receiver);
            let cfg = cfg_attrs(&receiver.attrs);
            quote!(#(#cfg)* #ty)
        }
        syn::FnArg::Typed(syn::PatType { attrs, ty, .. }) => {
            let cfg = cfg_attrs(attrs);
            quote!(#(#cfg)* #ty)
        }
    });

    let fn_ty = quote! { #safety #abi fn(#(#arg_tys),*) #output };
    let callee = set_token_stream_span(quote!(#callee), sig.span());

    // NOTE: Avoids signature drift
    quote! { let __co3_fn: #fn_ty = #callee; }
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

fn gen_fn_definition_body(
    item: &syn::ItemFn,
    failure_mode: FailureMode,
    callee: syn::Expr,
) -> TokenStream {
    let fn_by_val = item.attrs.iter().any(is_by_val_attr);
    let signature_check = gen_fn_signature_drift_check(item.sig.clone(), callee.clone());
    let body = gen_definition_body(item.sig.clone(), quote!(#callee), fn_by_val, failure_mode);

    quote! {{
        #signature_check
        #body
    }}
}

pub fn gen_impl_definition(
    abi: &syn::Abi,
    failure_mode: FailureMode,
    impl_: syn::ItemImpl,
) -> TokenStream {
    let trait_ = impl_.trait_.as_ref().map(|(path, _)| path);
    let drop_impl = is_drop_impl(&impl_);
    let impl_attrs = &impl_.attrs;

    let self_ty = &impl_.self_ty;

    let definitions = impl_.items.into_iter().filter_map(|item| {
        let syn::ImplItem::Fn(mut item) = item else {
            return None;
        };

        let has_receiver = item
            .sig
            .inputs
            .iter()
            .any(|input| matches!(input, syn::FnArg::Receiver(_)));
        normalize_fn_signature(&mut item.sig, Some(self_ty));

        let fn_name = &item.sig.ident;
        let callee = impl_method_callee(self_ty, trait_, fn_name);

        merge_impl_generics_for_raw_decl(
            impl_.generics.clone(),
            has_receiver,
            &mut item.sig.generics,
        );
        let check_callee = callee.clone();

        let fn_by_val = item.attrs.iter().any(is_by_val_attr);
        let fn_signature = gen_extern_fn_signature(item.sig.clone(), failure_mode);
        let signature_drift_check =
            // NOTE: `Drop::drop` has a fixed signature enforced by `validate_drop_impl`
            (!drop_impl).then(|| gen_fn_signature_drift_check(item.sig.clone(), check_callee));
        let body = if drop_impl {
            gen_drop_definition_body(&item.sig, failure_mode)
        } else {
            gen_definition_body(item.sig, quote!(#callee), fn_by_val, failure_mode)
        };

        let ffi_fn_body = quote! {{
            #signature_drift_check
            #body
        }};

        Some(emit_extern_definition(
            abi,
            &item.attrs,
            failure_mode,
            fn_signature,
            ffi_fn_body,
        ))
    });

    quote! {
        #(#impl_attrs)*
        const _: () = {
            #(#definitions)*
        };
    }
}

pub(crate) fn impl_method_callee(
    self_ty: &Type,
    trait_: Option<&Path>,
    fn_name: &syn::Ident,
) -> syn::Expr {
    qualified_method_callee(self_ty, trait_, fn_name)
}

pub(crate) fn merge_impl_generics_for_raw_decl(
    mut impl_generics: syn::Generics,
    has_receiver: bool,
    fn_generics: &mut syn::Generics,
) {
    if !has_receiver {
        impl_generics.params = impl_generics
            .params
            .into_iter()
            .filter(|param| matches!(param, syn::GenericParam::Lifetime(_)))
            .collect();
        impl_generics.where_clause = None;
    }

    merge_generics(impl_generics, fn_generics);
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

pub(crate) fn strip_dispatch_params(generics: &mut syn::Generics) {
    generics.params = core::mem::take(&mut generics.params)
        .into_iter()
        .filter(|param| matches!(param, syn::GenericParam::Lifetime(_)))
        .collect();
}

pub fn gen_fn_definition(
    abi: &syn::Abi,
    failure_mode: FailureMode,
    mut item: syn::ItemFn,
    callee: syn::Expr,
) -> TokenStream {
    normalize_fn_signature(&mut item.sig, None);

    let ffi_fn_body = gen_fn_definition_body(&item, failure_mode, callee);
    let fn_signature = gen_extern_fn_signature(item.sig, failure_mode);

    emit_extern_definition(abi, &item.attrs, failure_mode, fn_signature, ffi_fn_body)
}

pub(crate) fn gen_extern_fn_signature(
    sig: syn::Signature,
    failure_mode: FailureMode,
) -> TokenStream {
    let sig = lower_extern_fn_signature(sig, failure_mode);
    quote! { #sig }
}

pub(crate) fn lower_extern_fn_signature(
    mut sig: syn::Signature,
    failure_mode: FailureMode,
) -> syn::Signature {
    if let syn::ReturnType::Type(_, return_type) = &sig.output
        && matches!(failure_mode, FailureMode::Error)
    {
        let co3 = co3_path();
        sig.generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#return_type: #co3::Error));
    }

    explicitize_signature_lifetimes(&mut sig);
    // FIXME: This feels like a HACK
    synthesize_lifetime_bounds(&mut sig);

    lower_signature_inputs(&mut sig);
    lower_signature_output(&mut sig);

    sig.constness = None;
    sig.asyncness = None;
    sig.safety = syn::Safety::Default;
    sig.abi = None;

    sig
}

pub(crate) fn lower_raw_fn_signature(
    sig: syn::Signature,
    failure_mode: FailureMode,
    move_fn: bool,
) -> syn::Signature {
    let mut sig = lower_extern_fn_signature(sig, failure_mode);
    if !move_fn && let syn::ReturnType::Type(_, return_ty) = &mut sig.output {
        let c_type = return_ty.as_ref();
        **return_ty = parse_quote!(<#c_type as co3::borrow::BorrowCast>::AsConst);
    }
    sig
}

fn lower_signature_inputs(sig: &mut syn::Signature) {
    sig.inputs = core::mem::take(&mut sig.inputs)
        .into_iter()
        .flat_map(lower_signature_input)
        .collect();
}

fn lower_signature_input(input: syn::FnArg) -> Vec<syn::FnArg> {
    let (pat, attrs, arg_ty) = match input {
        syn::FnArg::Receiver(receiver) => {
            let arg_ty = crate::utils::receiver_ty(&receiver);
            (parse_quote!(__co3_self), receiver.attrs, arg_ty)
        }
        syn::FnArg::Typed(arg) => {
            let syn::PatType { attrs, pat, ty, .. } = arg;
            (parse_quote!(#pat), attrs, *ty)
        }
    };

    let cfg = cfg_attrs(&attrs).collect::<Vec<_>>();

    if is_unpack_arg(&attrs) {
        let arg_name = item_fn_input_ident(&pat);
        let (data_name, metadata_name) = unpack_arg_names(arg_name);
        let (source_part1_ty, source_part2_ty) =
            unpack_abi_parts(&attrs, &arg_ty).expect("validated #[unpack] attribute");
        let (part1_ty, part2_ty) = (source_part1_ty, source_part2_ty);

        return vec![
            parse_quote!(#(#cfg)* #data_name: #part1_ty),
            parse_quote!(#(#cfg)* #metadata_name: #part2_ty),
        ];
    }

    let ffi_ty = item_fn_input_arg_type(&attrs, &arg_ty);

    vec![parse_quote!(#(#cfg)* #pat: #ffi_ty)]
}

fn lower_signature_output(sig: &mut syn::Signature) {
    let return_type = core::mem::replace(&mut sig.output, syn::ReturnType::Default);

    let return_type = match return_type {
        syn::ReturnType::Default => return,
        syn::ReturnType::Type(_, return_type) => *return_type,
    };

    let lowered_return_type = item_fn_output_type(&return_type);
    sig.output = parse_quote!(-> #lowered_return_type);
}

pub(crate) fn explicitize_signature_lifetimes(sig: &mut syn::Signature) {
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

        fn push_lifetime_param(&mut self, lifetime: syn::Lifetime) {
            if self.generics.lt_token.is_none() {
                self.generics.lt_token = Some(Default::default());
                self.generics.gt_token = Some(Default::default());
            }

            self.generics
                .params
                .push(syn::GenericParam::Lifetime(syn::LifetimeParam::new(
                    lifetime,
                )));
        }

        fn explicitize_lifetime(&mut self, lifetime: &mut syn::Lifetime) -> syn::Lifetime {
            if lifetime.ident != "_" {
                return lifetime.clone();
            }

            let new_lifetime = format!("'__co3_{}", self.next_idx);
            let new_lifetime = syn::Lifetime::new(&new_lifetime, Span::call_site());

            self.push_lifetime_param(new_lifetime.clone());
            self.next_idx += 1;
            *lifetime = new_lifetime.clone();
            new_lifetime
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
            if let syn::ReceiverKind::Reference(_, lifetime, _) = &mut node.kind {
                let l = self.explicitize_lifetime(
                    lifetime.get_or_insert_with(|| syn::Lifetime::new("'_", Span::call_site())),
                );
                self.record_lifetime::<true>(l);
            } else if let syn::ReceiverKind::Typed(_, ty) = &mut node.kind {
                self.visit_type_mut(ty);
            }
        }

        fn visit_type_reference_mut(&mut self, node: &mut syn::TypeReference) {
            let l = self.explicitize_lifetime(
                node.lifetime
                    .get_or_insert_with(|| syn::Lifetime::new("'_", Span::call_site())),
            );

            self.record_lifetime::<false>(l);
            self.visit_type_mut(&mut node.elem);
        }

        fn visit_lifetime_mut(&mut self, node: &mut syn::Lifetime) {
            let l = self.explicitize_lifetime(node);

            self.record_lifetime::<false>(l);
        }

        fn visit_type_fn_ptr_mut(&mut self, _: &mut syn::TypeFnPtr) {}
    }

    struct OutputLifetimeExplicator<'a> {
        lifetime: &'a syn::Lifetime,
    }

    impl VisitMut for OutputLifetimeExplicator<'_> {
        fn visit_type_reference_mut(&mut self, node: &mut syn::TypeReference) {
            if node.lifetime.is_none() || matches!(&node.lifetime, Some(l) if l.ident == "_") {
                node.lifetime = Some(self.lifetime.clone());
            }

            self.visit_type_mut(&mut node.elem);
        }

        fn visit_lifetime_mut(&mut self, node: &mut syn::Lifetime) {
            if node.ident == "_" {
                *node = self.lifetime.clone();
            }
        }

        fn visit_type_fn_ptr_mut(&mut self, _: &mut syn::TypeFnPtr) {
            // Bare function pointer elision is scoped to the function pointer type.
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
        implied_type_bounds: Vec<(Type, syn::Lifetime)>,
        parent_lifetimes: Vec<&'a syn::Lifetime>,
    }

    impl<'a> syn::visit::Visit<'a> for LifetimeUseCollector<'a> {
        fn visit_named_arg(&mut self, _: &'a syn::NamedArg) {}

        fn visit_type_reference(&mut self, node: &'a syn::TypeReference) {
            if let Some(lifetime) = &node.lifetime {
                self.implied_type_bounds
                    .push(((*node.elem).clone(), lifetime.clone()));
                let parents = &self.parent_lifetimes;
                self.bounds.entry(lifetime).or_default().extend(parents);

                self.parent_lifetimes.push(lifetime);
                visit_type(self, &node.elem);
                self.parent_lifetimes.pop();
            } else {
                visit_type(self, &node.elem);
            }
        }

        fn visit_type_path(&mut self, node: &'a syn::TypePath) {
            if node.qself.is_some() {
                return;
            }

            syn::visit::visit_type_path(self, node);
        }

        fn visit_lifetime(&mut self, node: &'a syn::Lifetime) {
            let parents = &self.parent_lifetimes;
            self.bounds.entry(node).or_default().extend(parents);
        }
    }

    let mut lifetime_collector = LifetimeUseCollector::default();
    for input in &sig.inputs {
        match input {
            syn::FnArg::Receiver(_) => {}
            // Unpacked arguments are replaced by their two ABI parts below. Bounds implied by
            // the source type therefore do not belong to the raw declaration and, for dispatch
            // parameters, can refer to a generic that has already been erased from it.
            syn::FnArg::Typed(arg) if !is_unpack_arg(&arg.attrs) => {
                lifetime_collector.visit_type(&arg.ty)
            }
            syn::FnArg::Typed(_) => {}
        }
    }
    if let syn::ReturnType::Type(_, ty) = &sig.output {
        lifetime_collector.visit_type(ty);
    }

    for (ty, lifetime) in lifetime_collector.implied_type_bounds {
        sig.generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#ty: #lifetime));
    }

    let bounds = lifetime_collector
        .bounds
        .into_iter()
        .map(|(k, v)| (k.clone(), v.into_iter().cloned().collect::<Vec<_>>()))
        .collect::<Vec<_>>();

    for (lhs, rhs) in bounds {
        if rhs.is_empty() {
            continue;
        }

        sig.generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#lhs: #(#rhs)+*));
    }
}

pub(crate) fn item_fn_input_arg_type(attrs: &[syn::Attribute], arg_ty: &Type) -> TokenStream {
    let c_type = quote! { <#arg_ty as co3::ExternC>::CType };

    match ownership_mode_for_arg(attrs, arg_ty) {
        OwnershipMode::ByValue => quote! { #c_type },
        OwnershipMode::Borrow => quote! {
            <#c_type as co3::borrow::BorrowCast>::AsConst
        },
    }
}

pub(crate) fn item_fn_output_type(return_ty: &Type) -> Type {
    parse_quote!(<#return_ty as co3::ExternC>::CType)
}

pub(crate) fn normalize_fn_signature(sig: &mut syn::Signature, self_ty: Option<&Type>) {
    for input in &mut sig.inputs {
        if let syn::FnArg::Receiver(receiver) = input {
            let attrs = &receiver.attrs;
            let ty = crate::utils::receiver_ty(receiver);
            *input = parse_quote! { #(#attrs)* __co3_self: #ty }
        }
    }

    if let Some(self_ty) = self_ty {
        SelfConcretizer { self_ty }.visit_signature_mut(sig);
        TraitObjectReferenceParenthesizer.visit_signature_mut(sig);
    }
}

pub(crate) fn ownership_mode_for_arg(attrs: &[syn::Attribute], ty: &Type) -> OwnershipMode {
    if attrs.iter().any(is_by_val_attr) || is_implicitly_by_value(ty) {
        return OwnershipMode::ByValue;
    }

    OwnershipMode::Borrow
}

fn is_implicitly_by_value(ty: &Type) -> bool {
    let ty = peel_grouped_type(ty);

    if tag_id(ty).is_some() {
        return true;
    }

    match ty {
        Type::Reference(_) | Type::Ptr(_) | Type::FnPtr(_) => true,
        Type::Path(path) => {
            is_copy_primitive_path(path)
                || option_inner_type(ty).is_some_and(is_implicitly_by_value)
        }
        Type::Tuple(tuple) => {
            !tuple.elems.is_empty() && tuple.elems.iter().all(is_implicitly_by_value)
        }
        _ => false,
    }
}

fn is_copy_primitive_path(path: &syn::TypePath) -> bool {
    if path.qself.is_some() {
        return false;
    }

    path.path.segments.last().is_some_and(|segment| {
        segment.arguments.is_empty()
            && matches!(
                segment.ident.to_string().as_str(),
                "bool"
                    | "char"
                    | "u8"
                    | "u16"
                    | "u32"
                    | "u64"
                    | "u128"
                    | "usize"
                    | "i8"
                    | "i16"
                    | "i32"
                    | "i64"
                    | "i128"
                    | "isize"
                    | "f32"
                    | "f64"
            )
    })
}

pub(crate) struct SelfConcretizer<'a> {
    pub(crate) self_ty: &'a Type,
}

pub(crate) fn qualified_method_callee(
    self_ty: &Type,
    trait_: Option<&Path>,
    fn_name: &syn::Ident,
) -> syn::Expr {
    let mut callee = if let Some(trait_) = trait_ {
        parse_quote!(<() as #trait_>::#fn_name)
    } else {
        parse_quote!(<()>::#fn_name)
    };
    let syn::Expr::Path(path) = &mut callee else {
        unreachable!();
    };
    *path.qself.as_mut().unwrap().ty = self_ty.clone();
    callee
}

struct TraitObjectReferenceParenthesizer;

impl VisitMut for TraitObjectReferenceParenthesizer {
    fn visit_type_reference_mut(&mut self, reference: &mut syn::TypeReference) {
        syn::visit_mut::visit_type_reference_mut(self, reference);
        if matches!(reference.elem.as_ref(), Type::TraitObject(_)) {
            let elem = &reference.elem;
            *reference.elem = parse_quote!((#elem));
        }
    }
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
        if tag_id(node).is_some() {
            return;
        }

        syn::visit_mut::visit_type_mut(self, node);
        let Type::Path(syn::TypePath {
            qself: None, path, ..
        }) = node
        else {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn explicitized(mut sig: syn::Signature) -> syn::Signature {
        explicitize_signature_lifetimes(&mut sig);
        sig
    }

    fn with_synthesized_lifetime_bounds(mut sig: syn::Signature) -> syn::Signature {
        synthesize_lifetime_bounds(&mut sig);
        sig
    }

    #[test]
    fn explicitizes_single_elided_reference_input_and_output() {
        let sig = explicitized(parse_quote!(fn f(x: &u8) -> &u8));
        let expected: syn::Signature =
            parse_quote!(fn f<'__co3_0>(x: &'__co3_0 u8) -> &'__co3_0 u8);

        assert_eq!(sig, expected);
    }

    #[test]
    fn leaves_output_elided_for_multiple_input_lifetime_positions() {
        let sig = explicitized(parse_quote!(fn f(x: &u8, y: &u8) -> &u8));
        let expected: syn::Signature =
            parse_quote!(fn f<'__co3_0, '__co3_1>(x: &'__co3_0 u8, y: &'__co3_1 u8) -> &u8);

        assert_eq!(sig, expected);
    }

    #[test]
    fn assigns_receiver_lifetime_to_output_when_other_inputs_have_lifetimes() {
        let sig = explicitized(parse_quote!(fn f(&self, x: &u8) -> &u8));
        let expected: syn::Signature = parse_quote!(
            fn f<'__co3_0, '__co3_1>(&'__co3_0 self, x: &'__co3_1 u8) -> &'__co3_0 u8
        );

        assert_eq!(sig, expected);
    }

    #[test]
    fn counts_generic_lifetime_arguments_as_input_lifetime_positions() {
        let sig = explicitized(parse_quote!(fn f(x: Foo<'_>) -> Foo<'_>));
        let expected: syn::Signature =
            parse_quote!(fn f<'__co3_0>(x: Foo<'__co3_0>) -> Foo<'__co3_0>);

        assert_eq!(sig, expected);
    }

    #[test]
    fn nested_references_are_distinct_input_lifetime_positions() {
        let sig = explicitized(parse_quote!(fn f(x: &&u8) -> &u8));
        let expected: syn::Signature =
            parse_quote!(fn f<'__co3_0, '__co3_1>(x: &'__co3_0 &'__co3_1 u8) -> &u8);

        assert_eq!(sig, expected);
    }

    #[test]
    fn ignores_lifetimes_scoped_to_bare_function_pointer_inputs() {
        let sig = explicitized(parse_quote!(fn f(x: fn(&u8)) -> &u8));
        let expected: syn::Signature = parse_quote!(fn f(x: fn(&u8)) -> &u8);

        assert_eq!(sig, expected);
    }

    #[test]
    fn does_not_rewrite_bare_function_pointer_output_lifetimes() {
        let sig = explicitized(parse_quote!(fn f(x: &u8) -> fn(&u8) -> &u8));
        let expected: syn::Signature =
            parse_quote!(fn f<'__co3_0>(x: &'__co3_0 u8) -> fn(&u8) -> &u8);

        assert_eq!(sig, expected);
    }

    #[test]
    fn does_not_synthesize_bounds_from_bare_function_pointer_args() {
        let sig = with_synthesized_lifetime_bounds(parse_quote!(
            fn f<'a, 'b>(x: fn(&'a &'b u8))
        ));
        let expected: syn::Signature = parse_quote!(
            fn f<'a, 'b>(x: fn(&'a &'b u8))
        );

        assert_eq!(sig, expected);
    }

    #[test]
    fn preserves_implied_reference_pointee_bound() {
        let sig = with_synthesized_lifetime_bounds(parse_quote!(
            fn f<'a, T>(x: &'a mut Foo<T>)
        ));
        let expected: syn::Signature = parse_quote!(
            fn f<'a, T>(x: &'a mut Foo<T>) where Foo<T>: 'a
        );

        assert_eq!(sig, expected);
    }

    #[test]
    fn does_not_synthesize_bounds_from_projections() {
        let sig = with_synthesized_lifetime_bounds(parse_quote!(
            fn f<'a, 'b>(
                x: <&'a <Foo as ExternC>::CType<'b> as BorrowCast>::AsConst
            )
        ));
        let expected: syn::Signature = parse_quote!(
            fn f<'a, 'b>(
                x: <&'a <Foo as ExternC>::CType<'b> as BorrowCast>::AsConst
            )
        );

        assert_eq!(sig, expected);
    }

    #[test]
    fn selects_implicit_by_value_argument_types_during_generation() {
        let by_value: &[Type] = &[
            parse_quote!(<dyn Trait>::TAG),
            parse_quote!(&u8),
            parse_quote!(&mut str),
            parse_quote!(*const u8),
            parse_quote!(unsafe extern "C" fn(u8) -> u16),
            parse_quote!(u32),
            parse_quote!(core::primitive::char),
            parse_quote!(Option<&u8>),
            parse_quote!(Option<Option<&u8>>),
            parse_quote!((<dyn Trait>::TAG, Option<&u8>, (u32, fn()))),
            parse_quote!((&u8)),
        ];

        for ty in by_value {
            assert_eq!(
                ownership_mode_for_arg(&[], ty),
                OwnershipMode::ByValue,
                "expected `{}` to use by-value conversion",
                quote!(#ty),
            );
        }
    }

    #[test]
    fn leaves_other_argument_types_borrowed_by_default() {
        let borrowed: &[Type] = &[
            parse_quote!(String),
            parse_quote!(Option<String>),
            parse_quote!((&u8, String)),
            parse_quote!([u8; 4]),
            parse_quote!(()),
            parse_quote!(str),
        ];

        for ty in borrowed {
            assert_eq!(
                ownership_mode_for_arg(&[], ty),
                OwnershipMode::Borrow,
                "expected `{}` to use borrowed conversion",
                quote!(#ty),
            );
        }

        let attrs = [parse_quote!(#[by_val])];
        assert_eq!(
            ownership_mode_for_arg(&attrs, &parse_quote!(String)),
            OwnershipMode::ByValue,
        );
    }
}
