use std::collections::{BTreeSet, HashMap};

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{ItemFn, Result, parse_quote, visit::Visit, visit_mut::VisitMut};

use crate::{
    codec::{Boundary, Carrier, Conversion},
    ffi_fn,
    parse::{self, FailureMode},
    utils::{ParamUseDetector, cfg_attrs, co3_path, soft_for_arg},
};

fn lower_fn_pointer_type(
    pointer: &syn::TypeFnPtr,
    names: &mut BTreeSet<String>,
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
    lower_callback_fn_type(sig, &abi, failure_mode, false, names)
}

fn lower_raw_alias_signature(
    mut sig: syn::Signature,
    move_fn: bool,
    failure_mode: FailureMode,
    block_abi: &syn::Abi,
    names: &mut BTreeSet<String>,
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
    lower_callback_fn_type(sig, &abi, failure_mode, move_fn, names)
}

struct RawTypeLowerer<'a> {
    signatures: HashMap<syn::Ident, (syn::Signature, bool)>,
    block_abi: &'a syn::Abi,
    source_names: BTreeSet<String>,
    failure_mode: FailureMode,
    nested: bool,
    error: Option<syn::Error>,
}

impl RawTypeLowerer<'_> {
    fn signature(&self, ty: &syn::Type) -> Option<&(syn::Signature, bool)> {
        let syn::Type::Path(ty) = ty else { return None };
        self.signatures.get(ty.path.get_ident()?)
    }

    fn visit_groups(&mut self, groups: &mut crate::DispatchGroups) {
        for target in groups.groups.values_mut().flatten() {
            self.visit_angle_bracketed_generic_arguments_mut(target);
        }
    }
}

impl VisitMut for RawTypeLowerer<'_> {
    fn visit_type_mut(&mut self, ty: &mut syn::Type) {
        if self.error.is_some() {
            return;
        }
        if let Some((mut signature, moved)) = self.signature(ty).cloned() {
            let previous = std::mem::replace(&mut self.nested, true);
            self.visit_signature_mut(&mut signature);
            self.nested = previous;
            match lower_raw_alias_signature(
                signature,
                moved,
                self.failure_mode,
                self.block_abi,
                &mut self.source_names,
            ) {
                Ok(lowered) => *ty = lowered,
                Err(error) => self.error = Some(error),
            }
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
                match lower_fn_pointer_type(pointer, &mut self.source_names, self.failure_mode) {
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
    source_names: BTreeSet<String>,
    items: &mut [parse::ParsedItem],
    failure_mode: FailureMode,
    block_abi: &syn::Abi,
) -> Result<()> {
    let mut lowerer = RawTypeLowerer {
        signatures: signatures
            .into_iter()
            .map(|(slot, sig, moved)| (slot, (sig, moved)))
            .collect(),
        block_abi,
        source_names,
        failure_mode,
        nested: false,
        error: None,
    };
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
                alias.raw_root = lowerer.signature(&alias.item.ty).is_some();
                lowerer.visit_item_type_mut(&mut alias.item);
            }
        }
    }
    lowerer.error.map_or(Ok(()), Err)
}

/// Higher-ranked function pointers cannot use the blanket, monomorphic function
/// pointer codec. Their ABI representation is the same nullable pointer.
pub(crate) struct BoundCallback<'a> {
    pub(crate) pointer: &'a syn::TypeFnPtr,
    pub(crate) optional: bool,
}

impl<'a> BoundCallback<'a> {
    pub(crate) fn from_type(ty: &'a syn::Type) -> Option<Self> {
        let ty = crate::utils::peel_grouped_type(ty);
        let inner = ffi_fn::option_inner_type(ty);
        let syn::Type::FnPtr(pointer) = crate::utils::peel_grouped_type(inner.unwrap_or(ty)) else {
            return None;
        };
        (pointer.lifetimes.is_some() && pointer.abi.is_some()).then_some(Self {
            pointer,
            optional: inner.is_some(),
        })
    }

