use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::visit::Visit;
use syn::visit_mut::VisitMut;
use syn::{ItemFn, Result, parse_quote};

use crate::{
    ffi_fn,
    parse::{self, FailureMode},
    utils::{ParamUseDetector, cfg_attrs, co3_path, soft_for_arg},
};

pub(crate) fn lower_fn_pointer_type(
    pointer: &syn::TypeFnPtr,
    failure_mode: FailureMode,
) -> Result<syn::Type> {
    let abi = match &pointer.abi {
        None => {
            return Err(syn::Error::new_spanned(
                pointer.fn_token,
                "function pointers in `ffi!` require an explicit non-Rust `extern \"ABI\"`",
            ));
        }
        Some(abi) if abi.name.is_none() => {
            return Err(syn::Error::new_spanned(
                abi,
                "function pointers in `ffi!` require an explicit non-Rust `extern \"ABI\"`",
            ));
        }
        Some(abi) if abi.name.as_ref().is_some_and(|name| name.value() == "Rust") => {
            return Err(syn::Error::new_spanned(
                abi,
                "function pointers in `ffi!` cannot use the Rust ABI",
            ));
        }
        Some(abi) => abi.clone(),
    };
    if pointer.variadic.is_some() || pointer.inputs.len() > 12 {
        return Err(syn::Error::new_spanned(
            pointer,
            "function pointers require a non-variadic signature with at most 12 arguments",
        ));
    }
    let mut sig: syn::Signature = syn::parse_quote!(fn __co3_callback_type());
    if let Some(lifetimes) = &pointer.lifetimes {
        sig.generics.params = lifetimes.lifetimes.clone();
        sig.generics.lt_token = Some(Default::default());
        sig.generics.gt_token = Some(Default::default());
    }
    sig.inputs = pointer
        .inputs
        .iter()
        .enumerate()
        .map(|(index, arg)| -> syn::FnArg {
            let name = format_ident!("__co3_arg_{index}");
            let attrs = &arg.attrs;
            let ty = &arg.ty;
            syn::parse_quote!(#(#attrs)* #name: #ty)
        })
        .collect();
    sig.output = pointer.output.clone();
    crate::validate::validate_unpack(&sig, None)?;
    lower_nested_fn_pointers(&mut sig, failure_mode)?;
    lower_callback_fn_type(sig, &abi, failure_mode, false)
}

pub(crate) fn lower_fn_pointers_in_type(
    ty: &mut syn::Type,
    failure_mode: FailureMode,
) -> Result<()> {
    struct Lowerer {
        failure_mode: FailureMode,
        error: Option<syn::Error>,
    }

    impl VisitMut for Lowerer {
        fn visit_type_mut(&mut self, ty: &mut syn::Type) {
            if self.error.is_some() {
                return;
            }
            if let syn::Type::FnPtr(pointer) = ty {
                match lower_fn_pointer_type(pointer, self.failure_mode) {
                    Ok(lowered) => *ty = lowered,
                    Err(error) => self.error = Some(error),
                }
            }
        }
    }

    let mut lowerer = Lowerer {
        failure_mode,
        error: None,
    };
    lowerer.visit_type_mut(ty);
    lowerer.error.map_or(Ok(()), Err)
}

pub(crate) fn lower_nested_fn_pointers(
    sig: &mut syn::Signature,
    failure_mode: FailureMode,
) -> Result<()> {
    for input in &mut sig.inputs {
        if let syn::FnArg::Typed(arg) = input {
            lower_fn_pointers_in_type(&mut arg.ty, failure_mode)?;
        }
    }
    if let syn::ReturnType::Type(_, ty) = &mut sig.output {
        lower_fn_pointers_in_type(ty, failure_mode)?;
    }
    Ok(())
}

pub(crate) fn lower_raw_alias_signature(
    mut sig: syn::Signature,
    move_fn: bool,
    failure_mode: FailureMode,
    block_abi: &syn::Abi,
) -> Result<syn::Type> {
    let abi = sig.abi.get_or_insert_with(|| block_abi.clone());
    let Some(name) = &abi.name else {
        return Err(syn::Error::new_spanned(
            abi,
            "`extern fn` requires an explicit ABI",
        ));
    };
    if name.value() == "Rust" {
        return Err(syn::Error::new_spanned(
            abi,
            "raw function pointer aliases cannot use the Rust ABI",
        ));
    }
    let abi = abi.clone();
    if sig.asyncness.is_some()
        || matches!(sig.safety, syn::Safety::Unsafe(_))
        || sig.variadic.is_some()
        || !sig.generics.params.is_empty()
        || sig
            .generics
            .where_clause
            .as_ref()
            .is_some_and(|clause| !clause.predicates.is_empty())
        || sig.inputs.len() > 12
    {
        return Err(syn::Error::new_spanned(
            sig,
            "raw function pointers require a safe, synchronous, non-generic function with at most 12 arguments",
        ));
    }
    crate::validate::validate_unpack(&sig, None)?;
    lower_raw_fn_types_in_signature(&mut sig, failure_mode, block_abi)?;
    lower_nested_fn_pointers(&mut sig, failure_mode)?;
    lower_callback_fn_type(sig, &abi, failure_mode, move_fn)
}

struct RawTypeLowerer<'a> {
    failure_mode: FailureMode,
    block_abi: &'a syn::Abi,
    error: Option<syn::Error>,
}

