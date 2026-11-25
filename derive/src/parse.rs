use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use proc_macro2::{Delimiter, Group, Ident, TokenStream, TokenTree};
use quote::{format_ident, quote};
use syn::{
    Attribute, Error, FnArg, GenericArgument, GenericParam, ItemFn, ItemImpl, LitStr, PatType,
    Result, Type, TypePath,
    ext::IdentExt,
    parse::{Parse, ParseStream, Parser, discouraged::Speculative},
    parse_quote, parse_quote_spanned,
    punctuated::Punctuated,
    spanned::Spanned,
    visit::Visit,
    visit_mut::VisitMut,
};

use crate::{
    Co3Static, DeclKind, DispatchGroups,
    codec::Conversion,
    utils::{ParamUseDetector, push_error},
    validate::unsupported_attr,
};

const FN_BODIES_NOT_ALLOWED_MSG: &str = "fn bodies are not allowed in declarations";
const ITEM_NOT_SUPPORTED_MSG: &str = "item not supported";
const EXPECTED_FEATURE_NAME_MSG: &str = "Expected feature name in `#![feature(...)]`";

/// Syn parses the surrounding Rust type. Each embedded `raw fn` has its own
/// parsed signature, indexed by a fresh identifier reserved for this input.
struct RawTypes {
    signatures: Vec<(Ident, syn::Signature, bool)>,
    source_names: BTreeSet<String>,
    next_id: usize,
}

impl RawTypes {
    fn extract(&mut self, tokens: TokenStream) -> Result<TokenStream> {
        let tokens = tokens.into_iter().collect::<Vec<_>>();
        let mut output = TokenStream::new();
        let mut index = 0;
        while index < tokens.len() {
            if matches!(&tokens[index], TokenTree::Ident(_))
                && matches!(tokens.get(index + 1), Some(TokenTree::Punct(punct)) if punct.as_char() == '!')
                && matches!(tokens.get(index + 2), Some(TokenTree::Group(_)))
            {
                // Macro arguments belong to the invoked macro, including tokens
                // that resemble `raw fn` types.
                output.extend(tokens[index..index + 3].iter().cloned());
                index += 3;
                continue;
            }
            let is_fn = |at| {
                matches!(tokens.get(at), Some(TokenTree::Ident(ident)) if ident == "fn")
                    && matches!(tokens.get(at + 1), Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Parenthesis)
            };
            let is_raw_fn = matches!(&tokens[index], TokenTree::Ident(ident) if ident == "raw")
                && (is_fn(index + 1)
                    || (matches!(tokens.get(index + 1), Some(TokenTree::Ident(ident)) if ident == "extern")
                        && matches!(tokens.get(index + 2), Some(TokenTree::Literal(_)))
                        && is_fn(index + 3)));
            if is_raw_fn {
                let start = index;
                index += 1;
                let mut angle_depth = 0usize;
                while index < tokens.len() {
                    match &tokens[index] {
                        TokenTree::Punct(punct) if punct.as_char() == '<' => angle_depth += 1,
                        TokenTree::Punct(punct) if punct.as_char() == '>' => {
                            let arrow = index > start
                                && matches!(&tokens[index - 1], TokenTree::Punct(prev) if prev.as_char() == '-');
                            if !arrow {
                                if angle_depth == 0 {
                                    break;
                                }
                                angle_depth -= 1;
                            }
                        }
                        TokenTree::Ident(ident) if angle_depth == 0 && ident == "where" => break,
                        TokenTree::Punct(punct)
                            if angle_depth == 0 && matches!(punct.as_char(), ',' | ';') =>
                        {
                            break;
                        }
                        _ => {}
                    }
                    index += 1;
                }
                let raw = &tokens[start];
                let inner = self.extract(tokens[start + 1..index].iter().cloned().collect())?;
                let (signature, moved) =
                    parse_raw_function_type(std::iter::once(raw.clone()).chain(inner).collect())?;
                let ident = loop {
                    let name = format!("__Co3RawType{}", self.next_id);
                    self.next_id += 1;
                    if !self.source_names.contains(&name) {
                        break Ident::new(&name, raw.span());
                    }
                };
                self.signatures.push((ident.clone(), signature, moved));
                output.extend([TokenTree::Ident(ident)]);
                continue;
            }
            match &tokens[index] {
                TokenTree::Group(group) => {
                    let before = self.signatures.len();
                    let inner = self.extract(group.stream())?;
                    if before == self.signatures.len() {
                        output.extend([TokenTree::Group(group.clone())]);
                    } else {
                        let mut rebuilt = Group::new(group.delimiter(), inner);
                        rebuilt.set_span(group.span());
                        output.extend([TokenTree::Group(rebuilt)]);
                    }
                }
                token => output.extend([token.clone()]),
            }
            index += 1;
        }
        Ok(output)
    }
}

pub(crate) struct ParsedInput {
    pub(crate) kind: DeclKind,
    pub(crate) abi: syn::Abi,
    pub(crate) symbol_prefix: LitStr,
    pub(crate) language: Conversion,
    pub(crate) symbol_fragments: BTreeMap<String, LitStr>,
    pub(crate) features: MacroFeatures,
    pub(crate) failure_mode: FailureMode,
    pub(crate) attrs: Vec<Attribute>,
    pub(crate) items: Vec<ParsedItem>,
}