    pub(crate) fn abi_ty(&self) -> syn::Type {
        let pointer = self.pointer;
        parse_quote!(::core::option::Option<#pointer>)
    }

    pub(crate) fn encode(&self, value: TokenStream) -> TokenStream {
        if self.optional {
            value
        } else {
            quote!(::core::option::Option::Some(#value))
        }
    }

    pub(crate) fn decode(&self, value: TokenStream) -> TokenStream {
        if self.optional {
            quote!(::core::option::Option::Some(#value))
        } else {
            value
        }
    }
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
    let mut source = item.sig.clone();
    ffi_fn::explicitize_signature_lifetimes(&mut source);
    let mut raw_sig = lower_callback_signature(&source, failure_mode, fn_by_val);
    raw_sig.safety = syn::Safety::Unsafe(Default::default());
    raw_sig.abi = Some(abi);
    raw_sig.ident = ffi_fn::raw_definition_name(&item.sig.ident);
    add_generic_companion_bounds(contract, &mut raw_sig, &source, impl_generics, fn_by_val);
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

// Rust requires a lifetime used by a function pointer's return type to occur
// outside an associated-type projection in at least one input (E0581).
fn borrowed_callback_input(ty: &syn::Type) -> Option<syn::Type> {
    use crate::known_type::{Kind, recognize};
    let syn::Type::Reference(reference) = ty else {
        return None;
    };
    let elem = &reference.elem;
    let lifetime = reference.lifetime.as_ref()?;
    let primitive = |ty: &syn::Type| recognize(ty).is_some_and(|ty| ty.kind == Kind::Primitive);
    let wide = match elem.as_ref() {
        syn::Type::Slice(slice) if primitive(&slice.elem) => true,
        ty if recognize(ty).is_some_and(|ty| ty.kind == Kind::Str) => true,
        ty if primitive(ty) => false,
        _ => return None,
    };
    // A mutable reference to a type with validity constraints uses a soft carrier.
    if reference.mutability.is_some() {
        let pointee = match elem.as_ref() {
            syn::Type::Slice(slice) => &*slice.elem,
            ty => ty,
        };
        if !ffi_fn::is_identity_primitive(pointee) {
            return None;
        }
    }
    let carrier: syn::Path = match (wide, reference.mutability.is_some()) {
        (true, false) => parse_quote!(co3::wide::abi::WideRef),
        (true, true) => parse_quote!(co3::wide::abi::WideRefMut),
        (false, false) => parse_quote!(co3::reference::abi::Ref),
        (false, true) => parse_quote!(co3::reference::abi::RefMut),
    };
    Some(parse_quote!(#carrier<#lifetime, <#elem as co3::ReprC>::CType>))
}

fn lower_callback_signature(
    sig: &syn::Signature,
    failure_mode: FailureMode,
    move_fn: bool,
) -> syn::Signature {
    let mut lowered = ffi_fn::lower_abi_fn_signature(sig.clone(), failure_mode, move_fn);
    let output_names = crate::utils::source_idents(quote::ToTokens::to_token_stream(&sig.output));
    for arg in &sig.inputs {
        let syn::FnArg::Typed(arg) = arg else {
            continue;
        };
        let source = &arg.ty;
        let syn::Type::Reference(reference) = &**source else {
            continue;
        };
        if arg.attrs.iter().any(ffi_fn::is_unpack_attr) {
            continue;
        }
        let Some(syn::FnArg::Typed(input)) = lowered
            .inputs
            .iter_mut()
            .find(|input| matches!(input, syn::FnArg::Typed(input) if input.pat == arg.pat))
        else {
            continue;
        };
        let returned = reference
            .lifetime
            .as_ref()
            .is_some_and(|lt| output_names.contains(&lt.ident.to_string()));
        if !returned {
            continue;
        }
        if let Some(ty) = borrowed_callback_input(source) {
            *input.ty = ty;
        }
    }
    lowered
}

fn lower_callback_fn_type(
    mut sig: syn::Signature,
    abi: &syn::Abi,
    failure_mode: FailureMode,
    move_fn: bool,
    names: &mut BTreeSet<String>,
) -> Result<syn::Type> {
    ffi_fn::explicitize_lifetimes(&mut sig, names, "__co3_callback_type_");
    let type_sig = lower_callback_signature(&sig, failure_mode, move_fn);
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
