use std::collections::{BTreeMap, BTreeSet};

use quote::ToTokens;
use syn::{Attribute, Error, Expr, Result, Type, visit::Visit};

use crate::{
    Co3Static,
    dispatch::{TagId, tag_id},
    ffi_fn::{is_by_val_attr, is_unpack_attr, unpack_attr_name, unpack_types},
    is_symbol_name_attr,
    parse::validate_symbol_text,
    symbol_name_value,
    utils::{
        has_non_lifetime_generics, has_runtime_dispatch, is_drop_impl, is_payload_erased,
        is_type_erased, push_error, soft_for_arg,
    },
};

const UNUSED_GENERIC_ERR: &str = "unused generic parameter";
const GENERIC_USE_PREDICATE_ERR: &str = "unconstrained generic parameter";
const DISPATCH_OPAQUE_NESTING_ERR: &str =
    "ABI-erased type parameters cannot be used through named wrappers or associated projections";
const STATIC_DEFAULT_ERR: &str = "type defaults are only supported on `dyn(Tag) T` parameters";
const OPEN_PAYLOAD_ERASURE_ERR: &str =
    "ABI-erased runtime parameters must be constrained by a `use` predicate";
const PARAMETERIZED_DROP_DECLARATION_ERR: &str =
    "parameterized types require an explicit Drop declaration";

pub(crate) fn validate_export_decls(items: &[crate::ForeignItem]) -> Result<()> {
    let declared_types = crate::declared_foreign_type_idents(items);
    let mut errors = validate_shared(items, false).err();
    if let Err(err) = validate_export_type_ids(items) {
        push_error(&mut errors, err);
    }
    if let Err(err) = validate_export_parameter_bounds(items, &declared_types) {
        push_error(&mut errors, err);
    }
    if let Err(err) = validate_parameterized_drop_declarations(items) {
        push_error(&mut errors, err);
    }
    if let Err(err) = validate_export_parameterized_drop_declarations(items) {
        push_error(&mut errors, err);
    }
    for item in items {
        let result = match item {
            crate::ForeignItem::Type(item) => validate_export_type(item),
            crate::ForeignItem::Static(item) => validate_export_static(item),
            crate::ForeignItem::Fn(item) => validate_export_fn(item),
            crate::ForeignItem::Impl(_) => Ok(()),
        };
        if let Err(err) = result {
            push_error(&mut errors, err);
        }
    }
    if let Err(err) = validate_impls(items, |impl_| validate_export_impl(impl_, &declared_types)) {
        push_error(&mut errors, err);
    }
    errors.map_or(Ok(()), Err)
}

pub(crate) fn validate_extern_decls(items: &[crate::ForeignItem]) -> Result<()> {
    let mut errors = None;
    if let Err(err) = validate_shared(items, true) {
        push_error(&mut errors, err);
    }
    if let Err(err) = validate_parameterized_drop_declarations(items) {
        push_error(&mut errors, err);
    }
    for item in items {
        let result = match item {
            crate::ForeignItem::Type(_) => Ok(()),
            crate::ForeignItem::Static(item) => validate_extern_static(item),
            crate::ForeignItem::Fn(item) => validate_extern_fn(item),
            crate::ForeignItem::Impl(_) => Ok(()),
        };
        if let Err(err) = result {
            push_error(&mut errors, err);
        }
    }
    if let Err(err) = validate_impls(items, validate_extern_impl) {
        push_error(&mut errors, err);
    }
    errors.map_or(Ok(()), Err)
}

fn validate_export_type_ids(items: &[crate::ForeignItem]) -> Result<()> {
    let mut errors = None;
    for item in items {
        let crate::ForeignItem::Type(item) = item else {
            continue;
        };
        if has_non_lifetime_generics(&item.ty.generics)
            && let Some(value) = &item.id_value
        {
            let message = "parameterized exported types must not specify a tag value";
            push_error(&mut errors, Error::new_spanned(value, message));
        }
    }
    errors.map_or(Ok(()), Err)
}

fn validate_parameterized_drop_declarations(items: &[crate::ForeignItem]) -> Result<()> {
    let selected_drop_types = crate::selected_drop_types(items);
    let mut errors = None;

    for item in items {
        let crate::ForeignItem::Type(item) = item else {
            continue;
        };
        if !has_non_lifetime_generics(&item.ty.generics) {
            continue;
        }

        if item.drop.is_none() && !selected_drop_types.contains(&item.ty.ident) {
            push_error(
                &mut errors,
                Error::new_spanned(&item.ty, PARAMETERIZED_DROP_DECLARATION_ERR),
            );
        }
    }

    errors.map_or(Ok(()), Err)
}

fn validate_export_parameterized_drop_declarations(items: &[crate::ForeignItem]) -> Result<()> {
    let mut errors = None;
    for item in items {
        let crate::ForeignItem::Type(item) = item else {
            continue;
        };
        if !has_non_lifetime_generics(&item.ty.generics) {
            continue;
        }
        let Some(drop) = &item.drop else {
            continue;
        };
        if crate::trait_object_single_trait_bound(&drop.self_ty).is_none() {
            let message = "Drop for a parameterized exported type must use `dyn Self`";
            push_error(&mut errors, Error::new_spanned(&drop.self_ty, message));
        }
    }
    errors.map_or(Ok(()), Err)
}

fn validate_symbol_on_fn(attrs: &[Attribute], static_params: Vec<syn::Ident>) -> Result<()> {
    validate_symbol_name_attrs(attrs)?;
    validate_static_symbol_interpolations(attrs, &static_params)
}

fn validate_symbol_names(
    items: &[crate::ForeignItem],
    declared_types: &BTreeSet<syn::Ident>,
) -> Result<()> {
    let mut errors = None;
    for item in items {
        let validate_impl = |impl_: &crate::Co3Impl, errors: &mut Option<Error>| {
            if let Err(err) = validate_symbol_name_attrs(&impl_.attrs) {
                push_error(errors, err);
            }
            for item in &impl_.items {
                let syn::ImplItem::Fn(method) = item else {
                    continue;
                };
                if let Err(err) = validate_symbol_on_fn(
                    &method.attrs,
                    impl_method_symbol_binding_params(impl_, method, declared_types),
                ) {
                    push_error(errors, err);
                }
            }
        };

        match item {
            crate::ForeignItem::Type(item) => {
                if let Err(err) = validate_symbol_name_attrs(&item.ty.attrs) {
                    push_error(&mut errors, err);
                }
                for impl_ in &item.self_impls {
                    validate_impl(impl_, &mut errors);
                }
                if let Some(drop) = &item.drop {
                    validate_impl(drop, &mut errors);
                }
            }
            crate::ForeignItem::Static(item) => {
                if let Err(err) = validate_symbol_name_attrs(&item.attrs) {
                    push_error(&mut errors, err);
                }
            }
            crate::ForeignItem::Fn(item) => {
                if let Err(err) = validate_symbol_on_fn(
                    &item.attrs,
                    fn_symbol_binding_params(item, declared_types),
                ) {
                    push_error(&mut errors, err);
                }
            }
            crate::ForeignItem::Impl(impl_) => validate_impl(impl_, &mut errors),
        }
    }
    errors.map_or(Ok(()), Err)
}

pub(crate) fn fn_symbol_binding_params(
    item: &crate::Co3Fn,
    declared_types: &BTreeSet<syn::Ident>,
) -> Vec<syn::Ident> {
    symbol_binding_params(
        [(&item.sig.generics, false)],
        [&item.dispatch_args],
        declared_types,
        false,
        |uses| visit_signature_positions(uses, &item.sig),
    )
}

pub(crate) fn fn_static_binding_params(
    item: &crate::Co3Fn,
    declared_types: &BTreeSet<syn::Ident>,
) -> Vec<syn::Ident> {
    symbol_binding_params(
        [(&item.sig.generics, false)],
        [&item.dispatch_args],
        declared_types,
        true,
        |uses| visit_signature_positions(uses, &item.sig),
    )
}

pub(crate) fn impl_method_symbol_binding_params(
    impl_: &crate::Co3Impl,
    method: &syn::ImplItemFn,
    declared_types: &BTreeSet<syn::Ident>,
) -> Vec<syn::Ident> {
    let dispatch = impl_
        .method_dispatch_args
        .get(&method.sig.ident)
        .cloned()
        .unwrap_or_default();
    symbol_binding_params(
        [(&impl_.generics, true), (&method.sig.generics, false)],
        [&impl_.dispatch_args, &dispatch],
        declared_types,
        false,
        |uses| {
            visit_signature_positions(uses, &method.sig);
            uses.visit_type(&impl_.self_ty);
        },
    )
}

