use std::collections::BTreeSet;

use syn::{Error, Result, Type};

use crate::{
    ForeignItem,
    dispatch::extract_dispatch_id,
    find_dispatch_attr, has_unsafe_export_name,
    utils::{dyn_dispatch_repr, has_non_lifetime_generics, is_drop_impl, is_type_erased},
};

const GENERICS_ERR: &str = "Type and const generics on impls are not supported. Use `#[dispatch]`";

fn unsupported_attr(attr: &syn::Attribute) -> Error {
    Error::new_spanned(attr, "Attribute not supported in this position")
}

fn push_error(errors: &mut Option<Error>, err: Error) {
    if let Some(errors) = errors {
        errors.combine(err);
    } else {
        *errors = Some(err);
    }
}

fn validate_export_fn_attrs(attrs: &[syn::Attribute]) -> Result<()> {
    for attr in attrs {
        if crate::generate::is_unsafe_no_mangle(attr) || has_unsafe_export_name(attr) {
            continue;
        }

        return Err(unsupported_attr(attr));
    }

    Ok(())
}

fn validate_no_dispatch_attrs(attrs: &[syn::Attribute], errors: &mut Option<Error>) {
    for attr in attrs {
        if attr.path().is_ident("dispatch") {
            let err_msg = "`#[dispatch]` is only supported on impl blocks`";
            push_error(errors, Error::new_spanned(attr, err_msg));
        }
    }
}