pub(crate) struct ParsedRawInput {
    pub(crate) failure_mode: FailureMode,
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
    Type(crate::ForeignItemType),
    Raw(Box<RawFnDecl>),
    Alias(ParsedAlias),
    Static(Co3Static),
    Impl(ParsedImpl),
    Fn(crate::Co3Fn),
}

pub(crate) struct ParsedImpl {
    pub(crate) item: crate::Co3Impl,
    pub(crate) raw_methods: BTreeMap<usize, proc_macro2::Span>,
}

pub(crate) struct ParsedAlias {
    pub(crate) item: syn::ItemType,
    pub(crate) raw_root: bool,
}

pub(crate) struct RawFnDecl {
    pub(crate) attrs: Vec<Attribute>,
    pub(crate) vis: syn::Visibility,
    pub(crate) callee: syn::ExprPath,
    pub(crate) sig: syn::Signature,
    pub(crate) owner: Option<RawFnOwner>,
    pub(crate) dispatch_args: crate::DispatchGroups,
}

pub(crate) struct RawFnOwner {
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

impl ParsedItem {
    fn parse(input: ParseStream, raw_block: bool) -> Result<Self> {
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
            if is_type_alias(input)? {
                if raw_block {
                    ahead.parse::<syn::Token![type]>()?;
                    let name: Ident = ahead.parse()?;
                    return Err(Error::new_spanned(
                        name,
                        "type aliases are not allowed in `raw!`; use a Rust alias or declare a raw alias in `ffi!`",
                    ));
                }
                return Ok(Self::Alias(ParsedAlias {
                    item: input.parse()?,
                    raw_root: false,
                }));
            }
            return Ok(Self::Type(parse_type_item(input)?));
        }
        if ahead.peek(syn::Token![static]) {
            return Ok(Self::Static(parse_static_item(input)?));
        }
        if ahead.peek(syn::Ident) && ahead.parse::<syn::Ident>()? == "raw" {
            return Ok(Self::Raw(Box::new(parse_raw_fn_item(input)?)));
        }
        if is_fn_head(&ahead)? {
            return Ok(Self::Fn(parse_fn_item(input)?));
        }

        Err(input.error(ITEM_NOT_SUPPORTED_MSG))
    }
}

fn is_type_alias(input: ParseStream) -> syn::Result<bool> {
    let fork = input.fork();
    let _ = fork.call(Attribute::parse_outer)?;
    let _ = fork.parse::<syn::Visibility>()?;
    fork.parse::<syn::Token![type]>()?;
    let mut angle_depth = 0usize;
    while !fork.is_empty() && !fork.peek(syn::Token![;]) {
        let token = fork.parse::<TokenTree>()?;
        match &token {
            TokenTree::Punct(punct) if punct.as_char() == '<' => angle_depth += 1,
            TokenTree::Punct(punct) if punct.as_char() == '>' => {
                angle_depth = angle_depth.saturating_sub(1)
            }
            TokenTree::Punct(punct) if punct.as_char() == '=' && angle_depth == 0 => {
                return Ok(true);
            }
            _ => {}
        }
    }
    Ok(false)
}