pub(crate) fn impl_method_static_binding_params(
    impl_: &crate::Co3Impl,
    method: &syn::ImplItemFn,
    declared_types: &BTreeSet<syn::Ident>,
) -> Vec<syn::Ident> {
    let dispatch = impl_
        .method_dispatch_args
        .get(&method.sig.ident)
        .cloned()
        .unwrap_or_default();
    symbol_binding_params(
        [(&impl_.generics, true), (&method.sig.generics, false)],
        [&impl_.dispatch_args, &dispatch],
        declared_types,
        true,
        |uses| {
            visit_signature_positions(uses, &method.sig);
            uses.visit_type(&impl_.self_ty);
        },
    )
}

fn symbol_binding_params<'a>(
    generic_scopes: impl IntoIterator<Item = (&'a syn::Generics, bool)>,
    dispatch_scopes: impl IntoIterator<Item = &'a crate::DispatchGroups> + Clone,
    declared_types: &BTreeSet<syn::Ident>,
    include_direct_erased: bool,
    visit_positions: impl Fn(&mut SymbolUseDetector<'_>),
) -> Vec<syn::Ident> {
    let generic_scopes = generic_scopes.into_iter().collect::<Vec<_>>();
    let dispatch_scopes = dispatch_scopes.into_iter().collect::<Vec<_>>();
    generic_scopes
        .iter()
        .flat_map(|(generics, outer)| {
            generics.params.iter().filter_map(|param| {
                let (ident, dynamic) = match param {
                    syn::GenericParam::Type(param) => {
                        (&param.ident, param.attrs.iter().any(is_type_erased))
                    }
                    syn::GenericParam::Const(param) => (&param.ident, false),
                    syn::GenericParam::Lifetime(_) => return None,
                };
                if dynamic
                    || !dispatch_scopes
                        .iter()
                        .any(|scope| scope.contains_param(ident))
                {
                    return None;
                }
                if matches!(param, syn::GenericParam::Const(_)) {
                    return Some(ident.clone());
                }

                let direct_selection_is_erased = dispatch_scopes
                    .iter()
                    .find_map(|scope| direct_selection_types(ident, scope))
                    .is_some_and(|types| {
                        types
                            .into_iter()
                            .all(|ty| is_direct_declared_type(ty, declared_types))
                    });
                let mut uses = SymbolUseDetector {
                    param: ident,
                    declared_types,
                    direct_selection_is_erased,
                    inside_wrapper: false,
                    seen: false,
                    exposed: false,
                };
                visit_positions(&mut uses);
                uses.visit_dispatch_targets(&generic_scopes, &dispatch_scopes);
                (uses.exposed
                    || (include_direct_erased && direct_selection_is_erased)
                    || (!*outer && !uses.seen))
                    .then(|| ident.clone())
            })
        })
        .collect()
}

fn direct_selection_types<'a>(
    param: &syn::Ident,
    dispatch: &'a crate::DispatchGroups,
) -> Option<Vec<&'a syn::Type>> {
    let (params, targets) = dispatch
        .groups()
        .find(|(params, _)| params.contains(param))?;
    let index = params.iter().position(|candidate| candidate == param)?;
    targets
        .iter()
        .map(|target| match target.args.get(index) {
            Some(syn::GenericArgument::Type(ty)) => Some(ty),
            _ => None,
        })
        .collect()
}

struct SymbolUseDetector<'a> {
    param: &'a syn::Ident,
    declared_types: &'a BTreeSet<syn::Ident>,
    direct_selection_is_erased: bool,
    inside_wrapper: bool,
    seen: bool,
    exposed: bool,
}

impl SymbolUseDetector<'_> {
    fn visit_dispatch_targets(
        &mut self,
        generic_scopes: &[(&syn::Generics, bool)],
        dispatch_scopes: &[&crate::DispatchGroups],
    ) {
        for dispatch in dispatch_scopes {
            for (owners, targets) in dispatch.groups() {
                for (index, owner) in owners.iter().enumerate() {
                    let payloadless = generic_scopes.iter().any(|(generics, _)| {
                        generics.type_params().any(|candidate| {
                            candidate.ident == *owner
                                && candidate.attrs.iter().any(is_type_erased)
                                && candidate.default.is_none()
                        })
                    });
                    for target in targets {
                        let Some(argument) = target.args.get(index) else {
                            continue;
                        };
                        if payloadless {
                            if crate::utils::ParamUseDetector::new([self.param])
                                .generic_arg_mentions_param(argument)
                            {
                                self.seen = true;
                            }
                        } else {
                            self.visit_generic_argument(argument);
                        }
                    }
                }
            }
        }
    }
}

impl Visit<'_> for SymbolUseDetector<'_> {
    fn visit_path(&mut self, path: &syn::Path) {
        if path.segments.len() == 1 && self.declared_types.contains(&path.segments[0].ident) {
            if crate::utils::ParamUseDetector::new([self.param]).path_mentions_param(path) {
                self.seen = true;
            }
            return;
        }
        if path.leading_colon.is_none()
            && path
                .segments
                .first()
                .is_some_and(|segment| segment.ident == *self.param)
        {
            self.seen = true;
            self.exposed |=
                path.segments.len() != 1 || self.inside_wrapper || !self.direct_selection_is_erased;
            return;
        }
        let inside_wrapper = self.inside_wrapper;
        self.inside_wrapper = true;
        syn::visit::visit_path(self, path);
        self.inside_wrapper = inside_wrapper;
    }
}

fn validate_symbol_name_attrs(attrs: &[Attribute]) -> Result<()> {
    let mut seen = false;
    for attr in attrs {
        if !attr.path().is_ident("symbol_name") {
            continue;
        }
        if seen {
            return Err(Error::new_spanned(
                attr,
                "duplicate `#[symbol_name]` attribute",
            ));
        }
        seen = true;
        let syn::Meta::NameValue(name_value) = &attr.meta else {
            let err_msg = "expected `#[symbol_name = \"...\"]`";
            return Err(Error::new_spanned(attr, err_msg));
        };
        let Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(value),
            ..
        }) = &name_value.value
        else {
            let err_msg = "expected `#[symbol_name = \"...\"]`";
            return Err(Error::new_spanned(&name_value.value, err_msg));
        };
        validate_symbol_text(value, true)?;
    }
    Ok(())
}

fn validate_static_symbol_interpolations(
    attrs: &[syn::Attribute],
    static_params: &[syn::Ident],
) -> Result<()> {
    let Some(attr) = attrs.iter().find(|attr| is_symbol_name_attr(attr)) else {
        return Ok(());
    };
    let Some(expr) = symbol_name_value(attr) else {
        return Ok(());
    };
    let syn::Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Str(value),
        ..
    }) = expr
    else {
        return Ok(());
    };

    let mut interpolations = BTreeMap::<String, usize>::new();
    let text = value.value();
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        if character != '{' {
            continue;
        }
        let mut name = String::new();
        for character in chars.by_ref() {
            if character == '}' {
                break;
            }
            name.push(character);
        }
        *interpolations.entry(name).or_default() += 1;
    }

    let static_names = static_params
        .iter()
        .map(syn::Ident::to_string)
        .collect::<BTreeSet<_>>();
    let mut errors = None;
    for name in interpolations.keys() {
        if !static_names.contains(name) {
            let err_msg = format!("symbol interpolation `{{{name}}}` must name a static parameter");
            push_error(&mut errors, Error::new_spanned(value, err_msg));
        }
    }
    for name in &static_names {
        if interpolations.get(name) != Some(&1) {
            let err_msg =
                format!("symbol name must interpolate static parameter `{{{name}}}` exactly once");
            push_error(&mut errors, Error::new_spanned(value, err_msg));
        }
    }

    errors.map_or(Ok(()), Err)
}

fn is_type_param_path(ty: &syn::TypePath, type_params: &BTreeSet<&syn::Ident>) -> bool {
    ty.qself.is_none()
        && ty
            .path
            .get_ident()
            .is_some_and(|ident| type_params.contains(ident))
}

pub(crate) fn unsupported_attr(attr: &syn::Attribute) -> Error {
    Error::new_spanned(attr, "Attribute not supported in this position")
}

fn is_cfg_attr(attr: &syn::Attribute) -> bool {
    attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr")
}

fn is_doc_attr(attr: &syn::Attribute) -> bool {
    attr.path().is_ident("doc")
}

fn validate_export_fn_attrs(attrs: &[syn::Attribute]) -> Result<()> {
    for attr in attrs {
        if attr.path().is_ident("erased") {
            continue;
        }
        if is_symbol_name_attr(attr)
            || is_by_val_attr(attr)
            || is_cfg_attr(attr)
            || is_doc_attr(attr)
        {
            continue;
        }

        return Err(unsupported_attr(attr));
    }

    Ok(())
}

fn validate_no_dispatch_attrs(attrs: &[syn::Attribute], errors: &mut Option<Error>) {
    for attr in attrs {
        if attr.path().is_ident("erased") {
            let err_msg = "tagged-dispatch predicates are not allowed in this position";
            push_error(errors, Error::new_spanned(attr, err_msg));
        }
    }
}