fn ensure_no_handle_arg_attrs(sig: &syn::Signature) -> Result<()> {
    let mut errors = None;

    for input in &sig.inputs {
        match input {
            syn::FnArg::Receiver(receiver) => {
                validate_no_dispatch_attrs(&receiver.attrs, &mut errors);
            }
            syn::FnArg::Typed(arg) => {
                validate_no_dispatch_attrs(&arg.attrs, &mut errors);
            }
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(())
}

pub(crate) fn validate_export_decls(decls: &[ForeignItem]) -> Result<()> {
    let mut errors = None;

    if let Err(err) = validate_shared(decls) {
        push_error(&mut errors, err);
    }

    for decl in decls {
        match decl {
            ForeignItem::Impl(impl_decl) => {
                for item in &impl_decl.items {
                    let syn::ImplItem::Fn(method) = item else {
                        continue;
                    };

                    if let Err(err) = validate_export_fn_attrs(&method.attrs) {
                        push_error(&mut errors, err);
                    }
                }
            }
            ForeignItem::Fn(decl_fn) => {
                if let Err(err) = validate_export_fn_attrs(&decl_fn.attrs) {
                    push_error(&mut errors, err);
                }
            }
            ForeignItem::Type(decl) => {
                for attr in &decl.ty.attrs {
                    if !attr.path().is_ident("id") {
                        push_error(&mut errors, unsupported_attr(attr));
                    }
                }
            }
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(())
}

pub(crate) fn validate_extern_decls(decls: &[ForeignItem]) -> Result<()> {
    let mut errors = None;

    if let Err(err) = validate_shared(decls) {
        push_error(&mut errors, err);
    }

    for decl in decls {
        let ForeignItem::Impl(impl_) = decl else {
            continue;
        };

        if !is_drop_impl(impl_) {
            continue;
        }

        let Some(attr) = find_dispatch_attr(&impl_.attrs) else {
            continue;
        };

        if !matches!(attr.meta, syn::Meta::Path(_)) {
            let err_msg = "extern declared `impl Drop` only supports bare `#[dispatch]`";
            push_error(&mut errors, Error::new_spanned(attr, err_msg));
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(())
}

fn validate_shared(decls: &[ForeignItem]) -> Result<()> {
    let mut errors = None;

    for decl in decls {
        match decl {
            ForeignItem::Type(decl) => {
                for attr in &decl.ty.attrs {
                    if attr.path().is_ident("dispatch") {
                        push_error(&mut errors, unsupported_attr(attr));
                    }
                }
            }
            ForeignItem::Fn(decl_fn) => {
                if let Err(err) = validate_signature_shape(&decl_fn.sig) {
                    push_error(&mut errors, err);
                }
                if let Err(err) = ensure_no_handle_arg_attrs(&decl_fn.sig) {
                    push_error(&mut errors, err);
                }

                validate_no_dispatch_attrs(&decl_fn.attrs, &mut errors);
            }
            ForeignItem::Impl(impl_) => {
                let is_dispatch_impl = find_dispatch_attr(&impl_.attrs).is_some();

                for attr in &impl_.attrs {
                    if !attr.path().is_ident("dispatch") {
                        push_error(&mut errors, unsupported_attr(attr));
                    }
                }

                if has_non_lifetime_generics(&impl_.generics) && !is_dispatch_impl {
                    push_error(
                        &mut errors,
                        Error::new_spanned(&impl_.generics, GENERICS_ERR),
                    );
                }

                if is_dispatch_impl
                    && let Err(err) = validate_dispatch_impl_generics(&impl_.generics)
                {
                    push_error(&mut errors, err);
                }

                for item in &impl_.items {
                    if let syn::ImplItem::Fn(method) = item {
                        if is_drop_impl(impl_)
                            && let Err(err) = validate_drop_method(method)
                        {
                            push_error(&mut errors, err);
                        }

                        if let Err(err) = ensure_no_handle_arg_attrs(&method.sig) {
                            push_error(&mut errors, err);
                        }
                        if let Err(err) = validate_signature_shape(&method.sig) {
                            push_error(&mut errors, err);
                        }
                        if is_dispatch_impl
                            && let Err(err) =
                                validate_dispatch_signature(
                                    &impl_.generics,
                                    &impl_.self_ty,
                                    &method.sig,
                                )
                        {
                            push_error(&mut errors, err);
                        }

                        validate_no_dispatch_attrs(&method.attrs, &mut errors);
                    }
                }
            }
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(())
}

fn validate_dispatch_impl_generics(generics: &syn::Generics) -> Result<()> {
    let mut errors = None;

    for param in &generics.params {
        let syn::GenericParam::Type(param) = param else {
            continue;
        };

        let Some(attr) = dyn_dispatch_attr(param) else {
            let err_msg = "`#[dispatch]` impl type parameters must use `dyn(repr) T`";
            push_error(&mut errors, Error::new_spanned(param, err_msg));
            continue;
        };

        let Ok(repr) = dyn_dispatch_repr(attr) else {
            continue;
        };

        if !is_allowed_dyn_dispatch_repr(&repr) {
            let err_msg = "`dyn(repr)` repr must be one of `u8`, `i8`, `u16`, `i16`, `u32`, `i32`, `u64`, or `i64`";
            push_error(&mut errors, Error::new_spanned(&repr, err_msg));
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(())
}

fn validate_dispatch_signature(
    generics: &syn::Generics,
    self_ty: &syn::Type,
    sig: &syn::Signature,
) -> Result<()> {
    let mut seen_dispatch_tys = BTreeSet::new();
    let mut errors = None;

    for input in &sig.inputs {
        let syn::FnArg::Typed(arg) = input else {
            continue;
        };

        let Some(param) = extract_dispatch_id(generics, self_ty, &arg.ty) else {
            continue;
        };

        if !seen_dispatch_tys.insert(&param.ident) {
            let err_msg = "duplicate handle ID";
            push_error(&mut errors, Error::new_spanned(&arg.ty, err_msg));
        }
    }

    if let syn::ReturnType::Type(_, output) = &sig.output
        && extract_dispatch_id(generics, self_ty, output).is_some()
    {
        push_error(
            &mut errors,
            Error::new_spanned(output, "handle IDs are not allowed in return position"),
        );
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(())
}

fn validate_drop_method(method: &syn::ImplItemFn) -> Result<()> {
    const ERR_MSG: &str = "`Drop` signature incorrect";

    fn is_mut_self_ty(ty: &Type) -> bool {
        matches!(ty, Type::Reference(reference) if reference.mutability.is_some())
    }

    fn is_self_id_ty(ty: &Type) -> bool {
        match ty {
            Type::Path(type_path) if type_path.qself.is_none() => {
                let first_seg = type_path.path.segments.first();
                let last_seg = type_path.path.segments.last();

                type_path.path.segments.len() == 2
                    && first_seg.is_some_and(|segment| segment.ident == "Self")
                    && last_seg.is_some_and(|segment| segment.ident == "ID")
            }
            Type::Path(type_path) => {
                let Some(qself) = &type_path.qself else {
                    return false;
                };
                let Type::Path(self_ty) = qself.ty.as_ref() else {
                    return false;
                };
                if self_ty.qself.is_some() || !self_ty.path.is_ident("Self") {
                    return false;
                }

                let last_seg = type_path.path.segments.last();
                if last_seg.is_none_or(|s| s.ident == "ID") {
                    return false;
                }

                let segments = type_path
                    .path
                    .segments
                    .iter()
                    .take(type_path.path.segments.len().saturating_sub(1))
                    .map(|segment| segment.ident.to_string())
                    .collect::<Vec<_>>();

                segments == ["Handle"] || segments == ["co3", "handle", "Handle"]
            }
            _ => false,
        }
    }

    if method.sig.ident != "drop" {
        return Err(Error::new_spanned(&method.sig.ident, ERR_MSG));
    }
    if !matches!(method.sig.output, syn::ReturnType::Default) {
        return Err(Error::new_spanned(&method.sig.output, ERR_MSG));
    }

    match &method.sig.inputs.iter().collect::<Vec<_>>()[..] {
        [syn::FnArg::Receiver(receiver)] if is_mut_self_ty(receiver.ty.as_ref()) => {
            return Ok(());
        }

        [syn::FnArg::Receiver(receiver), syn::FnArg::Typed(arg)]
        | [syn::FnArg::Typed(arg), syn::FnArg::Receiver(receiver)]
            if is_mut_self_ty(receiver.ty.as_ref()) && is_self_id_ty(arg.ty.as_ref()) =>
        {
            return Ok(());
        }

        _ => {}
    }

    Err(Error::new_spanned(&method.sig.inputs, ERR_MSG))
}

fn validate_signature_shape(sig: &syn::Signature) -> Result<()> {
    if has_non_lifetime_generics(&sig.generics) {
        return Err(Error::new_spanned(&sig.generics, GENERICS_ERR));
    }
    if let Some(asyncness) = sig.asyncness {
        return Err(Error::new_spanned(
            asyncness,
            "Async functions not supported",
        ));
    }
    if let Some(variadic) = &sig.variadic {
        return Err(Error::new_spanned(
            variadic,
            "Variadic arguments not supported",
        ));
    }
    for input in &sig.inputs {
        let syn::FnArg::Typed(arg) = input else {
            continue;
        };

        validate_pat_type_shape(arg)?;
    }
    Ok(())
}

fn validate_pat_type_shape(arg: &syn::PatType) -> Result<()> {
    let err_msg = "patterns aren't allowed in function declarations";

    match arg.pat.as_ref() {
        syn::Pat::Ident(ident) => {
            if ident.by_ref.is_some() && ident.mutability.is_some() && ident.subpat.is_some() {
                return Err(Error::new_spanned(ident, err_msg));
            }

            Ok(())
        }
        _ => Err(Error::new_spanned(&arg.pat, err_msg)),
    }
}

fn dyn_dispatch_attr(param: &syn::TypeParam) -> Option<&syn::Attribute> {
    param.attrs.iter().find(|attr| is_type_erased(attr))
}

fn is_allowed_dyn_dispatch_repr(ty: &Type) -> bool {
    let allowed_reprs = ["u8", "i8", "u16", "i16", "u32", "i32", "u64", "i64"];

    let Type::Path(type_path) = ty else {
        return false;
    };
    if type_path.qself.is_some() {
        return false;
    }

    let last_seg = type_path.path.segments.last();
    last_seg.is_some_and(|s| allowed_reprs.contains(&s.ident.to_string().as_str()))
}
