use std::collections::{BTreeMap, BTreeSet};

use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Literal, TokenStream};
use quote::{ToTokens, format_ident, quote};
use syn::{
    Attribute, Expr, GenericArgument, Type, TypePath, parse_quote, visit::Visit,
    visit_mut::VisitMut,
};

const MAX_TUPLE_ARITY: usize = 12;

pub(crate) fn co3_path() -> TokenStream {
    match crate_name("co3") {
        Ok(FoundCrate::Itself) => quote!(::co3),
        Ok(FoundCrate::Name(name)) => {
            let name = format_ident!("{name}");
            quote!(::#name)
        }
        Err(_) => quote!(::co3),
    }
}

pub(crate) fn cfg_attrs(attrs: &[Attribute]) -> impl Iterator<Item = &Attribute> {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr"))
}

pub(crate) fn push_error(errors: &mut Option<syn::Error>, err: syn::Error) {
    if let Some(errors) = errors {
        errors.combine(err);
    } else {
        *errors = Some(err);
    }
}

pub(crate) fn soft_for_arg(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("soft"))
}

pub(crate) fn is_type_erased(attr: &Attribute) -> bool {
    attr.path().is_ident("erased")
}

pub(crate) fn is_payload_erased(param: &syn::TypeParam) -> bool {
    param.attrs.iter().any(is_type_erased) && param.default.is_some()
}

pub(crate) fn has_runtime_dispatch(generics: &syn::Generics) -> bool {
    generics
        .type_params()
        .any(|param| param.attrs.iter().any(is_type_erased))
}

pub(crate) fn strip_internal_generic_param(param: &mut syn::TypeParam) {
    let is_erased = param.attrs.iter().any(is_type_erased);
    param.attrs.retain(|attr| !is_type_erased(attr));

    if is_erased {
        param.default = None;
    }
}

pub(crate) fn erased_id_repr(param: &syn::TypeParam) -> Option<syn::Type> {
    let attr = param.attrs.iter().find(|attr| is_type_erased(attr))?;
    attr.parse_args().ok()
}

pub(crate) fn receiver_ty(receiver: &syn::Receiver) -> syn::Type {
    match &receiver.kind {
        syn::ReceiverKind::Value => parse_quote!(Self),
        syn::ReceiverKind::Reference(and_token, lifetime, mutability) => {
            syn::Type::Reference(syn::TypeReference {
                attrs: Vec::new(),
                and_token: *and_token,
                lifetime: lifetime.clone(),
                mutability: *mutability,
                elem: Box::new(parse_quote!(Self)),
            })
        }
        syn::ReceiverKind::Typed(_, ty) => (**ty).clone(),
        _ => parse_quote!(Self),
    }
}

pub(crate) struct ParamUseDetector<'a> {
    params: BTreeSet<&'a syn::Ident>,
    found: bool,
}

impl Visit<'_> for ParamUseDetector<'_> {
    fn visit_path(&mut self, node: &syn::Path) {
        if node.leading_colon.is_none()
            && let Some(first) = node.segments.first()
            && self.params.contains(&first.ident)
        {
            self.found = true;
            return;
        }

        syn::visit::visit_path(self, node);
    }
}

impl<'a> ParamUseDetector<'a> {
    pub fn new(params: impl IntoIterator<Item = &'a syn::Ident>) -> Self {
        Self {
            params: params.into_iter().collect(),
            found: false,
        }
    }

    pub fn type_mentions_param(&self, ty: &syn::Type) -> bool {
        let mut detector = Self::new(self.params.clone());
        detector.visit_type(ty);
        detector.found
    }

    pub fn path_mentions_param(&self, path: &syn::Path) -> bool {
        let mut detector = Self::new(self.params.clone());
        detector.visit_path(path);
        detector.found
    }

    pub fn predicate_mentions_param(&self, predicate: &syn::WherePredicate) -> bool {
        let mut detector = Self::new(self.params.clone());
        detector.visit_where_predicate(predicate);
        detector.found
    }

    pub fn generic_arg_mentions_param(&self, arg: &GenericArgument) -> bool {
        let mut detector = Self::new(self.params.clone());
        detector.visit_generic_argument(arg);
        detector.found
    }
}

