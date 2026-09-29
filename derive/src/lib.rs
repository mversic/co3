//! Crate containing FFI related macro functionality
//!
//! # Example
//!
//! ```rust
//! #[cfg(not(feature = "import"))]
//! struct Local(u8);
//!
//! #[cfg(not(feature = "import"))]
//! fn make_local() -> Box<Local> { Box::new(Local(0)) }
//!
//! #[cfg(not(feature = "import"))]
//! type LocalType = Box<Local>;
//! #[cfg(feature = "import")]
//! type LocalType = OwnedLocal;
//!
//! co3::ffi! {
//!     #![cfg_attr(not(feature = "import"), unsafe(export("C")))]
//!     #![cfg_attr(feature = "import", unsafe(extern("C")))]
//!
//!     #![symbol_prefix = "provider"]
//!
//!     type Local;
//!
//!     fn make_local() -> move LocalType;
//! }
//! # fn main() {}
//! ```
use std::collections::{BTreeMap, BTreeSet, HashMap};

use manyhow::manyhow;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Attribute, Expr, ItemFn, ItemImpl, LitStr, Path, Result, StaticMutability, Type, parse_quote,
    visit_mut::VisitMut,
};

use crate::{
    cfg_attr::{emit_macro_invocations, expand as expand_cfg_attr},
    dispatch::synthesize_dispatch_tag_ids,
    generate::{expand_export_decls, expand_extern_decls, gen_raw_companions},
    layout::derive_repr_c,
    parse::{ParsedInput, ParsedItem, ParsedRawInput},
    utils::{
        co3_path, has_non_lifetime_generics, is_drop_impl, path_symbol_name, push_error,
        type_symbol_name,
    },
    validate::{
        validate_export_attrs, validate_export_decls, validate_extern_decls,
        validate_raw_companions,
    },
};

mod abi_retype;
mod callback;
mod cfg_attr;
mod dispatch;
mod ffi_fn;
mod generate;
mod layout;
mod parse;
mod statics;
mod tag;
mod utils;
mod validate;
mod wrapper;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeclKind {
    Export,
    Extern,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ImportMode {
    #[default]
    Regular,
    Raw,
}

#[derive(Clone)]
pub(crate) enum ForeignItem {
    Type(ForeignItemType),
    Static(Co3Static),
    Impl(Co3Impl),
    Fn(Co3Fn),
}

enum NormalizedItem {
    Foreign(ForeignItem),
    Raw(parse::RawFnDecl),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NormalizationMode {
    Ffi(DeclKind),
    Raw,
}

impl NormalizationMode {
    fn macro_name(self) -> &'static str {
        match self {
            Self::Ffi(_) => "ffi!",
            Self::Raw => "raw!",
        }
    }
}

#[derive(Clone)]
struct ForeignItemType {
    ty: syn::ForeignItemType,
    id: Option<Box<syn::Type>>,
    id_value: Option<Box<Expr>>,
    covariant_lifetimes: Vec<syn::Lifetime>,
    drop: Option<Co3Impl>,

    self_impls: Vec<Co3Impl>,
}

#[derive(Clone)]
pub(crate) struct Co3Static {
    pub(crate) attrs: Vec<Attribute>,
    pub(crate) vis: syn::Visibility,
    pub(crate) static_token: syn::Token![static],
    pub(crate) mutability: StaticMutability,
    pub(crate) ident: syn::Ident,
    pub(crate) ty: Box<Type>,
    pub(crate) expr: Option<Box<Expr>>,
}

#[derive(Clone)]
struct Co3Impl {
    item: ItemImpl,
    import_mode: ImportMode,

    dispatch_args: DispatchGroups,

    method_dispatch_args: HashMap<syn::Ident, DispatchGroups>,
}

#[derive(Clone)]
struct Co3Fn {
    item: ItemFn,
    import_mode: ImportMode,
    dispatch_args: DispatchGroups,
}

#[derive(Clone, Default)]
struct DispatchGroups {
    groups: BTreeMap<Vec<syn::Ident>, Vec<syn::AngleBracketedGenericArguments>>,
}

pub(crate) struct DispatchSelection<'a> {
    pub(crate) params: &'a [syn::Ident],
    pub(crate) target: &'a syn::AngleBracketedGenericArguments,
}

impl Co3Impl {
    fn new(item: ItemImpl) -> Self {
        Self {
            item,
            import_mode: ImportMode::Regular,
            dispatch_args: DispatchGroups::default(),
            method_dispatch_args: HashMap::default(),
        }
    }
}

impl core::ops::Deref for Co3Impl {
    type Target = ItemImpl;

    fn deref(&self) -> &Self::Target {
        &self.item
    }
}

impl core::ops::DerefMut for Co3Impl {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.item
    }
}

impl core::ops::Deref for Co3Fn {
    type Target = ItemFn;

    fn deref(&self) -> &Self::Target {
        &self.item
    }
}

impl core::ops::DerefMut for Co3Fn {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.item
    }
}

impl DispatchGroups {
    fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    pub(crate) fn groups(
        &self,
    ) -> impl Iterator<Item = (&[syn::Ident], &[syn::AngleBracketedGenericArguments])> {
        self.groups
            .iter()
            .map(|(params, targets)| (params.as_slice(), targets.as_slice()))
    }

    pub(crate) fn contains_param(&self, param: &syn::Ident) -> bool {
        self.groups
            .keys()
            .any(|params| params.iter().any(|candidate| candidate == param))
    }

    pub(crate) fn visit_targets_mut(&mut self, mut visit: impl FnMut(&mut syn::GenericArgument)) {
        for targets in self.groups.values_mut() {
            for target in targets {
                for argument in &mut target.args {
                    visit(argument);
                }
            }
        }
    }

    pub(crate) fn concretize_self(&mut self, self_ty: &syn::Type) {
        let mut concretizer = crate::ffi_fn::SelfConcretizer { self_ty };
        for targets in self.groups.values_mut() {
            for target in targets {
                for argument in &mut target.args {
                    if let syn::GenericArgument::Type(ty) = argument {
                        concretizer.visit_type_mut(ty);
                    }
                }
            }
        }
    }