fn ensure_no_tag_arg_attrs(sig: &syn::Signature) -> Result<()> {
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

fn is_direct_declared_type(ty: &syn::Type, declared_types: &BTreeSet<syn::Ident>) -> bool {
    let path = match ty {
        syn::Type::Path(ty) if ty.qself.is_none() => Some(&ty.path),
        syn::Type::TraitObject(_) => {
            crate::trait_object_single_trait_bound(ty).map(|bound| &bound.path)
        }
        _ => None,
    };
    path.is_some_and(|path| {
        path.segments.len() == 1 && declared_types.contains(&path.segments[0].ident)
    })
}

fn validate_extern_static(item: &Co3Static) -> Result<()> {
    for attr in &item.attrs {
        if !is_symbol_name_attr(attr) && !is_cfg_attr(attr) && !is_doc_attr(attr) {
            return Err(unsupported_attr(attr));
        }
    }
    if item.expr.is_some() {
        let err_msg = "extern static declarations cannot have an initializer";
        return Err(Error::new_spanned(&item.ident, err_msg));
    }
    Ok(())
}

fn validate_extern_fn(item: &crate::Co3Fn) -> Result<()> {
    if item.dispatch_args.is_empty() && !has_runtime_dispatch(&item.sig.generics) {
        return Ok(());
    }
    let params = item
        .sig
        .generics
        .type_params()
        .filter(|param| param.attrs.iter().any(is_type_erased));
    validate_extern_dispatch_sig(params, &item.sig)
}

fn validate_extern_impl(impl_: &crate::Co3Impl) -> Result<()> {
    let impl_dispatch = !impl_.dispatch_args.is_empty() || has_runtime_dispatch(&impl_.generics);
    let mut errors = None;
    for item in &impl_.items {
        let syn::ImplItem::Fn(method) = item else {
            continue;
        };
        let method_dispatch = impl_.method_dispatch_args.contains_key(&method.sig.ident)
            || has_runtime_dispatch(&method.sig.generics);
        if !impl_dispatch && !method_dispatch {
            continue;
        }
        let params = impl_
            .generics
            .type_params()
            .filter(|param| impl_dispatch && param.attrs.iter().any(is_type_erased))
            .chain(
                method
                    .sig
                    .generics
                    .type_params()
                    .filter(|param| method_dispatch && param.attrs.iter().any(is_type_erased)),
            );
        if let Err(err) = validate_extern_dispatch_sig(params, &method.sig) {
            push_error(&mut errors, err);
        }
    }
    errors.map_or(Ok(()), Err)
}

pub(crate) fn validate_export_attrs(attrs: &[syn::Attribute]) -> Result<()> {
    for attr in attrs {
        if !attr.path().is_ident("feature") && !is_cfg_attr(attr) && !is_doc_attr(attr) {
            return Err(unsupported_attr(attr));
        }
    }

    Ok(())
}

fn validate_impls(
    items: &[crate::ForeignItem],
    mut validate: impl FnMut(&crate::Co3Impl) -> Result<()>,
) -> Result<()> {
    let mut errors = None;
    let mut validate_one = |impl_| {
        if let Err(err) = validate(impl_) {
            push_error(&mut errors, err);
        }
    };
    for item in items {
        match item {
            crate::ForeignItem::Type(item) => {
                for impl_ in &item.self_impls {
                    validate_one(impl_);
                }
                if let Some(drop) = &item.drop {
                    validate_one(drop);
                }
            }
            crate::ForeignItem::Impl(impl_) => {
                validate_one(impl_);
            }
            crate::ForeignItem::Fn(_) | crate::ForeignItem::Static(_) => {}
        }
    }
    errors.map_or(Ok(()), Err)
}

fn validate_export_static(item: &Co3Static) -> Result<()> {
    for attr in &item.attrs {
        if !is_symbol_name_attr(attr) && !is_cfg_attr(attr) && !is_doc_attr(attr) {
            return Err(unsupported_attr(attr));
        }
    }
    item.expr.as_ref().map_or_else(
        || {
            let err_msg = "export static declarations require an initializer";
            Err(Error::new_spanned(&item.ident, err_msg))
        },
        |_| Ok(()),
    )
}

fn validate_export_fn(item: &crate::Co3Fn) -> Result<()> {
    validate_export_visibility(&item.item.vis, &item.item.sig.ident)?;
    validate_unpack_export(&item.sig)?;
    validate_export_fn_attrs(&item.attrs)
}

fn validate_export_visibility(vis: &syn::Visibility, item: &impl ToTokens) -> Result<()> {
    if matches!(vis, syn::Visibility::Inherited) {
        Ok(())
    } else {
        Err(Error::new_spanned(
            item,
            "visibility qualifiers are not allowed on functions or methods in `ffi!` export blocks",
        ))
    }
}

fn validate_export_type(item: &crate::ForeignItemType) -> Result<()> {
    for attr in &item.ty.attrs {
        if !attr.path().is_ident("tag")
            && !attr.path().is_ident("erased")
            && !is_cfg_attr(attr)
            && !is_doc_attr(attr)
        {
            return Err(unsupported_attr(attr));
        }
    }
    Ok(())
}

fn validate_export_impl(
    impl_: &crate::Co3Impl,
    declared_types: &BTreeSet<syn::Ident>,
) -> Result<()> {
    let mut errors = None;
    let drop_impl = is_drop_impl(&impl_.item);
    for item in &impl_.items {
        let syn::ImplItem::Fn(method) = item else {
            continue;
        };
        if let Err(err) = validate_export_visibility(&method.vis, &method.sig.ident) {
            push_error(&mut errors, err);
        }
        if drop_impl && !matches!(method.sig.output, syn::ReturnType::Default) {
            let err_msg = "returning `Drop::drop` is supported only in extern declarations";
            push_error(&mut errors, Error::new_spanned(&method.sig.output, err_msg));
        }
        if let Err(err) = validate_unpack_export(&method.sig) {
            push_error(&mut errors, err);
        }
        if let Err(err) = validate_export_fn_attrs(&method.attrs) {
            push_error(&mut errors, err);
        }
        if !is_direct_declared_type(&impl_.self_ty, declared_types)
            && let Err(err) = reject_explicit_dispatch_ids(&method.sig)
        {
            push_error(&mut errors, err);
        }
        if let Err(err) = validate_export_receiver_position(&method.sig) {
            push_error(&mut errors, err);
        }
    }
    errors.map_or(Ok(()), Err)
}

fn validate_export_receiver_position(sig: &syn::Signature) -> Result<()> {
    let Some((receiver_position, receiver)) = sig
        .inputs
        .iter()
        .enumerate()
        .find(|(_, input)| matches!(input, syn::FnArg::Receiver(_)))
    else {
        return Ok(());
    };
    let first_non_tag_position = sig
        .inputs
        .iter()
        .position(|input| !matches!(input, syn::FnArg::Typed(arg) if tag_id(&arg.ty).is_some()));

    if first_non_tag_position == Some(receiver_position) {
        return Ok(());
    }

    let err_msg = "an exported method receiver must be the first non-tag argument";
    Err(Error::new_spanned(receiver, err_msg))
}

fn validate_shared(items: &[crate::ForeignItem], validate_unpacks: bool) -> Result<()> {
    validate_method_generic_declarations(items)?;

    let declared_types = crate::declared_foreign_type_idents(items);
    let mut errors = validate_packed_dyn_self_decls(items).err();
    if let Err(err) = validate_symbol_names(items, &declared_types) {
        push_error(&mut errors, err);
    }
    if let Err(err) = validate_parameter_bindings(items, &declared_types) {
        push_error(&mut errors, err);
    }
    if let Err(err) = validate_blanket_dyn_impls(items) {
        push_error(&mut errors, err);
    }
    for item in items {
        match item {
            crate::ForeignItem::Static(item) => {
                validate_no_dispatch_attrs(&item.attrs, &mut errors);
            }
            crate::ForeignItem::Type(item) => {
                validate_no_dispatch_attrs(&item.ty.attrs, &mut errors);
            }
            crate::ForeignItem::Fn(item) => {
                if let Err(err) = validate_shared_fn(item, validate_unpacks) {
                    push_error(&mut errors, err);
                }
            }
            crate::ForeignItem::Impl(_) => {}
        }
    }
    if let Err(err) = validate_impls(items, |impl_| validate_shared_impl(impl_, validate_unpacks)) {
        push_error(&mut errors, err);
    }
    errors.map_or(Ok(()), Err)
}

fn validate_type_param_defaults(generics: &syn::Generics) -> Result<()> {
    let mut errors = None;
    for param in generics.type_params() {
        if param.default.is_some() && !param.attrs.iter().any(is_type_erased) {
            push_error(&mut errors, Error::new_spanned(param, STATIC_DEFAULT_ERR));
        }
    }
    errors.map_or(Ok(()), Err)
}

fn validate_blanket_dyn_impls(items: &[crate::ForeignItem]) -> Result<()> {
    validate_impls(items, |impl_| {
        let Some(bound) = crate::trait_object_single_trait_bound(&impl_.self_ty) else {
            return Ok(());
        };
        let Some(ident) = bound.path.get_ident() else {
            return Ok(());
        };
        if impl_
            .generics
            .type_params()
            .any(|param| param.ident == *ident)
        {
            let err_msg = "`dyn Self` is only supported for declared extern types";
            return Err(Error::new_spanned(&impl_.self_ty, err_msg));
        }
        Ok(())
    })
}

fn validate_packed_dyn_self_decls(items: &[crate::ForeignItem]) -> Result<()> {
    let mut errors = None;
    let declared_types = crate::declared_foreign_type_idents(items);

    if let Err(err) = validate_impls(items, |impl_| {
        let syn::Type::TraitObject(trait_object) = impl_.self_ty.as_ref() else {
            return Ok(());
        };
        let contains_declared_type = trait_object.bounds.iter().any(|bound| {
            let syn::TypeParamBound::Trait(bound) = bound else {
                return false;
            };
            bound
                .path
                .get_ident()
                .is_some_and(|ident| declared_types.contains(ident))
        });
        if contains_declared_type
            && crate::trait_object_single_trait_bound(&impl_.self_ty).is_none()
        {
            let err_msg = "`dyn Self` must contain exactly one trait bound";
            return Err(Error::new_spanned(&impl_.self_ty, err_msg));
        }
        Ok(())
    }) {
        push_error(&mut errors, err);
    }

    for item in items {
        let crate::ForeignItem::Type(item) = item else {
            continue;
        };
        if item.id.is_some() {
            continue;
        }
        let dyn_self_impl = item
            .self_impls
            .iter()
            .chain(item.drop.iter())
            .find(|impl_| crate::trait_object_single_trait_bound(&impl_.self_ty).is_some());
        if let Some(impl_) = dyn_self_impl {
            let err_msg = "`dyn Self` requires the declared type to provide `#[tag(...)]`";
            push_error(&mut errors, Error::new_spanned(&impl_.self_ty, err_msg));
        }
    }

    errors.map_or(Ok(()), Err)
}

fn validate_shared_fn(item: &crate::Co3Fn, validate_unpacks: bool) -> Result<()> {
    let mut errors = None;
    if let Err(err) = validate_type_param_defaults(&item.sig.generics) {
        push_error(&mut errors, err);
    }
    if validate_unpacks && let Err(err) = validate_unpack(&item.sig, None) {
        push_error(&mut errors, err);
    }
    if let Err(err) = ensure_no_tag_arg_attrs(&item.sig) {
        push_error(&mut errors, err);
    }
    if let Err(err) = validate_signature_shape(&item.sig) {
        push_error(&mut errors, err);
    }
    if let Err(err) = validate_soft_lifetimes(&item.sig) {
        push_error(&mut errors, err);
    }
    if !item.dispatch_args.is_empty() || has_payload_dispatch(&item.sig.generics) {
        let params = item
            .sig
            .generics
            .type_params()
            .filter(|param| is_payload_erased(param));
        if let Err(err) = validate_dispatch_param_positions(params, &item.sig) {
            push_error(&mut errors, err);
        }
    }
    errors.map_or(Ok(()), Err)
}

fn validate_shared_impl(impl_: &crate::Co3Impl, validate_unpacks: bool) -> Result<()> {
    let mut errors = None;
    if let Err(err) = validate_type_param_defaults(&impl_.generics) {
        push_error(&mut errors, err);
    }
    for attr in &impl_.attrs {
        if !attr.path().is_ident("erased") && !is_cfg_attr(attr) && !is_doc_attr(attr) {
            push_error(&mut errors, unsupported_attr(attr));
        }
    }
    let impl_dispatch = !impl_.dispatch_args.is_empty() || has_payload_dispatch(&impl_.generics);
    for item in &impl_.items {
        let syn::ImplItem::Fn(method) = item else {
            continue;
        };
        let sig = &method.sig;
        if let Err(err) = validate_type_param_defaults(&sig.generics) {
            push_error(&mut errors, err);
        }
        let method_dispatch = impl_.method_dispatch_args.get(&sig.ident);
        if validate_unpacks && let Err(err) = validate_unpack(sig, Some(&impl_.generics)) {
            push_error(&mut errors, err);
        }
        if let Err(err) = ensure_no_tag_arg_attrs(sig) {
            push_error(&mut errors, err);
        }
        if impl_.trait_.is_some() && has_non_lifetime_generics(&sig.generics) {
            let err_msg = "trait methods cannot have generic type or const parameters";
            push_error(&mut errors, Error::new_spanned(&sig.generics, err_msg));
        }
        if let Err(err) = validate_signature_shape(sig) {
            push_error(&mut errors, err);
        }
        if let Err(err) = validate_soft_lifetimes(sig) {
            push_error(&mut errors, err);
        }
        if impl_dispatch
            || method_dispatch.is_some_and(|args| !args.is_empty())
            || has_payload_dispatch(&sig.generics)
        {
            let params = impl_
                .generics
                .type_params()
                .filter(|param| impl_dispatch && is_payload_erased(param))
                .chain(sig.generics.type_params().filter(|param| {
                    (method_dispatch.is_some_and(|args| !args.is_empty())
                        || has_payload_dispatch(&sig.generics))
                        && is_payload_erased(param)
                }));
            if let Err(err) = validate_dispatch_param_positions(params, sig) {
                push_error(&mut errors, err);
            }
        }
    }
    if is_drop_impl(impl_)
        && let Err(err) = validate_drop_impl(impl_)
    {
        push_error(&mut errors, err);
    }
    errors.map_or(Ok(()), Err)
}

fn validate_soft_lifetimes(sig: &syn::Signature) -> Result<()> {
    #[derive(Default)]
    struct LifetimeCollector {
        lifetimes: Vec<syn::Lifetime>,
    }

    impl Visit<'_> for LifetimeCollector {
        fn visit_lifetime(&mut self, lifetime: &syn::Lifetime) {
            self.lifetimes.push(lifetime.clone());
        }
    }

    fn input_type(input: &syn::FnArg) -> syn::Type {
        match input {
            syn::FnArg::Receiver(receiver) => crate::utils::receiver_ty(receiver),
            syn::FnArg::Typed(arg) => (*arg.ty).clone(),
        }
    }

    fn collect_lifetimes(ty: &syn::Type) -> Vec<syn::Lifetime> {
        let mut collector = LifetimeCollector::default();
        collector.visit_type(ty);
        collector.lifetimes
    }

    let mut explicit_lifetime_errors = None;
    for input in &sig.inputs {
        let attrs = match input {
            syn::FnArg::Receiver(receiver) => &receiver.attrs,
            syn::FnArg::Typed(arg) => &arg.attrs,
        };
        if !soft_for_arg(attrs) {
            continue;
        }

        let ty = input_type(input);
        for lifetime in collect_lifetimes(&ty) {
            let message = "explicit lifetimes are not allowed in a `#[soft]` argument";
            push_error(
                &mut explicit_lifetime_errors,
                Error::new_spanned(lifetime, message),
            );
        }
    }
    if let Some(errors) = explicit_lifetime_errors {
        return Err(errors);
    }

    let mut sig = sig.clone();
    crate::ffi_fn::explicitize_signature_lifetimes(&mut sig);

    let soft_lifetimes = sig
        .inputs
        .iter()
        .filter_map(|input| {
            let attrs = match input {
                syn::FnArg::Receiver(receiver) => &receiver.attrs,
                syn::FnArg::Typed(arg) => &arg.attrs,
            };
            soft_for_arg(attrs).then(|| input_type(input))
        })
        .flat_map(|ty| collect_lifetimes(&ty))
        .map(|lifetime| lifetime.ident.to_string())
        .collect::<BTreeSet<_>>();

    let mut errors = None;
    for input in &sig.inputs {
        let attrs = match input {
            syn::FnArg::Receiver(receiver) => &receiver.attrs,
            syn::FnArg::Typed(arg) => &arg.attrs,
        };
        if soft_for_arg(attrs) {
            continue;
        }

        let ty = input_type(input);
        if collect_lifetimes(&ty)
            .iter()
            .any(|lifetime| soft_lifetimes.contains(&lifetime.ident.to_string()))
        {
            push_error(
                &mut errors,
                Error::new_spanned(
                    ty,
                    "the lifetime of a `#[soft]` argument cannot be connected to another argument",
                ),
            );
        }
    }

    if let syn::ReturnType::Type(_, output) = &sig.output
        && collect_lifetimes(output)
            .iter()
            .any(|lifetime| soft_lifetimes.contains(&lifetime.ident.to_string()))
    {
        push_error(
            &mut errors,
            Error::new_spanned(
                output,
                "a return value cannot borrow from a `#[soft]` argument",
            ),
        );
    }

    #[derive(Default)]
    struct LifetimeConnectionCollector {
        connections: BTreeMap<String, BTreeSet<String>>,
        parents: Vec<String>,
    }

    impl LifetimeConnectionCollector {
        fn connect_to_parents(&mut self, lifetime: &syn::Lifetime) {
            let lifetime = lifetime.ident.to_string();
            for parent in &self.parents {
                self.connections
                    .entry(lifetime.clone())
                    .or_default()
                    .insert(parent.clone());
                self.connections
                    .entry(parent.clone())
                    .or_default()
                    .insert(lifetime.clone());
            }
        }
    }

    impl Visit<'_> for LifetimeConnectionCollector {
        fn visit_type_reference(&mut self, reference: &syn::TypeReference) {
            let Some(lifetime) = &reference.lifetime else {
                self.visit_type(&reference.elem);
                return;
            };

            self.connect_to_parents(lifetime);
            self.parents.push(lifetime.ident.to_string());
            self.visit_type(&reference.elem);
            self.parents.pop();
        }

        fn visit_lifetime(&mut self, lifetime: &syn::Lifetime) {
            self.connect_to_parents(lifetime);
        }
    }

    let mut lifetime_connection_collector = LifetimeConnectionCollector::default();
    for input in &sig.inputs {
        let attrs = match input {
            syn::FnArg::Receiver(receiver) => &receiver.attrs,
            syn::FnArg::Typed(arg) => &arg.attrs,
        };
        if soft_for_arg(attrs) {
            lifetime_connection_collector.visit_type(&input_type(input));
        }
    }

    let connected_soft_lifetime = soft_lifetimes.iter().any(|soft_lifetime| {
        lifetime_connection_collector
            .connections
            .get(soft_lifetime)
            .is_some_and(|connected| connected.iter().any(|lifetime| lifetime != soft_lifetime))
    });
    if connected_soft_lifetime {
        push_error(
            &mut errors,
            Error::new_spanned(
                &sig.inputs,
                "the lifetime of a `#[soft]` argument cannot be connected to another lifetime",
            ),
        );
    }

    errors.map_or(Ok(()), Err)
}