impl VisitMut for RawTypeLowerer<'_> {
    fn visit_type_mut(&mut self, ty: &mut syn::Type) {
        if self.error.is_some() {
            return;
        }
        if let syn::Type::Macro(mac) = ty
            && mac.mac.path.is_ident(parse::RAW_FN_TYPE_MACRO)
        {
            let lowered = parse::parse_raw_function_type(mac.mac.tokens.clone()).and_then(
                |(sig, move_fn)| {
                    lower_raw_alias_signature(sig, move_fn, self.failure_mode, self.block_abi)
                },
            );
            match lowered {
                Ok(lowered) => *ty = lowered,
                Err(error) => self.error = Some(error),
            }
        } else if let syn::Type::FnPtr(pointer) = ty
            && pointer
                .inputs
                .iter()
                .any(|arg| arg.attrs.iter().any(ffi_fn::is_unpack_attr))
        {
            match lower_fn_pointer_type(pointer, self.failure_mode) {
                Ok(lowered) => *ty = lowered,
                Err(error) => self.error = Some(error),
            }
        } else {
            syn::visit_mut::visit_type_mut(self, ty);
        }
    }
}

pub(crate) fn lower_raw_fn_types_in_signature(
    sig: &mut syn::Signature,
    failure_mode: FailureMode,
    block_abi: &syn::Abi,
) -> Result<()> {
    let mut lowerer = RawTypeLowerer {
        failure_mode,
        block_abi,
        error: None,
    };
    lowerer.visit_signature_mut(sig);
    lowerer.error.map_or(Ok(()), Err)
}

pub(crate) fn lower_raw_fn_types_in_items(
    items: &mut [parse::ParsedItem],
    failure_mode: FailureMode,
    block_abi: &syn::Abi,
) -> Result<()> {
    let mut lowerer = RawTypeLowerer {
        failure_mode,
        block_abi,
        error: None,
    };
    for item in items {
        match item {
            parse::ParsedItem::Fn(item) => lowerer.visit_item_fn_mut(item),
            parse::ParsedItem::Raw(item) => lowerer.visit_signature_mut(&mut item.sig),
            parse::ParsedItem::Static(item) => lowerer.visit_type_mut(&mut item.ty),
            parse::ParsedItem::Impl(item) => lowerer.visit_item_impl_mut(item),
            parse::ParsedItem::Type(item) => lowerer.visit_foreign_item_type_mut(item),
            parse::ParsedItem::Alias(alias) => lowerer.visit_item_type_mut(&mut alias.item),
        }
    }
    lowerer.error.map_or(Ok(()), Err)
}

pub(crate) fn expand_companion(
    failure_mode: FailureMode,
    item: &ItemFn,
    callee: TokenStream,
    impl_generics: Option<&syn::Generics>,
) -> Result<TokenStream> {
    let fn_by_val = item.attrs.iter().any(ffi_fn::is_by_val_attr);
    let abi = parse_quote!(extern "C");
    let mut raw_sig = ffi_fn::lower_abi_fn_signature(item.sig.clone(), failure_mode, fn_by_val);
    raw_sig.safety = syn::Safety::Unsafe(Default::default());
    raw_sig.abi = Some(abi);
    raw_sig.ident = ffi_fn::raw_definition_name(&item.sig.ident);
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
        ffi_fn::gen_definition_body(item.sig.clone(), quote!(#callee), fn_by_val, failure_mode);
    Ok(emit_companion(
        failure_mode,
        &item.attrs,
        &item.vis,
        raw_sig,
        signature_check,
        body,
        fn_by_val,
    ))
}

pub(crate) fn emit_companion(
    failure_mode: FailureMode,
    attrs: &[syn::Attribute],
    vis: &syn::Visibility,
    sig: syn::Signature,
    setup: TokenStream,
    body: TokenStream,
    fn_by_val: bool,
) -> TokenStream {
    let cfg = cfg_attrs(attrs);
    let co3 = co3_path();
    let attrs = quote! {
        #(#cfg)*
        #[doc = "Companion of the declared Rust function with lowered argument and return types."]
        #[doc = ""]
        #[doc = "# Safety"]
        #[doc = ""]
        #[doc = "The caller must uphold the safety requirements of `co3::decode` or `co3::soft_decode` for each argument, as applicable."]
    };
    ffi_fn::emit_abi_function(
        failure_mode,
        attrs,
        quote!(#vis),
        &sig,
        quote!(use #co3 as co3; #setup),
        body,
        fn_by_val,
    )
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
                <#ty as co3::ReprC>::CType: co3::borrow::BorrowCast + Copy
            ));
            predicates.push(parse_quote!(
                <<#ty as co3::ReprC>::CType as co3::borrow::BorrowCast>::AsConst: Copy
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
    let mut type_sig = ffi_fn::lower_abi_fn_signature(sig, failure_mode, move_fn);
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