    fn for_each_combination(&self, mut f: impl for<'a> FnMut(&[DispatchSelection<'a>])) {
        fn visit<'a>(
            groups: &[(&'a [syn::Ident], &'a [syn::AngleBracketedGenericArguments])],
            selections: &mut Vec<DispatchSelection<'a>>,
            f: &mut impl FnMut(&[DispatchSelection<'a>]),
        ) {
            let Some((params, targets)) = groups.first() else {
                f(selections);
                return;
            };

            for target in *targets {
                selections.push(DispatchSelection { params, target });
                visit(&groups[1..], selections, f);
                selections.pop();
            }
        }

        let groups = self.groups().collect::<Vec<_>>();
        visit(&groups, &mut Vec::new(), &mut f);
    }

    fn combined_with(&self, other: &Self) -> Self {
        let mut groups = self.groups.clone();
        groups.extend(other.groups.clone());
        Self { groups }
    }

    fn bindings_for(&self, root: &syn::Ident) -> Self {
        let all_params = self
            .groups()
            .flat_map(|(params, _)| params.iter().cloned())
            .collect::<BTreeSet<_>>();
        let mut selected = BTreeSet::from([root.clone()]);

        loop {
            let mut next = selected.clone();
            for (params, targets) in self.groups() {
                let selected_indices = params
                    .iter()
                    .enumerate()
                    .filter_map(|(index, param)| selected.contains(param).then_some(index))
                    .collect::<Vec<_>>();
                if selected_indices.is_empty() {
                    continue;
                }
                for candidate in &all_params {
                    let detector = crate::utils::ParamUseDetector::new([candidate]);
                    if targets.iter().any(|target| {
                        selected_indices.iter().any(|index| {
                            target
                                .args
                                .get(*index)
                                .is_some_and(|arg| detector.generic_arg_mentions_param(arg))
                        })
                    }) {
                        next.insert(candidate.clone());
                    }
                }
            }
            if next == selected {
                break;
            }
            selected = next;
        }

        let mut bindings = Self::default();
        for (params, targets) in self.groups() {
            let indices = params
                .iter()
                .enumerate()
                .filter_map(|(index, param)| selected.contains(param).then_some(index))
                .collect::<Vec<_>>();
            if indices.is_empty() {
                continue;
            }
            let projected_params = indices
                .iter()
                .map(|index| params[*index].clone())
                .collect::<Vec<_>>();
            let projected_targets = targets
                .iter()
                .map(|target| {
                    let mut target = target.clone();
                    target.args = target
                        .args
                        .into_iter()
                        .enumerate()
                        .filter_map(|(index, arg)| indices.contains(&index).then_some(arg))
                        .collect();
                    target
                })
                .fold(Vec::new(), |mut unique, target| {
                    if !unique.contains(&target) {
                        unique.push(target);
                    }
                    unique
                });
            bindings.groups.insert(projected_params, projected_targets);
        }
        bindings
    }

    fn static_bindings(
        &self,
        generics: &syn::Generics,
        static_binding_params: &BTreeSet<syn::Ident>,
    ) -> Vec<(Self, Self)> {
        fn is_static_param(
            generics: &syn::Generics,
            static_binding_params: &BTreeSet<syn::Ident>,
            ident: &syn::Ident,
        ) -> bool {
            if !static_binding_params.contains(ident) {
                return false;
            }

            generics.params.iter().any(|param| match param {
                syn::GenericParam::Type(param) => {
                    param.ident == *ident && !param.attrs.iter().any(crate::utils::is_type_erased)
                }
                syn::GenericParam::Const(param) => param.ident == *ident,
                syn::GenericParam::Lifetime(_) => false,
            })
        }

        fn project_target(
            target: &syn::AngleBracketedGenericArguments,
            indices: &[usize],
        ) -> syn::AngleBracketedGenericArguments {
            let mut projected = target.clone();
            projected.args = target
                .args
                .iter()
                .enumerate()
                .filter(|(idx, _)| indices.contains(idx))
                .map(|(_, arg)| arg.clone())
                .collect();
            projected
        }

        let mut bindings = vec![(Self::default(), Self::default())];
        for (params, targets) in self.groups() {
            let static_indices = params
                .iter()
                .enumerate()
                .filter_map(|(idx, param)| {
                    is_static_param(generics, static_binding_params, param).then_some(idx)
                })
                .collect::<Vec<_>>();
            let dynamic_indices = (0..params.len())
                .filter(|idx| !static_indices.contains(idx))
                .collect::<Vec<_>>();

            let static_params = static_indices
                .iter()
                .map(|idx| params[*idx].clone())
                .collect::<Vec<_>>();
            let dynamic_params = dynamic_indices
                .iter()
                .map(|idx| params[*idx].clone())
                .collect::<Vec<_>>();

            let mut variants = BTreeMap::<String, (Self, Self)>::new();
            for target in targets {
                let static_target = project_target(target, &static_indices);
                let key = quote::quote!(#static_target).to_string();
                let (static_groups, dynamic_groups) = variants.entry(key).or_default();
                if !static_params.is_empty() {
                    static_groups
                        .groups
                        .entry(static_params.clone())
                        .or_insert_with(|| vec![static_target]);
                }
                if !dynamic_params.is_empty() {
                    dynamic_groups
                        .groups
                        .entry(dynamic_params.clone())
                        .or_default()
                        .push(project_target(target, &dynamic_indices));
                }
            }

            let variants = variants.into_values().collect::<Vec<_>>();
            let mut next = Vec::new();
            for (binding_static, binding_dynamic) in bindings {
                for (group_static, group_dynamic) in &variants {
                    next.push((
                        binding_static.combined_with(group_static),
                        binding_dynamic.combined_with(group_dynamic),
                    ));
                }
            }
            bindings = next;
        }
        bindings
    }

    pub(crate) fn inject_unnamed_lifetimes(&mut self, generics: &mut syn::Generics) {
        for targets in self.groups.values_mut() {
            for target in targets {
                *target =
                    crate::dispatch::inject_unnamed_lifetimes(&mut generics.params, target.clone());
            }
        }
    }
}

/// Generate a C-compatible counterpart and conversions to and from the Rust type.
///
/// A type deriving [`co3::ReprC`](https://docs.rs/co3/latest/co3/derive.ReprC.html) can participate
/// in [`ffi!`] declarations that use C ABI. Note that most types will also require an
/// implementation of [`rust_spec::RustSpec`](https://docs.rs/rust-spec/latest/rust_spec/trait.RustSpec.html).
///
/// # Helper Attributes
///
/// * `#[repr_c(NICHE_VALUE = <expr>)]` on a struct customizes
///   [`co3::niche::Niche::NICHE_VALUE`](https://docs.rs/co3/latest/co3/niche/trait.Niche.html#associatedconstant.NICHE_VALUE)
/// * `#[repr_c(is_valid = |[fieldN]| ...)]` on a struct or enum variant customizes validation
/// * `#[repr_c(identity)]` uses a `repr(C)` or `repr(transparent)` struct directly as its CType
///
/// # Example
///
/// ```rust
/// use co3::{ReprC, rust_spec::RustSpec};
///
/// #[derive(RustSpec, ReprC)]
/// pub struct Hello(u32);
/// # fn main() {}
/// ```
#[manyhow]
// FIXME: It's totally weird that `tag` is part of ReprC
#[proc_macro_derive(ReprC, attributes(repr_c, tag))]
pub fn repr_c_derive(item: syn::DeriveInput) -> Result<TokenStream> {
    derive_repr_c(&item)
}

/// Derives the tagged-dispatch traits for a Rust type.
///
/// - `#[tag(Type, unsafe(value))]` defines both the tag type and value of the derived type.
/// - `#[tag(Type)]` defines only the tag type; `Tagged` trait is then implemented by hand.
#[manyhow]
#[proc_macro_derive(Tag, attributes(tag))]
pub fn tag_derive(item: syn::DeriveInput) -> Result<TokenStream> {
    tag::derive_tag(&item)
}

/// Declare FFI exports or imports via native API.
///
/// # Example
///
/// ```rust
/// use co3::{ReprC, ffi};
///
/// #[derive(Clone, Copy, ReprC)]
/// struct Step(u32);
///
/// ffi! {
///     #![unsafe(extern("C"))]
///
///     type Counter;
///
///     impl Counter {
///         #[symbol_name = "exported_by_int_inc"]
///         fn increment_by_int(&mut self, by: u32);
///
///         #[symbol_name = "exported_custom_inc"]
///         fn increment_custom(&mut self, by: Step);
///     }
/// }
/// ```
///
/// See [the crate level documentation](https://docs.rs/co3/latest/co3/) for more.
#[manyhow]
#[proc_macro]
pub fn ffi(input: TokenStream) -> Result<TokenStream> {
    let cfg_attr_variants = expand_cfg_attr(input.clone())?;

    if cfg_attr_variants.len() == 1 {
        let ParsedInput {
            kind,
            abi,
            symbol_prefix,
            features,
            failure_mode,
            symbol_fragments,
            attrs,
            items,
        } = ParsedInput::parse(input)?;

        if let Some(name) = &abi.name
            && name.value() == "Rust"
        {
            return Err(syn::Error::new_spanned(
                name,
                "`ffi!` blocks require a non-Rust ABI",
            ));
        }

        let (normalized, aliases) =
            normalize_items(items, NormalizationMode::Ffi(kind), Some(&abi))?;
        let normalized = pack_normalized_items(normalized)?;
        let (mut items, raw_decls) = partition_items(normalized);
        debug_assert!(raw_decls.is_empty());
        validate_items(kind, &attrs, &items)?;
        synthesize_items(&symbol_prefix, &mut items)?;

        let aliases = expand_type_aliases(aliases, failure_mode)?;
        if kind == DeclKind::Extern {
            validate_raw_import_names(&items)?;
        }
        let declarations = match kind {
            DeclKind::Export => {
                expand_export_decls(abi, features, failure_mode, items, &symbol_fragments)
            }
            DeclKind::Extern => expand_extern_decls(
                abi,
                features,
                failure_mode,
                &attrs,
                items,
                &symbol_fragments,
            ),
        };
        return Ok(quote!(#aliases #declarations));
    }

    let co3 = co3_path();
    Ok(emit_macro_invocations(
        quote!(#co3::ffi),
        TokenStream::new(),
        cfg_attr_variants,
    ))
}

/// Generate C-compatbile companions from existing functions.
///
/// Companion declarations name existing functions and generate `extern "C"` functions. Each
/// declaration produces an `unsafe` function named `{name}_raw` where `name` is the name of the
/// existing function. The arguments and the return type of the generated function are lowered into
/// their C-compatbile forms
///
/// # Example
///
/// ```rust
/// use co3::ops::CFn1;
///
/// fn increment(value: Box<u8>) -> u8 {
///     *value + 1
/// }
///
/// co3::raw! {
///     pub fn increment(value: move Box<u8>) -> u8;
/// }
///
/// // Coerce fn item into fn pointer to use `CFn1::call`
/// let companion: unsafe extern "C" fn(_) -> _ = increment_raw;
///
/// // Companion fn pointer call accepts regular Rust types via `CFn1::call`
/// let result: u8 = unsafe { companion.call(Box::new(41_u8)) }.unwrap();
///
/// assert_eq!(result, 42);
/// ```
#[manyhow]
#[proc_macro]
pub fn raw(input: TokenStream) -> Result<TokenStream> {
    let cfg_attr_variants = expand_cfg_attr(input.clone())?;
    if cfg_attr_variants.len() != 1 {
        let co3 = co3_path();
        return Ok(emit_macro_invocations(
            quote!(#co3::raw),
            TokenStream::new(),
            cfg_attr_variants,
        ));
    }

    let ParsedRawInput {
        failure_mode,
        items,
    } = ParsedRawInput::parse(input)?;
    for item in &items {
        match item {
            ParsedItem::Fn(_) => {}
            ParsedItem::Impl(impl_) => {
                if utils::has_runtime_dispatch(&impl_.generics)
                    || impl_
                        .attrs
                        .iter()
                        .any(|attr| attr.path().is_ident("erased"))
                {
                    return Err(syn::Error::new_spanned(
                        &impl_.self_ty,
                        "tagged dispatch in `raw!` is not yet supported",
                    ));
                }
            }
            ParsedItem::Alias(alias) => {
                let ident = match alias {
                    parse::TypeAlias::Rust(alias) => &alias.ident,
                    parse::TypeAlias::RawFunction { ident, .. } => ident,
                };
                return Err(syn::Error::new_spanned(
                    ident,
                    "type aliases are not allowed in `raw!`; use a Rust alias or declare a raw alias in `ffi!`",
                ));
            }
            ParsedItem::Raw(raw_decl) => {
                return Err(syn::Error::new_spanned(
                    &raw_decl.sig.ident,
                    "omit `raw` on `raw!` companion declarations",
                ));
            }
            _ => {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "`raw!` only supports function declarations and impl blocks",
                ));
            }
        }
    }
    let (normalized, aliases) = normalize_items(items, NormalizationMode::Raw, None)?;
    let (foreign_items, raw_decls) = partition_items(normalized);
    if !foreign_items.is_empty() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "`raw!` impl blocks may only contain function declarations",
        ));
    }
    validate_raw_companions(&raw_decls)?;
    debug_assert!(aliases.is_empty());
    let companions = gen_raw_companions(failure_mode, raw_decls);
    Ok(quote!(#(#companions)*))
}

fn normalize_items(
    items: Vec<ParsedItem>,
    mode: NormalizationMode,
    abi: Option<&syn::Abi>,
) -> Result<(Vec<NormalizedItem>, Vec<parse::TypeAlias>)> {
    let mut normalized = Vec::new();
    let mut aliases = Vec::new();
    for item in items {
        match item {
            ParsedItem::Raw(raw_decl) if matches!(mode, NormalizationMode::Ffi(_)) => {
                validate::validate_ffi_raw_signature(&raw_decl.sig, abi.expect("ffi ABI"))?;
                let item = ItemFn {
                    attrs: raw_decl.attrs,
                    vis: raw_decl.vis,
                    modifiers: Default::default(),
                    sig: raw_decl.sig,
                    block: Box::new(parse_quote!({})),
                };
                let ForeignItem::Fn(mut item) = ParsedItem::Fn(item).normalize()? else {
                    unreachable!("function declaration normalizes to a function")
                };
                if mode == NormalizationMode::Ffi(DeclKind::Extern) {
                    item.import_mode = ImportMode::Raw;
                }
                normalized.push(NormalizedItem::Foreign(ForeignItem::Fn(item)));
            }
            ParsedItem::Raw(raw_decl) => normalized.push(NormalizedItem::Raw(raw_decl)),
            ParsedItem::Alias(alias) => aliases.push(alias),
            ParsedItem::Fn(item) if mode == NormalizationMode::Raw => {
                let ident = &item.sig.ident;
                normalized.push(NormalizedItem::Raw(parse::RawFnDecl {
                    attrs: item.attrs,
                    vis: item.vis,
                    callee: quote!(#ident),
                    sig: item.sig,
                    owner: None,
                }));
            }
            ParsedItem::Impl(mut item) => {
                let self_ty = item.self_ty.clone();
                let trait_path = item.trait_.as_ref().map(|(path, _)| path.clone());
                let mut impl_items = Vec::new();
                let mut raw_impl_items = Vec::new();
                for impl_item in core::mem::take(&mut item.items) {
                    let syn::ImplItem::Fn(mut method) = impl_item else {
                        impl_items.push(impl_item);
                        continue;
                    };
                    let marker = method
                        .attrs
                        .iter()
                        .position(|attr| attr.path().is_ident("raw"));
                    if mode == NormalizationMode::Raw {
                        if let Some(marker) = marker {
                            return Err(syn::Error::new_spanned(
                                &method.attrs[marker],
                                "omit `raw` on `raw!` companion declarations",
                            ));
                        }
                    } else if let Some(marker) = marker {
                        method.attrs.remove(marker);
                        validate::validate_ffi_raw_signature(&method.sig, abi.expect("ffi ABI"))?;
                        if mode == NormalizationMode::Ffi(DeclKind::Export) {
                            impl_items.push(syn::ImplItem::Fn(method));
                            continue;
                        }
                    } else {
                        impl_items.push(syn::ImplItem::Fn(method));
                        continue;
                    }
                    let method_name = &method.sig.ident;
                    let callee = if let Some(trait_path) = &trait_path {
                        quote!(<#self_ty as #trait_path>::#method_name)
                    } else {
                        quote!(<#self_ty>::#method_name)
                    };
                    let mut sig = method.sig;
                    if let Some(syn::FnArg::Receiver(receiver)) = sig.inputs.first() {
                        let mut receiver_ty = crate::utils::receiver_ty(receiver);
                        crate::ffi_fn::SelfConcretizer { self_ty: &self_ty }
                            .visit_type_mut(&mut receiver_ty);
                        let attrs = &receiver.attrs;
                        let receiver_arg: syn::FnArg =
                            parse_quote!(#(#attrs)* __co3_self: #receiver_ty);
                        sig.inputs[0] = receiver_arg;
                    }
                    crate::ffi_fn::SelfConcretizer { self_ty: &self_ty }
                        .visit_signature_mut(&mut sig);
                    if matches!(mode, NormalizationMode::Ffi(_)) {
                        method.sig = sig;
                        raw_impl_items.push(syn::ImplItem::Fn(method));
                    } else {
                        normalized.push(NormalizedItem::Raw(parse::RawFnDecl {
                            attrs: method.attrs,
                            vis: method.vis,
                            callee,
                            sig,
                            owner: Some(parse::RawFnOwner {
                                attrs: crate::utils::cfg_attrs(&item.attrs).cloned().collect(),
                                generics: item.generics.clone(),
                                trait_path: trait_path.clone(),
                                self_ty: self_ty.clone(),
                            }),
                        }));
                    }
                }
                if !raw_impl_items.is_empty() {
                    let mut raw_impl = item.clone();
                    raw_impl.items = raw_impl_items;
                    let ForeignItem::Impl(mut raw_impl) = ParsedItem::Impl(raw_impl).normalize()?
                    else {
                        unreachable!("impl declaration normalizes to an impl")
                    };
                    raw_impl.import_mode = ImportMode::Raw;
                    normalized.push(NormalizedItem::Foreign(ForeignItem::Impl(raw_impl)));
                }
                item.items = impl_items;
                if !item.items.is_empty() {
                    normalized.push(NormalizedItem::Foreign(ParsedItem::Impl(item).normalize()?));
                }
            }
            item => normalized.push(NormalizedItem::Foreign(item.normalize()?)),
        }
    }
    reject_implicit_extern_abi(&normalized, mode.macro_name())?;
    Ok((normalized, aliases))
}

fn partition_items(items: Vec<NormalizedItem>) -> (Vec<ForeignItem>, Vec<parse::RawFnDecl>) {
    let mut foreign = Vec::new();
    let mut raw = Vec::new();
    for item in items {
        match item {
            NormalizedItem::Foreign(item) => foreign.push(item),
            NormalizedItem::Raw(item) => raw.push(item),
        }
    }
    (foreign, raw)
}

fn pack_normalized_items(items: Vec<NormalizedItem>) -> Result<Vec<NormalizedItem>> {
    let (foreign, raw) = partition_items(items);
    let mut items = pack_items(foreign)?
        .into_iter()
        .map(NormalizedItem::Foreign)
        .collect::<Vec<_>>();
    items.extend(raw.into_iter().map(NormalizedItem::Raw));
    Ok(items)
}

fn reject_implicit_extern_abi(items: &[NormalizedItem], macro_name: &str) -> Result<()> {
    fn check(sig: &syn::Signature, macro_name: &str) -> Result<()> {
        if let Some(abi) = &sig.abi
            && abi.name.is_none()
        {
            return Err(syn::Error::new_spanned(
                abi,
                format!("`extern fn` declarations inside `{macro_name}` require an explicit ABI"),
            ));
        }
        Ok(())
    }

    for item in items {
        match item {
            NormalizedItem::Foreign(ForeignItem::Fn(item)) => check(&item.item.sig, macro_name)?,
            NormalizedItem::Foreign(ForeignItem::Impl(item)) => {
                for impl_item in &item.item.items {
                    if let syn::ImplItem::Fn(method) = impl_item {
                        check(&method.sig, macro_name)?;
                    }
                }
            }
            NormalizedItem::Raw(raw_decl) => check(&raw_decl.sig, macro_name)?,
            NormalizedItem::Foreign(ForeignItem::Type(_) | ForeignItem::Static(_)) => {}
        }
    }
    Ok(())
}

fn expand_type_aliases(
    aliases: Vec<parse::TypeAlias>,
    failure_mode: parse::FailureMode,
) -> Result<TokenStream> {
    let aliases = aliases
        .into_iter()
        .map(|alias| match alias {
        parse::TypeAlias::Rust(item) => Ok(quote!(#item)),
        parse::TypeAlias::RawFunction {
            attrs,
            vis,
            ident,
            generics,
            sig,
            move_fn,
        } => {
            let Some(abi) = &sig.abi else {
                return Err(syn::Error::new_spanned(
                    &ident,
                    "raw function pointer aliases require `raw extern \"ABI\" fn`; specify the pointer's ABI explicitly",
                ));
            };
            let Some(name) = &abi.name else {
                return Err(syn::Error::new_spanned(abi, "`extern fn` requires an explicit ABI"));
            };
            if name.value() == "Rust" {
                return Err(syn::Error::new_spanned(abi, "raw function pointer aliases cannot use the Rust ABI"));
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
                    "raw function pointer aliases require a safe, synchronous, non-generic function with at most 12 arguments",
                ));
            }
            let raw_fn_type = callback::lower_callback_fn_type(*sig, &abi, failure_mode, move_fn)?;
            Ok(quote! {
                #(#attrs)* #vis type #ident #generics = #raw_fn_type;
            })
        }
    })
        .collect::<Result<Vec<_>>>()?;
    Ok(quote!(#(#aliases)*))
}

fn validate_raw_import_names(items: &[ForeignItem]) -> Result<()> {
    let collision = |name: &syn::Ident| {
        syn::Error::new_spanned(
            name,
            "raw and Rust imports need distinct Rust names; use `#[symbol_name = \"...\"]` to bind them to the same foreign symbol",
        )
    };
    let impls = items
        .iter()
        .flat_map(|item| match item {
            ForeignItem::Impl(impl_) => vec![impl_],
            ForeignItem::Type(ty) => ty.self_impls.iter().collect(),
            _ => Vec::new(),
        })
        .collect::<Vec<_>>();
    for item in items {
        match item {
            ForeignItem::Fn(raw)
                if raw.import_mode == ImportMode::Raw
                    && items.iter().any(|candidate| {
                    matches!(candidate, ForeignItem::Fn(regular)
                        if regular.import_mode == ImportMode::Regular && regular.sig.ident == raw.sig.ident)
                    }) =>
            {
                return Err(collision(&raw.sig.ident));
                }
            _ => {}
        }
    }
    for raw in impls
        .iter()
        .filter(|impl_| impl_.import_mode == ImportMode::Raw)
    {
        let raw_self = &raw.self_ty;
        let raw_trait = raw
            .trait_
            .as_ref()
            .map(|(path, _)| quote!(#path).to_string());
        for method in &raw.items {
            let syn::ImplItem::Fn(method) = method else {
                continue;
            };
            if impls.iter().any(|regular| {
                let regular_self = &regular.self_ty;
                regular.import_mode == ImportMode::Regular
                    && quote!(#regular_self).to_string() == quote!(#raw_self).to_string()
                    && regular.trait_.as_ref().map(|(path, _)| quote!(#path).to_string())
                        == raw_trait
                    && regular.items.iter().any(|item| {
                        matches!(item, syn::ImplItem::Fn(other) if other.sig.ident == method.sig.ident)
                    })
            }) {
                return Err(collision(&method.sig.ident));
            }
        }
    }
    Ok(())
}

fn pack_items(items: Vec<ForeignItem>) -> Result<Vec<ForeignItem>> {
    pack_type_drop_impls(pack_type_self_impls(items))
}

fn validate_items(kind: DeclKind, attrs: &[Attribute], items: &[ForeignItem]) -> Result<()> {
    match kind {
        DeclKind::Export => {
            validate_export_attrs(attrs)?;
            validate_export_decls(items)
        }
        DeclKind::Extern => validate_extern_decls(items),
    }
}

fn synthesize_items(symbol_prefix: &LitStr, items: &mut [ForeignItem]) -> Result<()> {
    synthesize_dispatch_tag_ids_in_decls(items)?;

    let declared_types = declared_foreign_type_idents(items);
    for item in &mut *items {
        ensure_symbol_names(item, symbol_prefix, &declared_types);
    }

    synthesize_default_drop_impls(symbol_prefix, items)?;

    Ok(())
}

fn normalize_dyn_self_tag_ids(impl_: &mut ItemImpl) {
    struct Normalizer<'a> {
        receiver: &'a Type,
    }

    impl VisitMut for Normalizer<'_> {
        fn visit_type_path_mut(&mut self, ty: &mut syn::TypePath) {
            let receiver_bound =
                trait_object_single_trait_bound(self.receiver).map(|bound| &bound.path);
            if ty.path.segments.len() == 1
                && ty.path.segments[0].ident == "TAG"
                && ty.path.segments[0].arguments.is_none()
                && ty.qself.as_ref().is_some_and(|qself| {
                    qself.ty.as_ref() == self.receiver
                        || matches!(
                            qself.ty.as_ref(),
                            Type::Path(path)
                                if receiver_bound.is_some_and(|bound| path.path == *bound)
                        )
                })
            {
                *ty.qself.as_mut().unwrap().ty = parse_quote!(dyn Self);
            }

            syn::visit_mut::visit_type_path_mut(self, ty);
        }
    }

    let receiver = (*impl_.self_ty).clone();
    let mut normalizer = Normalizer {
        receiver: &receiver,
    };
    for item in &mut impl_.items {
        let syn::ImplItem::Fn(method) = item else {
            continue;
        };
        normalizer.visit_signature_mut(&mut method.sig);
    }
}

pub(crate) fn is_declared_dyn_self_impl(
    impl_: &ItemImpl,
    declared_types: &BTreeSet<syn::Ident>,
) -> bool {
    let Some(bound) = trait_object_single_trait_bound(&impl_.self_ty) else {
        return false;
    };
    let Some(ident) = direct_trait_bound_ident(bound) else {
        return false;
    };
    declared_types.contains(ident)
}

fn direct_trait_bound_ident(bound: &syn::TraitBound) -> Option<&syn::Ident> {
    (bound.path.segments.len() == 1).then(|| &bound.path.segments[0].ident)
}

fn synthesize_default_drop_impls(
    symbol_prefix: &syn::LitStr,
    items: &mut [ForeignItem],
) -> Result<()> {
    let selected_drop_types = selected_drop_types(items);
    for item in items {
        let ForeignItem::Type(item) = item else {
            continue;
        };
        if item.drop.is_some() || selected_drop_types.contains(&item.ty.ident) {
            continue;
        }

        if has_non_lifetime_generics(&item.ty.generics) {
            continue;
        }

        item.drop = Some(Co3Impl::new(synthesize_default_drop_impl(
            symbol_prefix,
            &item.ty,
        )));
    }

    Ok(())
}

fn synthesize_dispatch_tag_ids_in_decls(items: &mut [ForeignItem]) -> Result<()> {
    let declared_types = declared_foreign_type_idents(items);

    fn synthesize_impl(impl_: &mut Co3Impl, declared_types: &BTreeSet<syn::Ident>) -> Result<()> {
        let generics = impl_.generics.clone();
        let self_ty = (*impl_.self_ty).clone();
        let dyn_self = is_declared_dyn_self_impl(impl_, declared_types);
        let mut dispatched_methods = impl_
            .method_dispatch_args
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        dispatched_methods.extend(impl_.items.iter().filter_map(|item| {
            let syn::ImplItem::Fn(method) = item else {
                return None;
            };
            method
                .sig
                .generics
                .type_params()
                .any(|param| param.attrs.iter().any(utils::is_type_erased))
                .then(|| method.sig.ident.clone())
        }));

        if dyn_self {
            for item in &mut impl_.items {
                let syn::ImplItem::Fn(method) = item else {
                    continue;
                };
                synthesize_dispatch_tag_ids(Some(&self_ty), &generics, &mut method.sig.inputs);
            }
        }

        if !impl_.dispatch_args.is_empty() || utils::has_runtime_dispatch(&generics) {
            for item in &mut impl_.items {
                let syn::ImplItem::Fn(method) = item else {
                    continue;
                };
                synthesize_dispatch_tag_ids(None, &generics, &mut method.sig.inputs);
            }
        }

        for item in &mut impl_.items {
            let syn::ImplItem::Fn(method) = item else {
                continue;
            };
            if dispatched_methods.contains(&method.sig.ident) {
                let mut visible_generics = method.sig.generics.clone();
                ffi_fn::merge_generics(generics.clone(), &mut visible_generics);
                synthesize_dispatch_tag_ids(
                    dyn_self.then_some(&self_ty),
                    &visible_generics,
                    &mut method.sig.inputs,
                );
            }
        }

        Ok(())
    }

    for item in items {
        match item {
            ForeignItem::Type(item) => {
                for impl_ in &mut item.self_impls {
                    synthesize_impl(impl_, &declared_types)?;
                }
                if let Some(drop) = &mut item.drop {
                    synthesize_impl(drop, &declared_types)?;
                }
            }
            ForeignItem::Impl(impl_) => synthesize_impl(impl_, &declared_types)?,
            ForeignItem::Fn(item)
                if !item.dispatch_args.is_empty()
                    || utils::has_runtime_dispatch(&item.sig.generics) =>
            {
                let generics = item.sig.generics.clone();
                synthesize_dispatch_tag_ids(None, &generics, &mut item.sig.inputs);
            }
            ForeignItem::Fn(_) | ForeignItem::Static(_) => {}
        }
    }

    Ok(())
}

fn declared_foreign_type_idents(items: &[ForeignItem]) -> BTreeSet<syn::Ident> {
    items
        .iter()
        .filter_map(|item| match item {
            ForeignItem::Type(item) => Some(item.ty.ident.clone()),
            _ => None,
        })
        .collect()
}

fn ensure_symbol_names(
    decl: &mut ForeignItem,
    symbol_prefix: &LitStr,
    declared_types: &BTreeSet<syn::Ident>,
) {
    match decl {
        ForeignItem::Fn(item) => ensure_symbol_names_on_fn(item, symbol_prefix, declared_types),
        ForeignItem::Impl(impl_) => {
            ensure_symbol_names_on_impl(impl_, symbol_prefix, declared_types);
        }
        ForeignItem::Type(item) => {
            for impl_ in &mut item.self_impls {
                ensure_symbol_names_on_impl(impl_, symbol_prefix, declared_types);
            }
            if let Some(drop) = &mut item.drop {
                ensure_symbol_names_on_impl(drop, symbol_prefix, declared_types);
            }
        }
        ForeignItem::Static(static_) => {
            ensure_symbol_name_on_static(&mut static_.attrs, symbol_prefix, &static_.ident);
        }
    }
}

fn ensure_symbol_names_on_fn(
    item: &mut Co3Fn,
    symbol_prefix: &LitStr,
    declared_types: &BTreeSet<syn::Ident>,
) {
    let ident = item.sig.ident.clone();

    let static_params = crate::validate::fn_symbol_binding_params(item, declared_types);
    ensure_symbol_name_on_fn(&mut item.attrs, symbol_prefix, &ident, &static_params);
}

fn ensure_symbol_names_on_impl(
    impl_: &mut Co3Impl,
    symbol_prefix: &LitStr,
    declared_types: &BTreeSet<syn::Ident>,
) {
    let trait_ = impl_.trait_.as_ref().map(|(path, _)| path.clone());
    let self_ty = (*impl_.self_ty).clone();
    let generics = impl_.generics.clone();
    let static_params = impl_
        .items
        .iter()
        .filter_map(|item| {
            let syn::ImplItem::Fn(method) = item else {
                return None;
            };
            Some((
                method.sig.ident.clone(),
                crate::validate::impl_method_symbol_binding_params(impl_, method, declared_types),
            ))
        })
        .collect::<HashMap<_, _>>();

    for item in &mut impl_.items {
        let syn::ImplItem::Fn(syn::ImplItemFn { attrs, sig, .. }) = item else {
            continue;
        };

        ensure_symbol_name_on_impl_fn(
            attrs,
            symbol_prefix,
            trait_.as_ref(),
            &self_ty,
            &generics,
            &sig.ident,
            &static_params[&sig.ident],
        );
    }
}

pub(crate) fn is_symbol_name_attr(attr: &Attribute) -> bool {
    attr.path().is_ident("symbol_name")
}

pub(crate) fn symbol_name_value(attr: &Attribute) -> Option<&Expr> {
    if !is_symbol_name_attr(attr) {
        return None;
    }

    let syn::Meta::NameValue(nv) = &attr.meta else {
        return None;
    };

    Some(&nv.value)
}

fn selected_drop_target_idents(drop: &Co3Impl) -> Option<Vec<syn::Ident>> {
    if !is_drop_impl(&drop.item) {
        return None;
    }

    let Type::Path(self_ty) = drop.item.self_ty.as_ref() else {
        return None;
    };
    let param = self_ty.path.get_ident()?;
    if !drop
        .item
        .generics
        .type_params()
        .any(|candidate| candidate.ident == *param)
        || !drop.dispatch_args.contains_param(param)
    {
        return None;
    }

    let (params, targets) = drop
        .dispatch_args
        .groups()
        .find(|(params, _)| params.contains(param))?;
    let param_index = params.iter().position(|candidate| candidate == param)?;
    Some(
        targets
            .iter()
            .filter_map(|target| match target.args.get(param_index) {
                Some(syn::GenericArgument::Type(Type::Path(target)))
                    if target.qself.is_none() && target.path.segments.len() == 1 =>
                {
                    target
                        .path
                        .segments
                        .first()
                        .map(|segment| segment.ident.clone())
                }
                _ => None,
            })
            .collect(),
    )
}

pub(crate) fn selected_drop_types(decls: &[ForeignItem]) -> BTreeSet<syn::Ident> {
    decls
        .iter()
        .filter_map(|decl| match decl {
            ForeignItem::Impl(impl_) => selected_drop_target_idents(impl_),
            ForeignItem::Type(_) | ForeignItem::Fn(_) | ForeignItem::Static(_) => None,
        })
        .flatten()
        .collect()
}

fn pack_type_drop_impls(decls: Vec<ForeignItem>) -> Result<Vec<ForeignItem>> {
    fn insert_drop(
        explicit_drops: &mut BTreeMap<syn::Ident, Co3Impl>,
        errors: &mut Option<syn::Error>,
        self_ty: syn::Ident,
        drop_impl: Co3Impl,
    ) {
        if let Some(prev) = explicit_drops.insert(self_ty, drop_impl) {
            let err_msg = "duplicate explicit `impl Drop` declaration";
            push_error(errors, syn::Error::new_spanned(prev.item.self_ty, err_msg));
        }
    }

    fn self_ty_ident(impl_: &syn::ItemImpl) -> Option<syn::Ident> {
        match &*impl_.self_ty {
            Type::Path(syn::TypePath {
                qself: None, path, ..
            }) if path.segments.len() == 1 => path.segments.last().map(|seg| seg.ident.clone()),
            self_ty => trait_object_single_trait_bound(self_ty)
                .filter(|bound| bound.path.segments.len() == 1)
                .and_then(|bound| bound.path.segments.first())
                .map(|segment| segment.ident.clone()),
        }
    }

    const UNKNOWN_DROP: &str = "explicit `impl Drop` is only allowed for declared types";

    let mut dynamic_drop_coverage = BTreeMap::<syn::Ident, usize>::new();
    for decl in &decls {
        let ForeignItem::Impl(drop) = decl else {
            continue;
        };
        let Some(targets) = selected_drop_target_idents(drop) else {
            continue;
        };
        for target in targets.into_iter().collect::<BTreeSet<_>>() {
            *dynamic_drop_coverage.entry(target).or_default() += 1;
        }
    }
    let mut kept_decls = Vec::with_capacity(decls.len());
    let mut explicit_drops = BTreeMap::new();
    let mut errors = None::<syn::Error>;

    for decl in decls {
        match decl {
            ForeignItem::Type(mut item) => {
                let mut kept_self_impls = Vec::with_capacity(item.self_impls.len());

                let self_ty = &item.ty.ident;
                if let Some(drop) = item.drop.take() {
                    insert_drop(&mut explicit_drops, &mut errors, self_ty.clone(), drop);
                }
                for dyn_impl in item.self_impls {
                    if is_drop_impl(&dyn_impl.item) {
                        insert_drop(&mut explicit_drops, &mut errors, self_ty.clone(), dyn_impl);
                    } else {
                        kept_self_impls.push(dyn_impl);
                    }
                }

                item.self_impls = kept_self_impls;
                kept_decls.push(ForeignItem::Type(item));
            }
            ForeignItem::Impl(impl_) => {
                if is_drop_impl(&impl_) {
                    if selected_drop_target_idents(&impl_).is_some() {
                        kept_decls.push(ForeignItem::Impl(impl_));
                        continue;
                    }
                    if let Some(self_ty) = self_ty_ident(&impl_) {
                        insert_drop(&mut explicit_drops, &mut errors, self_ty, impl_);
                    } else {
                        let err = syn::Error::new_spanned(&impl_.self_ty, UNKNOWN_DROP);
                        push_error(&mut errors, err);
                    }
                } else {
                    kept_decls.push(ForeignItem::Impl(impl_));
                }
            }
            ForeignItem::Fn(item) => kept_decls.push(ForeignItem::Fn(item)),
            ForeignItem::Static(item) => kept_decls.push(ForeignItem::Static(item)),
        }
    }

    for decl in &mut kept_decls {
        let ForeignItem::Type(item) = decl else {
            continue;
        };

        item.drop = explicit_drops.remove(&item.ty.ident);
        let dynamic_count = dynamic_drop_coverage
            .get(&item.ty.ident)
            .copied()
            .unwrap_or_default();
        if dynamic_count > 1 || (dynamic_count == 1 && item.drop.is_some()) {
            let err_msg = "duplicate `impl Drop` declaration for opaque type";
            push_error(&mut errors, syn::Error::new_spanned(&item.ty, err_msg));
        }
    }

    for drop_impl in explicit_drops.into_values() {
        push_error(
            &mut errors,
            syn::Error::new_spanned(drop_impl.item.self_ty, UNKNOWN_DROP),
        );
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(kept_decls)
}

pub(crate) fn trait_object_single_trait_bound(self_ty: &syn::Type) -> Option<&syn::TraitBound> {
    let syn::Type::TraitObject(trait_object) = self_ty else {
        return None;
    };
    if trait_object.bounds.len() != 1 {
        return None;
    }

    let bound = trait_object.bounds.first()?;
    let syn::TypeParamBound::Trait(trait_bound) = bound else {
        return None;
    };
    if trait_bound.maybe.is_some() || trait_bound.lifetimes.is_some() {
        return None;
    }

    Some(trait_bound)
}

fn pack_type_self_impls(decls: Vec<ForeignItem>) -> Vec<ForeignItem> {
    fn declared_self_type(impl_: &ItemImpl) -> Option<syn::Ident> {
        match &*impl_.self_ty {
            Type::Path(syn::TypePath {
                qself: None, path, ..
            }) if path.segments.len() == 1 => {
                path.segments.first().map(|segment| segment.ident.clone())
            }
            _ => trait_object_single_trait_bound(&impl_.self_ty)
                .filter(|bound| bound.path.segments.len() == 1)
                .and_then(|bound| bound.path.segments.first())
                .map(|segment| segment.ident.clone()),
        }
    }

    let mut type_dispatch = decls
        .iter()
        .filter_map(|decl| {
            if let ForeignItem::Type(item) = decl {
                Some((item.ty.ident.clone(), Vec::new()))
            } else {
                None
            }
        })
        .collect::<BTreeMap<_, _>>();

    let mut kept_decls = Vec::with_capacity(decls.len());

    for decl in decls {
        let dispatch = match decl {
            ForeignItem::Impl(dispatch) => dispatch,
            decl => {
                kept_decls.push(decl);
                continue;
            }
        };
        let Some(ident) = declared_self_type(&dispatch.item) else {
            kept_decls.push(ForeignItem::Impl(dispatch));
            continue;
        };
        if !type_dispatch.contains_key(&ident) {
            kept_decls.push(ForeignItem::Impl(dispatch));
            continue;
        }

        type_dispatch.entry(ident).or_default().push(dispatch);
    }

    for decl in &mut kept_decls {
        let ForeignItem::Type(item) = decl else {
            continue;
        };

        item.self_impls = type_dispatch.remove(&item.ty.ident).unwrap_or_default();
    }

    kept_decls
}

fn synthesize_default_drop_impl(symbol_prefix: &LitStr, ty: &syn::ForeignItemType) -> ItemImpl {
    let (impl_generics, ty_generics, where_clause) = ty.generics.split_for_impl();

    let ident = &ty.ident;
    let symbol_name = LitStr::new(
        &format!(
            "{}__{}__{}__drop",
            symbol_prefix.value(),
            path_symbol_name(&parse_quote!(Drop), &Default::default()),
            type_symbol_name(&parse_quote!(#ident #ty_generics), &ty.generics),
        ),
        ident.span(),
    );

    parse_quote! {
        impl #impl_generics Drop for #ident #ty_generics #where_clause {
            #[symbol_name = #symbol_name]
            fn drop(&mut self) {}
        }
    }
}

fn ensure_symbol_name_on_fn(
    attrs: &mut Vec<Attribute>,
    symbol_prefix: &LitStr,
    fn_name: &syn::Ident,
    static_params: &[syn::Ident],
) {
    if !attrs.iter().any(is_symbol_name_attr) {
        let suffix = static_params_suffix(static_params);
        let symbol_name = LitStr::new(
            &format!("{}__{fn_name}{suffix}", symbol_prefix.value()),
            fn_name.span(),
        );

        attrs.push(parse_quote! {
            #[symbol_name = #symbol_name]
        });
    }
}

fn static_params_suffix(static_params: &[syn::Ident]) -> String {
    static_params
        .iter()
        .map(|param| format!("__{{{param}}}"))
        .collect()
}

fn ensure_symbol_name_on_static(
    attrs: &mut Vec<Attribute>,
    symbol_prefix: &LitStr,
    static_name: &syn::Ident,
) {
    ensure_symbol_name_on_fn(attrs, symbol_prefix, static_name, &[]);
}

fn ensure_symbol_name_on_impl_fn(
    attrs: &mut Vec<Attribute>,
    symbol_prefix: &LitStr,
    trait_: Option<&Path>,
    self_ty: &Type,
    generics: &syn::Generics,
    fn_name: &syn::Ident,
    static_params: &[syn::Ident],
) {
    if !attrs.iter().any(is_symbol_name_attr) {
        let default_symbol_name = trait_.map_or_else(
            || format!("{}__{}", type_symbol_name(self_ty, generics), fn_name),
            |trait_| {
                format!(
                    "{}__{}__{}",
                    path_symbol_name(trait_, generics),
                    type_symbol_name(self_ty, generics),
                    fn_name
                )
            },
        );
        let symbol_name = LitStr::new(
            &format!(
                "{}__{}{suffix}",
                symbol_prefix.value(),
                default_symbol_name,
                suffix = static_params_suffix(static_params),
            ),
            proc_macro2::Span::call_site(),
        );

        attrs.push(parse_quote! {
            #[symbol_name = #symbol_name]
        });
    }
}

#[cfg(test)]
mod dyn_self_tag_id_tests {
    use super::*;

    fn id_type(item: &ItemImpl) -> &Type {
        let syn::ImplItem::Fn(method) = &item.items[0] else {
            panic!("expected method");
        };
        let syn::FnArg::Typed(arg) = &method.sig.inputs[0] else {
            panic!("expected typed argument");
        };
        arg.ty.as_ref()
    }

    #[test]
    fn leaves_dyn_type_id_for_concrete_impl_self() {
        let mut item: ItemImpl = parse_quote! {
            impl<T> Trait for T {
                fn kita(tag: <dyn T>::TAG) {}
            }
        };

        normalize_dyn_self_tag_ids(&mut item);
        let expected = parse_quote!(<dyn T>::TAG);
        assert_eq!(id_type(&item), &expected);
    }

    #[test]
    fn rewrites_dyn_type_id_for_dyn_self_impl() {
        let mut item: ItemImpl = parse_quote! {
            impl<T> Trait for dyn T {
                fn kita(tag: <dyn T>::TAG) {}
            }
        };

        normalize_dyn_self_tag_ids(&mut item);
        let expected = parse_quote!(<dyn Self>::TAG);
        assert_eq!(id_type(&item), &expected);
    }
}