pub(crate) fn gen_store_name(arg_name: &syn::Ident) -> syn::Ident {
    format_ident!("__co3_{arg_name}_store")
}

/// Builds an order-preserving tuple tree whose nodes fit the tuple impls
/// available in `co3`. Partial groups use their exact arity, and singleton
/// remainders above the leaf level are promoted instead of wrapped again.
pub fn build_extern_c_type_tuple(types: &[&Type]) -> (TokenStream, TokenStream, Vec<TokenStream>) {
    if types.is_empty() {
        return (quote!(()), quote!(()), Vec::new());
    }

    let mut nodes = types
        .chunks(MAX_TUPLE_ARITY)
        .map(|chunk| {
            let c_types = chunk.iter().map(|ty| quote!(<#ty as co3::ReprC>::CType));
            let accessors = (0..chunk.len())
                .map(|index| {
                    let index = Literal::usize_unsuffixed(index);
                    quote!(#index)
                })
                .collect();
            let c_tuple_ident = format_ident!("ReprCTuple{}", chunk.len());

            (
                quote!((#(#chunk,)*)),
                quote!(co3::tuple::#c_tuple_ident<#(#c_types),*>),
                accessors,
            )
        })
        .collect::<Vec<_>>();

    // Repeatedly pack consecutive nodes into the largest supported tuple. A
    // lone remainder is promoted unchanged, avoiding redundant unary tuple
    // layers while preserving field order.
    while nodes.len() > 1 {
        let mut input = nodes.into_iter();
        let mut next = Vec::new();

        loop {
            let chunk = input.by_ref().take(MAX_TUPLE_ARITY).collect::<Vec<_>>();
            if chunk.is_empty() {
                break;
            }
            next.push(pack_tuple_nodes(chunk));
        }

        nodes = next;
    }

    nodes.pop().expect("non-empty input produces a tuple")
}

fn pack_tuple_nodes(
    mut nodes: Vec<(TokenStream, TokenStream, Vec<TokenStream>)>,
) -> (TokenStream, TokenStream, Vec<TokenStream>) {
    if nodes.len() == 1 {
        return nodes.pop().expect("singleton node");
    }

    let tuple_elems = nodes.iter().map(|(tuple, _, _)| tuple);
    let c_tuple_elems = nodes.iter().map(|(_, c_tuple, _)| c_tuple);
    let tuple = quote!((#(#tuple_elems,)*));
    let c_tuple_ident = format_ident!("ReprCTuple{}", nodes.len());
    let c_tuple = quote!(co3::tuple::#c_tuple_ident<#(#c_tuple_elems),*>);
    let accessors = nodes
        .into_iter()
        .enumerate()
        .flat_map(|(index, (_, _, accessors))| {
            let index = Literal::usize_unsuffixed(index);
            accessors
                .into_iter()
                .map(move |accessor| quote!(#index.#accessor))
        })
        .collect();

    (tuple, c_tuple, accessors)
}

pub(crate) struct DispatchMonomorphizer<'a> {
    subst: BTreeMap<&'a syn::Ident, &'a GenericArgument>,
}

impl<'a> DispatchMonomorphizer<'a> {
    pub(crate) fn for_substitutions(
        generics: &'a syn::Generics,
        substitutions: impl Iterator<Item = (&'a syn::Ident, &'a GenericArgument)>,
    ) -> Self {
        let subst = substitutions
            .filter_map(|(ident, arg)| {
                generics.params.iter().find_map(|param| match param {
                    syn::GenericParam::Type(param) if param.ident == *ident => Some((ident, arg)),
                    syn::GenericParam::Const(param) if param.ident == *ident => Some((ident, arg)),
                    _ => None,
                })
            })
            .collect();

        Self { subst }
    }

    pub(crate) fn for_dispatch_group(
        generics: &'a syn::Generics,
        selections: &[crate::DispatchSelection<'a>],
    ) -> Self {
        Self::for_substitutions(
            generics,
            selections
                .iter()
                .flat_map(|selection| selection.params.iter().zip(&selection.target.args)),
        )
    }

    pub(crate) fn for_static_dispatch_group(
        generics: &'a syn::Generics,
        selections: &[crate::DispatchSelection<'a>],
    ) -> Self {
        Self::for_substitutions(
            generics,
            selections.iter().flat_map(|selection| {
                selection
                    .params
                    .iter()
                    .zip(&selection.target.args)
                    .filter(|(ident, _)| {
                        !generics.type_params().any(|param| {
                            param.ident == **ident && param.attrs.iter().any(is_type_erased)
                        })
                    })
            }),
        )
    }

    pub(crate) fn interpolate_symbol_attrs(
        &self,
        attrs: &mut [syn::Attribute],
        symbol_fragments: &BTreeMap<String, syn::LitStr>,
    ) {
        for attr in attrs {
            if !crate::is_symbol_name_attr(attr) {
                continue;
            }
            let syn::Meta::NameValue(name_value) = &mut attr.meta else {
                continue;
            };
            let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(symbol),
                ..
            }) = &mut name_value.value
            else {
                continue;
            };

            let mut value = symbol.value();
            for (ident, argument) in &self.subst {
                let key = argument.to_token_stream().to_string();
                let fragment = symbol_fragments.get(&key).map_or_else(
                    || match argument {
                        GenericArgument::Type(ty) => {
                            type_symbol_name(ty, &syn::Generics::default())
                        }
                        GenericArgument::Const(expr) => {
                            sanitize_symbol_component(&expr.to_token_stream().to_string())
                        }
                        _ => sanitize_symbol_component(&key),
                    },
                    syn::LitStr::value,
                );
                value = value.replace(&format!("{{{ident}}}"), &fragment);
            }
            *symbol = syn::LitStr::new(&value, symbol.span());
        }
    }
}

impl VisitMut for DispatchMonomorphizer<'_> {
    fn visit_expr_mut(&mut self, node: &mut Expr) {
        if let Expr::Path(path) = node
            && path.qself.is_none()
            && let Some(ident) = path.path.get_ident()
            && let Some(GenericArgument::Const(subst)) = self.subst.get(ident)
        {
            *node = subst.clone();
            return;
        }

        syn::visit_mut::visit_expr_mut(self, node);
    }

    fn visit_item_impl_mut(&mut self, node: &mut syn::ItemImpl) {
        syn::visit_mut::visit_item_impl_mut(self, node);
    }

    fn visit_type_mut(&mut self, node: &mut Type) {
        syn::visit_mut::visit_type_mut(self, node);

        if let Type::Path(TypePath {
            qself: None, path, ..
        }) = node
            && let Some(first) = path.segments.first()
            && let Some(GenericArgument::Type(subst)) = self.subst.get(&first.ident).cloned()
        {
            if path.segments.len() == 1 {
                let mut replacement = parse_quote!(#subst);
                self.visit_type_mut(&mut replacement);
                *node = replacement;
                return;
            }

            let mut rest = syn::Path {
                leading_colon: None,
                segments: Default::default(),
            };

            for segment in path.segments.iter().skip(1) {
                rest.segments.push(segment.clone());
            }

            *node = parse_quote!(#subst::#rest);
        }
    }
}

pub(crate) fn is_drop_impl(impl_: &syn::ItemImpl) -> bool {
    impl_
        .trait_
        .as_ref()
        .is_some_and(|(path, _)| path.segments.last().is_some_and(|seg| seg.ident == "Drop"))
}

pub(crate) fn has_non_lifetime_generics(generics: &syn::Generics) -> bool {
    generics
        .params
        .iter()
        .any(|param| !matches!(param, syn::GenericParam::Lifetime(_)))
}

pub(crate) fn path_symbol_name(path: &syn::Path, generics: &syn::Generics) -> String {
    let mut builder = SymbolNameBuilder::new(generics);
    builder.visit_path(path);
    builder.finish()
}

pub(crate) fn type_symbol_name(ty: &Type, generics: &syn::Generics) -> String {
    let mut builder = SymbolNameBuilder::new(generics);
    builder.visit_type(ty);
    builder.finish()
}

#[derive(Default)]
struct SymbolNameBuilder {
    out: String,
    generic_params: BTreeMap<String, String>,
}

impl SymbolNameBuilder {
    fn new(generics: &syn::Generics) -> Self {
        let generic_params = generics
            .params
            .iter()
            .filter_map(|param| match param {
                syn::GenericParam::Type(param) => Some(param.ident.to_string()),
                _ => None,
            })
            .enumerate()
            .map(|(idx, ident)| (ident, format!("T{idx}")))
            .collect();

        Self {
            out: String::new(),
            generic_params,
        }
    }

    fn finish(self) -> String {
        sanitize_symbol_component(&self.out)
    }

    fn push_sep(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with('_') {
            self.out.push('_');
        }
    }

    fn push_atom(&mut self, value: &str) {
        let sanitized = sanitize_symbol_component(value);

        if sanitized.is_empty() {
            return;
        }

        self.push_sep();
        self.out.push_str(&sanitized);
    }
}

impl Visit<'_> for SymbolNameBuilder {
    fn visit_path(&mut self, path: &syn::Path) {
        if path.segments.is_empty() {
            self.push_atom("Self");
            return;
        }

        for seg in &path.segments {
            let ident = seg.ident.to_string();
            let atom = self.generic_params.get(&ident).cloned().unwrap_or(ident);
            self.push_atom(&atom);
            self.visit_path_arguments(&seg.arguments);
        }
    }

    fn visit_path_arguments(&mut self, arguments: &syn::PathArguments) {
        if let syn::PathArguments::AngleBracketed(args) = arguments {
            for arg in &args.args {
                self.visit_generic_argument(arg);
            }
        }
    }

    fn visit_type_path(&mut self, type_path: &syn::TypePath) {
        self.visit_path(&type_path.path);
    }

    fn visit_type_reference(&mut self, reference: &syn::TypeReference) {
        self.push_atom(if reference.mutability.is_some() {
            "ref_mut"
        } else {
            "ref"
        });
        self.visit_type(&reference.elem);
    }

    fn visit_type_slice(&mut self, slice: &syn::TypeSlice) {
        self.push_atom("slice");
        self.visit_type(&slice.elem);
    }

    fn visit_type_array(&mut self, array: &syn::TypeArray) {
        self.push_atom("array");
        self.visit_type(&array.elem);
        self.push_atom(&array.len.to_token_stream().to_string());
    }

    fn visit_type_ptr(&mut self, ptr: &syn::TypePtr) {
        self.push_atom(
            if matches!(ptr.mutability, syn::PointerMutability::Mut(_)) {
                "mut_ptr"
            } else {
                "const_ptr"
            },
        );
        self.visit_type(&ptr.elem);
    }

    fn visit_type_tuple(&mut self, tuple: &syn::TypeTuple) {
        if tuple.elems.is_empty() {
            self.push_atom("unit");
        } else {
            self.push_atom("tuple");
            for elem in &tuple.elems {
                self.visit_type(elem);
            }
        }
    }

    fn visit_type_param_bound(&mut self, bound: &syn::TypeParamBound) {
        match bound {
            syn::TypeParamBound::Lifetime(_) => {}
            syn::TypeParamBound::Trait(trait_bound) => self.visit_path(&trait_bound.path),
            other => self.push_atom(&other.to_token_stream().to_string()),
        }
    }

    fn visit_expr(&mut self, expr: &syn::Expr) {
        self.push_atom(&expr.to_token_stream().to_string());
    }
}

fn sanitize_symbol_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_is_us = false;

    for ch in input.chars() {
        let keep = ch.is_ascii_alphanumeric() || ch == '_';
        if keep {
            out.push(ch);
            prev_is_us = ch == '_';
        } else if !prev_is_us {
            out.push('_');
            prev_is_us = true;
        }
    }

    let out = out.trim_matches('_');
    if out.is_empty() {
        String::from("ty")
    } else {
        out.to_string()
    }
}

#[cfg(test)]
mod symbol_name_tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn path_symbol_name_preserves_namespace_segments() {
        let generics = syn::Generics::default();

        assert_eq!(path_symbol_name(&parse_quote!(a::Ops), &generics), "a_Ops");
        assert_eq!(path_symbol_name(&parse_quote!(b::Ops), &generics), "b_Ops");
    }

    #[test]
    fn type_symbol_name_preserves_namespace_segments_and_arguments() {
        let generics: syn::Generics = parse_quote!(<T>);
        let ty: Type = parse_quote!(outer::Value<inner::Wrapper<T>>);

        assert_eq!(
            type_symbol_name(&ty, &generics),
            "outer_Value_inner_Wrapper_T0"
        );
    }
}
