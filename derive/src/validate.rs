use std::collections::BTreeSet;

use syn::{Error, Result, Type, visit::Visit};

use crate::{
    dispatch::HandleId,
    find_dispatch_attr, has_unsafe_export_name,
    parse::ParsedForeignItem,
    trait_object_single_trait_bound,
    utils::{has_non_lifetime_generics, is_drop_impl, is_type_erased, push_error},
};

const GENERICS_ERR: &str = "Type and const generics on impls are not supported. Use `#[dispatch]`";
const ALLOWED_REPRS: [&str; 8] = ["u8", "i8", "u16", "i16", "u32", "i32", "u64", "i64"];

fn unsupported_attr(attr: &syn::Attribute) -> Error {
    Error::new_spanned(attr, "Attribute not supported in this position")
}

fn handle_id<'a>(ty: &'a syn::Type, self_ty: &syn::Type) -> Option<HandleId<'a>> {
    if let Type::Path(ty) = ty
        && ty.qself.as_ref().is_some_and(|q| &*q.ty == self_ty)
    {
        return Some(HandleId::DynSelf);
    }

    if let Some(handle_id) = crate::dispatch::handle_id(ty) {
        return Some(handle_id);
    }

    None
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

pub(crate) fn validate_export_decls(decls: &[ParsedForeignItem]) -> Result<()> {
    let mut errors = None;

    if let Err(err) = validate_shared(decls) {
        push_error(&mut errors, err);
    }

    for decl in decls {
        match decl {
            ParsedForeignItem::Impl(impl_) => {
                for item in &impl_.items {
                    let syn::ImplItem::Fn(method) = item else {
                        continue;
                    };

                    if let Err(err) = validate_export_fn_attrs(&method.attrs) {
                        push_error(&mut errors, err);
                    }
                    if let Err(err) =
                        validate_export_dispatch_signature(&method.sig, &impl_.self_ty)
                    {
                        push_error(&mut errors, err);
                    }
                }
            }
            ParsedForeignItem::Fn(decl_fn) => {
                if let Err(err) = validate_export_fn_attrs(&decl_fn.attrs) {
                    push_error(&mut errors, err);
                }
            }
            ParsedForeignItem::Type(decl) => {
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

pub(crate) fn validate_extern_decls(decls: &[ParsedForeignItem]) -> Result<()> {
    let mut errors = None;

    if let Err(err) = validate_shared(decls) {
        push_error(&mut errors, err);
    }

    for decl in decls {
        let ParsedForeignItem::Impl(impl_) = decl else {
            continue;
        };

        for item in &impl_.items {
            let syn::ImplItem::Fn(method) = item else {
                continue;
            };

            if let Err(err) =
                validate_extern_dispatch_signature(&impl_.generics, &impl_.self_ty, &method.sig)
            {
                push_error(&mut errors, err);
            }
        }

        if is_drop_impl(impl_)
            && let Some(attr) = find_dispatch_attr(&impl_.attrs)
            && !matches!(attr.meta, syn::Meta::Path(_))
        {
            let err_msg = "extern declared `impl Drop` only supports bare `#[dispatch]`";
            push_error(&mut errors, Error::new_spanned(attr, err_msg));
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(())
}

fn validate_shared(decls: &[ParsedForeignItem]) -> Result<()> {
    let mut errors = None;

    for decl in decls {
        match decl {
            ParsedForeignItem::Type(decl) => {
                for attr in &decl.ty.attrs {
                    if attr.path().is_ident("dispatch") {
                        push_error(&mut errors, unsupported_attr(attr));
                    }
                }
            }
            ParsedForeignItem::Fn(decl_fn) => {
                if let Err(err) = ensure_no_handle_arg_attrs(&decl_fn.sig) {
                    push_error(&mut errors, err);
                }
                if let Err(err) = validate_signature_shape(&decl_fn.sig, None) {
                    push_error(&mut errors, err);
                }

                validate_no_dispatch_attrs(&decl_fn.attrs, &mut errors);
            }
            ParsedForeignItem::Impl(impl_) => {
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

                if is_dispatch_impl
                    && let Err(err) =
                        validate_dispatch_impl_targets(&impl_.generics, &impl_.self_ty)
                {
                    push_error(&mut errors, err);
                }

                if is_dispatch_impl
                    && let Err(err) = validate_dispatched_self_ty(&impl_.generics, &impl_.self_ty)
                {
                    push_error(&mut errors, err);
                }

                for item in &impl_.items {
                    if let syn::ImplItem::Fn(method) = item {
                        if let Err(err) = ensure_no_handle_arg_attrs(&method.sig) {
                            push_error(&mut errors, err);
                        }
                        if let Err(err) =
                            validate_signature_shape(&method.sig, Some(&impl_.self_ty))
                        {
                            push_error(&mut errors, err);
                        }

                        validate_no_dispatch_attrs(&method.attrs, &mut errors);
                    }
                }

                if is_drop_impl(impl_)
                    && let Err(err) = validate_drop_impl(impl_)
                {
                    push_error(&mut errors, err);
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
            continue;
        };

        if matches!(attr.meta, syn::Meta::Path(_)) {
            continue;
        }

        let Ok(repr) = attr.parse_args() else {
            continue;
        };

        if !is_allowed_dyn_dispatch_repr(&repr) {
            let err_msg = format!(
                "`dyn(repr)` repr must be one of {}",
                ALLOWED_REPRS.join(", ")
            );
            push_error(&mut errors, Error::new_spanned(&repr, err_msg));
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(())
}

fn validate_dispatch_impl_targets(generics: &syn::Generics, self_ty: &syn::Type) -> Result<()> {
    let err_msg = "`#[dispatch]` requires at least one `dyn Type` or `dyn Self`";

    let mut type_params = generics.type_params();
    if type_params.any(|p| p.attrs.iter().any(is_type_erased)) {
        return Ok(());
    }
    if matches!(self_ty, syn::Type::TraitObject(_)) {
        return Ok(());
    }

    Err(Error::new_spanned(self_ty, err_msg))
}

fn validate_export_dispatch_signature(sig: &syn::Signature, self_ty: &syn::Type) -> Result<()> {
    let err_msg = "explicit `<dyn Type>::ID` is only supported in extern declarations";

    let mut errors = None;
    for input in &sig.inputs {
        let syn::FnArg::Typed(arg) = input else {
            continue;
        };

        if handle_id(&arg.ty, self_ty).is_some() {
            push_error(&mut errors, Error::new_spanned(&arg.ty, err_msg));
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(())
}

fn validate_extern_dispatch_signature(
    generics: &syn::Generics,
    self_ty: &syn::Type,
    sig: &syn::Signature,
) -> Result<()> {
    let mut handle_ids = BTreeSet::new();

    let handle_tys = generics
        .type_params()
        .filter(|param| param.attrs.iter().any(is_type_erased))
        .map(|param| &param.ident)
        .collect::<BTreeSet<_>>();

    let mut errors = None;
    for input in &sig.inputs {
        let syn::FnArg::Typed(arg) = input else {
            continue;
        };
        let Some(handle_id) = handle_id(&arg.ty, self_ty) else {
            continue;
        };

        let is_dyn_param = match handle_id {
            HandleId::DynType(ident) => handle_tys.contains(ident),
            HandleId::DynSelf => true,
        };

        if !is_dyn_param {
            let err_msg = "`<dyn Type>::ID` arg must target declared `dyn Type`";
            push_error(&mut errors, Error::new_spanned(&arg.ty, err_msg));
            continue;
        }

        if !handle_ids.insert(handle_id) {
            let err_msg = "duplicate `<dyn Type>::ID`";
            push_error(&mut errors, Error::new_spanned(&arg.ty, err_msg));
        }
    }

    let handle_ids = handle_ids
        .iter()
        .filter_map(|handle_id| match handle_id {
            HandleId::DynType(ident) => Some(*ident),
            HandleId::DynSelf => None,
        })
        .collect::<BTreeSet<_>>();

    if handle_ids.len() != handle_tys.len() {
        let missing = handle_tys
            .difference(&handle_ids)
            .map(|ident| format!("`<dyn {ident}>::ID`"))
            .collect::<Vec<_>>();

        let err_msg = format!("missing explicit handle IDs: {}", missing.join(", "));
        push_error(&mut errors, Error::new_spanned(&sig.inputs, err_msg));
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(())
}

fn validate_drop_impl(impl_: &syn::ItemImpl) -> Result<()> {
    const UNKNOWN_METHOD: &str = "`Drop` must have exactly one method `drop`";

    fn is_mut_self_ty(ty: &Type) -> bool {
        matches!(ty, Type::Reference(reference) if reference.mutability.is_some())
    }

    let mut items = impl_.items.iter();
    let Some(item) = items.next() else {
        return Err(Error::new_spanned(impl_, UNKNOWN_METHOD));
    };
    if let Some(item) = items.next() {
        return Err(Error::new_spanned(item, UNKNOWN_METHOD));
    }
    let syn::ImplItem::Fn(method) = item else {
        return Err(Error::new_spanned(item, UNKNOWN_METHOD));
    };
    if method.sig.ident != "drop" {
        return Err(Error::new_spanned(&method.sig.ident, UNKNOWN_METHOD));
    }
    if !matches!(method.sig.output, syn::ReturnType::Default) {
        let err_msg = "`Drop::drop` must have no return type";
        return Err(Error::new_spanned(&method.sig.output, err_msg));
    }

    let mut was_receiver = false;
    for input in &method.sig.inputs {
        match input {
            syn::FnArg::Typed(arg) if handle_id(&arg.ty, &impl_.self_ty).is_some() => {}
            syn::FnArg::Receiver(receiver) if is_mut_self_ty(receiver.ty.as_ref()) => {
                if was_receiver {
                    let err_msg = "`Drop::drop` can have only one receiver argument `&mut self`";
                    return Err(Error::new_spanned(&method.sig.inputs, err_msg));
                }

                was_receiver = true;
            }
            _ => {
                let err_msg = "`Drop::drop` supports only `&mut self` and optionally a handle ID";
                return Err(Error::new_spanned(&method.sig.inputs, err_msg));
            }
        }
    }

    Ok(())
}

fn validate_signature_shape(sig: &syn::Signature, self_ty: Option<&syn::Type>) -> Result<()> {
    if has_non_lifetime_generics(&sig.generics) {
        return Err(Error::new_spanned(&sig.generics, GENERICS_ERR));
    }
    if let Some(asyncness) = sig.asyncness {
        let err_msg = "Async functions are not supported";
        return Err(Error::new_spanned(asyncness, err_msg));
    }
    if let Some(variadic) = &sig.variadic {
        let err_msg = "Variadic arguments are not supported";
        return Err(Error::new_spanned(variadic, err_msg));
    }
    for input in &sig.inputs {
        let syn::FnArg::Typed(arg) = input else {
            continue;
        };

        validate_pat_type_shape(arg)?;
        if let Some(self_ty) = self_ty {
            validate_handle_id_pos(&arg.ty, self_ty)?;
        }
    }

    if let syn::ReturnType::Type(_, output) = &sig.output
        && let Some(self_ty) = self_ty
    {
        if handle_id(output, self_ty).is_some() {
            let err_msg = "`<dyn Type>::ID` is not allowed in return position";
            return Err(Error::new_spanned(output, err_msg));
        }

        validate_handle_id_pos(output, self_ty)?;
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

fn validate_handle_id_pos(ty: &Type, self_ty: &syn::Type) -> Result<()> {
    struct NestedHandleIdVisitor<'a> {
        errors: Option<Error>,
        self_ty: &'a syn::Type,
        depth: usize,
    }

    impl Visit<'_> for NestedHandleIdVisitor<'_> {
        fn visit_type(&mut self, node: &Type) {
            if self.depth != 0 && handle_id(node, self.self_ty).is_some() {
                let err_msg = "`<dyn Type>::ID` is only allowed as a top-level function argument";
                push_error(&mut self.errors, Error::new_spanned(node, err_msg));
                return;
            }

            self.depth += 1;
            syn::visit::visit_type(self, node);
            self.depth -= 1;
        }
    }

    let mut visitor = NestedHandleIdVisitor {
        errors: None,
        depth: 0,
        self_ty,
    };

    visitor.visit_type(ty);
    if let Some(errors) = visitor.errors {
        return Err(errors);
    }

    Ok(())
}

fn dyn_dispatch_attr(param: &syn::TypeParam) -> Option<&syn::Attribute> {
    param.attrs.iter().find(|attr| is_type_erased(attr))
}

fn is_allowed_dyn_dispatch_repr(ty: &Type) -> bool {
    let Type::Path(type_path) = ty else {
        return false;
    };
    if type_path.qself.is_some() {
        return false;
    }

    let last_seg = type_path.path.segments.last();
    last_seg.is_some_and(|s| ALLOWED_REPRS.contains(&s.ident.to_string().as_str()))
}

fn validate_dispatched_self_ty(generics: &syn::Generics, self_ty: &syn::Type) -> Result<()> {
    struct TypeParamUseVisitor<'a> {
        type_params: BTreeSet<&'a syn::Ident>,
        found: bool,
    }

    impl Visit<'_> for TypeParamUseVisitor<'_> {
        fn visit_type_path(&mut self, node: &syn::TypePath) {
            if node.qself.is_none()
                && let Some(ident) = node.path.get_ident()
                && self.type_params.contains(ident)
            {
                self.found = true;
                return;
            }

            syn::visit::visit_type_path(self, node);
        }
    }

    let err_msg = "`dyn Self` is only supported for declared types";
    let Some(trait_bound) = trait_object_single_trait_bound(self_ty) else {
        return Ok(());
    };

    let type_params = generics
        .type_params()
        .map(|param| &param.ident)
        .collect::<BTreeSet<_>>();

    if trait_bound.path.segments.len() > 1 {
        return Err(Error::new_spanned(self_ty, err_msg));
    }

    if let Some(ident) = trait_bound.path.get_ident()
        && type_params.contains(ident)
    {
        return Err(Error::new_spanned(self_ty, err_msg));
    }

    let mut visitor = TypeParamUseVisitor {
        type_params,
        found: false,
    };

    visitor.visit_trait_bound(trait_bound);

    if visitor.found {
        return Ok(());
    }

    let err_msg = "`dyn Self` is only supported on generic `Self`";
    Err(Error::new_spanned(self_ty, err_msg))
}

/// Returns error if any method is missing explicit `<dyn Self>::ID`
pub(crate) fn validate_dispatch_self_id(impl_: &syn::ItemImpl) -> Result<()> {
    let mut errors = None;

    for item in &impl_.items {
        let syn::ImplItem::Fn(method) = item else {
            continue;
        };

        let explicit_ids = method
            .sig
            .inputs
            .iter()
            .filter_map(|input| {
                let syn::FnArg::Typed(arg) = input else {
                    return None;
                };

                crate::dispatch::handle_id(&arg.ty)
            })
            .collect::<Vec<_>>();

        let has_self_id = explicit_ids
            .iter()
            .any(|id| matches!(id, HandleId::DynSelf));

        if !has_self_id {
            let err_msg = "missing explicit handle ID: `<dyn Self>::ID`";
            let err = syn::Error::new_spanned(&method.sig.inputs, err_msg);

            push_error(&mut errors, err);
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_dispatched_self_without_type_param_in_generic_args() {
        let generics: syn::Generics = syn::parse_quote!(<T>);
        let self_ty: syn::Type = syn::parse_quote!(dyn Opaque<u8>);

        let err = validate_dispatched_self_ty(&generics, &self_ty).unwrap_err();
        let msg = "`dyn Self` is only supported on generic `Self`";
        assert!(err.to_string().contains(msg));
    }

    #[test]
    fn accepts_dispatched_self_with_type_param_in_generic_args() {
        let generics: syn::Generics = syn::parse_quote!(<T>);
        let self_ty: syn::Type = syn::parse_quote!(dyn Opaque<T>);
        validate_dispatched_self_ty(&generics, &self_ty).unwrap();
    }

    #[test]
    fn rejects_dispatched_self_type_param() {
        let generics: syn::Generics = syn::parse_quote!(<T>);
        let self_ty: syn::Type = syn::parse_quote!(dyn T);

        let err = validate_dispatched_self_ty(&generics, &self_ty).unwrap_err();
        let msg = "`dyn Self` is only supported for declared types";
        assert!(err.to_string().contains(msg));
    }
}
