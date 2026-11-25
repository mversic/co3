use std::collections::HashMap;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{ItemFn, Result, parse_quote, visit::Visit, visit_mut::VisitMut};

use crate::{
    codec::{Boundary, Carrier, Conversion},
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
    lower_callback_fn_type(sig, &abi, failure_mode, false)
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
    lower_callback_fn_type(sig, &abi, failure_mode, move_fn)
}

struct RawTypeLowerer {
    resolved: HashMap<syn::Ident, syn::Type>,
    failure_mode: FailureMode,
    nested: bool,
    error: Option<syn::Error>,
}

impl RawTypeLowerer {
    fn resolve(&self, ty: &syn::Type) -> Option<&syn::Type> {
        let syn::Type::Path(ty) = ty else { return None };
        self.resolved.get(ty.path.get_ident()?)
    }

    fn visit_groups(&mut self, groups: &mut crate::DispatchGroups) {
        for target in groups.groups.values_mut().flatten() {
            self.visit_angle_bracketed_generic_arguments_mut(target);
        }
    }
}

impl VisitMut for RawTypeLowerer {
    fn visit_type_mut(&mut self, ty: &mut syn::Type) {
        if self.error.is_some() {
            return;
        }
        if let Some(callback) = self.resolve(ty) {
            *ty = callback.clone();
        } else if let syn::Type::FnPtr(pointer) = ty
            && (self.nested
                || pointer
                    .inputs
                    .iter()
                    .any(|arg| arg.attrs.iter().any(ffi_fn::is_unpack_attr)))
        {
            let previous = std::mem::replace(&mut self.nested, true);
            self.visit_type_fn_ptr_mut(pointer);
            self.nested = previous;
            if self.error.is_none() {
                match lower_fn_pointer_type(pointer, self.failure_mode) {
                    Ok(lowered) => *ty = lowered,
                    Err(error) => self.error = Some(error),
                }
            }
        } else {
            syn::visit_mut::visit_type_mut(self, ty);
        }
    }
}

pub(crate) fn lower_raw_types(
    signatures: Vec<(syn::Ident, syn::Signature, bool)>,
    items: &mut [parse::ParsedItem],
    failure_mode: FailureMode,
    block_abi: &syn::Abi,
) -> Result<()> {
    let mut lowerer = RawTypeLowerer {
        resolved: HashMap::new(),
        failure_mode,
        nested: true,
        error: None,
    };
    // Nested signatures precede every signature that uses them.
    for (slot, mut signature, moved) in signatures {
        lowerer.visit_signature_mut(&mut signature);
        if let Some(error) = lowerer.error.take() {
            return Err(error);
        }
        let ty = lower_raw_alias_signature(signature, moved, failure_mode, block_abi)?;
        lowerer.resolved.insert(slot, ty);
    }
    lowerer.nested = false;
    for item in items {
        match item {
            parse::ParsedItem::Fn(item) => {
                lowerer.visit_item_fn_mut(&mut item.item);
                lowerer.visit_groups(&mut item.dispatch_args);
            }
            parse::ParsedItem::Raw(item) => {
                lowerer.visit_signature_mut(&mut item.sig);
                lowerer.visit_groups(&mut item.dispatch_args);
            }
            parse::ParsedItem::Static(item) => {
                lowerer.visit_type_mut(&mut item.ty);
                if let Some(expr) = &mut item.expr {
                    lowerer.visit_expr_mut(expr);
                }
            }
            parse::ParsedItem::Impl(parsed) => {
                let item = &mut parsed.item;
                lowerer.visit_item_impl_mut(&mut item.item);
                lowerer.visit_groups(&mut item.dispatch_args);
                for groups in item.method_dispatch_args.values_mut() {
                    lowerer.visit_groups(groups);
                }
            }
            parse::ParsedItem::Type(item) => {
                lowerer.visit_foreign_item_type_mut(&mut item.ty);
                if let Some(ty) = &mut item.id {
                    lowerer.visit_type_mut(ty);
                }
                if let Some(expr) = &mut item.id_value {
                    lowerer.visit_expr_mut(expr);
                }
            }
            parse::ParsedItem::Alias(alias) => {
                alias.raw_root = lowerer.resolve(&alias.item.ty).is_some();
                lowerer.visit_item_type_mut(&mut alias.item);
            }
        }
    }
    lowerer.error.map_or(Ok(()), Err)
}