pub(crate) fn parse_raw_function_type(tokens: TokenStream) -> Result<(syn::Signature, bool)> {
    (|input: ParseStream| {
        let raw: Ident = input.parse()?;
        if raw != "raw" {
            return Err(Error::new(raw.span(), "expected `raw fn`"));
        }
        let abi = input.parse()?;
        let fn_token = input.parse()?;
        let content;
        let paren_token = syn::parenthesized!(content in input);
        let mut inputs = Punctuated::new();
        while !content.is_empty() {
            let mut attrs = content.call(Attribute::parse_outer)?;
            parse_move_by_val(&content, &mut attrs)?;
            let name = format_ident!("__co3_arg_{}", inputs.len());
            inputs.push_value(FnArg::Typed(PatType {
                attrs,
                pat: Box::new(parse_quote!(#name)),
                colon_token: Default::default(),
                ty: content.parse()?,
            }));
            if content.is_empty() {
                break;
            }
            inputs.push_punct(content.parse()?);
        }
        let mut attrs = Vec::new();
        let output = parse_output(input, &mut attrs)?;
        let sig = syn::Signature {
            constness: None,
            asyncness: None,
            safety: Default::default(),
            abi,
            fn_token,
            ident: format_ident!("__co3_callback_type"),
            generics: Default::default(),
            paren_token,
            inputs,
            variadic: None,
            output,
        };
        Ok((sig, !attrs.is_empty()))
    })
    .parse2(tokens)
}

fn parse_raw_fn_item(input: ParseStream) -> Result<RawFnDecl> {
    let mut attrs = input.call(Attribute::parse_outer)?;
    let vis = input.parse::<syn::Visibility>()?;
    let keyword = input.parse::<syn::Ident>()?;
    debug_assert_eq!(keyword, "raw");
    let (sig, predicates) = parse_signature(input, &mut attrs)?;
    let dispatch_args = dispatch_groups(&mut attrs, &sig.generics, predicates, None)?;
    end_fn(input)?;

    let ident = &sig.ident;
    Ok(RawFnDecl {
        attrs,
        vis,
        callee: syn::parse_quote!(#ident),
        sig,
        owner: None,
        dispatch_args,
    })
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

fn parse_items(input: ParseStream, raw_block: bool) -> Result<Vec<ParsedItem>> {
    let mut items = Vec::new();

    while !input.is_empty() {
        items.push(ParsedItem::parse(input, raw_block)?);
    }

    Ok(items)
}

impl ParsedInput {
    pub(crate) fn parse(tokens: TokenStream) -> Result<Self> {
        let mut raw = RawTypes {
            signatures: Vec::new(),
            source_names: crate::utils::source_idents(tokens.clone()),
            next_id: 0,
        };
        let FfiBody {
            mut attrs,
            mut items,
        } = parse_ffi_body(raw.extract(tokens)?, false)?;
        let (kind, abi) = take_decl_attr(&mut attrs)?;
        let symbol_prefix =
            parse_symbol_prefix_attr(&mut attrs)?.unwrap_or_else(default_symbol_prefix);
        validate_symbol_text(&symbol_prefix, false)?;
        let symbol_fragments = parse_symbol_fragments_attr(&mut attrs)?;
        let failure_mode = parse_failure_attr(&mut attrs)?;
        let features = parse_feature_attrs(&mut attrs)?;
        let language = parse_language_attr(&mut attrs, kind)?;
        crate::callback::lower_raw_types(
            raw.signatures,
            raw.source_names,
            &mut items,
            failure_mode,
            &abi,
        )?;

        Ok(Self {
            kind,
            abi,
            symbol_prefix,
            language,
            symbol_fragments,
            features,
            failure_mode,
            attrs,
            items,
        })
    }
}

impl ParsedRawInput {
    pub(crate) fn parse(tokens: TokenStream) -> Result<Self> {
        let FfiBody { mut attrs, items } = parse_ffi_body(tokens, true)?;
        let failure_mode = parse_failure_attr(&mut attrs)?;
        if let Some(attr) = attrs.first() {
            return Err(syn::Error::new_spanned(
                attr,
                "unsupported `raw!` block attribute",
            ));
        }
        Ok(Self {
            failure_mode,
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

fn parse_ffi_body(tokens: TokenStream, raw_block: bool) -> Result<FfiBody> {
    let parser = |input: syn::parse::ParseStream| -> Result<FfiBody> {
        let attrs = input.call(Attribute::parse_inner)?;
        let items = parse_items(input, raw_block)?;

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

    attr.parse_args_with(|input: ParseStream| {
        let direction = input.call(Ident::parse_any)?;
        let kind = match direction.unraw().to_string().as_str() {
            "export" => DeclKind::Export,
            "extern" => DeclKind::Extern,
            _ => return Err(input.error(err_msg)),
        };
        let value;
        syn::parenthesized!(value in input);
        let abi = syn::Abi {
            extern_token: Default::default(),
            name: Some(value.parse()?),
        };
        if !value.is_empty() {
            return Err(value.error(err_msg));
        }
        Ok((kind, abi))
    })
    .map_err(|_| Error::new_spanned(attr, err_msg))
}

fn parse_language_attr(attrs: &mut Vec<Attribute>, kind: DeclKind) -> Result<Conversion> {
    let mut language = None;
    let mut kept = Vec::new();
    for attr in attrs.drain(..) {
        if !attr.path().is_ident("lang") {
            kept.push(attr);
            continue;
        }
        if language.is_some() {
            return Err(syn::Error::new_spanned(
                attr,
                "Duplicate `#![lang = \"...\"]`",
            ));
        }
        let value = match &attr.meta {
            syn::Meta::NameValue(nv) => match &nv.value {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(value),
                    ..
                }) => value,
                _ => {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "Expected `#![lang = \"...\"]`",
                    ));
                }
            },
            _ => {
                return Err(syn::Error::new_spanned(
                    attr,
                    "Expected `#![lang = \"...\"]`",
                ));
            }
        };
        let name = match value.value().as_str() {
            "C" => "C",
            "C++" => "Cpp",
            "Rust" => "Rust",
            "Swift" => "Swift",
            "Objective-C" => "ObjectiveC",
            "Java" => "Java",
            "C#" => "CSharp",
            "Python" => "Python",
            _ => {
                return Err(syn::Error::new_spanned(
                    value,
                    "Unsupported language; expected C, C++, Rust, Swift, Objective-C, Java, C#, or Python",
                ));
            }
        };
        if kind == DeclKind::Export && !matches!(name, "Rust" | "C") {
            return Err(Error::new_spanned(
                value,
                "Exports support only `#![lang = \"C\"]` or `#![lang = \"Rust\"]`",
            ));
        }
        if name == "Rust" {
            language = Some(Conversion::Unchecked);
            continue;
        }
        let ident = syn::Ident::new(name, value.span());
        let co3 = crate::utils::co3_path();
        language = Some(Conversion::Checked(syn::parse_quote!(#co3::lang::#ident)));
    }
    *attrs = kept;
    Ok(language.unwrap_or_else(|| {
        let co3 = crate::utils::co3_path();
        Conversion::Checked(syn::parse_quote!(#co3::lang::C))
    }))
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

struct DispatchPredicate {
    params: syn::PreciseCapture,
    targets: Vec<syn::AngleBracketedGenericArguments>,
}

impl Parse for DispatchPredicate {
    fn parse(input: ParseStream) -> Result<Self> {
        let params = input.parse()?;
        input.parse::<syn::Token![@]>()?;
        let parse_target = |input: ParseStream| {
            input
                .parse()
                .map_err(|_| input.error("expected a tagged-dispatch type list such as `<Type>`"))
        };
        let targets = if input.peek(syn::token::Paren) {
            let group;
            syn::parenthesized!(group in input);
            group
                .parse_terminated(parse_target, syn::Token![|])?
                .into_iter()
                .collect()
        } else {
            let target = parse_target(input)?;
            if input.peek(syn::Token![|]) {
                return Err(input.error(
                    "enclose tagged-dispatch alternatives in parentheses, as in `(<T> | <U>)`",
                ));
            }
            vec![target]
        };
        Ok(Self { params, targets })
    }
}

fn dispatch_groups(
    attrs: &mut Vec<Attribute>,
    generics: &syn::Generics,
    mut predicates: Vec<DispatchPredicate>,
    self_ty: Option<&Type>,
) -> Result<DispatchGroups> {
    // Explicit attributes, when present, contain the caller's original syntax.
    if let Some(i) = attrs.iter().position(|a| a.path().is_ident("erased")) {
        let attr = attrs.remove(i);
        if !predicates.is_empty() || attrs.iter().any(|a| a.path().is_ident("erased")) {
            return Err(Error::new_spanned(
                attr,
                "duplicate tagged-dispatch predicate",
            ));
        }
        predicates = attr
            .parse_args_with(Punctuated::<DispatchPredicate, syn::Token![,]>::parse_terminated)?
            .into_iter()
            .collect();
    }

    let mut assigned = HashSet::new();
    let mut groups = BTreeMap::new();
    for predicate in predicates {
        let group = parse_dispatch_group(generics, &mut assigned, &predicate.params)?;
        groups.insert(group, predicate.targets);
    }
    validate_dispatch_targets(generics, &groups)?;
    let mut dispatch = DispatchGroups { groups };
    validate_dispatch_cycles(&dispatch, self_ty)?;
    if let Some(self_ty) = self_ty {
        dispatch.concretize_self(self_ty);
    }
    Ok(dispatch)
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

fn parse_dispatch_group(
    generics: &syn::Generics,
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

        if !generics.type_params().any(|p| p.ident == ident)
            && !generics.const_params().any(|p| p.ident == ident)
        {
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
        if attr.path().is_ident("derive") {
            return Err(syn::Error::new_spanned(
                attr,
                "`#[derive(...)]` is not supported on opaque or extern type declarations",
            ));
        }

        if attr.path().is_ident("tag") {
            let (ty, value) = attr.parse_args_with(crate::tag::parse_tag_args)?;
            if tag_ty.replace(ty).is_some() {
                return Err(syn::Error::new_spanned(attr, "duplicate `#[tag(...)]`"));
            }
            tag_value = value;
            continue;
        }

        if attr.path().is_ident("unsafe") {
            return Err(unsupported_attr(&attr));
        }
        if !attr.path().is_ident("covariant") {
            kept.push(attr);
            continue;
        }

        let syn::Meta::List(list) = &attr.meta else {
            return Err(unsupported_attr(&attr));
        };

        if list.path.is_ident("covariant") {
            let lifetimes = list
                .parse_args_with(
                    syn::punctuated::Punctuated::<syn::Lifetime, syn::Token![,]>::parse_terminated,
                )
                .map_err(|_| syn::Error::new_spanned(&attr, "expected `#[covariant('a, ...)]`"))?;
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

fn is_fn_head(input: syn::parse::ParseStream) -> syn::Result<bool> {
    let ahead = input.fork();

    let _ = ahead.parse::<Option<syn::Token![const]>>()?;
    let _ = ahead.parse::<Option<syn::Token![async]>>()?;
    let _ = ahead.parse::<Option<syn::Token![unsafe]>>()?;
    let _ = ahead.parse::<Option<syn::Abi>>()?;
    Ok(ahead.peek(syn::Token![fn]))
}

fn parse_where_clause(
    input: ParseStream,
    generics: &mut syn::Generics,
) -> Result<Vec<DispatchPredicate>> {
    if !input.peek(syn::Token![where]) {
        return Ok(Vec::new());
    }
    let where_token = input.parse()?;
    let mut predicates = Punctuated::new();
    let mut dispatch = Vec::new();
    while !input.is_empty() && !input.peek(syn::Token![;]) && !input.peek(syn::token::Brace) {
        if input.peek(syn::Token![use]) {
            dispatch.push(input.parse()?);
        } else {
            let predicate: syn::WherePredicate = input.parse()?;
            if !dispatch.is_empty() {
                return Err(Error::new_spanned(
                    predicate,
                    "tagged-dispatch predicates must be last in a `where` clause",
                ));
            }
            predicates.push_value(predicate);
        }
        if !input.peek(syn::Token![,]) {
            break;
        }
        let comma = input.parse::<syn::Token![,]>()?;
        if dispatch.is_empty() {
            predicates.push_punct(comma);
        }
    }
    if !predicates.is_empty() {
        generics.where_clause = Some(syn::WhereClause {
            where_token,
            predicates,
        });
    }
    Ok(dispatch)
}

fn parse_generics(input: ParseStream) -> Result<syn::Generics> {
    if !input.peek(syn::Token![<]) {
        return Ok(Default::default());
    }
    let lt_token = Some(input.parse()?);
    let mut params = Punctuated::new();
    while !input.peek(syn::Token![>]) {
        let param = if input.peek(syn::Token![dyn]) {
            let keyword: syn::Token![dyn] = input.parse()?;
            if !input.peek(syn::token::Paren) {
                return Err(Error::new(
                    keyword.span,
                    "tagged-dispatch type parameters must use `dyn(TagTy) T`",
                ));
            }
            let repr;
            let parens = syn::parenthesized!(repr in input);
            // Keep the user's tag type tokens on their parameter.
            let mut attr: Attribute = parse_quote_spanned!(keyword.span=> #[erased()]);
            let syn::Meta::List(list) = &mut attr.meta else {
                unreachable!()
            };
            list.tokens = repr.parse()?;
            syn::parse2::<Type>(list.tokens.clone()).map_err(|_| {
                Error::new(parens.span.join(), "expected id repr in `dyn(TagTy) T`")
            })?;
            list.delimiter = syn::MacroDelimiter::Paren(parens);
            let mut param: syn::TypeParam = input.parse()?;
            param.attrs.push(attr);
            GenericParam::Type(param)
        } else {
            input.parse()?
        };
        params.push_value(param);
        if !input.peek(syn::Token![,]) {
            break;
        }
        params.push_punct(input.parse()?);
    }
    Ok(syn::Generics {
        lt_token,
        params,
        gt_token: Some(input.parse()?),
        where_clause: None,
    })
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

fn parse_arg(input: ParseStream) -> Result<FnArg> {
    let mut attrs = input.call(Attribute::parse_outer)?;
    if input.peek(syn::Token![self]) && input.peek2(syn::Token![:]) {
        let self_token = input.parse()?;
        let colon = input.parse()?;
        parse_move_by_val(input, &mut attrs)?;
        return Ok(FnArg::Receiver(syn::Receiver {
            attrs,
            mutability: None,
            self_token,
            kind: syn::ReceiverKind::Typed(colon, input.parse()?),
        }));
    }
    let receiver_input = input.fork();
    if let Ok(mut receiver) = receiver_input.parse::<syn::Receiver>() {
        input.advance_to(&receiver_input);
        receiver.attrs = attrs;
        return Ok(FnArg::Receiver(receiver));
    }
    let pat = Box::new(syn::Pat::parse_single(input)?);
    let colon_token = input.parse()?;
    parse_move_by_val(input, &mut attrs)?;
    Ok(FnArg::Typed(PatType {
        attrs,
        pat,
        colon_token,
        ty: input.parse()?,
    }))
}

fn parse_output(input: ParseStream, attrs: &mut Vec<Attribute>) -> Result<syn::ReturnType> {
    if !input.peek(syn::Token![->]) {
        return Ok(syn::ReturnType::Default);
    }
    let arrow = input.parse()?;
    parse_move_by_val(input, attrs)?;
    Ok(syn::ReturnType::Type(arrow, input.parse()?))
}

fn parse_signature(
    input: ParseStream,
    attrs: &mut Vec<Attribute>,
) -> Result<(syn::Signature, Vec<DispatchPredicate>)> {
    let constness = input.parse()?;
    let asyncness = input.parse()?;
    let safety = if input.peek(syn::Token![unsafe]) {
        syn::Safety::Unsafe(input.parse()?)
    } else {
        syn::Safety::Default
    };
    let abi = input.parse()?;
    let fn_token = input.parse()?;
    let ident = input.parse()?;
    let mut generics = parse_generics(input)?;
    let content;
    let paren_token = syn::parenthesized!(content in input);
    let inputs = content.parse_terminated(parse_arg, syn::Token![,])?;
    let output = parse_output(input, attrs)?;
    let dispatch = parse_where_clause(input, &mut generics)?;
    rewrite_erased_param_bounds_to_where_clause(&mut generics);
    let mut sig = syn::Signature {
        constness,
        asyncness,
        safety,
        abi,
        fn_token,
        ident,
        generics,
        paren_token,
        inputs,
        variadic: None,
        output,
    };
    normalize_const_args_in_fn(&mut sig);
    Ok((sig, dispatch))
}

fn parse_type_item(input: ParseStream) -> Result<crate::ForeignItemType> {
    let attrs = input.call(Attribute::parse_outer)?;
    let vis = input.parse()?;
    let type_token = input.parse()?;
    let ident = input.parse()?;
    let mut generics = input.parse()?;
    let dispatch = parse_where_clause(input, &mut generics)?;
    let mut ty = syn::ForeignItemType {
        attrs,
        vis,
        modifiers: Default::default(),
        type_token,
        ident,
        generics,
        semi_token: input.parse()?,
    };
    let (id, id_value, covariant_lifetimes) = parse_opaque_type_attrs(&mut ty.attrs, &ty.generics)?;
    Ok(crate::ForeignItemType {
        ty,
        id: id.map(Box::new),
        id_value: id_value.map(Box::new),
        covariant_lifetimes,
        dispatch_span: dispatch
            .first()
            .map(|predicate| predicate.params.use_token.span),
        drop: None,
        self_impls: Vec::new(),
    })
}

fn end_fn(input: ParseStream) -> Result<()> {
    if input.peek(syn::token::Brace) {
        return Err(input.error(FN_BODIES_NOT_ALLOWED_MSG));
    }
    input.parse::<syn::Token![;]>()?;
    Ok(())
}

fn parse_fn_item(input: ParseStream) -> Result<crate::Co3Fn> {
    let mut attrs = input.call(Attribute::parse_outer)?;
    let vis = input.parse()?;
    let (sig, predicates) = parse_signature(input, &mut attrs)?;
    let dispatch_args = dispatch_groups(&mut attrs, &sig.generics, predicates, None)?;
    end_fn(input)?;
    let item = ItemFn {
        attrs,
        vis,
        modifiers: Default::default(),
        sig,
        block: Box::new(parse_quote!({})),
    };
    Ok(crate::Co3Fn {
        item,
        dispatch_args,
        import_mode: crate::ImportMode::Regular,
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

fn parse_impl_item(input: ParseStream) -> Result<ParsedImpl> {
    let attrs = input.call(Attribute::parse_outer)?;
    let mut modifiers = syn::ImplModifiers::default();
    modifiers.defaultness = input.parse()?;
    let unsafety = input.parse()?;
    let impl_token = input.parse()?;
    let ahead = input.fork();
    // An impl may start with generic parameters or a qualified self type.
    let (mut generics, generic_error) = match parse_generics(&ahead) {
        Ok(generics) => {
            input.advance_to(&ahead);
            (generics, None)
        }
        Err(error) => (Default::default(), Some(error)),
    };
    if input.peek(syn::Token![!]) && !input.peek2(syn::token::Brace) {
        modifiers.polarity = input.parse()?;
    }
    let first: Type = input
        .parse()
        .map_err(|error| generic_error.unwrap_or(error))?;
    let (trait_, self_ty) = if input.peek(syn::Token![for]) {
        let Type::Path(TypePath {
            qself: None, path, ..
        }) = crate::utils::peel_grouped_type(&first)
        else {
            return Err(Error::new_spanned(first, "expected trait path"));
        };
        (Some((path.clone(), input.parse()?)), input.parse()?)
    } else {
        if let Some(polarity) = modifiers.polarity {
            return Err(Error::new(
                polarity.span,
                "inherent impls cannot be negative",
            ));
        }
        (None, Box::new(first))
    };
    let mut attrs = attrs;
    let predicates = parse_where_clause(input, &mut generics)?;
    let mut errors = None;
    let mut check_dispatch = |attrs: &mut Vec<Attribute>, generics: &syn::Generics, predicates| {
        dispatch_groups(attrs, generics, predicates, Some(&self_ty)).unwrap_or_else(|error| {
            push_error(&mut errors, error);
            Default::default()
        })
    };
    let dispatch_args = check_dispatch(&mut attrs, &generics, predicates);
    rewrite_erased_param_bounds_to_where_clause(&mut generics);
    let content;
    let brace_token = syn::braced!(content in input);
    let mut items = Vec::new();
    let mut method_dispatch_args = HashMap::new();
    let mut raw_methods = BTreeMap::new();
    while !content.is_empty() {
        if content.peek(syn::Token![;]) {
            content.parse::<syn::Token![;]>()?;
            continue;
        }
        let ahead = content.fork();
        ahead.call(Attribute::parse_outer)?;
        ahead.parse::<syn::Visibility>()?;
        if ahead.peek(syn::Token![type]) {
            items.push(syn::ImplItem::Type(content.parse()?));
            continue;
        }
        if ahead.peek(syn::Token![const]) && !is_fn_head(&ahead)? {
            items.push(syn::ImplItem::Const(content.parse()?));
            continue;
        }
        let mut attrs = content.call(Attribute::parse_outer)?;
        let vis = content.parse()?;
        if content.peek(syn::Ident) && content.fork().parse::<Ident>()? == "raw" {
            raw_methods.insert(items.len(), content.parse::<Ident>()?.span());
        } else if let Some(index) = attrs.iter().position(|attr| attr.path().is_ident("raw")) {
            raw_methods.insert(items.len(), attrs.remove(index).span());
        }
        let (sig, predicates) = parse_signature(&content, &mut attrs)?;
        end_fn(&content)?;
        let groups = check_dispatch(&mut attrs, &sig.generics, predicates);
        if !groups.is_empty()
            && method_dispatch_args
                .insert(sig.ident.clone(), groups)
                .is_some()
        {
            return Err(Error::new_spanned(&sig.ident, "duplicate method"));
        }
        items.push(syn::ImplItem::Fn(syn::ImplItemFn {
            attrs,
            vis,
            modifiers: Default::default(),
            sig,
            block: parse_quote!({}),
        }));
    }
    if let Some(errors) = errors {
        return Err(errors);
    }
    let mut item = ItemImpl {
        attrs,
        modifiers,
        unsafety,
        impl_token,
        generics,
        trait_,
        self_ty,
        brace_token,
        items,
    };
    normalize_const_generic_args_in_impl(&mut item);
    normalize_self_tag_ids(&mut item);
    Ok(ParsedImpl {
        item: crate::Co3Impl {
            item,
            dispatch_args,
            method_dispatch_args,
            import_mode: crate::ImportMode::Regular,
        },
        raw_methods,
    })
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
    #[test]
    fn inline_raw_callbacks_preserve_bound_and_free_lifetimes() {
        let parsed = super::ParsedInput::parse(quote::quote! {
            #![unsafe(extern("C"))]
            fn accept<'__co3_callback_type_0>(
                callback: raw fn(&'__co3_callback_type_0 u16, &u32),
                value: &'__co3_callback_type_0 u8,
            );
        })
        .unwrap();
        let super::ParsedItem::Fn(item) = &parsed.items[0] else {
            panic!("expected function");
        };
        let sig = &item.item.sig;
        assert_eq!(sig.generics.lifetimes().count(), 1);
        let syn::FnArg::Typed(first) = &sig.inputs[0] else {
            panic!("expected callback argument");
        };
        let syn::Type::FnPtr(pointer) = first.ty.as_ref() else {
            panic!("expected lowered callback");
        };
        let Some(bound) = &pointer.lifetimes else {
            panic!("expected callback binder")
        };
        let expected: syn::BoundLifetimes = syn::parse_quote!(for<'__co3_callback_type_1>);
        assert_eq!(bound, &expected);
        let explicit = &pointer.inputs[0].ty;
        let elided = &pointer.inputs[1].ty;
        assert_eq!(
            explicit,
            &syn::parse_quote!(<&'__co3_callback_type_0 u16 as co3::ReprC>::CType)
        );
        assert_eq!(
            elided,
            &syn::parse_quote!(<&'__co3_callback_type_1 u32 as co3::ReprC>::CType)
        );
    }

    #[test]
    fn raw_aliases_and_explicit_function_binders_keep_their_lifetime_scope() {
        let parsed = super::ParsedInput::parse(quote::quote! {
            #![unsafe(extern("C"))]
            type Callback = raw fn(&str);
            fn accept(callback: for<'a> unsafe extern "C" fn(&'a u8));
        })
        .unwrap();
        let super::ParsedItem::Alias(alias) = &parsed.items[0] else {
            panic!("expected alias");
        };
        let syn::Type::FnPtr(pointer) = alias.item.ty.as_ref() else {
            panic!("expected lowered callback");
        };
        assert!(pointer.lifetimes.is_some());
        assert!(alias.item.generics.params.is_empty());
        let super::ParsedItem::Fn(item) = &parsed.items[1] else {
            panic!("expected function");
        };
        let expected: syn::Signature = syn::parse_quote!(
            fn accept(callback: for<'a> unsafe extern "C" fn(&'a u8))
        );
        assert!(item.item.sig.generics.params.is_empty());
        assert_eq!(item.item.sig.inputs, expected.inputs);
    }

    #[test]
    fn export_languages_are_restricted_but_import_languages_are_open() {
        for name in [
            "C",
            "Rust",
            "C++",
            "Swift",
            "Objective-C",
            "Java",
            "C#",
            "Python",
        ] {
            let language = syn::LitStr::new(name, proc_macro2::Span::call_site());
            let export = super::ParsedInput::parse(quote::quote! {
                #![unsafe(export("C"))]
                #![lang = #language]
            });
            assert_eq!(export.is_ok(), matches!(name, "C" | "Rust"));
            let import = super::ParsedInput::parse(quote::quote! {
                #![unsafe(extern("C"))]
                #![lang = #language]
            });
            assert!(import.is_ok());
        }
    }

    #[test]
    fn language_attributes() {
        for name in [
            "C",
            "C++",
            "Rust",
            "Swift",
            "Objective-C",
            "Java",
            "C#",
            "Python",
        ] {
            let value = syn::LitStr::new(name, proc_macro2::Span::call_site());
            let mut attrs = vec![syn::parse_quote!(#![lang = #value])];
            assert!(super::parse_language_attr(&mut attrs, DeclKind::Extern).is_ok());
            assert!(attrs.is_empty());
        }
        for tokens in [
            quote::quote!(#![lang = "Go"]),
            quote::quote!(#![lang = 1]),
            quote::quote!(#![lang("C")]),
            quote::quote!(#![lang = "C"] #![lang = "Rust"]),
        ] {
            let mut attrs =
                syn::parse::Parser::parse2(syn::Attribute::parse_inner, tokens).unwrap();
            assert!(super::parse_language_attr(&mut attrs, DeclKind::Extern).is_err());
        }
        let default = super::parse_language_attr(&mut vec![], DeclKind::Extern).unwrap();
        let mut explicit = vec![syn::parse_quote!(#![lang = "C"])];
        assert_eq!(
            default,
            super::parse_language_attr(&mut explicit, DeclKind::Extern).unwrap()
        );
    }

    use super::*;

    #[test]
    fn parses_move_self_receiver() {
        let impl_ = "impl Value { fn name(self: move Self); }";
        let item = Parser::parse_str(parse_impl_item, impl_).unwrap();

        let syn::ImplItem::Fn(method) = &item.item.items[0] else {
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
    fn parses_soft_move_arguments_and_receivers() {
        let function =
            Parser::parse_str(parse_fn_item, "fn name(#[soft] value: move u8);").unwrap();
        let implementation = Parser::parse_str(
            parse_impl_item,
            "impl Value { fn name(#[soft] self: move Self); }",
        )
        .unwrap();
        let syn::ImplItem::Fn(method) = &implementation.item.items[0] else {
            panic!("expected method");
        };
        let (callback, _) =
            parse_raw_function_type(quote!(raw extern "C" fn(#[soft] move u8))).unwrap();

        for signature in [&function.item.sig, &method.sig, &callback] {
            let attrs = match &signature.inputs[0] {
                FnArg::Typed(arg) => &arg.attrs,
                FnArg::Receiver(receiver) => &receiver.attrs,
            };
            assert!(attrs.iter().any(|attr| attr.path().is_ident("soft")));
            assert!(attrs.iter().any(|attr| attr.path().is_ident("by_val")));
        }
    }

    #[test]
    fn parses_move_return_type() {
        let item = Parser::parse_str(parse_fn_item, "fn name() -> move Value;").unwrap();

        assert!(
            item.item
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("by_val"))
        );
    }

    #[test]
    fn leaves_implicit_ownership_unmarked_during_parsing() {
        let item = Parser::parse_str(
            parse_fn_item,
            "fn name(a: *const u8, b: *mut u8, c: extern \"C\" fn(u8) -> u8, d: (*const u8));",
        )
        .unwrap();

        for input in &item.item.sig.inputs {
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
    fn parses_qualified_move_return_type() {
        let item = Parser::parse_str(
            parse_fn_item,
            "unsafe extern \"C\" fn name() -> move Value;",
        )
        .unwrap();

        assert!(
            item.item
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("by_val"))
        );
        assert!(matches!(item.item.sig.safety, syn::Safety::Unsafe(_)));
        assert!(item.item.sig.abi.is_some());
    }

    #[test]
    fn rejects_reordered_move_fn_return() {
        Parser::parse_str(parse_fn_item, "move unsafe fn name() -> Value;")
            .err()
            .unwrap();
        Parser::parse_str(parse_fn_item, "move async fn name() -> Value;")
            .err()
            .unwrap();
    }

    #[test]
    fn parses_dyn_dispatch_impl_param() {
        let impl_ = "impl<dyn(u8) T> Trait<T> for Value<T> { fn name(&mut self, value: &T); }";
        let item = Parser::parse_str(parse_impl_item, impl_).unwrap();

        let syn::GenericParam::Type(param) = item.item.generics.params.first().unwrap() else {
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

        let syn::GenericParam::Type(param) = item.item.sig.generics.params.first().unwrap() else {
            panic!("expected type param");
        };
        assert_eq!(param.default.as_ref().unwrap().1, parse_quote!((u8, i8)));
    }

    #[test]
    fn parses_receiver_after_id_arg() {
        let impl_ = "impl<dyn(u8) T> Trait<T> for Value { fn name(self_id: <dyn Self>::TAG, &mut self, value: &T); }";
        let item = Parser::parse_str(parse_impl_item, impl_).unwrap();

        let syn::ImplItem::Fn(method) = &item.item.items[0] else {
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

        let syn::ImplItem::Fn(method) = &item.item.items[0] else {
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
        let err = parse_fn_item
            .parse2(quote! {
                fn name<T>() where use<T> @ <u32>, T: Copy
            })
            .err()
            .unwrap();

        assert!(
            err.to_string()
                .contains("tagged-dispatch predicates must be last in a `where` clause")
        );
    }

    #[test]
    fn rejects_lifetime_dispatch_parameter() {
        let err = parse_fn_item
            .parse2(quote! {
                fn name<'a, T>() where use<'a, T> @ <u32>
            })
            .err()
            .unwrap();

        assert!(
            err.to_string()
                .contains("dispatch parameters cannot be lifetimes")
        );
    }

    #[test]
    fn parses_statically_selected_const_generic() {
        let generics: syn::Generics = parse_quote!(<const N: usize>);
        let mut attrs: Vec<syn::Attribute> = vec![parse_quote! {
            #[erased(use<N> @ (<1> | <2>))]
        }];

        let dispatch = dispatch_groups(&mut attrs, &generics, Vec::new(), None).unwrap();
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
