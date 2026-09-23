use proc_macro2::TokenStream;
use quote::{format_ident, quote};
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

    let abi: syn::Abi = parse_quote!(extern #abi_name);
    let mut raw_sig = ffi_fn::lower_extern_fn_signature(item.sig.clone(), failure_mode);
    raw_sig.ident = format_ident!("{}_raw", item.sig.ident);
    let assertions = gen_abi_assertions(&raw_sig, &abi);
    let callee: syn::Expr = syn::parse2(callee)?;
    let signature_check = ffi_fn::gen_fn_signature_drift_check(item.sig.clone(), callee.clone());
    let fn_by_val = item.attrs.iter().any(ffi_fn::is_by_val_attr);
    let body = ffi_fn::gen_definition_body(
        item.sig.clone(),
        quote!(#callee),
        fn_by_val,
        failure_mode,
    );
    let vis = &item.vis;
    let cfg = cfg_attrs(&item.attrs);
    let co3 = co3_path();
    let error_handler = match failure_mode {
        FailureMode::Panic => ffi_fn::gen_failure_panic(quote!(err)),
        FailureMode::Error => quote!(co3::encode(err)),
    };

    Ok(quote! {
        #(#cfg)*
        #vis #abi #raw_sig {
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