fn validate_method_generic_declarations(items: &[crate::ForeignItem]) -> Result<()> {
    validate_impls(items, |impl_| {
        let impl_params = impl_
            .generics
            .params
            .iter()
            .map(generic_param_ident)
            .collect::<BTreeSet<_>>();
        let mut errors = None;

        for item in &impl_.items {
            let syn::ImplItem::Fn(method) = item else {
                continue;
            };
            for param in &method.sig.generics.params {
                let ident = generic_param_ident(param);
                if impl_params.contains(ident) {
                    let err_msg = "method generic parameter shadows an impl generic parameter";
                    push_error(&mut errors, Error::new_spanned(ident, err_msg));
                }
            }
        }

        errors.map_or(Ok(()), Err)
    })
}

fn validate_unpack_export(sig: &syn::Signature) -> Result<()> {
    for input in &sig.inputs {
        let syn::FnArg::Typed(input) = input else {
            continue;
        };
        if let Some(attr) = input.attrs.iter().find(|attr| is_unpack_attr(attr)) {
            let err_msg = format!(
                "{} is only supported in extern declarations",
                unpack_attr_name(attr)
            );

            return Err(Error::new_spanned(attr, err_msg));
        }
    }
    Ok(())
}

fn validate_unpack(sig: &syn::Signature, outer: Option<&syn::Generics>) -> Result<()> {
    let runtime_parameters = outer
        .into_iter()
        .flat_map(|generics| generics.type_params())
        .chain(sig.generics.type_params())
        .filter(|param| param.attrs.iter().any(is_type_erased))
        .map(|param| &param.ident)
        .collect::<Vec<_>>();
    let detector = crate::utils::ParamUseDetector::new(runtime_parameters);
    for input in &sig.inputs {
        let syn::FnArg::Typed(input) = input else {
            continue;
        };
        let mut unpack_attrs = input.attrs.iter().filter(|attr| is_unpack_attr(attr));
        if let Some(first) = unpack_attrs.next()
            && let Some(duplicate) = unpack_attrs.next()
        {
            return Err(Error::new_spanned(
                duplicate,
                format!("duplicate {} attribute", unpack_attr_name(first)),
            ));
        }
        let Some(attr) = input.attrs.iter().find(|attr| is_unpack_attr(attr)) else {
            continue;
        };
        if crate::ffi_fn::validate_single_unpack(&input.attrs)? {
            continue;
        }
        let (part1, part2) = unpack_types(&input.attrs)?.expect("unpack attribute was found");
        if detector.type_mentions_param(&input.ty)
            && (matches!(part1, syn::Type::Infer(_)) || matches!(part2, syn::Type::Infer(_)))
        {
            let err_msg = format!(
                "runtime-dispatched {} arguments cannot use `_` placeholders",
                unpack_attr_name(attr),
            );
            return Err(Error::new_spanned(attr, err_msg));
        }
        if matches!(part1, syn::Type::Infer(_)) || matches!(part2, syn::Type::Infer(_)) {
            crate::ffi_fn::unpack_abi_parts(&input.attrs, &input.ty)?;
        }
    }

    Ok(())
}

