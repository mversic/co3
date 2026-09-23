use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use proc_macro2::{Delimiter, Group, Ident, TokenStream, TokenTree};
use quote::{quote, quote_spanned};
use syn::{
    Attribute, Error, FnArg, GenericArgument, GenericParam, ItemFn, ItemImpl, LitStr, PatType,
    Result, Type, TypePath,
    parse::{Parse, ParseStream, Parser},
    parse_quote, parse_quote_spanned,
    punctuated::Punctuated,
    spanned::Spanned,
    visit::Visit,
    visit_mut::VisitMut,
};

use crate::{
    Co3Static, DeclKind, DispatchGroups, ForeignItem,
    utils::{ParamUseDetector, push_error},
    validate::unsupported_attr,
};

const FN_BODIES_NOT_ALLOWED_MSG: &str = "fn bodies are not allowed in declarations";
const ITEM_NOT_SUPPORTED_MSG: &str = "item not supported";
const EXPECTED_FEATURE_NAME_MSG: &str = "Expected feature name in `#![feature(...)]`";
const RAW_IMPL_ITEM_ATTR: &str = "raw";
pub(crate) struct ParsedInput {
    pub(crate) kind: DeclKind,
    pub(crate) abi: syn::Abi,
    pub(crate) symbol_prefix: LitStr,
    pub(crate) symbol_fragments: BTreeMap<String, LitStr>,
    pub(crate) features: MacroFeatures,
    pub(crate) failure_mode: FailureMode,
    pub(crate) attrs: Vec<Attribute>,
    pub(crate) items: Vec<ParsedItem>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MacroFeatures {
    pub(crate) extern_types: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FailureMode {
    #[default]
    Panic,
    Error,
}

struct FfiBody {
    attrs: Vec<Attribute>,
    items: Vec<ParsedItem>,
}

pub(crate) enum ParsedItem {
    Type(syn::ForeignItemType),
    Static(Co3Static),
    Impl(ItemImpl),
    Fn(ItemFn),
    Callback(CallbackDecl),
}

pub(crate) struct CallbackDecl {
    pub(crate) attrs: Vec<Attribute>,
    pub(crate) vis: syn::Visibility,
    pub(crate) callee: TokenStream,
    pub(crate) sig: syn::Signature,
    pub(crate) owner: Option<CallbackOwner>,
}

pub(crate) struct CallbackOwner {
    pub(crate) attrs: Vec<Attribute>,
    pub(crate) generics: syn::Generics,
    pub(crate) trait_path: Option<syn::Path>,
    pub(crate) self_ty: Box<Type>,
}

struct ConstGenericArgNormalizer {
    const_params: HashSet<syn::Ident>,
}

impl VisitMut for ConstGenericArgNormalizer {
    fn visit_generic_argument_mut(&mut self, node: &mut GenericArgument) {
        syn::visit_mut::visit_generic_argument_mut(self, node);

        let GenericArgument::Type(Type::Path(TypePath {
            qself: None, path, ..
        })) = node
        else {
            return;
        };
        let Some(ident) = path.get_ident() else {
            return;
        };
        if !self.const_params.contains(ident) {
            return;
        }

        *node = GenericArgument::Const(parse_quote!({#ident}));
    }
}

impl syn::parse::Parse for ParsedItem {
    fn parse(input: ParseStream) -> Result<Self> {
        let ahead = input.fork();
        let _ = ahead.call(syn::Attribute::parse_outer)?;
        if ahead.peek(syn::Token![impl]) {
            return Ok(Self::Impl(parse_impl_item(input)?));
        }

        let _ = ahead.parse::<syn::Visibility>()?;
        if ahead.peek(syn::Token![struct]) {
            return Err(ahead.error(ITEM_NOT_SUPPORTED_MSG));
        }
        if ahead.peek(syn::Token![type]) {
            let ty = if contains_dispatch_predicate(input)? {
                parse_type_item(input)?
            } else {
                input.parse::<syn::ForeignItemType>()?
            };
            return Ok(Self::Type(ty));
        }
        if ahead.peek(syn::Token![static]) {
            return Ok(Self::Static(parse_static_item(input)?));
        }
        if ahead.peek(syn::Ident) && ahead.parse::<syn::Ident>()? == "raw" {
            return Ok(Self::Callback(parse_callback_item(input)?));
        }
        if is_fn_head(&ahead)? {
            return Ok(Self::Fn(parse_fn_item(input)?));
        }

        Err(input.error(ITEM_NOT_SUPPORTED_MSG))
    }
}

impl ParsedItem {
    pub(crate) fn normalize(self) -> Result<ForeignItem> {
        match self {
            Self::Type(mut ty) => {
                let (id, id_value, covariant_lifetimes) =
                    parse_opaque_type_attrs(&mut ty.attrs, &ty.generics)?;
                Ok(ForeignItem::Type(crate::ForeignItemType {
                    ty,
                    id: id.map(Box::new),
                    id_value: id_value.map(Box::new),
                    covariant_lifetimes,
                    drop: None,
                    self_impls: Vec::new(),
                }))
            }
            Self::Static(item) => Ok(ForeignItem::Static(item)),
            Self::Impl(item) => normalize_impl(item).map(ForeignItem::Impl),
            Self::Fn(item) => normalize_fn(item).map(ForeignItem::Fn),
            Self::Callback(item) => Err(syn::Error::new_spanned(
                item.sig.ident,
                "raw function declarations must be collected before normalization",
            )),
        }
    }
}

fn parse_callback_item(input: ParseStream) -> Result<CallbackDecl> {
    let mut attrs = input.call(Attribute::parse_outer)?;
    let vis = input.parse::<syn::Visibility>()?;
    let keyword = input.parse::<syn::Ident>()?;
    debug_assert_eq!(keyword, "raw");

    let sig = parse_signature(input, &mut attrs)?;
    if input.peek(syn::token::Brace) {
        return Err(input.error(FN_BODIES_NOT_ALLOWED_MSG));
    }
    input.parse::<syn::Token![;]>()?;

    let ident = &sig.ident;
    Ok(CallbackDecl {
        attrs,
        vis,
        callee: quote!(#ident),
        sig,
        owner: None,
    })
}

fn normalize_impl(mut item: ItemImpl) -> Result<crate::Co3Impl> {
    crate::normalize_dyn_self_tag_ids(&mut item);

    let mut errors = None;
    let mut dispatch_args = match parse_dispatch_attr(&item.attrs, &item.generics) {
        Ok(groups) => groups,
        Err(err) => {
            push_error(&mut errors, err);
            Default::default()
        }
    };
    if let Err(err) = validate_dispatch_cycles(&dispatch_args, Some(&item.self_ty)) {
        push_error(&mut errors, err);
    }
    dispatch_args.concretize_self(&item.self_ty);
    if !dispatch_args.is_empty() {
        item.attrs.retain(|attr| !attr.path().is_ident("erased"));
    }
    let method_dispatch_args = match parse_dyn_methods(&mut item) {
        Ok(groups) => groups,
        Err(err) => {
            push_error(&mut errors, err);
            Default::default()
        }
    };
    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(crate::Co3Impl {
        item,
        dispatch_args,
        method_dispatch_args,
    })
}

fn normalize_fn(mut item: ItemFn) -> Result<crate::Co3Fn> {
    let dispatch_args = parse_dispatch_attr(&item.attrs, &item.sig.generics)?;
    validate_dispatch_cycles(&dispatch_args, None)?;
    if !dispatch_args.is_empty() {
        item.attrs.retain(|attr| !attr.path().is_ident("erased"));
    }

    Ok(crate::Co3Fn {
        item,
        dispatch_args,
    })
}

fn parse_dyn_methods(impl_: &mut ItemImpl) -> Result<HashMap<syn::Ident, DispatchGroups>> {
    let mut dispatch_args = HashMap::new();
    let mut errors = None;

    for item in &mut impl_.items {
        let syn::ImplItem::Fn(syn::ImplItemFn { attrs, sig, .. }) = item else {
            continue;
        };
        if !attrs.iter().any(|attr| attr.path().is_ident("erased")) {
            continue;
        }

        let mut groups = match parse_dispatch_attr(attrs, &sig.generics) {
            Ok(groups) => groups,
            Err(err) => {
                push_error(&mut errors, err);
                continue;
            }
        };
        if let Err(err) = validate_dispatch_cycles(&groups, Some(&impl_.self_ty)) {
            push_error(&mut errors, err);
        }
        groups.concretize_self(&impl_.self_ty);
        attrs.retain(|attr| !attr.path().is_ident("erased"));

        if dispatch_args.insert(sig.ident.clone(), groups).is_some() {
            return Err(syn::Error::new_spanned(&sig.ident, "duplicate method"));
        }
    }

    errors.map_or(Ok(dispatch_args), Err)
}

fn parse_static_item(input: ParseStream) -> Result<Co3Static> {
    let attrs = input.call(syn::Attribute::parse_outer)?;
    let vis = input.parse()?;
    let static_token = input.parse()?;
    let mutability = input.parse()?;
    let ident = input.parse()?;
    input.parse::<syn::Token![:]>()?;
    let ty = input.parse()?;
    let expr = if input.peek(syn::Token![=]) {
        input.parse::<syn::Token![=]>()?;
        Some(input.parse()?)
    } else {
        None
    };
    input.parse::<syn::Token![;]>()?;

    Ok(Co3Static {
        attrs,
        vis,
        static_token,
        mutability,
        ident,
        ty,
        expr,
    })
}

fn contains_dispatch_predicate(input: ParseStream) -> Result<bool> {
    let ahead = input.fork();
    let mut in_where_clause = false;

    while !ahead.is_empty() && !ahead.peek(syn::Token![;]) {
        let token = ahead.parse::<TokenTree>()?;
        match token {
            TokenTree::Ident(ident) if ident == "where" => in_where_clause = true,
            TokenTree::Punct(punct) if in_where_clause && punct.as_char() == '@' => {
                return Ok(true);
            }
            _ => {}
        }
    }

    Ok(false)
}

fn parse_items(input: ParseStream) -> Result<Vec<ParsedItem>> {
    let mut items = Vec::new();

    while !input.is_empty() {
        items.push(input.parse()?);
    }

    Ok(items)
}

impl ParsedInput {
    pub(crate) fn parse(tokens: TokenStream) -> Result<Self> {
        let FfiBody { mut attrs, items } = parse_ffi_body(tokens)?;
        let (kind, abi) = take_decl_attr(&mut attrs)?;
        let symbol_prefix =
            parse_symbol_prefix_attr(&mut attrs)?.unwrap_or_else(default_symbol_prefix);
        validate_symbol_text(&symbol_prefix, false)?;
        let symbol_fragments = parse_symbol_fragments_attr(&mut attrs)?;
        let failure_mode = parse_failure_attr(&mut attrs)?;
        let features = parse_feature_attrs(&mut attrs)?;

        Ok(Self {
            kind,
            abi,
            symbol_prefix,
            symbol_fragments,
            features,
            failure_mode,
            attrs,
            items,
        })
    }
}

fn parse_symbol_fragments_attr(attrs: &mut Vec<Attribute>) -> Result<BTreeMap<String, LitStr>> {
    struct SymbolFragmentEntry {
        key: TokenStream,
        value: LitStr,
    }

    impl Parse for SymbolFragmentEntry {
        fn parse(input: ParseStream<'_>) -> Result<Self> {
            let path = input.parse::<syn::Path>()?;
            if is_builtin_symbol_fragment_type(&path) {
                return Err(syn::Error::new_spanned(
                    path,
                    "Rust primitive types have stable, built-in symbol fragments",
                ));
            }
            let key = quote!(#path);
            input.parse::<syn::Token![=]>()?;
            let value = input.parse()?;
            Ok(Self { key, value })
        }
    }

    let mut kept = Vec::with_capacity(attrs.len());
    let mut fragments = BTreeMap::new();

    for attr in attrs.drain(..) {
        if !attr.path().is_ident("symbol_fragments") {
            kept.push(attr);
            continue;
        }

        let syn::Meta::List(list) = &attr.meta else {
            let err_msg = "expected `#![symbol_fragments(Type = \"fragment\", ...)]`";
            return Err(syn::Error::new_spanned(attr, err_msg));
        };
        let entries = Punctuated::<SymbolFragmentEntry, syn::Token![,]>::parse_terminated
            .parse2(list.tokens.clone())?;
        for entry in entries {
            let name = entry.key.to_string();
            validate_symbol_text(&entry.value, false)?;
            if fragments.insert(name.clone(), entry.value).is_some() {
                let err_msg = format!("duplicate symbol fragment for `{name}`");
                return Err(syn::Error::new_spanned(entry.key, err_msg));
            }
        }
    }

    *attrs = kept;
    Ok(fragments)
}

fn is_builtin_symbol_fragment_type(path: &syn::Path) -> bool {
    const PRIMITIVES: &[&str] = &[
        "bool", "char", "str", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32",
        "u64", "u128", "usize", "f32", "f64",
    ];

    path.get_ident()
        .is_some_and(|ident| PRIMITIVES.iter().any(|primitive| ident == primitive))
}

fn default_symbol_prefix() -> LitStr {
    LitStr::new(
        &std::env::var("CARGO_CRATE_NAME").unwrap_or_else(|_| "co3".to_owned()),
        proc_macro2::Span::call_site(),
    )
}

fn parse_ffi_body(tokens: TokenStream) -> Result<FfiBody> {
    let parser = |input: syn::parse::ParseStream| -> Result<FfiBody> {
        let mut attr_tokens = TokenStream::new();

        while input.peek(syn::Token![#]) {
            let ahead = input.fork();
            ahead.parse::<syn::Token![#]>()?;
            if !ahead.peek(syn::Token![!]) {
                break;
            }

            input.parse::<syn::Token![#]>()?;
            input.parse::<syn::Token![!]>()?;
            let content;
            syn::bracketed!(content in input);
            let tokens = normalize_extern_attr_tokens(content.parse::<TokenStream>()?);
            attr_tokens.extend(quote!(#![#tokens]));
        }

        let attrs = Attribute::parse_inner.parse2(attr_tokens)?;
        let items = parse_items(input)?;

        Ok(FfiBody { attrs, items })
    };

    parser.parse2(tokens)
}

fn take_decl_attr(attrs: &mut Vec<Attribute>) -> Result<(DeclKind, syn::Abi)> {
    let mut decl = None;

    attrs.retain(|attr| {
        if !attr.path().is_ident("unsafe") {
            return true;
        }

        let next = parse_decl_attr(attr);
        if decl.replace(next).is_some() {
            decl = Some(Err(syn::Error::new_spanned(
                &attr.meta,
                "duplicate declaration kind attribute",
            )));
        }

        false
    });

    match decl {
        Some(result) => result,
        None => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "missing `#![unsafe(export(\"...\"))]` or `#![unsafe(extern(\"...\"))]`",
        )),
    }
}

fn parse_decl_attr(attr: &Attribute) -> Result<(DeclKind, syn::Abi)> {
    let err_msg = "expected `#![unsafe(export(\"...\"))]` or `#![unsafe(extern(\"...\"))]`";

    let syn::Meta::List(list) = &attr.meta else {
        return Err(syn::Error::new_spanned(attr, err_msg));
    };

    let nested = syn::parse2::<syn::Meta>(list.tokens.clone())
        .map_err(|_| syn::Error::new_spanned(attr, err_msg))?;
    let syn::Meta::List(nested) = nested else {
        return Err(syn::Error::new_spanned(attr, err_msg));
    };

    let path = &nested.path;
    let kind = if path.is_ident("export") {
        DeclKind::Export
    } else if path.is_ident("r#extern") {
        DeclKind::Extern
    } else {
        return Err(syn::Error::new_spanned(attr, err_msg));
    };

    let abi_lit = syn::parse2::<LitStr>(nested.tokens.clone())
        .map_err(|_| syn::Error::new_spanned(attr, err_msg))?;
    let abi = syn::parse2(quote!(extern #abi_lit))?;

    Ok((kind, abi))
}

fn parse_symbol_prefix_attr(attrs: &mut Vec<Attribute>) -> Result<Option<LitStr>> {
    let mut kept = Vec::with_capacity(attrs.len());

    let mut symbol_prefix = None;
    for attr in attrs.drain(..) {
        if !attr.path().is_ident("symbol_prefix") {
            kept.push(attr);
            continue;
        }

        let err_msg = "Expected `#![symbol_prefix = \"...\"]`";
        let syn::Meta::NameValue(nv) = &attr.meta else {
            return Err(syn::Error::new_spanned(&attr, err_msg));
        };

        let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(value),
            ..
        }) = &nv.value
        else {
            return Err(syn::Error::new_spanned(&nv.value, err_msg));
        };

        if symbol_prefix.replace(value.clone()).is_some() {
            return Err(syn::Error::new_spanned(
                attr,
                "Duplicate `#![symbol_prefix = \"...\"]`",
            ));
        }
    }

    *attrs = kept;
    Ok(symbol_prefix)
}

pub(crate) fn validate_symbol_text(value: &LitStr, interpolation: bool) -> Result<()> {
    let text = value.value();
    if text.is_empty() {
        let err_msg = "symbol names cannot be empty";
        return Err(syn::Error::new_spanned(value, err_msg));
    }
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if interpolation && character == '{' {
            let mut name = String::new();
            let mut closed = false;
            while let Some(&next) = chars.peek() {
                chars.next();
                if next == '}' {
                    closed = true;
                    break;
                }
                name.push(next);
            }
            if !closed
                || name.is_empty()
                || !name.chars().enumerate().all(|(index, next)| {
                    (index == 0 && (next == '_' || next.is_ascii_alphabetic()))
                        || (index > 0 && (next == '_' || next.is_ascii_alphanumeric()))
                })
            {
                let err_msg = "symbol interpolation must use `{Ident}`";
                return Err(syn::Error::new_spanned(value, err_msg));
            }
            continue;
        }
        if character == '{' || character == '}' {
            let err_msg = "`{` and `}` are reserved for symbol interpolation";
            return Err(syn::Error::new_spanned(value, err_msg));
        }
        if !(character == '_' || character.is_ascii_alphanumeric()) {
            let err_msg = "symbol names may contain only ASCII letters, digits, and `_`";
            return Err(syn::Error::new_spanned(value, err_msg));
        }
    }
    Ok(())
}

fn parse_feature_attrs(attrs: &mut Vec<Attribute>) -> Result<MacroFeatures> {
    let mut kept = Vec::with_capacity(attrs.len());
    let mut features = MacroFeatures::default();

    for attr in attrs.drain(..) {
        if !attr.path().is_ident("feature") {
            kept.push(attr);
            continue;
        }

        let syn::Meta::List(list) = &attr.meta else {
            return Err(syn::Error::new_spanned(attr, "Expected `#![feature(...)]`"));
        };

        let metas =
            list.parse_args_with(Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)?;

        for meta in metas {
            let syn::Meta::Path(path) = &meta else {
                return Err(syn::Error::new_spanned(meta, EXPECTED_FEATURE_NAME_MSG));
            };

            let Some(ident) = path.get_ident() else {
                return Err(syn::Error::new_spanned(path, EXPECTED_FEATURE_NAME_MSG));
            };

            let feature = ident.to_string();
            let already_enabled = match feature.as_str() {
                "extern_types" => &mut features.extern_types,
                _ => {
                    let err_msg = "Only `extern_types` is supported";
                    return Err(syn::Error::new_spanned(ident, err_msg));
                }
            };

            if core::mem::replace(already_enabled, true) {
                return Err(syn::Error::new_spanned(
                    ident,
                    format!("Duplicate `{feature}` feature"),
                ));
            }
        }
    }

    *attrs = kept;
    Ok(features)
}

fn parse_failure_attr(attrs: &mut Vec<Attribute>) -> Result<FailureMode> {
    let mut kept = Vec::with_capacity(attrs.len());
    let mut failure_mode = None;

    for attr in attrs.drain(..) {
        if !attr.path().is_ident("failure") {
            kept.push(attr);
            continue;
        }

        let err_msg = "Expected `#![failure = \"panic\"]` or `#![failure = \"error\"]`";
        let syn::Meta::NameValue(nv) = &attr.meta else {
            return Err(syn::Error::new_spanned(&attr, err_msg));
        };

        let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(value),
            ..
        }) = &nv.value
        else {
            return Err(syn::Error::new_spanned(&nv.value, err_msg));
        };

        let next = match value.value().as_str() {
            "panic" => FailureMode::Panic,
            "error" => FailureMode::Error,
            _ => return Err(syn::Error::new_spanned(value, err_msg)),
        };

        if failure_mode.replace(next).is_some() {
            return Err(syn::Error::new_spanned(
                attr,
                "Duplicate `#![failure = \"...\"]`",
            ));
        }
    }

    *attrs = kept;
    Ok(failure_mode.unwrap_or_default())
}

pub(crate) fn parse_dispatch_attr(
    attrs: &[syn::Attribute],
    generics: &syn::Generics,
) -> Result<DispatchGroups> {
    let mut dispatch_attrs = attrs.iter().filter(|attr| attr.path().is_ident("erased"));
    let Some(attr) = dispatch_attrs.next() else {
        return Ok(Default::default());
    };
    if let Some(duplicate) = dispatch_attrs.next() {
        let err_msg = "duplicate tagged-dispatch predicate";
        return Err(syn::Error::new_spanned(duplicate, err_msg));
    }

    let syn::Meta::List(list) = &attr.meta else {
        let err = "tagged dispatch must provide concrete generic arguments";
        return Err(syn::Error::new_spanned(attr, err));
    };

    let groups = (|input: ParseStream| parse_dispatch_groups(generics, input))
        .parse2(list.tokens.clone())?;

    validate_dispatch_targets(generics, &groups)?;
    Ok(DispatchGroups { groups })
}

fn validate_dispatch_targets(
    generics: &syn::Generics,
    groups: &BTreeMap<Vec<syn::Ident>, Vec<syn::AngleBracketedGenericArguments>>,
) -> Result<()> {
    let mut errors = None::<syn::Error>;
    for (group, targets) in groups {
        let err_msg = format!("expected {} concrete generic argument(s)", group.len(),);

        if targets.is_empty() {
            let err = "dispatch groups must contain at least one concrete target";
            push_error(&mut errors, syn::Error::new_spanned(&group[0], err));
            continue;
        }

        for target in targets {
            if target.args.len() != group.len() {
                let err = syn::Error::new_spanned(target, &err_msg);
                push_error(&mut errors, err);
                continue;
            }

            for (param, arg) in group.iter().zip(&target.args) {
                let matches_parameter_kind = generics.params.iter().any(|generic| match generic {
                    GenericParam::Type(generic) => {
                        generic.ident == *param && matches!(arg, GenericArgument::Type(_))
                    }
                    GenericParam::Const(generic) => {
                        generic.ident == *param && matches!(arg, GenericArgument::Const(_))
                    }
                    GenericParam::Lifetime(_) => false,
                });
                if !matches_parameter_kind {
                    let err = "argument kind must match declared parameter kind";
                    let err = syn::Error::new_spanned(arg, err);

                    push_error(&mut errors, err);
                }
            }
        }
    }

    errors.map_or(Ok(()), Err)
}

fn validate_dispatch_cycles(dispatch: &DispatchGroups, self_ty: Option<&Type>) -> Result<()> {
    let mut errors = None;

    dispatch.for_each_combination(|selections| {
        if errors.is_some() {
            return;
        }

        let params = selections
            .iter()
            .flat_map(|selection| selection.params.iter().cloned())
            .collect::<BTreeSet<_>>();
        let mut dependencies = params
            .iter()
            .cloned()
            .map(|param| (param, BTreeSet::new()))
            .collect::<BTreeMap<_, _>>();

        for selection in selections {
            for (param, target) in selection.params.iter().zip(&selection.target.args) {
                let dependencies_for_param = dependencies
                    .get_mut(param)
                    .expect("dispatch parameter was collected above");
                dependencies_for_param.extend(
                    params
                        .iter()
                        .filter(|candidate| {
                            dispatch_target_mentions_param(target, candidate, self_ty)
                        })
                        .cloned(),
                );
            }
        }

        loop {
            let resolved = dependencies
                .iter()
                .filter_map(|(param, dependencies)| {
                    dependencies.is_empty().then_some(param.clone())
                })
                .collect::<Vec<_>>();
            if resolved.is_empty() {
                break;
            }
            for param in &resolved {
                dependencies.remove(param);
            }
            for dependencies in dependencies.values_mut() {
                dependencies.retain(|param| !resolved.contains(param));
            }
        }

        if let Some((param, _)) = dependencies.into_iter().next() {
            push_error(
                &mut errors,
                Error::new_spanned(param, "cyclic tagged-dispatch selection"),
            );
        }
    });

    errors.map_or(Ok(()), Err)
}

fn dispatch_target_mentions_param(
    target: &GenericArgument,
    param: &Ident,
    self_ty: Option<&Type>,
) -> bool {
    if ParamUseDetector::new([param]).generic_arg_mentions_param(target) {
        return true;
    }

    let Some(self_ty) = self_ty else {
        return false;
    };
    let mut uses_self = UsesSelf::default();
    uses_self.visit_generic_argument(target);
    uses_self.seen && ParamUseDetector::new([param]).type_mentions_param(self_ty)
}

#[derive(Default)]
struct UsesSelf {
    seen: bool,
}

impl<'ast> Visit<'ast> for UsesSelf {
    fn visit_type(&mut self, ty: &'ast Type) {
        if matches!(ty, Type::Path(TypePath { qself: None, path, .. }) if path.segments.first().is_some_and(|segment| segment.ident == "Self"))
        {
            self.seen = true;
        }
        syn::visit::visit_type(self, ty);
    }
}

fn parse_dispatch_groups(
    generics: &syn::Generics,
    input: ParseStream,
) -> Result<BTreeMap<Vec<syn::Ident>, Vec<syn::AngleBracketedGenericArguments>>> {
    let mut assigned = HashSet::new();
    let mut groups = BTreeMap::new();

    let declared_types = generics
        .params
        .iter()
        .filter_map(|param| match param {
            GenericParam::Type(param) => Some(param.ident.clone()),
            GenericParam::Const(_) | GenericParam::Lifetime(_) => None,
        })
        .collect::<HashSet<_>>();

    let declared_consts = generics
        .params
        .iter()
        .filter_map(|param| match param {
            GenericParam::Const(param) => Some(param.ident.clone()),
            GenericParam::Type(_) | GenericParam::Lifetime(_) => None,
        })
        .collect::<HashSet<_>>();

    while !input.is_empty() {
        let dispatch_params = input.parse::<syn::PreciseCapture>()?;

        input.parse::<syn::Token![@]>()?;
        let targets;
        syn::parenthesized!(targets in input);
        let generic_args = Punctuated::<_, syn::Token![|]>::parse_terminated(&targets)?;

        let group = parse_dispatch_group(
            &declared_types,
            &declared_consts,
            &mut assigned,
            &dispatch_params,
        )?;
        groups.insert(group, generic_args.into_iter().collect());

        if input.is_empty() {
            break;
        }
        input.parse::<syn::Token![,]>()?;
    }

    Ok(groups)
}

fn parse_dispatch_group(
    declared_types: &HashSet<syn::Ident>,
    declared_consts: &HashSet<syn::Ident>,
    assigned: &mut HashSet<syn::Ident>,
    params: &syn::PreciseCapture,
) -> Result<Vec<syn::Ident>> {
    let mut group = Vec::with_capacity(params.params.len());
    let mut errors = None;

    if params.params.is_empty() {
        let err = "dispatch groups must contain at least one parameter";
        return Err(syn::Error::new_spanned(params, err));
    }

    for param in &params.params {
        let Some(ident) = (match param {
            syn::CapturedParam::Ident(ident) => Some(ident.clone()),
            syn::CapturedParam::Lifetime(lifetime) => {
                let err = "dispatch parameters cannot be lifetimes";
                push_error(&mut errors, syn::Error::new_spanned(lifetime, err));
                None
            }
            _ => {
                let err = "dispatch parameters must be type or const parameters";
                push_error(&mut errors, syn::Error::new_spanned(param, err));
                None
            }
        }) else {
            continue;
        };

        if !declared_types.contains(&ident) && !declared_consts.contains(&ident) {
            let err = "dispatch parameter is not declared on this item";
            push_error(&mut errors, syn::Error::new_spanned(ident, err));
            continue;
        }
        if !assigned.insert(ident.clone()) {
            let err_msg = "dispatch parameter cannot appear in more than one group";
            push_error(&mut errors, syn::Error::new_spanned(ident, err_msg));
            continue;
        }

        group.push(ident);
    }

    errors.map_or(Ok(group), Err)
}

pub(crate) fn parse_opaque_type_attrs(
    attrs: &mut Vec<syn::Attribute>,
    generics: &syn::Generics,
) -> Result<(Option<syn::Type>, Option<syn::Expr>, Vec<syn::Lifetime>)> {
    let mut kept = Vec::with_capacity(attrs.len());

    let mut tag_ty = None;
    let mut tag_value = None;
    let mut covariant_lifetimes = Vec::new();
    for attr in attrs.drain(..) {
        if attr.path().is_ident("tag") {
            let (ty, value) = attr.parse_args_with(crate::tag::parse_tag_args)?;
            if tag_ty.replace(ty).is_some() {
                return Err(syn::Error::new_spanned(attr, "duplicate `#[tag(...)]`"));
            }
            tag_value = value;
            continue;
        }

        if !attr.path().is_ident("covariant") && !attr.path().is_ident("unsafe") {
            kept.push(attr);
            continue;
        }

        let syn::Meta::List(list) = &attr.meta else {
            return Err(unsupported_attr(&attr));
        };

        let list = if list.path.is_ident("unsafe") {
            let nested = syn::parse2::<syn::Meta>(list.tokens.clone())
                .map_err(|_| unsupported_attr(&attr))?;
            let syn::Meta::List(nested) = nested else {
                return Err(unsupported_attr(&attr));
            };
            nested
        } else {
            return Err(unsupported_attr(&attr));
        };

        if list.path.is_ident("covariant") {
            let lifetimes = list
                .parse_args_with(
                    syn::punctuated::Punctuated::<syn::Lifetime, syn::Token![,]>::parse_terminated,
                )
                .map_err(|_| {
                    syn::Error::new_spanned(&attr, "expected `#[unsafe(covariant('a, ...))]`")
                })?;
            if lifetimes.is_empty() {
                return Err(syn::Error::new_spanned(
                    attr,
                    "covariant lifetime list must not be empty",
                ));
            }
            for lifetime in lifetimes {
                if !generics
                    .lifetimes()
                    .any(|param| param.lifetime.ident == lifetime.ident)
                {
                    return Err(syn::Error::new_spanned(
                        lifetime,
                        "covariant lifetime is not declared on this opaque type",
                    ));
                }
                if covariant_lifetimes
                    .iter()
                    .any(|existing: &syn::Lifetime| existing.ident == lifetime.ident)
                {
                    return Err(syn::Error::new_spanned(
                        lifetime,
                        "duplicate covariant lifetime",
                    ));
                }
                covariant_lifetimes.push(lifetime);
            }
            continue;
        }

        return Err(unsupported_attr(&attr));
    }

    *attrs = kept;
    Ok((tag_ty, tag_value, covariant_lifetimes))
}

fn normalize_extern_attr_tokens(tokens: TokenStream) -> TokenStream {
    tokens
        .into_iter()
        .map(|token| match token {
            TokenTree::Ident(ident) if ident == "extern" => {
                let mut ident = Ident::new_raw("extern", ident.span());

                ident.set_span(ident.span());
                TokenTree::Ident(ident)
            }
            TokenTree::Group(group) => {
                let mut normalized = Group::new(
                    match group.delimiter() {
                        Delimiter::Parenthesis => Delimiter::Parenthesis,
                        Delimiter::Brace => Delimiter::Brace,
                        Delimiter::Bracket => Delimiter::Bracket,
                        Delimiter::None => Delimiter::None,
                    },
                    normalize_extern_attr_tokens(group.stream()),
                );
                normalized.set_span(group.span());
                TokenTree::Group(normalized)
            }
            token => token,
        })
        .collect()
}

fn is_fn_head(input: syn::parse::ParseStream) -> syn::Result<bool> {
    let ahead = input.fork();

    let _ = ahead.parse::<Option<syn::Token![const]>>()?;
    let _ = ahead.parse::<Option<syn::Token![async]>>()?;
    let _ = ahead.parse::<Option<syn::Token![unsafe]>>()?;
    let _ = ahead.parse::<Option<syn::Abi>>()?;
    let _ = ahead.parse::<Option<syn::Token![move]>>()?;

    Ok(ahead.peek(syn::Token![fn]))
}

fn preprocess_dispatch_where_clause(
    header: TokenStream,
) -> syn::Result<(TokenStream, Vec<Attribute>)> {
    fn split_top_level(tokens: TokenStream, separator: char) -> Vec<TokenStream> {
        let mut parts = Vec::new();
        let mut angle_depth = 0usize;
        for token in tokens {
            if parts.is_empty() {
                parts.push(TokenStream::new());
            }
            match &token {
                proc_macro2::TokenTree::Punct(punct) if punct.as_char() == '<' => angle_depth += 1,
                proc_macro2::TokenTree::Punct(punct) if punct.as_char() == '>' => {
                    angle_depth = angle_depth.saturating_sub(1)
                }
                proc_macro2::TokenTree::Punct(punct)
                    if punct.as_char() == separator && angle_depth == 0 =>
                {
                    parts.push(TokenStream::new());
                    continue;
                }
                _ => {}
            }
            parts.last_mut().unwrap().extend(core::iter::once(token));
        }
        parts
    }

    fn is_where(token: &proc_macro2::TokenTree) -> bool {
        matches!(token, proc_macro2::TokenTree::Ident(ident) if ident == "where")
    }

    fn normalize_targets(tokens: TokenStream) -> syn::Result<Vec<TokenStream>> {
        let tokens = tokens.into_iter().collect::<Vec<_>>();
        let target_tokens = if let [proc_macro2::TokenTree::Group(group)] = tokens.as_slice()
            && group.delimiter() == proc_macro2::Delimiter::Parenthesis
        {
            split_top_level(group.stream(), '|')
        } else {
            vec![tokens.into_iter().collect()]
        };

        for target in &target_tokens {
            syn::parse2::<syn::AngleBracketedGenericArguments>(target.clone()).map_err(|_| {
                let err = "expected a tagged-dispatch type list such as `<Type>`";
                syn::Error::new_spanned(target, err)
            })?;
        }

        Ok(target_tokens)
    }

    let tokens = header.into_iter().collect::<Vec<_>>();
    let Some(where_idx) = tokens.iter().position(is_where) else {
        return Ok((tokens.into_iter().collect(), Vec::new()));
    };

    let prefix = tokens[..where_idx].iter().cloned().collect::<TokenStream>();
    let predicates = tokens[where_idx + 1..]
        .iter()
        .cloned()
        .collect::<TokenStream>();

    let mut dispatch_span = None;
    let mut kept = Vec::new();

    let mut dispatch_predicates = Vec::new();
    for predicate in split_top_level(predicates, ',') {
        if predicate.is_empty() {
            continue;
        }

        let predicate_tokens = predicate.clone().into_iter().collect::<Vec<_>>();
        let Some(at_idx) = predicate_tokens.iter().position(
            |token| matches!(token, proc_macro2::TokenTree::Punct(punct) if punct.as_char() == '@'),
        ) else {
            if !dispatch_predicates.is_empty() {
                let err = "tagged-dispatch predicates must be last in a `where` clause";
                return Err(syn::Error::new_spanned(predicate, err));
            }
            kept.push(predicate);
            continue;
        };

        let params = predicate_tokens[..at_idx]
            .iter()
            .cloned()
            .collect::<TokenStream>();
        let params = syn::parse2::<syn::PreciseCapture>(params).map_err(|_| {
            let err_msg = "expected dispatch parameters such as `use<T>` before `@`";
            syn::Error::new_spanned(&predicate, err_msg)
        })?;
        for param in &params.params {
            if let syn::CapturedParam::Lifetime(lifetime) = param {
                let err = "dispatch parameters cannot be lifetimes";
                return Err(syn::Error::new_spanned(lifetime, err));
            }
        }
        let targets = predicate_tokens[at_idx + 1..]
            .iter()
            .cloned()
            .collect::<TokenStream>();
        let targets = normalize_targets(targets)?;
        dispatch_span.get_or_insert(predicate.span());
        dispatch_predicates.push(quote!(#params @ (#(#targets)|*)));
    }

    let mut attrs = Vec::new();
    if !dispatch_predicates.is_empty() {
        let span = dispatch_span.expect("dispatch predicate has a span");
        attrs.push(parse_quote_spanned!(span=> #[erased(#(#dispatch_predicates),*)]));
    }

    if kept.is_empty() {
        return Ok((prefix, attrs));
    }
    Ok((quote!(#prefix where #(#kept),*), attrs))
}

fn preprocess_dispatch_params(header: TokenStream) -> syn::Result<TokenStream> {
    let mut out = TokenStream::new();
    let tokens = header.into_iter().collect::<Vec<_>>();
    let mut angle_depth = 0usize;
    let mut at_param_start = false;
    let mut generic_params_finished = false;

    let mut idx = 0usize;
    while idx < tokens.len() {
        let tt = tokens[idx].clone();

        if generic_params_finished {
            out.extend(std::iter::once(tt));
            idx += 1;
            continue;
        }

        match &tt {
            proc_macro2::TokenTree::Group(group)
                if angle_depth == 0 && group.delimiter() == proc_macro2::Delimiter::Parenthesis =>
            {
                generic_params_finished = true;
                out.extend(std::iter::once(tt));
                idx += 1;
            }
            proc_macro2::TokenTree::Punct(punct) if punct.as_char() == '<' => {
                angle_depth += 1;
                at_param_start = angle_depth == 1;
                out.extend(std::iter::once(tt));
                idx += 1;
            }
            proc_macro2::TokenTree::Punct(punct) if punct.as_char() == '>' => {
                angle_depth = angle_depth.saturating_sub(1);
                at_param_start = false;
                out.extend(std::iter::once(tt));
                idx += 1;
            }
            proc_macro2::TokenTree::Punct(punct) if punct.as_char() == ',' && angle_depth == 1 => {
                at_param_start = true;
                out.extend(std::iter::once(tt));
                idx += 1;
            }
            proc_macro2::TokenTree::Ident(ident)
                if angle_depth == 1 && at_param_start && ident == "dyn" =>
            {
                let err_msg = "tagged-dispatch type parameters must use `dyn(TagTy) T`";

                let Some(proc_macro2::TokenTree::Group(group)) = tokens.get(idx + 1) else {
                    return Err(syn::Error::new(ident.span(), err_msg));
                };
                if group.delimiter() != proc_macro2::Delimiter::Parenthesis {
                    return Err(syn::Error::new(group.span(), err_msg));
                }

                let repr = syn::parse2::<Type>(group.stream()).map_err(|_| {
                    syn::Error::new(group.span(), "expected id repr in `dyn(TagTy) T`")
                })?;

                out.extend(quote_spanned!(ident.span()=> #[erased(#repr)]));
                at_param_start = true;
                idx += 2;
            }
            proc_macro2::TokenTree::Punct(punct) if angle_depth == 1 && punct.as_char() == '#' => {
                at_param_start = false;
                out.extend(std::iter::once(tt));
                idx += 1;
            }
            proc_macro2::TokenTree::Ident(_) if angle_depth == 1 && at_param_start => {
                at_param_start = false;
                out.extend(std::iter::once(tt));
                idx += 1;
            }
            _ => {
                out.extend(std::iter::once(tt));
                idx += 1;
            }
        }
    }

    Ok(out)
}

struct PreprocessedArg {
    tokens: TokenStream,
}

fn push_by_val(attrs: &mut Vec<Attribute>, span: proc_macro2::Span) {
    if !attrs.iter().any(|attr| attr.path().is_ident("by_val")) {
        attrs.push(parse_quote_spanned!(span=> #[by_val]));
    }
}

fn parse_move_by_val(
    input: syn::parse::ParseStream,
    attrs: &mut Vec<Attribute>,
) -> syn::Result<bool> {
    if input.peek(syn::Token![move]) {
        let move_token = input.parse::<syn::Token![move]>()?;
        push_by_val(attrs, move_token.span);
        return Ok(true);
    }

    Ok(false)
}

impl PreprocessedArg {
    fn parse_with(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let attrs = input.call(syn::Attribute::parse_outer)?;
        let mut merged_attrs = attrs;
        let _ = parse_move_by_val(input, &mut merged_attrs)?;

        let receiver = if input.peek(syn::Token![&]) {
            let ahead = input.fork();
            let _ = ahead.parse::<syn::Token![&]>()?;
            let _ = ahead.parse::<Option<syn::Lifetime>>()?;
            let _ = ahead.parse::<Option<syn::Token![mut]>>()?;
            ahead.peek(syn::Token![self])
        } else if input.peek(syn::Token![self]) {
            let ahead = input.fork();
            let _ = ahead.parse::<syn::Token![self]>()?;
            !ahead.peek(syn::Token![:])
        } else {
            false
        };

        if receiver {
            let mut receiver = input.parse::<syn::Receiver>()?;
            merged_attrs.append(&mut receiver.attrs);
            let ty = if let syn::ReceiverKind::Reference(and_token, lifetime, mutability) =
                receiver.kind
            {
                Type::Reference(syn::TypeReference {
                    attrs: Vec::new(),
                    and_token,
                    lifetime,
                    mutability,
                    elem: Box::new(parse_quote!(Self)),
                })
            } else {
                parse_quote!(Self)
            };
            return Ok(Self {
                tokens: quote!(#(#merged_attrs)* __co3_self: #ty),
            });
        }

        if input.peek(syn::Token![self]) {
            let ahead = input.fork();
            let _ = ahead.parse::<syn::Token![self]>()?;
            if ahead.peek(syn::Token![:]) {
                input.parse::<syn::Token![self]>()?;
                input.parse::<syn::Token![:]>()?;
                let ty = input.parse::<Type>()?;
                return Ok(Self {
                    tokens: quote! { #(#merged_attrs)* __co3_self: #ty },
                });
            }
        }

        let pat = syn::Pat::parse_single(input)?;
        let colon_token = input.parse::<syn::Token![:]>()?;
        let ty = input.parse::<Type>()?;
        let arg = PatType {
            attrs: merged_attrs,
            pat: Box::new(pat),
            colon_token,
            ty: Box::new(ty),
        };
        Ok(Self {
            tokens: quote!(#arg),
        })
    }
}

fn preprocess_signature_inputs(inputs: TokenStream) -> syn::Result<TokenStream> {
    let parser = move |input: syn::parse::ParseStream| -> syn::Result<TokenStream> {
        let mut args = Vec::new();
        while !input.is_empty() {
            args.push(PreprocessedArg::parse_with(input)?.tokens);
            if input.is_empty() {
                break;
            }
            input.parse::<syn::Token![,]>()?;
        }
        Ok(quote!(#(#args),*))
    };
    parser.parse2(inputs)
}

fn preprocess_signature_tokens(
    signature_tokens: TokenStream,
    attrs: &mut Vec<Attribute>,
) -> syn::Result<TokenStream> {
    let mut rewritten = Vec::new();
    let mut saw_inputs = false;
    let mut saw_fn = false;
    let mut by_val = false;
    let mut angle_depth = 0usize;
    let mut signature_tokens = signature_tokens.into_iter().peekable();
    while let Some(tt) = signature_tokens.next() {
        if !saw_fn && let proc_macro2::TokenTree::Ident(ident) = &tt {
            if ident == "move"
                && signature_tokens.peek().is_some_and(
                    |next| matches!(next, proc_macro2::TokenTree::Ident(next) if next == "fn"),
                )
            {
                if by_val {
                    return Err(syn::Error::new(ident.span(), "duplicate `move fn`"));
                }
                by_val = true;
                push_by_val(attrs, ident.span());
                continue;
            }
            if ident == "fn" {
                saw_fn = true;
            }
        }
        if let proc_macro2::TokenTree::Punct(punct) = &tt {
            match punct.as_char() {
                '<' => angle_depth += 1,
                '>' => angle_depth = angle_depth.saturating_sub(1),
                _ => {}
            }
        }
        if let proc_macro2::TokenTree::Group(group) = &tt
            && group.delimiter() == proc_macro2::Delimiter::Parenthesis
            && saw_fn
            && angle_depth == 0
            && !saw_inputs
        {
            let rewritten_args = preprocess_signature_inputs(group.stream())?;
            let mut new_group =
                proc_macro2::Group::new(proc_macro2::Delimiter::Parenthesis, rewritten_args);
            new_group.set_span(group.span());
            rewritten.push(proc_macro2::TokenTree::Group(new_group));
            saw_inputs = true;
            continue;
        }
        rewritten.push(tt);
    }

    Ok(rewritten.into_iter().collect())
}

fn parse_signature(
    input: syn::parse::ParseStream,
    attrs: &mut Vec<Attribute>,
) -> syn::Result<syn::Signature> {
    let mut signature_tokens = TokenStream::new();
    let mut angle_depth = 0usize;
    while !input.peek(syn::Token![;]) && !(angle_depth == 0 && input.peek(syn::token::Brace)) {
        let tt: proc_macro2::TokenTree = input.parse()?;
        if let proc_macro2::TokenTree::Punct(punct) = &tt {
            match punct.as_char() {
                '<' => angle_depth += 1,
                '>' => angle_depth = angle_depth.saturating_sub(1),
                _ => {}
            }
        }
        signature_tokens.extend(std::iter::once(tt));
    }

    let (signature_tokens, dispatch_attrs) = preprocess_dispatch_where_clause(signature_tokens)?;
    attrs.extend(dispatch_attrs);
    let signature_tokens = preprocess_dispatch_params(signature_tokens)?;
    let rewritten = preprocess_signature_tokens(signature_tokens, attrs)?;
    let mut sig = syn::parse2::<syn::Signature>(rewritten)?;
    rewrite_erased_param_bounds_to_where_clause(&mut sig.generics);
    normalize_const_args_in_fn(&mut sig);

    Ok(sig)
}

fn parse_type_item(input: ParseStream) -> syn::Result<syn::ForeignItemType> {
    let mut attrs = input.call(syn::Attribute::parse_outer)?;
    let mut type_tokens = TokenStream::new();
    while !input.peek(syn::Token![;]) {
        let token = input.parse::<TokenTree>()?;
        type_tokens.extend(core::iter::once(token));
    }
    input.parse::<syn::Token![;]>()?;

    let (type_tokens, dispatch_attrs) = preprocess_dispatch_where_clause(type_tokens)?;
    attrs.extend(dispatch_attrs);
    syn::parse2(quote!(#(#attrs)* #type_tokens;))
}

fn parse_fn_item(input: syn::parse::ParseStream) -> syn::Result<ItemFn> {
    let mut attrs = input.call(syn::Attribute::parse_outer)?;
    let vis = input.parse::<syn::Visibility>()?;
    let sig = parse_signature(input, &mut attrs)?;
    if input.peek(syn::token::Brace) {
        return Err(input.error(FN_BODIES_NOT_ALLOWED_MSG));
    }
    input.parse::<syn::Token![;]>()?;
    syn::parse2(quote! {
        #(#attrs)* #vis #sig {}
    })
}

fn rewrite_erased_param_bounds_to_where_clause(generics: &mut syn::Generics) {
    let mut erased_bounds = Vec::<syn::WherePredicate>::new();

    for param in generics.type_params_mut() {
        let ident = &param.ident;

        if !param.attrs.iter().any(crate::utils::is_type_erased) {
            continue;
        }

        let bounds = core::mem::take(&mut param.bounds)
            .into_iter()
            .collect::<Vec<_>>();

        if !bounds.is_empty() {
            erased_bounds.push(parse_quote!(#ident: #(#bounds)+*));
        }
    }

    generics
        .make_where_clause()
        .predicates
        .extend(erased_bounds);
}

fn parse_impl_item(input: syn::parse::ParseStream) -> syn::Result<ItemImpl> {
    fn preprocess_impl_items(input: syn::parse::ParseStream) -> syn::Result<TokenStream> {
        let mut out = TokenStream::new();
        while !input.is_empty() {
            while input.peek(syn::Token![;]) {
                let _: syn::Token![;] = input.parse()?;
            }
            if input.is_empty() {
                break;
            }

            let mut attrs = input.call(syn::Attribute::parse_outer)?;
            let vis = input.parse::<syn::Visibility>()?;
            let ahead = input.fork();
            if ahead.peek(syn::Ident) && ahead.parse::<syn::Ident>()? == "raw" {
                let _: syn::Ident = input.parse()?;
                let sig = parse_signature(input, &mut attrs)?;
                if input.peek(syn::token::Brace) {
                    return Err(input.error(FN_BODIES_NOT_ALLOWED_MSG));
                }
                input.parse::<syn::Token![;]>()?;
                let marker = Ident::new(RAW_IMPL_ITEM_ATTR, proc_macro2::Span::call_site());
                attrs.push(parse_quote!(#[#marker]));
                out.extend(quote!(#(#attrs)* #vis #sig {}));
                continue;
            }
            if ahead.peek(syn::Token![type]) {
                let item = input.parse::<syn::ImplItemType>()?;
                out.extend(quote!(#(#attrs)* #vis #item));
                continue;
            }
            if ahead.peek(syn::Token![const]) && !is_fn_head(&ahead)? {
                let item = input.parse::<syn::ImplItemConst>()?;
                out.extend(quote!(#(#attrs)* #vis #item));
                continue;
            }

            let sig = parse_signature(input, &mut attrs)?;
            let body = if input.peek(syn::token::Brace) {
                return Err(input.error(FN_BODIES_NOT_ALLOWED_MSG));
            } else {
                input.parse::<syn::Token![;]>()?;
                quote!({})
            };
            out.extend(quote!(#(#attrs)* #vis #sig #body));
        }
        Ok(out)
    }

    let mut attrs = input.call(syn::Attribute::parse_outer)?;
    let defaultness = input.parse::<Option<syn::Token![default]>>()?;
    let unsafety = input.parse::<Option<syn::Token![unsafe]>>()?;
    input.parse::<syn::Token![impl]>()?;

    let mut header = TokenStream::new();
    while !input.peek(syn::token::Brace) {
        let tt: proc_macro2::TokenTree = input.parse()?;
        header.extend(std::iter::once(tt));
    }

    let content;
    syn::braced!(content in input);

    let items = preprocess_impl_items(&content)?;
    let (header, dispatch_attrs) = preprocess_dispatch_where_clause(header)?;
    attrs.extend(dispatch_attrs);
    let header = preprocess_dispatch_params(header)?;

    let impl_tokens = quote! {
        #(#attrs)* #defaultness #unsafety impl #header {
            #items
        }
    };

    let mut impl_: ItemImpl = syn::parse2(impl_tokens)?;
    rewrite_erased_param_bounds_to_where_clause(&mut impl_.generics);
    normalize_const_generic_args_in_impl(&mut impl_);

    normalize_self_tag_ids(&mut impl_);

    for item in &mut impl_.items {
        let syn::ImplItem::Fn(method) = item else {
            continue;
        };

        restore_synthetic_receiver(&mut method.sig);
        normalize_const_args_in_fn(&mut method.sig);
    }

    Ok(impl_)
}

fn normalize_self_tag_ids(impl_: &mut ItemImpl) {
    struct SelfTagIdNormalizer {
        self_ty: syn::Type,
    }

    impl VisitMut for SelfTagIdNormalizer {
        fn visit_type_mut(&mut self, node: &mut Type) {
            syn::visit_mut::visit_type_mut(self, node);

            let self_ty = &self.self_ty;
            if *node == parse_quote! { <dyn Self>::TAG } {
                *node = if matches!(self_ty, Type::TraitObject(_)) {
                    parse_quote_spanned!(node.span()=> <#self_ty>::TAG)
                } else {
                    parse_quote_spanned!(node.span()=> <dyn #self_ty>::TAG)
                };
            }
        }
    }

    let mut normalizer = SelfTagIdNormalizer {
        self_ty: (*impl_.self_ty).clone(),
    };

    normalizer.visit_item_impl_mut(impl_);
}

fn restore_synthetic_receiver(signature: &mut syn::Signature) {
    let Some(position) = signature.inputs.iter().position(|input| {
        matches!(
            input,
            FnArg::Typed(PatType { pat, .. })
                if matches!(pat.as_ref(), syn::Pat::Ident(pat_ident) if pat_ident.ident == "__co3_self")
        )
    }) else {
        return;
    };

    let mut inputs = core::mem::take(&mut signature.inputs)
        .into_iter()
        .collect::<Vec<_>>();

    let Some(FnArg::Typed(PatType { attrs, ty, .. })) = inputs.get_mut(position) else {
        signature.inputs = inputs.into_iter().collect();
        return;
    };

    let receiver_span = ty.span();
    let mut receiver: syn::Receiver = match &**ty {
        Type::Path(ty_path) if ty_path.qself.is_none() && ty_path.path.is_ident("Self") => {
            parse_quote_spanned!(receiver_span=> self)
        }
        Type::Reference(ty)
            if matches!(
                ty.elem.as_ref(),
                Type::Path(ty_path) if ty_path.qself.is_none() && ty_path.path.is_ident("Self")
            ) =>
        {
            let lifetime = &ty.lifetime;

            if ty.mutability.is_some() {
                parse_quote_spanned!(receiver_span=> &#lifetime mut self)
            } else {
                parse_quote_spanned!(receiver_span=> &#lifetime self)
            }
        }
        ty => {
            parse_quote_spanned!(receiver_span=> self: #ty)
        }
    };

    receiver.attrs = core::mem::take(attrs);
    inputs[position] = FnArg::Receiver(receiver);
    signature.inputs = inputs.into_iter().collect();
}

fn normalize_const_generic_args_in_impl(impl_: &mut ItemImpl) {
    let const_params = impl_
        .generics
        .params
        .iter()
        .filter_map(|param| match param {
            GenericParam::Const(param) => Some(param.ident.clone()),
            GenericParam::Lifetime(_) | GenericParam::Type(_) => None,
        })
        .collect();

    ConstGenericArgNormalizer { const_params }.visit_item_impl_mut(impl_);
}

fn normalize_const_args_in_fn(sig: &mut syn::Signature) {
    let const_params = sig
        .generics
        .params
        .iter()
        .filter_map(|param| match param {
            GenericParam::Const(param) => Some(param.ident.clone()),
            GenericParam::Lifetime(_) | GenericParam::Type(_) => None,
        })
        .collect();

    ConstGenericArgNormalizer { const_params }.visit_signature_mut(sig);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_move_self_receiver() {
        let impl_ = "impl Value { fn name(move self); }";
        let item = Parser::parse_str(parse_impl_item, impl_).unwrap();

        let syn::ImplItem::Fn(method) = &item.items[0] else {
            panic!("expected method");
        };
        let FnArg::Receiver(receiver) = &method.sig.inputs[0] else {
            panic!("expected receiver");
        };

        assert!(
            receiver
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("by_val"))
        );
    }

    #[test]
    fn parses_move_fn_return() {
        let item = Parser::parse_str(parse_fn_item, "move fn name() -> Value;").unwrap();

        assert!(item.attrs.iter().any(|attr| attr.path().is_ident("by_val")));
    }

    #[test]
    fn leaves_implicit_ownership_unmarked_during_parsing() {
        let item = Parser::parse_str(
            parse_fn_item,
            "fn name(a: *const u8, b: *mut u8, c: extern \"C\" fn(u8) -> u8, d: (*const u8));",
        )
        .unwrap();

        for input in &item.sig.inputs {
            let FnArg::Typed(input) = input else {
                panic!("expected typed argument");
            };
            assert!(
                !input
                    .attrs
                    .iter()
                    .any(|attr| attr.path().is_ident("by_val"))
            );
        }
    }

    #[test]
    fn parses_braced_const_generic_in_free_fn_signature() {
        Parser::parse_str(
            parse_fn_item,
            "fn name<const N: usize>(value: Foo<{ N + 1 }>);",
        )
        .unwrap();
    }

    #[test]
    fn parses_braced_const_generic_in_impl_method_signature() {
        Parser::parse_str(
            parse_impl_item,
            "impl<const N: usize> Trait for Value { fn name(value: Foo<{ N + 1 }>); }",
        )
        .unwrap();
    }

    #[test]
    fn parses_qualified_move_fn_return() {
        let item = Parser::parse_str(
            parse_fn_item,
            "unsafe extern \"C\" move fn name() -> Value;",
        )
        .unwrap();

        assert!(item.attrs.iter().any(|attr| attr.path().is_ident("by_val")));
        assert!(matches!(item.sig.safety, syn::Safety::Unsafe(_)));
        assert!(item.sig.abi.is_some());
    }

    #[test]
    fn rejects_reordered_move_fn_return() {
        Parser::parse_str(parse_fn_item, "move unsafe fn name() -> Value;").unwrap_err();
        Parser::parse_str(parse_fn_item, "move async fn name() -> Value;").unwrap_err();
    }

    #[test]
    fn parses_dyn_dispatch_impl_param() {
        let impl_ = "impl<dyn(u8) T> Trait<T> for Value<T> { fn name(&mut self, value: &T); }";
        let item = Parser::parse_str(parse_impl_item, impl_).unwrap();

        let syn::GenericParam::Type(param) = item.generics.params.first().unwrap() else {
            panic!("expected type param");
        };
        assert!(
            param
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("erased"))
        );
    }

    #[test]
    fn parses_tuple_default_before_function_inputs() {
        let item =
            Parser::parse_str(parse_fn_item, "fn name<dyn(u8) T = (u8, i8)>(value: T);").unwrap();

        let syn::GenericParam::Type(param) = item.sig.generics.params.first().unwrap() else {
            panic!("expected type param");
        };
        assert_eq!(param.default.as_ref().unwrap().1, parse_quote!((u8, i8)));
    }

    #[test]
    fn restores_receiver_after_id_arg() {
        let impl_ = "impl<dyn(u8) T> Trait<T> for Value { fn name(self_id: <dyn Self>::TAG, &mut self, value: &T); }";
        let item = Parser::parse_str(parse_impl_item, impl_).unwrap();

        let syn::ImplItem::Fn(method) = &item.items[0] else {
            panic!("expected method");
        };
        let FnArg::Receiver(receiver) = method.sig.inputs.iter().nth(1).unwrap() else {
            panic!("expected receiver");
        };

        assert!(matches!(
            receiver.kind,
            syn::ReceiverKind::Reference(_, _, Some(_))
        ));
    }

    #[test]
    fn normalizes_dyn_self_tag_id() {
        let impl_ = "impl<dyn(u32) U> Trait<T> for U { fn name(self_id: <dyn Self>::TAG); }";
        let item = Parser::parse_str(parse_impl_item, impl_).unwrap();

        let syn::ImplItem::Fn(method) = &item.items[0] else {
            panic!("expected method");
        };
        let FnArg::Typed(syn::PatType { ty, .. }) = &method.sig.inputs[0] else {
            panic!("expected typed arg");
        };

        assert_eq!(**ty, parse_quote! { <dyn U>::TAG });
    }

    #[test]
    fn parses_dispatched_dyn_self_type_param() {
        let impl_ = "impl<T> Trait for dyn T { fn name(&self); }";
        Parser::parse_str(parse_impl_item, impl_).unwrap();
    }

    #[test]
    fn rejects_where_predicate_after_dispatch_predicate() {
        let err = preprocess_dispatch_where_clause(quote! {
            fn name<T>() where use<T> @ <u32>, T: Copy
        })
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("tagged-dispatch predicates must be last in a `where` clause")
        );
    }

    #[test]
    fn rejects_lifetime_dispatch_parameter() {
        let err = preprocess_dispatch_where_clause(quote! {
            fn name<'a, T>() where use<'a, T> @ <u32>
        })
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("dispatch parameters cannot be lifetimes")
        );
    }

    #[test]
    fn parses_statically_selected_const_generic() {
        let generics: syn::Generics = parse_quote!(<const N: usize>);
        let attrs: Vec<syn::Attribute> = vec![parse_quote! {
            #[erased(use<N> @ (<1> | <2>))]
        }];

        let dispatch = parse_dispatch_attr(&attrs, &generics).unwrap();
        let (params, targets) = dispatch.groups().next().unwrap();

        assert_eq!(params.len(), 1);
        assert_eq!(params[0], "N");
        assert_eq!(targets.len(), 2);
        assert!(matches!(
            targets[0].args.first(),
            Some(syn::GenericArgument::Const(syn::Expr::Lit(_)))
        ));
    }

    #[test]
    fn validates_symbol_name_interpolation_without_escape_syntax() {
        assert!(
            validate_symbol_text(&LitStr::new("", proc_macro2::Span::call_site()), true).is_err()
        );
        assert!(
            validate_symbol_text(
                &LitStr::new("SQLPrepare{C}", proc_macro2::Span::call_site()),
                true,
            )
            .is_ok()
        );
        assert!(
            validate_symbol_text(
                &LitStr::new("SQLPrepare{C}", proc_macro2::Span::call_site()),
                false,
            )
            .is_err()
        );
        assert!(
            validate_symbol_text(
                &LitStr::new("SQLPrepare\\{C}", proc_macro2::Span::call_site()),
                true,
            )
            .is_err()
        );
        assert!(
            validate_symbol_text(
                &LitStr::new("SQL Prepare", proc_macro2::Span::call_site()),
                true,
            )
            .is_err()
        );
        assert!(
            validate_symbol_text(
                &LitStr::new("SQL{C}Prepare{D", proc_macro2::Span::call_site()),
                true,
            )
            .is_err()
        );
    }
}
