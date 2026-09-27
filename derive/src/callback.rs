use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::visit::Visit;
use syn::visit_mut::VisitMut;
use syn::{ItemFn, Result, parse_quote};

use crate::{
    ffi_fn,
    parse::FailureMode,
    utils::{ParamUseDetector, cfg_attrs, co3_path, soft_for_arg},
};

pub(crate) fn expand_companion(
    failure_mode: FailureMode,
    item: &ItemFn,
    callee: TokenStream,
    impl_generics: Option<&syn::Generics>,
) -> Result<TokenStream> {
    let fn_by_val = item.attrs.iter().any(ffi_fn::is_by_val_attr);
    let abi = parse_quote!(extern "C");
    let mut raw_sig = ffi_fn::lower_raw_fn_signature(item.sig.clone(), failure_mode, fn_by_val);
    raw_sig.safety = syn::Safety::Unsafe(Default::default());
    raw_sig.abi = Some(abi);
    raw_sig.ident = format_ident!("{}_raw", item.sig.ident);
    add_generic_companion_bounds(&mut raw_sig, &item.sig, impl_generics, fn_by_val);
    let generic_args = item
        .sig
        .generics
        .params
        .iter()
        .filter_map(|param| match param {
            syn::GenericParam::Type(param) => {
                let ident = &param.ident;
                Some(quote!(#ident))
            }
            syn::GenericParam::Const(param) => {
                let ident = &param.ident;
                Some(quote!(#ident))
            }
            syn::GenericParam::Lifetime(_) => None,
        })
        .collect::<Vec<_>>();
    let callee: syn::Expr = if generic_args.is_empty() {
        syn::parse2(callee)?
    } else {
        syn::parse2(quote!(#callee::<#(#generic_args),*>))?
    };
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
        #[doc = "Companion of the declared Rust function with lowered argument and return types."]
        #[doc = ""]
        #[doc = "# Safety"]
        #[doc = ""]
        #[doc = "The caller must uphold the safety requirements of `co3::decode` or `co3::soft_decode` for each argument, as applicable."]
        #vis #raw_sig {
            use #co3 as co3;
            #signature_check
            let __co3_raw_body = || #body;
            match __co3_raw_body() {
                Ok(value) => value,
                Err(err) => #error_handler,
            }
        }
    })
}

fn add_generic_companion_bounds(
    raw_sig: &mut syn::Signature,
    source_sig: &syn::Signature,
    impl_generics: Option<&syn::Generics>,
    fn_by_val: bool,
) {
    let generic_idents = source_sig
        .generics
        .type_params()
        .chain(
            impl_generics
                .into_iter()
                .flat_map(syn::Generics::type_params),
        )
        .map(|param| &param.ident)
        .collect::<Vec<_>>();
    if generic_idents.is_empty() {
        return;
    }
    let detector = ParamUseDetector::new(generic_idents);
    struct Lifetimes(Vec<syn::Lifetime>);
    impl<'ast> Visit<'ast> for Lifetimes {
        fn visit_lifetime(&mut self, lifetime: &'ast syn::Lifetime) {
            if !self.0.iter().any(|existing| existing == lifetime) {
                self.0.push(lifetime.clone());
            }
        }
    }
    let mut predicates = Vec::<syn::WherePredicate>::new();
    for input in &source_sig.inputs {
        let syn::FnArg::Typed(arg) = input else {
            continue;
        };
        let ty = arg.ty.as_ref();
        if !detector.type_mentions_param(ty) {
            continue;
        }
        predicates.push(parse_quote!(#ty: co3::ReprC));
        let mut lifetimes = Lifetimes(Vec::new());
        lifetimes.visit_type(ty);
        let decode_lifetime = (lifetimes.0.len() == 1).then(|| lifetimes.0.remove(0));
        if ffi_fn::ownership_mode_for_arg(&arg.attrs, ty) == crate::generate::OwnershipMode::ByValue
        {
            if let Some(lifetime) = decode_lifetime {
                if soft_for_arg(&arg.attrs) {
                    predicates.push(parse_quote!(#ty: co3::Decode<#lifetime>));
                } else {
                    predicates.push(parse_quote!(
                        #ty: co3::Decode<#lifetime, Store: co3::stored::EmptyStore>
                    ));
                }
            } else if soft_for_arg(&arg.attrs) {
                predicates.push(parse_quote!(for<'__co3_d> #ty: co3::Decode<'__co3_d>));
            } else {
                predicates.push(parse_quote!(
                    for<'__co3_d> #ty: co3::Decode<'__co3_d, Store: co3::stored::EmptyStore>
                ));
            }
        } else {
            predicates.push(parse_quote!(#ty: co3::borrow::Borrow));
            predicates.push(parse_quote!(
                <#ty as co3::ReprC>::CType: co3::borrow::BorrowCast
            ));
            predicates.push(parse_quote!(for<'__co3_d> #ty: co3::borrow::FromBorrow<'__co3_d>));
            if soft_for_arg(&arg.attrs) {
                predicates.push(parse_quote!(
                    for<'__co3_d> <#ty as co3::borrow::Borrow>::Borrowed<'__co3_d>:
                        co3::Decode<
                            '__co3_d,
                            CType = <<#ty as co3::ReprC>::CType as co3::borrow::BorrowCast>::AsConst,
                        >
                ));
            } else {
                predicates.push(parse_quote!(
                    for<'__co3_d> <#ty as co3::borrow::Borrow>::Borrowed<'__co3_d>:
                        co3::Decode<
                            '__co3_d,
                            CType = <<#ty as co3::ReprC>::CType as co3::borrow::BorrowCast>::AsConst,
                            Store: co3::stored::EmptyStore,
                        >
                ));
            }
        }
    }
    if let syn::ReturnType::Type(_, ty) = &source_sig.output
        && detector.type_mentions_param(ty)
    {
        predicates.push(parse_quote!(#ty: co3::Encode<Store: co3::stored::EmptyStore>));
        if !fn_by_val {
            predicates.push(parse_quote!(
                <#ty as co3::ReprC>::CType: co3::borrow::BorrowCast
            ));
            predicates.push(parse_quote!(
                #ty: co3::borrow::Borrow<Owner: co3::stored::EmptyStore>
            ));
        }
    }
    raw_sig
        .generics
        .make_where_clause()
        .predicates
        .extend(predicates);
}

pub(crate) fn lower_callback_fn_type(
    sig: syn::Signature,
    abi: &syn::Abi,
    failure_mode: FailureMode,
    move_fn: bool,
) -> Result<syn::Type> {
    let mut type_sig = ffi_fn::lower_raw_fn_signature(sig, failure_mode, move_fn);
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
    Ok(callback_type)
}