fn validate_parameter_bindings(
    items: &[crate::ForeignItem],
    declared_types: &BTreeSet<syn::Ident>,
) -> Result<()> {
    let mut errors = validate_parameter_usage(items, declared_types).err();
    if let Err(err) = validate_parameter_bounds(items, declared_types) {
        push_error(&mut errors, err);
    }
    errors.map_or(Ok(()), Err)
}

fn validate_parameter_usage(
    items: &[crate::ForeignItem],
    declared_types: &BTreeSet<syn::Ident>,
) -> Result<()> {
    validate_parameter_pass(
        items,
        declared_types,
        |item, declared_types| {
            validate_callable_parameter_usage(
                &item.sig.generics,
                &item.dispatch_args,
                &item.sig,
                declared_types,
            )
        },
        validate_impl_parameter_usage,
    )
}

fn validate_parameter_bounds(
    items: &[crate::ForeignItem],
    declared_types: &BTreeSet<syn::Ident>,
) -> Result<()> {
    validate_parameter_bounds_with(items, declared_types, false)
}

fn validate_export_parameter_bounds(
    items: &[crate::ForeignItem],
    declared_types: &BTreeSet<syn::Ident>,
) -> Result<()> {
    validate_parameter_bounds_with(items, declared_types, true)
}

fn validate_parameter_bounds_with(
    items: &[crate::ForeignItem],
    declared_types: &BTreeSet<syn::Ident>,
    require_dispatch: bool,
) -> Result<()> {
    validate_parameter_pass(
        items,
        declared_types,
        |item, declared_types| {
            let errors = validate_callable_parameter_bounds(
                &item.sig.generics,
                &item.dispatch_args,
                &item.sig,
                declared_types,
                require_dispatch,
            )
            .err();
            errors.map_or(Ok(()), Err)
        },
        |impl_, declared_types| {
            validate_impl_parameter_bounds(impl_, declared_types, require_dispatch)
        },
    )
}

fn validate_parameter_pass(
    items: &[crate::ForeignItem],
    declared_types: &BTreeSet<syn::Ident>,
    validate_fn: impl Fn(&crate::Co3Fn, &BTreeSet<syn::Ident>) -> Result<()>,
    validate_impl: impl Fn(&crate::Co3Impl, &BTreeSet<syn::Ident>) -> Result<()>,
) -> Result<()> {
    let mut errors = None;
    for item in items {
        match item {
            crate::ForeignItem::Fn(item) => {
                if let Err(err) = validate_fn(item, declared_types) {
                    push_error(&mut errors, err);
                }
            }
            crate::ForeignItem::Impl(impl_) => {
                if let Err(err) = validate_impl(impl_, declared_types) {
                    push_error(&mut errors, err);
                }
            }
            crate::ForeignItem::Type(item) => {
                for impl_ in &item.self_impls {
                    if let Err(err) = validate_impl(impl_, declared_types) {
                        push_error(&mut errors, err);
                    }
                }
                if let Some(drop) = &item.drop
                    && let Err(err) = validate_impl(drop, declared_types)
                {
                    push_error(&mut errors, err);
                }
            }
            crate::ForeignItem::Static(_) => {}
        }
    }
    errors.map_or(Ok(()), Err)
}

fn validate_impl_parameter_usage(
    impl_: &crate::Co3Impl,
    declared_types: &BTreeSet<syn::Ident>,
) -> Result<()> {
    let mut errors = validate_impl_generic_usage(impl_, declared_types).err();
    if let Err(err) = validate_impl_method_usage(impl_, declared_types) {
        push_error(&mut errors, err);
    }
    errors.map_or(Ok(()), Err)
}

fn validate_impl_parameter_bounds(
    impl_: &crate::Co3Impl,
    declared_types: &BTreeSet<syn::Ident>,
    require_dispatch: bool,
) -> Result<()> {
    let mut errors = validate_impl_generic_bounds(impl_, declared_types, require_dispatch).err();
    if let Err(err) = validate_impl_method_bounds(impl_, declared_types, require_dispatch) {
        push_error(&mut errors, err);
    }
    errors.map_or(Ok(()), Err)
}