pub(crate) fn expand_companion(
    boundary: Boundary<'_>,
    item: &ItemFn,
    mut callee: syn::ExprPath,
    impl_generics: Option<&syn::Generics>,
) -> Result<TokenStream> {
    let contract = boundary.contract;
    let failure_mode = boundary.failure_mode;
    let fn_by_val = item.attrs.iter().any(ffi_fn::is_by_val_attr);
    let abi = parse_quote!(extern "C");
    let mut raw_sig = ffi_fn::lower_abi_fn_signature(item.sig.clone(), failure_mode, fn_by_val);
    raw_sig.safety = syn::Safety::Unsafe(Default::default());
    raw_sig.abi = Some(abi);
    raw_sig.ident = ffi_fn::raw_definition_name(&item.sig.ident);
    add_generic_companion_bounds(contract, &mut raw_sig, &item.sig, impl_generics, fn_by_val);
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
    if !generic_args.is_empty() {
        callee
            .path
            .segments
            .last_mut()
            .expect("companion callee")
            .arguments = syn::PathArguments::AngleBracketed(parse_quote!(::<#(#generic_args),*>));
    }
    let callee = syn::Expr::Path(callee);
    let signature_check = ffi_fn::gen_fn_signature_drift_check(item.sig.clone(), callee.clone());
    let body = ffi_fn::gen_definition_body(boundary, item.sig.clone(), quote!(#callee), fn_by_val);
    Ok(emit_companion(
        boundary,
        &item.attrs,
        &item.vis,
        raw_sig,
        signature_check,
        body,
        ffi_fn::borrows_return(&item.sig, fn_by_val),
    ))
}

pub(crate) fn emit_companion(
    boundary: Boundary<'_>,
    attrs: &[syn::Attribute],
    vis: &syn::Visibility,
    sig: syn::Signature,
    setup: TokenStream,
    body: TokenStream,
    borrow_output: bool,
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
        boundary,
        attrs,
        quote!(#vis),
        &sig,
        quote!(use #co3 as co3; #setup),
        body,
        borrow_output,
    )
}

fn add_generic_companion_bounds(
    contract: &Conversion,
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
        let mut lifetimes = Lifetimes(Vec::new());
        lifetimes.visit_type(ty);
        let decode_lifetime = (lifetimes.0.len() == 1).then(|| lifetimes.0.remove(0));
        let carrier = Carrier::for_borrow(
            ffi_fn::ownership_mode_for_arg(&arg.attrs, ty)
                != crate::generate::OwnershipMode::ByValue,
        );
        let store = (!soft_for_arg(&arg.attrs)).then(|| contract.empty_store());
        let fallback = parse_quote!('__co3_d);
        let lt = decode_lifetime.as_ref().unwrap_or(&fallback);
        let binder = decode_lifetime.is_none().then(|| quote!(for<#lt>));
        let outlives = (matches!(carrier, Carrier::Borrowed) && decode_lifetime.is_some())
            .then(|| quote!(+ #lt));
        let path = contract.bound(carrier, Some(lt), quote!(#store));
        predicates.push(parse_quote!(#binder #ty: #path #outlives));
    }
    if let syn::ReturnType::Type(_, ty) = &source_sig.output
        && detector.type_mentions_param(ty)
    {
        let borrowed =
            ffi_fn::ownership_mode(ty, fn_by_val) != crate::generate::OwnershipMode::ByValue;
        let store = contract.empty_store();
        let owner = borrowed.then(|| quote!(, Owner: co3::convert::EmptyStore));
        let path = contract.bound(Carrier::for_borrow(borrowed), None, quote!(#store #owner));
        predicates.push(parse_quote!(#ty: #path));
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
    let lifetimes = (!type_sig.generics.params.is_empty()).then(|| syn::BoundLifetimes {
        lifetimes: type_sig.generics.params,
        ..Default::default()
    });
    let inputs = type_sig
        .inputs
        .into_iter()
        .map(|input| {
            let syn::FnArg::Typed(arg) = input else {
                unreachable!("normalized callback signature has typed arguments")
            };
            syn::NamedArg {
                attrs: cfg_attrs(&arg.attrs).cloned().collect(),
                name: None,
                ty: *arg.ty,
            }
        })
        .collect();
    Ok(syn::Type::FnPtr(syn::TypeFnPtr {
        attrs: Vec::new(),
        lifetimes,
        unsafety: Some(Default::default()),
        abi: Some(abi.clone()),
        fn_token: type_sig.fn_token,
        paren_token: type_sig.paren_token,
        inputs,
        variadic: None,
        output: type_sig.output,
    }))
}
