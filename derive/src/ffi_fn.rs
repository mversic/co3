use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{
    Ident, Path, Type, parse_quote,
    spanned::Spanned,
    visit::{Visit, visit_type},
    visit_mut::VisitMut,
};

use crate::{
    argument::{self, Argument, Unpack, UnpackSyntax},
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
    mut signature: syn::Signature,
    ffi_fn_body: TokenStream,
    borrow_output: bool,
) -> TokenStream {
    let attrs = export_definition_attrs(attrs);
    let abi_assertions = gen_abi_assertions(&signature);
    signature.ident = raw_definition_name(&signature.ident);
    signature.safety = syn::Safety::Unsafe(Default::default());
    signature.abi = Some(abi.clone());

    emit_abi_function(
        failure_mode,
        attrs,
        quote!(),
        &signature,
        abi_assertions,
        ffi_fn_body,
        borrow_output,
    )
}

pub(crate) fn raw_definition_name(name: &Ident) -> Ident {
    format_ident!("{name}_raw")
}

pub(crate) fn emit_abi_function(
    failure_mode: FailureMode,
    attrs: TokenStream,
    vis: TokenStream,
    signature: &syn::Signature,
    setup: TokenStream,
    body: TokenStream,
    borrow_output: bool,
) -> TokenStream {
    let error_handler = match failure_mode {
        FailureMode::Panic => gen_failure_panic(quote!(err)),
        FailureMode::Error if fn_return_ty(signature).is_none_or(is_identity_primitive) => {
            quote!(err)
        }
        FailureMode::Error => gen_abi_return_encode(quote!(err), borrow_output),
    };

    quote! {
        #attrs
        #vis #signature {
            #setup
            let __co3_abi_body = || #body;

            match __co3_abi_body() {
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
        StaticLifetimeNormalizer::default().visit_type_mut(&mut ty);
        let cfg = cfg_attrs(attrs).collect::<Vec<_>>();
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
    StaticLifetimeNormalizer::default().visit_type_mut(&mut return_ty);
    quote! {
        #(#arguments)*
        const {
            assert!(co3::impls!((#return_ty): co3::CFnReturn), "co3 FFI return must implement CFnReturn");
        };
    }
}

pub(crate) fn gen_definition_body(
    sig: syn::Signature,
    callee: TokenStream,
    fn_by_val: bool,
    failure_mode: FailureMode,
) -> TokenStream {
    let inputs = argument::analyze(&sig.inputs);
    let return_ty = fn_return_ty(&sig);

    let pack_input_stmts = gen_pack_input_stmts(&inputs, failure_mode);
    let decode_input_stmts = gen_analyzed_input_decode_stmts(&inputs, failure_mode);
    let store_sync_stmts = gen_store_sync_stmts(inputs.len());
    let sync_error = gen_sync_error(failure_mode);
    let sync_check = gen_sync_check(store_sync_stmts, sync_error);

    let arg_names = inputs.iter().map(|input| {
        let name = input.name();
        let cfg = input.cfg();
        quote!(#(#cfg)* #name)
    });

    let return_borrow_check = if let Some(return_ty) = &return_ty {
        gen_return_borrow_check(return_ty, fn_by_val)
    } else {
        quote! {}
    };
    let encoded_output = if return_ty.is_none_or(is_identity_primitive) {
        quote!(__co3_output)
    } else {
        gen_abi_return_encode(quote!(__co3_output), borrows_return(&sig, fn_by_val))
    };
    let output = match failure_mode {
        FailureMode::Panic => quote! { Ok::<_, ()>(#encoded_output) },
        FailureMode::Error => match return_ty {
            Some(return_ty) => quote! { Ok::<_, #return_ty>(#encoded_output) },
            None => quote! { Ok(#encoded_output) },
        },
    };

    quote! {{
        #return_borrow_check

        #pack_input_stmts
        #decode_input_stmts
        let __co3_output = (#callee)(
            #(#arg_names),*
        );

        let __co3_encoded_output = #output;
        #sync_check
        __co3_encoded_output
    }}
}

fn gen_pack_input_stmts(inputs: &[Argument<'_>], failure_mode: FailureMode) -> TokenStream {
    let decode_error = gen_decode_error(failure_mode);
    let stmts = inputs.iter().filter_map(|input| {
        let name = input.name();
        let decode_ty = input.unpack_ty();
        let cfg = input.cfg().collect::<Vec<_>>();
        let conversion = match &input.unpack {
            Unpack::None => return None,
            Unpack::Single(part) => {
                let source_ty = input.conversion_ty();
                let packed = quote!(<#source_ty as co3::pack::Pack<<#part as co3::ReprC>::CType>>::pack(#name));
                if input.is_borrowed() && !input.identity {
                    quote!((#packed).map(co3::marker::Borrowed::< <#source_ty as co3::ReprC>::CType >))
                } else {
                    packed
                }
            },
            Unpack::Pair(pair) => {
                let (data_name, metadata_name) = unpack_arg_names(name);
                let first = &pair.first.logical;
                let second = &pair.second.logical;
                let data = if pair.first.erased {
                    crate::abi_retype::gen_retype(quote!(#data_name), &pair.first.abi, first)
                } else {
                    quote!(#data_name)
                };
                let metadata = if pair.second.erased {
                    crate::abi_retype::gen_retype(quote!(#metadata_name), &pair.second.abi, second)
                } else {
                    quote!(#metadata_name)
                };
                let packed = quote!(unsafe { <#decode_ty as co3::pack::Pack2<#first, #second>>::pack(#data, #metadata) });
                if input.is_borrowed() && !input.identity {
                    quote!((#packed).map(co3::marker::Borrowed))
                } else {
                    packed
                }
            }
        };
        Some(quote! {
            #(#cfg)*
            let #name = match #conversion {
                Ok(value) => value,
                Err(_) => { #decode_error }
            };
        })
    });
    quote!(#(#stmts)*)
}

pub(crate) fn gen_abi_return_encode(value: TokenStream, borrow_output: bool) -> TokenStream {
    if borrow_output {
        quote!(co3::borrow::encode(#value, &mut Default::default()))
    } else {
        quote!(co3::encode(#value))
    }
}

pub(crate) fn gen_opaque_drop_definition_body(
    sig: &syn::Signature,
    failure_mode: FailureMode,
) -> TokenStream {
    let self_ty = sig
        .inputs
        .iter()
        .find_map(|input| match input {
            syn::FnArg::Typed(arg) if item_fn_input_ident(&arg.pat) == "__co3_self" => {
                let Type::Reference(reference) = arg.ty.as_ref() else {
                    unreachable!("`Drop::drop` requires `&mut self`");
                };
                Some(reference.elem.as_ref())
            }
            _ => None,
        })
        .expect("`Drop::drop` requires a receiver");

    let decode_error = gen_decode_error(failure_mode);
    let output = match failure_mode {
        FailureMode::Panic => quote! { Ok::<_, ()>(co3::encode(())) },
        FailureMode::Error => quote! { Ok(co3::encode(())) },
    };

    quote! {{
        let Some(__co3_value) = (unsafe { co3::decode::<co3::boxed::Box<#self_ty>>(__co3_self) }) else {
            #decode_error
        };
        core::mem::drop(__co3_value);

        #output
    }}
}

pub(crate) fn gen_regular_drop_definition_body(
    sig: &syn::Signature,
    failure_mode: FailureMode,
) -> TokenStream {
    let (self_ty, soft) = sig
        .inputs
        .iter()
        .find_map(|input| match input {
            syn::FnArg::Typed(arg) if item_fn_input_ident(&arg.pat) == "__co3_self" => {
                let self_ty = match arg.ty.as_ref() {
                    Type::Reference(reference) => reference.elem.as_ref(),
                    owned => owned,
                };
                Some((self_ty, soft_for_arg(&arg.attrs)))
            }
            _ => None,
        })
        .expect("`Drop::drop` requires a receiver");
    let decode_error = gen_decode_error(failure_mode);
    let output = match failure_mode {
        FailureMode::Panic => quote! { Ok::<_, ()>(co3::encode(())) },
        FailureMode::Error => quote! { Ok(co3::encode(())) },
    };

    if !soft {
        return quote! {{
            let Some(__co3_value) = (unsafe { co3::convert::decode_move::<#self_ty>(__co3_self) }) else {
                #decode_error
            };
            core::mem::drop(__co3_value);
            #output
        }};
    }

    let sync_error = gen_sync_error(failure_mode);
    quote! {{
        let mut __co3_store = <#self_ty as co3::convert::DecodeMove<'_>>::Store::default();
        let __co3_value = unsafe {
            <#self_ty as co3::convert::DecodeMove>::soft_decode(__co3_self, &mut __co3_store)
        };
        let Some(__co3_value) = __co3_value else {
            #decode_error
        };
        core::mem::drop(__co3_value);
        if co3::convert::Store::sync(__co3_store).is_none() {
            #sync_error
        }
        #output
    }}
}

pub(crate) fn drop_owned_receiver(sig: &mut syn::Signature, owned_ty: Option<&Type>) {
    let arg = sig
        .inputs
        .iter_mut()
        .find_map(|input| match input {
            syn::FnArg::Typed(arg) if item_fn_input_ident(&arg.pat) == "__co3_self" => Some(arg),
            _ => None,
        })
        .expect("`Drop::drop` requires a normalized receiver");
    let syn::Type::Reference(reference) = arg.ty.as_ref() else {
        unreachable!("`Drop::drop` requires `&mut self`");
    };
    arg.ty = owned_ty
        .cloned()
        .map(Box::new)
        .unwrap_or_else(|| reference.elem.clone());
    arg.attrs.push(syn::parse_quote!(#[by_val]));
}

pub(crate) fn gen_return_borrow_check(return_ty: &syn::Type, fn_by_val: bool) -> TokenStream {
    if ownership_mode(return_ty, fn_by_val) == OwnershipMode::ByValue {
        return quote! {};
    }

    quote! {
        const {
            assert!(
                co3::impls!((#return_ty): co3::borrow::BorrowedEncode<Store: co3::convert::EmptyStore>)
                    && co3::impls!((#return_ty): co3::borrow::BorrowedEncodeUnchecked<Owner: co3::convert::EmptyStore>),
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

pub(crate) fn logical_unpack_part(arg_ty: &Type, part: &Type, position: u8) -> syn::Result<Type> {
    if matches!(part, Type::Infer(_)) {
        inferred_unpack_part(arg_ty, position)
    } else {
        Ok(parse_quote!(<#part as co3::ReprC>::CType))
    }
}

fn inferred_unpack_part(arg_ty: &Type, part: u8) -> syn::Result<Type> {
    let arg_ty = peel_grouped_type(arg_ty);

    if let Some(inner_ty) = option_inner_type(arg_ty) {
        return inferred_non_option_unpack_part(inner_ty, part);
    }
    if let Some((_, payload)) = inferred_result_type(arg_ty) {
        return inferred_non_option_unpack_part(payload, part);
    }
    if let Some((_, inner)) = transparent_unpack_inner_type(arg_ty) {
        return inferred_unpack_part(inner, part);
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
            parse_quote!(<#part1 as co3::ReprC>::CType)
        } else {
            parse_quote!(<#part2 as co3::ReprC>::CType)
        });
    }

    if let Some(wide_ty) = boxed_or_nonnull_pointee(arg_ty) {
        return Ok(if part == 1 {
            parse_quote!(*mut <<<#wide_ty as co3::wide::Wide>::Header as co3::wide::WideHeader>::Data as co3::ReprC>::CType)
        } else {
            parse_quote!(<#wide_ty as co3::wide::Wide>::Metadata)
        });
    }

    if let Type::Reference(reference) = arg_ty {
        let wide_ty = &reference.elem;
        let interior_mutable = crate::known_type::recognize(peel_grouped_type(wide_ty))
            .is_some_and(|known| {
                matches!(
                    known.kind,
                    crate::known_type::Kind::Cell | crate::known_type::Kind::UnsafeCell
                )
            });
        return Ok(if part == 1 {
            if reference.mutability.is_some() || interior_mutable {
                parse_quote!(*mut <<<#wide_ty as co3::wide::Wide>::Header as co3::wide::WideHeader>::Data as co3::ReprC>::CType)
            } else {
                parse_quote!(*const <<<#wide_ty as co3::wide::Wide>::Header as co3::wide::WideHeader>::Data as co3::ReprC>::CType)
            }
        } else {
            parse_quote!(<#wide_ty as co3::wide::Wide>::Metadata)
        });
    }

    if let Type::Ptr(pointer) = arg_ty {
        let wide_ty = &pointer.elem;
        return Ok(if part == 1 {
            if matches!(pointer.mutability, syn::PointerMutability::Mut(_)) {
                parse_quote!(*mut <<<#wide_ty as co3::wide::Wide>::Header as co3::wide::WideHeader>::Data as co3::ReprC>::CType)
            } else {
                parse_quote!(*const <<<#wide_ty as co3::wide::Wide>::Header as co3::wide::WideHeader>::Data as co3::ReprC>::CType)
            }
        } else {
            parse_quote!(<#wide_ty as co3::wide::Wide>::Metadata)
        });
    }

    let position = if part == 1 { "first" } else { "second" };
    Err(syn::Error::new_spanned(
        arg_ty,
        format!(
            "the {position} `_` unpack argument is only supported for two-element tuples, references or raw pointers to `Wide` types, and `Box` or `NonNull` pointers to `Wide` types"
        ),
    ))
}

pub(crate) fn peel_grouped_type(mut ty: &Type) -> &Type {
    loop {
        ty = match ty {
            Type::Paren(paren) => &paren.elem,
            Type::Group(group) => &group.elem,
            _ => return ty,
        };
    }
}

pub(crate) fn option_inner_type(ty: &Type) -> Option<&Type> {
    let known = crate::known_type::recognize(ty)?;
    (known.kind == crate::known_type::Kind::Option)
        .then_some(known.inner)
        .flatten()
}

pub(crate) fn transparent_unpack_inner_type(ty: &Type) -> Option<(&syn::Ident, &Type)> {
    let Type::Path(path) = ty else { return None };
    let known = crate::known_type::recognize_path(path)?;
    if !known.is_transparent_wrapper() {
        return None;
    }
    Some((&path.path.segments.last()?.ident, known.inner?))
}

pub(crate) fn inferred_result_type(ty: &Type) -> Option<(Type, &Type)> {
    let Type::Path(path) = ty else { return None };
    let known = crate::known_type::recognize_path(path)?;
    if known.kind != crate::known_type::Kind::Result {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &path.path.segments.last()?.arguments else {
        return None;
    };
    let mut args = args.args.iter();
    let (Some(syn::GenericArgument::Type(ok)), Some(syn::GenericArgument::Type(err))) =
        (args.next(), args.next())
    else {
        return None;
    };
    let unit = |ty: &Type| matches!(ty, Type::Tuple(tuple) if tuple.elems.is_empty());
    let payload = match (unit(ok), unit(err)) {
        (false, true) => ok,
        (true, false) => err,
        _ => return None,
    };
    Some((parse_quote!(core::result::Result<#ok, #err>), payload))
}

fn boxed_or_nonnull_pointee(ty: &Type) -> Option<&Type> {
    let known = crate::known_type::recognize(ty)?;
    match known.kind {
        crate::known_type::Kind::Box | crate::known_type::Kind::NonNull => known.inner,
        _ => None,
    }
}

fn tuple_parts(ty: &Type) -> Option<(&Type, &Type)> {
    match ty {
        Type::Tuple(tuple) if tuple.elems.len() == 2 => Some((&tuple.elems[0], &tuple.elems[1])),
        _ => None,
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

pub(crate) fn gen_analyzed_input_decode_stmts(
    inputs: &[Argument<'_>],
    failure_mode: FailureMode,
) -> TokenStream {
    let mut stmts = quote! {};
    let mut value_initializers = Vec::with_capacity(inputs.len());
    let mut store_initializers = Vec::with_capacity(inputs.len());
    let mut bindings = Vec::with_capacity(inputs.len());

    let store_sync_stmts = gen_store_sync_stmts(inputs.len());
    let decode_error = gen_decode_error(failure_mode);

    let mut value_present = Vec::new();
    for (idx, input) in inputs.iter().enumerate() {
        let idx = syn::Index::from(idx);

        let attrs = input.attrs;
        let arg_name = input.name();
        let arg_ty = &input.ty;

        let cfg = parameter_cfg(attrs);
        let enabled = cfg.as_ref().map(|(enabled, _)| enabled);
        let disabled = cfg.as_ref().map(|(_, disabled)| disabled);
        // Keep an inactive slot so every generated tuple index remains stable.
        let inactive_value = disabled.map(|disabled| quote!(#disabled Some(()),));
        let inactive_store = disabled.map(|disabled| quote!(#disabled (),));

        let decode_ty = input.conversion_ty();
        let decode_arg = quote! { #arg_name };

        let decode_call = if input.identity {
            quote!(Some(#decode_arg))
        } else if input.is_borrowed() {
            if input.soft {
                quote!(<#decode_ty as co3::borrow::BorrowedDecode<'_>>::soft_decode(#decode_arg.0, &mut __co3_input_stores.#idx))
            } else {
                quote!(co3::borrow::decode::<#decode_ty>(#decode_arg))
            }
        } else if input.soft {
            quote!(co3::soft_decode(#decode_arg, &mut __co3_input_stores.#idx))
        } else {
            quote!(co3::decode(#decode_arg))
        };
        let from_borrow = quote!(#arg_name);

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
        let store_ty = if input.soft {
            if input.is_borrowed() {
                quote!(<#decode_ty as co3::borrow::BorrowedDecode<'_>>::Store)
            } else {
                quote!(<#decode_ty as co3::convert::DecodeMove<'_>>::Store)
            }
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

pub(crate) fn gen_static_fn_signature_drift_check(
    mut sig: syn::Signature,
    mut callee: syn::Expr,
) -> TokenStream {
    StaticLifetimeNormalizer::default().visit_signature_mut(&mut sig);
    StaticLifetimeNormalizer::default().visit_expr_mut(&mut callee);

    gen_fn_signature_drift_check(sig, callee)
}

pub(crate) fn gen_fn_signature_drift_check(sig: syn::Signature, callee: syn::Expr) -> TokenStream {
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

        if __co3_input_present[#idxs] && co3::convert::Store::sync(__co3_input_stores.#idxs).is_none() {
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
    let signature_check = gen_static_fn_signature_drift_check(item.sig.clone(), callee.clone());
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
    declared_self: bool,
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

        let return_borrow_check =
            if impl_.generics.params.is_empty() && item.sig.generics.params.is_empty() {
                fn_return_ty(&item.sig).map(|ty| {
                    let mut ty = ty.clone();
                    ConcreteSelfType(self_ty).visit_type_mut(&mut ty);
                    let check = gen_return_borrow_check(&ty, item.attrs.iter().any(is_by_val_attr));
                    let cfg = cfg_attrs(&item.attrs);
                    quote! { #(#cfg)* const _: () = { #check }; }
                })
            } else {
                None
            };

        let fn_name = &item.sig.ident;
        let callee = impl_method_callee(self_ty, trait_, fn_name);

        merge_impl_generics_for_raw_decl(
            impl_.generics.clone(),
            has_receiver,
            &mut item.sig.generics,
        );
        let check_callee = callee.clone();

        let fn_by_val = item.attrs.iter().any(is_by_val_attr);
        let borrow_output = borrows_return(&item.sig, fn_by_val);
        let mut abi_sig = item.sig.clone();
        if drop_impl {
            if declared_self {
                let owned_ty: Type = syn::parse_quote!(co3::boxed::Box<#self_ty>);
                drop_owned_receiver(&mut abi_sig, Some(&owned_ty));
            } else {
                drop_owned_receiver(&mut abi_sig, None);
            }
        }
        let fn_signature = if drop_impl && declared_self {
            let mut sig = lower_abi_fn_signature(abi_sig, failure_mode, fn_by_val);
            let receiver = sig
                .inputs
                .iter_mut()
                .find_map(|input| match input {
                    syn::FnArg::Typed(arg) if item_fn_input_ident(&arg.pat) == "__co3_self" => {
                        Some(arg)
                    }
                    _ => None,
                })
                .expect("opaque Drop requires a receiver");
            receiver.ty = syn::parse_quote!(co3::boxed::abi::Box<<#self_ty as co3::ReprC>::CType>);
            sig
        } else {
            lower_abi_fn_signature(abi_sig, failure_mode, fn_by_val)
        };
        let signature_drift_check =
            // NOTE: `Drop::drop` has a fixed signature enforced by `validate_drop_impl`
            (!drop_impl)
                .then(|| gen_static_fn_signature_drift_check(item.sig.clone(), check_callee));
        let body = if drop_impl && declared_self {
            gen_opaque_drop_definition_body(&item.sig, failure_mode)
        } else if drop_impl {
            gen_regular_drop_definition_body(&item.sig, failure_mode)
        } else {
            gen_definition_body(item.sig, quote!(#callee), fn_by_val, failure_mode)
        };

        let ffi_fn_body = quote! {{
            #signature_drift_check
            #body
        }};

        let definition = emit_extern_definition(
            abi,
            &item.attrs,
            failure_mode,
            fn_signature,
            ffi_fn_body,
            borrow_output,
        );
        Some(quote! { #return_borrow_check #definition })
    });

    quote! {
        #(#impl_attrs)*
        const _: () = {
            #(#definitions)*
        };
    }
}

struct ConcreteSelfType<'a>(&'a Type);

impl VisitMut for ConcreteSelfType<'_> {
    fn visit_type_mut(&mut self, ty: &mut Type) {
        if let Type::Path(path) = ty
            && path.qself.is_none()
            && path.path.is_ident("Self")
        {
            *ty = self.0.clone();
        } else {
            syn::visit_mut::visit_type_mut(self, ty);
        }
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

    let return_borrow_check = if item.sig.generics.params.is_empty() {
        fn_return_ty(&item.sig).map(|ty| {
            let check = gen_return_borrow_check(ty, item.attrs.iter().any(is_by_val_attr));
            let cfg = cfg_attrs(&item.attrs);
            quote! { #(#cfg)* const _: () = { #check }; }
        })
    } else {
        None
    };
    let ffi_fn_body = gen_fn_definition_body(&item, failure_mode, callee);
    let fn_by_val = item.attrs.iter().any(is_by_val_attr);
    let borrow_output = borrows_return(&item.sig, fn_by_val);
    let fn_signature = lower_abi_fn_signature(item.sig, failure_mode, fn_by_val);

    let definition = emit_extern_definition(
        abi,
        &item.attrs,
        failure_mode,
        fn_signature,
        ffi_fn_body,
        borrow_output,
    );
    quote! { #return_borrow_check #definition }
}

fn lower_extern_fn_signature_with_plain_inputs(
    mut sig: syn::Signature,
    failure_mode: FailureMode,
    plain_inputs: &[Ident],
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

    let inputs = argument::analyze_with_plain_inputs(&sig.inputs, plain_inputs);
    sig.inputs = inputs.iter().flat_map(lower_signature_input).collect();
    lower_signature_output(&mut sig);

    sig.constness = None;
    sig.asyncness = None;
    sig.safety = syn::Safety::Default;
    sig.abi = None;

    sig
}

pub(crate) fn lower_abi_fn_signature(
    sig: syn::Signature,
    failure_mode: FailureMode,
    move_fn: bool,
) -> syn::Signature {
    lower_abi_fn_signature_with_plain_inputs(sig, failure_mode, move_fn, &[])
}

pub(crate) fn lower_abi_fn_signature_with_plain_inputs(
    sig: syn::Signature,
    failure_mode: FailureMode,
    move_fn: bool,
    plain_inputs: &[Ident],
) -> syn::Signature {
    let mut sig = sig;
    explicitize_signature_lifetimes(&mut sig);
    let borrowed_output = fn_return_ty(&sig)
        .filter(|ty| ownership_mode(ty, move_fn) == OwnershipMode::Borrow)
        .cloned();
    let mut sig = lower_extern_fn_signature_with_plain_inputs(sig, failure_mode, plain_inputs);
    if let Some(ty) = borrowed_output {
        sig.output = parse_quote!(-> co3::marker::Borrowed< <#ty as co3::ReprC>::CType >);
    }
    sig
}

fn lower_signature_input(input: &Argument<'_>) -> Vec<syn::FnArg> {
    let pat = &input.pat;
    let cfg = input.cfg().collect::<Vec<_>>();
    let ffi_ty = match &input.unpack {
        Unpack::Pair(pair) => {
            let (data_name, metadata_name) = unpack_arg_names(input.name());
            let first = &pair.first.abi;
            let second = &pair.second.abi;
            return vec![
                parse_quote!(#(#cfg)* #data_name: #first),
                parse_quote!(#(#cfg)* #metadata_name: #second),
            ];
        }
        Unpack::Single(target) => parse_quote!(<#target as co3::ReprC>::CType),
        Unpack::None => input.abi_ty(),
    };
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
            syn::FnArg::Typed(arg)
                if !matches!(
                    UnpackSyntax::parse(&arg.attrs).expect("validated #[unpack] attribute"),
                    UnpackSyntax::Pair(_)
                ) =>
            {
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

pub(crate) fn item_fn_output_type(return_ty: &Type) -> Type {
    if is_identity_primitive(return_ty) {
        return return_ty.clone();
    }

    parse_quote!(<#return_ty as co3::ReprC>::CType)
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
    ownership_mode(ty, attrs.iter().any(is_by_val_attr))
}

pub(crate) fn borrows_return(sig: &syn::Signature, move_fn: bool) -> bool {
    fn_return_ty(sig).is_some_and(|ty| ownership_mode(ty, move_fn) == OwnershipMode::Borrow)
}

/// Use the same ownership classification for arguments and return values.
pub(crate) fn ownership_mode(ty: &Type, explicitly_moved: bool) -> OwnershipMode {
    if explicitly_moved || is_implicitly_by_value(ty) {
        return OwnershipMode::ByValue;
    }

    OwnershipMode::Borrow
}

pub(crate) fn is_implicitly_by_value(ty: &Type) -> bool {
    use crate::known_type::{self, Kind};

    struct ByValueVisitor {
        by_value: bool,
        shared: bool,
    }

    impl ByValueVisitor {
        fn with_access(&mut self, ty: &Type, shared: bool) {
            let previous = core::mem::replace(&mut self.shared, shared);
            self.visit_type(ty);
            self.shared = previous;
        }
    }

    impl<'ast> Visit<'ast> for ByValueVisitor {
        fn visit_type(&mut self, ty: &'ast Type) {
            if !self.by_value {
                return;
            }
            let ty = peel_grouped_type(ty);
            if tag_id(ty).is_some() {
                return;
            }
            match ty {
                Type::Reference(reference) => {
                    // A shared outer borrow cannot grant mutable access through &mut.
                    self.with_access(
                        &reference.elem,
                        self.shared || reference.mutability.is_none(),
                    );
                }
                Type::Ptr(_) | Type::FnPtr(_) => {}
                Type::Tuple(tuple) => syn::visit::visit_type_tuple(self, tuple),
                Type::Array(array) => self.visit_type(&array.elem),
                Type::Slice(slice) => self.visit_type(&slice.elem),
                Type::Path(path) => {
                    let Some(known) = known_type::recognize_path(path) else {
                        self.by_value = false;
                        return;
                    };
                    match known.kind {
                        Kind::Primitive
                        | Kind::NonZeroInt
                        | Kind::Str
                        | Kind::CStr
                        | Kind::NonNull
                        | Kind::MaybeUninit
                        | Kind::PhantomData => {}
                        Kind::String | Kind::CString => self.by_value = self.shared,
                        Kind::Box | Kind::Vec if !self.shared => self.by_value = false,
                        Kind::Cell | Kind::UnsafeCell => {
                            // Interior mutability permits ownership transfer even under &T.
                            self.with_access(known.inner.unwrap(), false);
                        }
                        Kind::Box
                        | Kind::Vec
                        | Kind::Option
                        | Kind::Result
                        | Kind::ManuallyDrop
                        | Kind::NonZero
                        | Kind::Tuple => {
                            syn::visit::visit_type_path(self, path);
                        }
                    }
                }
                _ => self.by_value = false,
            }
        }
    }

    let mut visitor = ByValueVisitor {
        by_value: true,
        shared: false,
    };
    visitor.visit_type(ty);
    visitor.by_value
}

pub(crate) fn copy_primitive_type(path: &syn::TypePath) -> Option<Type> {
    let known = crate::known_type::recognize_path(path)?;
    (known.kind == crate::known_type::Kind::Primitive).then_some(known.canonical)
}

pub(crate) fn is_identity_primitive(ty: &Type) -> bool {
    let Type::Path(path) = peel_grouped_type(ty) else {
        return false;
    };
    copy_primitive_type(path).is_some()
        && !matches!(
            path.path
                .segments
                .last()
                .unwrap()
                .ident
                .to_string()
                .as_str(),
            "bool" | "char"
        )
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

pub(crate) struct SelfConcretizer<'a> {
    pub(crate) self_ty: &'a Type,
}

impl SelfConcretizer<'_> {
    pub(crate) fn new(self_ty: &Type) -> SelfConcretizer<'_> {
        SelfConcretizer { self_ty }
    }
}

impl VisitMut for SelfConcretizer<'_> {
    fn visit_type_mut(&mut self, ty: &mut Type) {
        if tag_id(ty).is_some() {
            return;
        }
        syn::visit_mut::visit_type_mut(self, ty);
        let Type::Path(syn::TypePath {
            qself: None, path, ..
        }) = ty
        else {
            return;
        };
        if path
            .segments
            .first()
            .is_some_and(|segment| segment.ident == "Self")
        {
            let rest = Path {
                leading_colon: None,
                segments: path.segments.iter().skip(1).cloned().collect(),
            };
            *ty = qualify_self_path(self.self_ty, &rest);
        }
    }

    fn visit_expr_path_mut(&mut self, expr: &mut syn::ExprPath) {
        syn::visit_mut::visit_expr_path_mut(self, expr);
        if expr.qself.is_some() {
            return;
        }
        if expr
            .path
            .segments
            .first()
            .is_some_and(|segment| segment.ident == "Self")
        {
            let rest = Path {
                leading_colon: None,
                segments: expr.path.segments.iter().skip(1).cloned().collect(),
            };
            let self_ty = self.self_ty;
            *expr = parse_quote!(<#self_ty>::#rest);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_paths_are_concretized_without_trait_qualification() {
        let self_ty: Type = parse_quote!(Widget);
        let mut sig: syn::Signature = parse_quote!(
            fn example(
                value: Self::Item,
                qualified: <Self as Trait>::Item,
                other: Widget::Item,
                untouched: Other::Item,
            ) -> [Self::Item; Self::COUNT + Widget::COUNT + Self::make()]
        );

        normalize_fn_signature(&mut sig, Some(&self_ty));

        let expected: syn::Signature = parse_quote!(
            fn example(
                value: Widget::Item,
                qualified: <Widget as Trait>::Item,
                other: Widget::Item,
                untouched: Other::Item,
            ) -> [Widget::Item; <Widget>::COUNT + Widget::COUNT + <Widget>::make()]
        );
        assert_eq!(quote!(#sig).to_string(), quote!(#expected).to_string());
    }

    #[test]
    fn exported_definition_has_private_raw_name_and_explicit_symbol() {
        let abi: syn::Abi = parse_quote!(extern "C");
        let attrs = vec![parse_quote!(#[symbol_name = "public_symbol"])];
        let definition = emit_extern_definition(
            &abi,
            &attrs,
            FailureMode::Panic,
            parse_quote!(fn example(value: u8) -> u8),
            quote!(Ok::<_, ()>(value)),
            false,
        );
        let item: syn::ItemFn = syn::parse2(definition).unwrap();

        assert_eq!(item.sig.ident, "example_raw");
        assert!(matches!(item.vis, syn::Visibility::Inherited));
        assert!(item.attrs.iter().any(|attr| {
            let syn::Meta::List(list) = &attr.meta else {
                return false;
            };
            list.path.is_ident("unsafe")
                && list.tokens.to_string() == "export_name = \"public_symbol\""
        }));
    }

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
                x: <&'a <Foo as ReprC>::CType<'b> as Projection>::Output
            )
        ));
        let expected: syn::Signature = parse_quote!(
            fn f<'a, 'b>(
                x: <&'a <Foo as ReprC>::CType<'b> as Projection>::Output
            )
        );

        assert_eq!(sig, expected);
    }

    #[test]
    fn selects_implicit_by_value_argument_types_during_generation() {
        let by_value: &[Type] = &[
            parse_quote!(<dyn Trait>::TAG),
            parse_quote!(*const u8),
            parse_quote!(unsafe extern "C" fn(u8) -> u16),
            parse_quote!(u32),
            parse_quote!(bool),
            parse_quote!(core::primitive::char),
            parse_quote!(&u8),
            parse_quote!(&mut str),
            parse_quote!(Option<&u8>),
            parse_quote!(Option<Option<&u8>>),
            parse_quote!(Option<&mut u8>),
            parse_quote!((<dyn Trait>::TAG, Option<&u8>, (u32, fn()))),
            parse_quote!((&u8)),
            parse_quote!((&u8, &mut u8)),
            parse_quote!([u8; 4]),
            parse_quote!(()),
            parse_quote!(&[u8]),
            parse_quote!(Result<u32, bool>),
            parse_quote!(core::marker::PhantomData<Box<T>>),
            parse_quote!(MaybeUninit<Box<T>>),
            parse_quote!(core::mem::MaybeUninit<T>),
            parse_quote!(&mut MaybeUninit<String>),
            parse_quote!(Option<MaybeUninit<T>>),
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
    fn copy_primitives_with_custom_carriers_are_not_identity_primitives() {
        assert!(!is_identity_primitive(&parse_quote!(bool)));
        assert!(!is_identity_primitive(&parse_quote!(core::primitive::char)));
        assert!(is_identity_primitive(&parse_quote!(u32)));
    }

    #[test]
    fn leaves_other_argument_types_borrowed_by_default() {
        let borrowed: &[Type] = &[
            parse_quote!(String),
            parse_quote!(Option<String>),
            parse_quote!((&u8, String)),
            parse_quote!([Box<u8>; 4]),
            parse_quote!(Result<u32, String>),
            parse_quote!(T),
            parse_quote!(&T),
            parse_quote!(&mut T),
            parse_quote!(&alias::String),
            parse_quote!(alias::u32),
            parse_quote!(&<T as Trait>::Output),
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
        assert_eq!(
            ownership_mode_for_arg(&attrs, &parse_quote!(&mut u8)),
            OwnershipMode::ByValue,
        );
        assert_eq!(
            ownership_mode_for_arg(&attrs, &parse_quote!(&u8)),
            OwnershipMode::ByValue,
        );
    }

    #[test]
    fn reference_ownership_tracks_access_to_nested_owners() {
        for ty in [
            parse_quote!(&String),
            parse_quote!(&alloc::string::String),
            parse_quote!(&CString),
            parse_quote!(&Box<u32>),
            parse_quote!(&Vec<String>),
            parse_quote!(&mut [u8]),
            parse_quote!(&Cell<u32>),
            parse_quote!(&UnsafeCell<u32>),
            parse_quote!(&&mut Box<u32>),
            parse_quote!(&mut &String),
            parse_quote!(&UnsafeCell<&String>),
            parse_quote!((&UnsafeCell<u32>, &Box<u32>)),
        ] {
            assert_eq!(
                ownership_mode(&ty, false),
                OwnershipMode::ByValue,
                "{}",
                quote!(#ty)
            );
        }
        for ty in [
            parse_quote!(&mut Box<T>),
            parse_quote!(&mut String),
            parse_quote!(&UnsafeCell<Box<T>>),
            parse_quote!(&core::cell::UnsafeCell<Box<T>>),
            parse_quote!(&Cell<Box<T>>),
            parse_quote!(&UnsafeCell<&mut Box<T>>),
            parse_quote!(&Box<UnsafeCell<Box<T>>>),
            parse_quote!(&Vec<Cell<Box<T>>>),
            parse_quote!(&[UnsafeCell<Box<T>>]),
            parse_quote!(&Option<UnsafeCell<Box<T>>>),
            parse_quote!(&mut [Box<T>]),
            parse_quote!((&String, Box<u32>)),
            parse_quote!((&String, &mut Box<u32>)),
        ] {
            assert_eq!(
                ownership_mode(&ty, false),
                OwnershipMode::Borrow,
                "{}",
                quote!(#ty)
            );
        }
    }
}