fn validate_impl_method_usage(
    impl_: &crate::Co3Impl,
    declared_types: &BTreeSet<syn::Ident>,
) -> Result<()> {
    validate_impl_methods(impl_, |method, dispatch| {
        validate_callable_parameter_usage(
            &method.sig.generics,
            dispatch,
            &method.sig,
            declared_types,
        )
    })
}

fn validate_impl_method_bounds(
    impl_: &crate::Co3Impl,
    declared_types: &BTreeSet<syn::Ident>,
    require_dispatch: bool,
) -> Result<()> {
    validate_impl_methods(impl_, |method, dispatch| {
        let errors = validate_callable_parameter_bounds(
            &method.sig.generics,
            dispatch,
            &method.sig,
            declared_types,
            require_dispatch,
        )
        .err();
        errors.map_or(Ok(()), Err)
    })
}

fn validate_impl_methods(
    impl_: &crate::Co3Impl,
    mut validate: impl FnMut(&syn::ImplItemFn, &crate::DispatchGroups) -> Result<()>,
) -> Result<()> {
    let mut errors = None;
    for item in &impl_.items {
        let syn::ImplItem::Fn(method) = item else {
            continue;
        };
        let dispatch = impl_
            .method_dispatch_args
            .get(&method.sig.ident)
            .cloned()
            .unwrap_or_default();
        if let Err(err) = validate(method, &dispatch) {
            push_error(&mut errors, err);
        }
    }
    errors.map_or(Ok(()), Err)
}

fn visit_signature_positions<'ast>(uses: &mut impl Visit<'ast>, sig: &'ast syn::Signature) {
    for input in &sig.inputs {
        match input {
            syn::FnArg::Receiver(receiver) => {
                if let syn::ReceiverKind::Typed(_, ty) = &receiver.kind {
                    uses.visit_type(ty);
                }
            }
            syn::FnArg::Typed(input) => uses.visit_type(&input.ty),
        }
    }
    if let syn::ReturnType::Type(_, output) = &sig.output {
        uses.visit_type(output);
    }
}

fn visit_signature_lifetime_positions(uses: &mut LifetimeUseDetector<'_>, sig: &syn::Signature) {
    for input in &sig.inputs {
        match input {
            syn::FnArg::Receiver(receiver) => {
                if let syn::ReceiverKind::Typed(_, ty) = &receiver.kind {
                    uses.visit_type(ty);
                }
            }
            syn::FnArg::Typed(input) => uses.visit_type(&input.ty),
        }
    }
    if let syn::ReturnType::Type(_, output) = &sig.output {
        uses.visit_type(output);
    }
    uses.visit_generic_constraints(&sig.generics);
}

fn validate_callable_parameter_usage(
    local_generics: &syn::Generics,
    local_dispatch: &crate::DispatchGroups,
    sig: &syn::Signature,
    declared_types: &BTreeSet<syn::Ident>,
) -> Result<()> {
    let mut errors = None;
    for param in &local_generics.params {
        if matches!(param, syn::GenericParam::Type(param)
            if is_open_payload_dispatch_param(param, local_dispatch))
        {
            continue;
        }
        let ident = match param {
            syn::GenericParam::Type(param) => &param.ident,
            syn::GenericParam::Const(param) => &param.ident,
            syn::GenericParam::Lifetime(param) => {
                let mut uses = LifetimeUseDetector::new(&param.lifetime);
                visit_signature_lifetime_positions(&mut uses, sig);
                if !uses.seen {
                    let err = Error::new_spanned(&param.lifetime.ident, UNUSED_GENERIC_ERR);
                    push_error(&mut errors, err);
                }
                continue;
            }
        };
        if local_dispatch.contains_param(ident) {
            continue;
        }
        let mut uses = ErasedUseDetector::new(ident, declared_types);
        visit_signature_positions(&mut uses, sig);
        let dependency = payloadless_dependency(local_generics, local_dispatch, ident);
        if !uses.seen && !dependency.is_valid() {
            push_error(&mut errors, Error::new_spanned(ident, UNUSED_GENERIC_ERR));
        }
    }
    errors.map_or(Ok(()), Err)
}

fn validate_callable_parameter_bounds(
    local_generics: &syn::Generics,
    local_dispatch: &crate::DispatchGroups,
    sig: &syn::Signature,
    declared_types: &BTreeSet<syn::Ident>,
    require_dispatch: bool,
) -> Result<()> {
    fn runtime_param_has_stable_abi(param: &syn::Ident, sig: &syn::Signature) -> bool {
        let detector = crate::utils::ParamUseDetector::new([param]);
        let mut represented = false;

        for input in &sig.inputs {
            let syn::FnArg::Typed(input) = input else {
                continue;
            };
            if matches!(tag_id(&input.ty), Some(TagId::DynType(ident)) if ident == param) {
                represented = true;
                continue;
            }
            if input.attrs.iter().any(is_unpack_attr) && detector.type_mentions_param(&input.ty) {
                represented = true;
                continue;
            }
            if detector.type_mentions_param(&input.ty) {
                return false;
            }
        }

        represented
            && !matches!(&sig.output, syn::ReturnType::Type(_, ty) if detector.type_mentions_param(ty))
    }

    let mut errors = None;
    for param in &local_generics.params {
        if let syn::GenericParam::Type(param) = param
            && is_open_payload_dispatch_param(param, local_dispatch)
        {
            push_error(
                &mut errors,
                Error::new_spanned(&param.ident, OPEN_PAYLOAD_ERASURE_ERR),
            );
            continue;
        }
        let (ident, dynamic) = match param {
            syn::GenericParam::Type(param) => {
                (&param.ident, param.attrs.iter().any(is_type_erased))
            }
            syn::GenericParam::Const(param) => (&param.ident, false),
            syn::GenericParam::Lifetime(_) => continue,
        };
        if local_dispatch.contains_param(ident) {
            continue;
        }

        if dynamic && !require_dispatch && runtime_param_has_stable_abi(ident, sig) {
            continue;
        }

        let mut uses = ErasedUseDetector::new(ident, declared_types);
        visit_signature_positions(&mut uses, sig);
        let dependency = payloadless_dependency(local_generics, local_dispatch, ident);
        if !uses.seen && !dependency.is_valid() {
            continue;
        }
        if dynamic || dependency.invalid || uses.exposed {
            if require_dispatch {
                continue;
            }
            let err = Error::new_spanned(ident, GENERIC_USE_PREDICATE_ERR);
            push_error(&mut errors, err);
        } else if require_dispatch {
            let err = Error::new_spanned(ident, GENERIC_USE_PREDICATE_ERR);
            push_error(&mut errors, err);
        }
    }
    errors.map_or(Ok(()), Err)
}

fn generic_param_ident(param: &syn::GenericParam) -> &syn::Ident {
    match param {
        syn::GenericParam::Type(param) => &param.ident,
        syn::GenericParam::Const(param) => &param.ident,
        syn::GenericParam::Lifetime(param) => &param.lifetime.ident,
    }
}

fn validate_impl_generic_usage(
    impl_: &crate::Co3Impl,
    declared_types: &BTreeSet<syn::Ident>,
) -> Result<()> {
    let mut errors = None;
    for param in &impl_.generics.params {
        if matches!(param, syn::GenericParam::Type(param)
            if is_open_payload_dispatch_param(param, &impl_.dispatch_args))
        {
            continue;
        }
        let ident = generic_param_ident(param);
        if matches!(param, syn::GenericParam::Lifetime(_)) {
            let mut uses = LifetimeUseDetector::new(match param {
                syn::GenericParam::Lifetime(param) => &param.lifetime,
                _ => unreachable!(),
            });
            uses.visit_type(&impl_.self_ty);
            if let Some((trait_, _)) = &impl_.trait_ {
                uses.visit_path(trait_);
            }
            uses.visit_generic_constraints(&impl_.generics);
            if !uses.seen {
                let err = Error::new_spanned(ident, UNUSED_GENERIC_ERR);
                push_error(&mut errors, err);
            }
            continue;
        }
        let (used, _) = impl_param_use(impl_, declared_types, ident);
        if !used {
            let err = Error::new_spanned(ident, UNUSED_GENERIC_ERR);
            push_error(&mut errors, err);
        }
    }
    errors.map_or(Ok(()), Err)
}

