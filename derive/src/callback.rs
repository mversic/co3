use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::visit_mut::VisitMut;
use syn::{Error, ItemFn, LitStr, Result, parse_quote};

use crate::{
    ffi_fn::{self, gen_abi_assertions},
    parse::FailureMode,
    utils::{cfg_attrs, co3_path},
};

pub(crate) fn expand_companion(
    abi_name: &LitStr,
    failure_mode: FailureMode,
    item: &ItemFn,
    callee: TokenStream,
) -> Result<TokenStream> {
    if item.sig.asyncness.is_some()
        || matches!(item.sig.safety, syn::Safety::Unsafe(_))
        || item.sig.variadic.is_some()
        || item.sig.inputs.len() > 12
        || !item.sig.generics.params.is_empty()
        || item
            .sig
            .generics
            .where_clause
            .as_ref()
            .is_some_and(|clause| !clause.predicates.is_empty())
    {
        return Err(Error::new_spanned(
            &item.sig,
            "raw function declarations require a safe, synchronous, non-generic function with at most 12 arguments",
        ));
    }
    for (index, input) in item.sig.inputs.iter().enumerate() {
        let syn::FnArg::Typed(arg) = input else {
            if index == 0 {
                continue;
            }
            return Err(Error::new_spanned(
                input,
                "raw function receiver must be first",
            ));
        };
        if !matches!(arg.pat.as_ref(), syn::Pat::Ident(_)) {
            return Err(Error::new_spanned(
                &arg.pat,
                "raw function parameters must be identifiers",
            ));
        }
    }

    let fn_by_val = item.attrs.iter().any(ffi_fn::is_by_val_attr);
    let abi: syn::Abi = parse_quote!(extern #abi_name);
    let mut raw_sig = ffi_fn::lower_raw_fn_signature(item.sig.clone(), failure_mode, fn_by_val);
    raw_sig.safety = syn::Safety::Unsafe(Default::default());
    raw_sig.abi = Some(abi);
    raw_sig.ident = format_ident!("{}_raw", item.sig.ident);
    let assertions = gen_abi_assertions(&raw_sig);
    let callee: syn::Expr = syn::parse2(callee)?;
    let signature_check = ffi_fn::gen_fn_signature_drift_check(item.sig.clone(), callee.clone());
    let body =
        ffi_fn::gen_raw_definition_body(item.sig.clone(), quote!(#callee), fn_by_val, failure_mode);
    let vis = &item.vis;
    let cfg = cfg_attrs(&item.attrs);
    let co3 = co3_path();
    let error_handler = match failure_mode {
        FailureMode::Panic => ffi_fn::gen_failure_panic(quote!(err)),
        FailureMode::Error => ffi_fn::gen_abi_return_encode(quote!(err), !fn_by_val),
    };

    Ok(quote! {
        #(#cfg)*
        #[doc = "C-compatible companion of the declared Rust function."]
        #[doc = ""]
        #[doc = "# Safety"]
        #[doc = ""]
        #[doc = "The caller must uphold the safety requirements of `co3::decode` or `co3::soft_decode` for each argument, as applicable."]
        #vis #raw_sig {
            use #co3 as co3;
            #assertions
            #signature_check
            let __co3_raw_body = || #body;
            match __co3_raw_body() {
                Ok(value) => value,
                Err(err) => #error_handler,
            }
        }
    })
}

pub(crate) fn lower_callback_fn_type(
    sig: syn::Signature,
    abi: &syn::Abi,
    failure_mode: FailureMode,
    move_fn: bool,
) -> Result<(syn::Signature, syn::Type)> {
    let mut type_sig = ffi_fn::lower_raw_fn_signature(sig, failure_mode, move_fn);
    let lowered_sig = type_sig.clone();
    let replacements = type_sig
        .generics
        .lifetimes()
        .enumerate()
        .map(|(index, param)| {
            (
                param.lifetime.ident.to_string(),
                syn::Lifetime::new(
                    &format!("'__co3_callback_type_{index}"),
                    proc_macro2::Span::call_site(),
                ),
            )
        })
        .collect::<Vec<_>>();
    struct RenameLifetimes(Vec<(String, syn::Lifetime)>);
    impl VisitMut for RenameLifetimes {
        fn visit_lifetime_mut(&mut self, lifetime: &mut syn::Lifetime) {
            if let Some((_, replacement)) = self
                .0
                .iter()
                .find(|(ident, _)| ident == &lifetime.ident.to_string())
            {
                *lifetime = replacement.clone();
            }
        }
    }
    RenameLifetimes(replacements).visit_signature_mut(&mut type_sig);
    let callback_lifetimes = type_sig
        .generics
        .lifetimes()
        .map(|param| &param.lifetime)
        .collect::<Vec<_>>();
    let binder = (!callback_lifetimes.is_empty()).then(|| quote!(for<#(#callback_lifetimes),*>));
    let inputs = type_sig.inputs.iter().map(|input| {
        let syn::FnArg::Typed(arg) = input else {
            unreachable!("normalized callback signature has typed arguments")
        };
        let cfg = cfg_attrs(&arg.attrs).collect::<Vec<_>>();
        let ty = &arg.ty;
        quote!(#(#cfg)* #ty)
    });
    let output = &type_sig.output;
    let callback_type = syn::parse2(quote!(#binder unsafe #abi fn(#(#inputs),*) #output))?;
    Ok((lowered_sig, callback_type))
}