fn validate_impl_generic_bounds(
    impl_: &crate::Co3Impl,
    declared_types: &BTreeSet<syn::Ident>,
    require_dispatch: bool,
) -> Result<()> {
    let mut errors = None;
    for param in &impl_.generics.params {
        if matches!(param, syn::GenericParam::Lifetime(_)) {
            continue;
        }
        let ident = generic_param_ident(param);
        if impl_.dispatch_args.contains_param(ident) {
            continue;
        }
        if let syn::GenericParam::Type(param) = param
            && is_open_payload_dispatch_param(param, &impl_.dispatch_args)
        {
            push_error(
                &mut errors,
                Error::new_spanned(&param.ident, OPEN_PAYLOAD_ERASURE_ERR),
            );
            continue;
        }

        let (used, exposed) = impl_param_use(impl_, declared_types, ident);
        if !used {
            continue;
        }
        let dynamic = matches!(
            param,
            syn::GenericParam::Type(param)
                if param.attrs.iter().any(is_type_erased)
        );
        if dynamic || exposed {
            if require_dispatch {
                continue;
            }
            let err = Error::new_spanned(ident, GENERIC_USE_PREDICATE_ERR);
            push_error(&mut errors, err);
        } else if require_dispatch {
            let err = Error::new_spanned(ident, GENERIC_USE_PREDICATE_ERR);
            push_error(&mut errors, err);
        }
    }
    errors.map_or(Ok(()), Err)
}

fn has_payload_dispatch(generics: &syn::Generics) -> bool {
    generics.type_params().any(is_payload_erased)
}

fn is_open_payload_dispatch_param(
    param: &syn::TypeParam,
    dispatch: &crate::DispatchGroups,
) -> bool {
    is_payload_erased(param) && !dispatch.contains_param(&param.ident)
}

fn impl_param_use(
    impl_: &crate::Co3Impl,
    declared_types: &BTreeSet<syn::Ident>,
    param: &syn::Ident,
) -> (bool, bool) {
    let detector = crate::utils::ParamUseDetector::new([param]);
    let appears_in_impl = detector.type_mentions_param(&impl_.self_ty)
        || impl_
            .trait_
            .as_ref()
            .is_some_and(|(path, _)| detector.path_mentions_param(path));
    let mut uses = ErasedUseDetector::new(param, declared_types);
    uses.visit_type(&impl_.self_ty);
    if let Some((trait_path, _)) = &impl_.trait_ {
        uses.visit_path(trait_path);
    }
    let dependency = payloadless_dependency(&impl_.generics, &impl_.dispatch_args, param);
    (
        appears_in_impl || uses.seen || dependency.is_valid(),
        uses.exposed || dependency.invalid,
    )
}

struct PayloadlessDependency {
    found: bool,
    invalid: bool,
}

impl PayloadlessDependency {
    fn is_valid(&self) -> bool {
        self.found && !self.invalid
    }
}

fn payloadless_dependency(
    generics: &syn::Generics,
    dispatch_args: &crate::DispatchGroups,
    param: &syn::Ident,
) -> PayloadlessDependency {
    let detector = crate::utils::ParamUseDetector::new([param]);
    let mut dependency = PayloadlessDependency {
        found: false,
        invalid: false,
    };

    for (owners, targets) in dispatch_args.groups() {
        for (owner_index, owner) in owners.iter().enumerate() {
            let mentions = targets.iter().map(|target| {
                target
                    .args
                    .get(owner_index)
                    .is_some_and(|arg| detector.generic_arg_mentions_param(arg))
            });
            let mentions = mentions.collect::<Vec<_>>();
            if !mentions.iter().any(|mentions| *mentions) {
                continue;
            }

            dependency.found = true;
            let is_payloadless = generics.type_params().any(|candidate| {
                candidate.ident == *owner
                    && candidate.attrs.iter().any(is_type_erased)
                    && candidate.default.is_none()
            });
            if !is_payloadless || !mentions.into_iter().all(|mentions| mentions) {
                dependency.invalid = true;
            }
        }
    }

    dependency
}

struct ErasedUseDetector<'a> {
    param: &'a syn::Ident,
    declared_types: &'a BTreeSet<syn::Ident>,
    seen: bool,
    exposed: bool,
}

impl<'a> ErasedUseDetector<'a> {
    fn new(param: &'a syn::Ident, declared_types: &'a BTreeSet<syn::Ident>) -> Self {
        Self {
            param,
            declared_types,
            seen: false,
            exposed: false,
        }
    }
}

impl Visit<'_> for ErasedUseDetector<'_> {
    fn visit_path(&mut self, path: &syn::Path) {
        if path.segments.len() == 1 && self.declared_types.contains(&path.segments[0].ident) {
            if crate::utils::ParamUseDetector::new([self.param]).path_mentions_param(path) {
                self.seen = true;
            }
            return;
        }
        if path.leading_colon.is_none() && path.get_ident().is_some_and(|ident| ident == self.param)
        {
            self.seen = true;
            self.exposed = true;
            return;
        }
        syn::visit::visit_path(self, path);
    }
}

struct LifetimeUseDetector<'a> {
    lifetime: &'a syn::Lifetime,
    seen: bool,
}

impl<'a> LifetimeUseDetector<'a> {
    fn new(lifetime: &'a syn::Lifetime) -> Self {
        Self {
            lifetime,
            seen: false,
        }
    }

    fn visit_generic_constraints(&mut self, generics: &syn::Generics) {
        for generic in &generics.params {
            if let syn::GenericParam::Lifetime(param) = generic {
                for bound in &param.bounds {
                    self.visit_lifetime(bound);
                }
            }
        }
        if let Some(where_clause) = &generics.where_clause {
            for predicate in &where_clause.predicates {
                self.visit_where_predicate(predicate);
            }
        }
    }
}

impl Visit<'_> for LifetimeUseDetector<'_> {
    fn visit_lifetime(&mut self, lifetime: &syn::Lifetime) {
        self.seen |= lifetime == self.lifetime;
    }
}

fn reject_explicit_dispatch_ids(sig: &syn::Signature) -> Result<()> {
    let err_msg = "explicit `<dyn Type>::TAG` is only supported in extern declarations";

    let mut errors = None;
    for input in &sig.inputs {
        let syn::FnArg::Typed(arg) = input else {
            continue;
        };

        if tag_id(&arg.ty).is_some() {
            push_error(&mut errors, Error::new_spanned(&arg.ty, err_msg));
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(())
}

fn validate_extern_dispatch_sig<'a>(
    dispatch_params: impl Iterator<Item = &'a syn::TypeParam>,
    sig: &syn::Signature,
) -> Result<()> {
    let mut tag_ids = BTreeSet::new();

    let tag_tys = dispatch_params
        .map(|param| &param.ident)
        .collect::<BTreeSet<_>>();

    let mut errors = None;
    for input in &sig.inputs {
        let syn::FnArg::Typed(arg) = input else {
            continue;
        };

        let Some(tag_id) = tag_id(&arg.ty) else {
            continue;
        };

        let is_dyn_param = match tag_id {
            TagId::DynType(ident) => tag_tys.contains(ident),
            TagId::DynSelf => true,
        };

        if !is_dyn_param {
            let err_msg = "`<dyn Type>::TAG` arg must target declared `dyn Type`";
            push_error(&mut errors, Error::new_spanned(&arg.ty, err_msg));
            continue;
        }

        if !tag_ids.insert(tag_id) {
            let err_msg = "duplicate `<dyn Type>::TAG`";
            push_error(&mut errors, Error::new_spanned(&arg.ty, err_msg));
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(())
}

fn validate_dispatch_param_positions<'a>(
    dispatch_params: impl Iterator<Item = &'a syn::TypeParam>,
    sig: &syn::Signature,
) -> Result<()> {
    struct DispatchParamPositionValidator<'a> {
        params: BTreeSet<&'a syn::Ident>,
        errors: Option<Error>,
    }

    impl Visit<'_> for DispatchParamPositionValidator<'_> {
        fn visit_type_path(&mut self, ty: &syn::TypePath) {
            if is_type_param_path(ty, &self.params) {
                return;
            }

            let detector = crate::utils::ParamUseDetector::new(self.params.iter().copied());
            if detector.type_mentions_param(&syn::Type::Path(ty.clone())) {
                push_error(
                    &mut self.errors,
                    Error::new_spanned(ty, DISPATCH_OPAQUE_NESTING_ERR),
                );
                return;
            }

            syn::visit::visit_type_path(self, ty);
        }
    }

    let mut validator = DispatchParamPositionValidator {
        params: dispatch_params.map(|param| &param.ident).collect(),
        errors: None,
    };

    for input in &sig.inputs {
        let ty = match input {
            syn::FnArg::Receiver(_) => continue,
            syn::FnArg::Typed(arg) => {
                if arg.attrs.iter().any(is_unpack_attr) {
                    continue;
                }
                &arg.ty
            }
        };
        if tag_id(ty).is_none() {
            validator.visit_type(ty);
        }
    }

    if let syn::ReturnType::Type(_, ty) = &sig.output {
        validator.visit_type(ty);
    }

    validator.errors.map_or(Ok(()), Err)
}

fn validate_drop_impl(impl_: &syn::ItemImpl) -> Result<()> {
    const UNKNOWN_METHOD: &str = "`Drop` must have exactly one method `drop`";

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
    let mut was_receiver = false;
    for input in &method.sig.inputs {
        match input {
            syn::FnArg::Typed(arg) if tag_id(&arg.ty).is_some() => {}
            syn::FnArg::Receiver(syn::Receiver {
                kind: syn::ReceiverKind::Reference(_, _, Some(_)),
                ..
            }) => {
                if was_receiver {
                    let err_msg = "`Drop::drop` can have only one receiver argument `&mut self`";
                    return Err(Error::new_spanned(&method.sig.inputs, err_msg));
                }

                was_receiver = true;
            }
            _ => {
                let err_msg = "`Drop::drop` supports only `&mut self` and optionally a tag";
                return Err(Error::new_spanned(&method.sig.inputs, err_msg));
            }
        }
    }

    if !was_receiver {
        let err_msg = "`Drop::drop` requires a `&mut self` receiver";
        return Err(Error::new_spanned(&method.sig.inputs, err_msg));
    }

    Ok(())
}

fn validate_signature_shape(sig: &syn::Signature) -> Result<()> {
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
        validate_tag_id_pos(&arg.ty)?;
    }

    if let syn::ReturnType::Type(_, output) = &sig.output {
        if tag_id(output).is_some() {
            let err_msg = "`<dyn Type>::TAG` is not allowed in return position";
            return Err(Error::new_spanned(output, err_msg));
        }

        validate_tag_id_pos(output)?;
    }

    Ok(())
}

fn validate_pat_type_shape(arg: &syn::PatType) -> Result<()> {
    let err_msg = "patterns aren't allowed in function declarations";

    match arg.pat.as_ref() {
        syn::Pat::Ident(ident) => {
            if ident.by_ref.is_some() || ident.mutability.is_some() || ident.subpat.is_some() {
                return Err(Error::new_spanned(ident, err_msg));
            }

            Ok(())
        }
        _ => Err(Error::new_spanned(&arg.pat, err_msg)),
    }
}

fn validate_tag_id_pos(ty: &Type) -> Result<()> {
    struct NestedTagIdVisitor {
        errors: Option<Error>,
        depth: usize,
    }

    impl Visit<'_> for NestedTagIdVisitor {
        fn visit_type(&mut self, node: &Type) {
            if self.depth != 0 && tag_id(node).is_some() {
                let err_msg = "`<dyn Type>::TAG` is only allowed as a top-level function argument";
                push_error(&mut self.errors, Error::new_spanned(node, err_msg));
                return;
            }

            self.depth += 1;
            syn::visit::visit_type(self, node);
            self.depth -= 1;
        }
    }

    let mut visitor = NestedTagIdVisitor {
        errors: None,
        depth: 0,
    };

    visitor.visit_type(ty);
    if let Some(errors) = visitor.errors {
        return Err(errors);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_dispatch_param_associated_type() {
        let param: syn::TypeParam = syn::parse_quote!(T);
        let sig: syn::Signature = syn::parse_quote!(fn dispatch(value: T::Assoc));

        let err = validate_dispatch_param_positions([&param].into_iter(), &sig).unwrap_err();
        assert!(err.to_string().contains(DISPATCH_OPAQUE_NESTING_ERR));
    }

    #[test]
    fn rejects_qualified_dispatch_param_associated_type() {
        let param: syn::TypeParam = syn::parse_quote!(T);
        let sig: syn::Signature = syn::parse_quote!(fn dispatch(value: <T as Trait>::Assoc));

        let err = validate_dispatch_param_positions([&param].into_iter(), &sig).unwrap_err();
        assert!(err.to_string().contains(DISPATCH_OPAQUE_NESTING_ERR));
    }

    #[test]
    fn rejects_dispatch_param_inside_wrapper() {
        let param: syn::TypeParam = syn::parse_quote!(T = u8);
        let sig: syn::Signature = syn::parse_quote!(fn dispatch(value: Option<(u32, T)>));

        let err = validate_dispatch_param_positions([&param].into_iter(), &sig).unwrap_err();
        assert!(err.to_string().contains(DISPATCH_OPAQUE_NESTING_ERR));
    }

    #[test]
    fn accepts_dispatch_param_in_structural_containers() {
        let param: syn::TypeParam = syn::parse_quote!(T = u8);
        let sig: syn::Signature = syn::parse_quote!(fn dispatch(value: &(u32, [T; 2])));

        validate_dispatch_param_positions([&param].into_iter(), &sig).unwrap();
    }

    #[test]
    fn accepts_dispatch_param_projection_in_unpack_argument() {
        let param: syn::TypeParam = syn::parse_quote!(T = u8);
        let sig: syn::Signature = syn::parse_quote!(
            fn dispatch(#[unpack(u32, u32)] value: <T as Trait>::Assoc)
        );

        validate_dispatch_param_positions([&param].into_iter(), &sig).unwrap();
    }

    #[test]
    fn accepts_open_runtime_param_used_by_explicit_unpack() {
        let generics: syn::Generics = syn::parse_quote!(<#[erased(u8)] A>);
        let sig: syn::Signature = syn::parse_quote!(
            fn dispatch(
                tag: <dyn A>::TAG,
                #[unpack(*const core::ffi::c_void, usize)] value: A::Value,
            )
        );

        validate_callable_parameter_bounds(
            &generics,
            &crate::DispatchGroups::default(),
            &sig,
            &BTreeSet::new(),
            false,
        )
        .unwrap();
    }

    #[test]
    fn rejects_nested_tag_id_in_free_function() {
        let sig: syn::Signature = syn::parse_quote!(fn dispatch(value: (<dyn T>::TAG,)));

        let err = validate_signature_shape(&sig).unwrap_err();
        assert!(
            err.to_string()
                .contains("`<dyn Type>::TAG` is only allowed as a top-level function argument")
        );
    }

    #[test]
    fn rejects_tag_id_return_in_free_function() {
        let sig: syn::Signature = syn::parse_quote!(fn dispatch() -> <dyn T>::TAG);

        let err = validate_signature_shape(&sig).unwrap_err();
        assert!(
            err.to_string()
                .contains("`<dyn Type>::TAG` is not allowed in return position")
        );
    }

    #[test]
    fn rejects_nontrivial_argument_patterns() {
        for sig in [
            syn::parse_quote!(fn dispatch(mut value: u8)),
            syn::parse_quote!(fn dispatch(ref value: u8)),
            syn::parse_quote!(fn dispatch(value @ _: u8)),
        ] {
            let err = validate_signature_shape(&sig).unwrap_err();
            assert!(
                err.to_string()
                    .contains("patterns aren't allowed in function declarations")
            );
        }
    }

    #[test]
    fn accepts_plain_argument_patterns() {
        let sig: syn::Signature = syn::parse_quote!(fn dispatch(value: u8));

        validate_signature_shape(&sig).unwrap();
    }

    #[test]
    fn rejects_explicit_lifetimes_in_soft_arguments() {
        for sig in [
            syn::parse_quote!(fn soft<'a>(#[soft] value: &'a mut bool)),
            syn::parse_quote!(fn soft(#[soft] value: &'static mut bool)),
            syn::parse_quote!(fn soft(#[soft] value: &'_ mut bool)),
            syn::parse_quote!(fn soft<'a>(#[soft] value: Container<'a>)),
        ] {
            let err = validate_soft_lifetimes(&sig).unwrap_err();
            assert!(
                err.to_string()
                    .contains("explicit lifetimes are not allowed in a `#[soft]` argument")
            );
        }
    }

    #[test]
    fn rejects_implicitly_connected_soft_lifetime() {
        let sig: syn::Signature = syn::parse_quote!(fn soft(#[soft] value: &mut bool) -> &bool);

        let err = validate_soft_lifetimes(&sig).unwrap_err();
        assert!(
            err.to_string()
                .contains("a return value cannot borrow from a `#[soft]` argument")
        );
    }

    #[test]
    fn rejects_nested_elided_soft_lifetimes() {
        let sig: syn::Signature = syn::parse_quote!(fn soft(#[soft] value: &&bool));

        let err = validate_soft_lifetimes(&sig).unwrap_err();
        assert!(
            err.to_string()
                .contains("the lifetime of a `#[soft]` argument cannot be connected")
        );
    }

    #[test]
    fn accepts_independent_elided_soft_lifetimes() {
        let sig: syn::Signature = syn::parse_quote!(fn soft(#[soft] value: (&bool, &bool)));

        validate_soft_lifetimes(&sig).unwrap();
    }

    #[test]
    fn accepts_isolated_soft_lifetime() {
        let sig: syn::Signature = syn::parse_quote!(
            fn soft<'output>(#[soft] value: &mut bool, output: &'output bool) -> &'output bool
        );

        validate_soft_lifetimes(&sig).unwrap();
    }

    #[test]
    fn rejects_drop_without_receiver() {
        for item in [
            syn::parse_quote!(impl Drop for Value { fn drop() {} }),
            syn::parse_quote!(impl Drop for Value {
                fn drop(id: <dyn T>::TAG) {}
            }),
        ] {
            let err = validate_drop_impl(&item).unwrap_err();
            assert!(
                err.to_string()
                    .contains("`Drop::drop` requires a `&mut self` receiver")
            );
        }
    }
}
