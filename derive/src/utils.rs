use proc_macro2::{Literal, TokenStream};
use quote::{ToTokens, format_ident, quote};
use syn::{Attribute, Type, TypePath, parse_quote, visit::Visit, visit_mut::VisitMut};

const MAX_TUPLE_ARITY: usize = 12;

enum ImplTraitNormalization {
    CollectVec,
    Into,
    AsRef,
    AsMut,
    Borrow,
    BorrowMut,
    ToOwned,
}

pub(crate) struct ImplTraitResolution {
    normalization: ImplTraitNormalization,
    target: Type,
}

pub(crate) struct TypeImplTraitResolver;

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

pub(crate) fn gen_store_name(arg_name: &syn::Ident) -> syn::Ident {
    format_ident!("__co3_{arg_name}_store")
}

pub fn gen_normalization_stmts(arg_name: &syn::Ident, arg_ty: &Type) -> TokenStream {
    struct NormalizationVisitor {
        input: TokenStream,
        output: TokenStream,
        found_impl_trait: bool,
    }

    impl NormalizationVisitor {
        fn new() -> Self {
            Self {
                input: quote! {},
                output: quote! {},
                found_impl_trait: false,
            }
        }

        fn emit_current_expr(&mut self) {
            let expr = &self.input;
            self.output.extend(quote!(#expr));
        }

        fn render_nested(&mut self, ty: &Type, expr: TokenStream) -> Option<TokenStream> {
            let prev_found = core::mem::replace(&mut self.found_impl_trait, false);
            let prev_input = core::mem::replace(&mut self.input, expr);
            let prev_output = core::mem::take(&mut self.output);

            self.visit_type(ty);

            self.input = prev_input;
            let found_impl_trait = self.found_impl_trait;
            self.found_impl_trait |= prev_found;
            let output = core::mem::replace(&mut self.output, prev_output);

            found_impl_trait.then_some(output)
        }

        fn normalize_impl_trait_expr(
            &self,
            normalization: &ImplTraitNormalization,
            expr: &TokenStream,
        ) -> TokenStream {
            match normalization {
                ImplTraitNormalization::CollectVec => {
                    quote!(core::iter::IntoIterator::into_iter(#expr).collect())
                }
                ImplTraitNormalization::Into => {
                    quote!(core::convert::Into::into(#expr))
                }
                ImplTraitNormalization::AsRef => {
                    quote!(core::convert::AsRef::as_ref(&#expr))
                }
                ImplTraitNormalization::AsMut => {
                    quote!({
                        let mut __co3_norm = #expr;
                        core::convert::AsMut::as_mut(&mut __co3_norm)
                    })
                }
                ImplTraitNormalization::Borrow => {
                    quote!(std::borrow::Borrow::borrow(&#expr))
                }
                ImplTraitNormalization::BorrowMut => {
                    quote!({
                        let mut __co3_norm = #expr;
                        std::borrow::BorrowMut::borrow_mut(&mut __co3_norm)
                    })
                }
                ImplTraitNormalization::ToOwned => {
                    quote!(std::borrow::ToOwned::to_owned(&#expr))
                }
            }
        }

        fn first_type_arg(args: &syn::PathArguments) -> Option<&Type> {
            let syn::PathArguments::AngleBracketed(args) = args else {
                return None;
            };

            args.args.iter().find_map(|arg| match arg {
                syn::GenericArgument::Type(ty) => Some(ty),
                _ => None,
            })
        }

        fn two_type_args(args: &syn::PathArguments) -> Option<(&Type, &Type)> {
            let syn::PathArguments::AngleBracketed(args) = args else {
                return None;
            };

            let mut types = args.args.iter().filter_map(|arg| match arg {
                syn::GenericArgument::Type(ty) => Some(ty),
                _ => None,
            });

            let first = types.next()?;
            let second = types.next()?;
            if types.next().is_some() {
                return None;
            }
            Some((first, second))
        }
    }

    impl<'ast> Visit<'ast> for NormalizationVisitor {
        fn visit_type_group(&mut self, node: &'ast syn::TypeGroup) {
            self.visit_type(&node.elem);
        }

        fn visit_type_paren(&mut self, node: &'ast syn::TypeParen) {
            self.visit_type(&node.elem);
        }

        fn visit_type_impl_trait(&mut self, node: &'ast syn::TypeImplTrait) {
            let Some(resolution) = resolve_impl_trait(node) else {
                self.emit_current_expr();
                return;
            };

            self.found_impl_trait = true;
            let expr = core::mem::take(&mut self.input);
            self.output
                .extend(self.normalize_impl_trait_expr(&resolution.normalization, &expr));
            self.input = expr;
        }

        fn visit_type_tuple(&mut self, node: &'ast syn::TypeTuple) {
            let expr = core::mem::take(&mut self.input);

            let child_exprs = node.elems.iter().enumerate().map(|(idx, _)| {
                let idx = Literal::usize_unsuffixed(idx);
                quote!(#expr.#idx)
            });

            let elems = node
                .elems
                .iter()
                .zip(child_exprs)
                .map(|(ty, child_expr)| {
                    self.render_nested(ty, child_expr.clone())
                        .unwrap_or(child_expr)
                })
                .collect::<Vec<_>>();

            if self.found_impl_trait {
                self.output.extend(quote!((#(#elems,)*)));
            }

            self.input = expr;
        }

        fn visit_type_array(&mut self, node: &'ast syn::TypeArray) {
            let expr = core::mem::take(&mut self.input);

            if let Some(elem_expr) = self.render_nested(&node.elem, quote!(__co3_elem)) {
                self.output
                    .extend(quote!(#expr.map(|__co3_elem| #elem_expr)));
            }

            self.input = expr;
        }

        fn visit_type_slice(&mut self, node: &'ast syn::TypeSlice) {
            let expr = core::mem::take(&mut self.input);

            if let Some(elem_expr) = self.render_nested(&node.elem, quote!(__co3_elem)) {
                self.output.extend(quote! {
                   #expr.into_iter().map(|__co3_elem| #elem_expr).collect::<Vec<_>>()
                });
            }

            self.input = expr;
        }

        fn visit_type_path(&mut self, node: &'ast TypePath) {
            let segment = node.path.segments.last().unwrap();

            if node.qself.is_some() {
                self.emit_current_expr();
                return;
            }

            let expr = core::mem::take(&mut self.input);
            match segment.ident.to_string().as_str() {
                "Option" => {
                    let Some(inner) = Self::first_type_arg(&segment.arguments) else {
                        self.input = expr;
                        self.emit_current_expr();
                        return;
                    };
                    if let Some(inner_expr) = self.render_nested(inner, quote!(__co3_elem)) {
                        self.output
                            .extend(quote!(#expr.map(|__co3_elem| #inner_expr)));
                    }
                }
                "Vec" => {
                    let Some(inner) = Self::first_type_arg(&segment.arguments) else {
                        self.input = expr;
                        self.emit_current_expr();
                        return;
                    };
                    if let Some(inner_expr) = self.render_nested(inner, quote!(__co3_elem)) {
                        self.output.extend(
                            quote!(#expr.into_iter().map(|__co3_elem| #inner_expr).collect::<Vec<_>>()),
                        );
                    }
                }
                "Box" => {
                    let Some(inner) = Self::first_type_arg(&segment.arguments) else {
                        self.input = expr;
                        self.emit_current_expr();
                        return;
                    };
                    if let Some(inner_expr) = self.render_nested(inner, quote!(*#expr)) {
                        self.output.extend(quote!(Box::new(#inner_expr)));
                    }
                }
                "Result" => {
                    let Some((ok_ty, err_ty)) = Self::two_type_args(&segment.arguments) else {
                        self.input = expr;
                        self.emit_current_expr();
                        return;
                    };
                    let ok_expr = self.render_nested(ok_ty, quote!(__co3_ok));
                    let err_expr = self.render_nested(err_ty, quote!(__co3_err));
                    if ok_expr.is_some() || err_expr.is_some() {
                        let ok_expr = ok_expr.unwrap_or(quote!(__co3_ok));
                        let err_expr = err_expr.unwrap_or(quote!(__co3_err));
                        self.output.extend(
                            quote!(#expr.map(|__co3_ok| #ok_expr).map_err(|__co3_err| #err_expr)),
                        );
                    }
                }
                _ => self.emit_current_expr(),
            }
            self.input = expr;
        }

        fn visit_type_bare_fn(&mut self, _node: &'ast syn::TypeBareFn) {
            self.emit_current_expr();
        }

        fn visit_type_infer(&mut self, _node: &'ast syn::TypeInfer) {
            self.emit_current_expr();
        }

        fn visit_type_macro(&mut self, _node: &'ast syn::TypeMacro) {
            self.emit_current_expr();
        }

        fn visit_type_never(&mut self, _node: &'ast syn::TypeNever) {
            self.emit_current_expr();
        }

        fn visit_type_ptr(&mut self, _node: &'ast syn::TypePtr) {
            self.emit_current_expr();
        }

        fn visit_type_trait_object(&mut self, _node: &'ast syn::TypeTraitObject) {
            self.emit_current_expr();
        }
    }

    let mut visitor = NormalizationVisitor::new();
    visitor.input = quote!(#arg_name);
    visitor.visit_type(arg_ty);

    if visitor.found_impl_trait {
        let expr = visitor.output;
        quote! { let #arg_name = #expr; }
    } else {
        quote! {}
    }
}

pub fn unwrap_result_type(node: &Type) -> Option<(&Type, &Type)> {
    let Type::Path(type_) = node else {
        return None;
    };
    if type_.qself.is_some() {
        return None;
    }

    let segments = type_
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>();

    let is_result_path = segments == ["Result"]
        || segments == ["core", "result", "Result"]
        || segments == ["std", "result", "Result"];

    if !is_result_path {
        return None;
    }

    let last_seg = type_.path.segments.last()?;
    let syn::PathArguments::AngleBracketed(args) = &last_seg.arguments else {
        return None;
    };

    let mut type_args = args.args.iter().filter_map(|arg| match arg {
        syn::GenericArgument::Type(ty) => Some(ty),
        _ => None,
    });
    let ok = type_args.next()?;
    let err = type_args.next()?;

    if type_args.next().is_some() {
        return None;
    }

    Some((ok, err))
}

fn calculate_tuple_depth(n: usize) -> usize {
    if n == 0 {
        return 1;
    }
    let mut depth = 1;
    let mut capacity = MAX_TUPLE_ARITY;
    while capacity < n {
        depth += 1;
        capacity *= MAX_TUPLE_ARITY;
    }
    depth
}

pub fn build_type_tuple(types: &[&Type]) -> (TokenStream, TokenStream, Vec<TokenStream>) {
    let depth = calculate_tuple_depth(types.len());
    build_type_tuple_at_depth(types, depth, true)
}

pub fn build_extern_c_type_tuple(types: &[&Type]) -> (TokenStream, TokenStream, Vec<TokenStream>) {
    let depth = calculate_tuple_depth(types.len());
    build_type_tuple_at_depth(types, depth, false)
}

fn build_type_tuple_at_depth(
    types: &[&Type],
    depth: usize,
    use_flat_transmute: bool,
) -> (TokenStream, TokenStream, Vec<TokenStream>) {
    if depth == 1 {
        let c_types = types.iter().map(|ty| {
            if use_flat_transmute {
                quote!(<#ty as co3::transmute::FlatTransmute>::Target)
            } else {
                quote!(<#ty as co3::ExternC>::CType)
            }
        });
        let accessors = (0..types.len())
            .map(|i| {
                let lit = Literal::usize_unsuffixed(i);
                quote!(#lit)
            })
            .collect();

        let c_tuple_ident = format_ident!("CTuple{}", types.len());
        return (
            quote!((#(#types,)*)),
            quote!(co3::tuple::#c_tuple_ident<#(#c_types),*>),
            accessors,
        );
    }

    let chunk_size = MAX_TUPLE_ARITY.pow(depth as u32 - 1);
    let mut sub_tuples = Vec::new();
    let mut sub_c_tuples = Vec::new();
    let mut all_accessors = Vec::new();

    for (chunk_idx, chunk) in types.chunks(chunk_size).enumerate() {
        let (sub_tuple, sub_c_tuple, sub_accessors) =
            build_type_tuple_at_depth(chunk, depth - 1, use_flat_transmute);
        sub_tuples.push(sub_tuple);
        sub_c_tuples.push(sub_c_tuple);

        let chunk_idx_lit = Literal::usize_unsuffixed(chunk_idx);
        for accessor in sub_accessors {
            all_accessors.push(quote!(#chunk_idx_lit.#accessor));
        }
    }

    let c_tuple_ident = format_ident!("CTuple{}", sub_c_tuples.len());

    (
        quote!((#(#sub_tuples,)*)),
        quote!(co3::tuple::#c_tuple_ident<#(#sub_c_tuples),*>),
        all_accessors,
    )
}

fn path_matches(path: &syn::Path, expected: &[&str]) -> bool {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .eq(expected.iter().copied())
}

fn impl_trait_type_arg(args: &syn::PathArguments) -> Option<Type> {
    let syn::PathArguments::AngleBracketed(args) = args else {
        return None;
    };

    args.args.iter().find_map(|arg| match arg {
        syn::GenericArgument::Type(ty) => Some(ty.clone()),
        _ => None,
    })
}

fn impl_trait_assoc_type_arg(args: &syn::PathArguments, name: &str) -> Option<Type> {
    let syn::PathArguments::AngleBracketed(args) = args else {
        return None;
    };

    args.args.iter().find_map(|arg| match arg {
        syn::GenericArgument::AssocType(binding) if binding.ident == name => {
            Some(binding.ty.clone())
        }
        _ => None,
    })
}

pub(crate) fn resolve_impl_trait(impl_trait: &syn::TypeImplTrait) -> Option<ImplTraitResolution> {
    let mut resolution = None;

    for bound in &impl_trait.bounds {
        let syn::TypeParamBound::Trait(trait_) = bound else {
            continue;
        };

        let path = &trait_.path;
        let Some(segment) = path.segments.last() else {
            continue;
        };

        let resolved = if path.is_ident("IntoIterator")
            || path_matches(path, &["core", "iter", "IntoIterator"])
            || path_matches(path, &["std", "iter", "IntoIterator"])
            || path.is_ident("Iterator")
            || path_matches(path, &["core", "iter", "Iterator"])
            || path_matches(path, &["std", "iter", "Iterator"])
            || path.is_ident("ExactSizeIterator")
            || path_matches(path, &["core", "iter", "ExactSizeIterator"])
            || path_matches(path, &["std", "iter", "ExactSizeIterator"])
        {
            impl_trait_assoc_type_arg(&segment.arguments, "Item").map(|mut ty| {
                TypeImplTraitResolver.visit_type_mut(&mut ty);

                ImplTraitResolution {
                    normalization: ImplTraitNormalization::CollectVec,
                    target: parse_quote!(Vec<#ty>),
                }
            })
        } else if path.is_ident("Into")
            || path_matches(path, &["core", "convert", "Into"])
            || path_matches(path, &["std", "convert", "Into"])
        {
            impl_trait_type_arg(&segment.arguments).map(|mut ty| {
                TypeImplTraitResolver.visit_type_mut(&mut ty);

                ImplTraitResolution {
                    normalization: ImplTraitNormalization::Into,
                    target: ty,
                }
            })
        } else if path.is_ident("AsRef")
            || path_matches(path, &["core", "convert", "AsRef"])
            || path_matches(path, &["std", "convert", "AsRef"])
        {
            impl_trait_type_arg(&segment.arguments).map(|ty| ImplTraitResolution {
                normalization: ImplTraitNormalization::AsRef,
                target: parse_quote!(&#ty),
            })
        } else if path.is_ident("AsMut")
            || path_matches(path, &["core", "convert", "AsMut"])
            || path_matches(path, &["std", "convert", "AsMut"])
        {
            impl_trait_type_arg(&segment.arguments).map(|ty| ImplTraitResolution {
                normalization: ImplTraitNormalization::AsMut,
                target: parse_quote!(&mut #ty),
            })
        } else if path.is_ident("Borrow")
            || path_matches(path, &["core", "borrow", "Borrow"])
            || path_matches(path, &["std", "borrow", "Borrow"])
        {
            impl_trait_type_arg(&segment.arguments).map(|ty| ImplTraitResolution {
                normalization: ImplTraitNormalization::Borrow,
                target: parse_quote!(&#ty),
            })
        } else if path.is_ident("BorrowMut")
            || path_matches(path, &["core", "borrow", "BorrowMut"])
            || path_matches(path, &["std", "borrow", "BorrowMut"])
        {
            impl_trait_type_arg(&segment.arguments).map(|ty| ImplTraitResolution {
                normalization: ImplTraitNormalization::BorrowMut,
                target: parse_quote!(&mut #ty),
            })
        } else if path.is_ident("ToOwned")
            || path_matches(path, &["alloc", "borrow", "ToOwned"])
            || path_matches(path, &["std", "borrow", "ToOwned"])
        {
            impl_trait_assoc_type_arg(&segment.arguments, "Owned").map(|mut ty| {
                TypeImplTraitResolver.visit_type_mut(&mut ty);

                ImplTraitResolution {
                    normalization: ImplTraitNormalization::ToOwned,
                    target: ty,
                }
            })
        } else {
            None
        };

        if let Some(resolved) = resolved {
            resolution = Some(resolved);
        }
    }

    resolution
}

pub(crate) struct DispatchMonomorphizer<'a> {
    subst: std::collections::BTreeMap<&'a syn::Ident, &'a syn::GenericArgument>,
}

impl<'a> DispatchMonomorphizer<'a> {
    pub(crate) fn new(
        generics: &'a syn::Generics,
        entry: &'a syn::AngleBracketedGenericArguments,
    ) -> Self {
        let subst = generics
            .params
            .iter()
            .filter(|param| !matches!(param, syn::GenericParam::Lifetime(_)))
            .zip(&entry.args)
            .filter_map(|(param, arg)| match param {
                syn::GenericParam::Type(param) => Some((&param.ident, arg)),
                syn::GenericParam::Const(param) => Some((&param.ident, arg)),
                syn::GenericParam::Lifetime(_) => None,
            })
            .collect();

        Self { subst }
    }
}

impl VisitMut for DispatchMonomorphizer<'_> {
    fn visit_path_mut(&mut self, node: &mut syn::Path) {
        syn::visit_mut::visit_path_mut(self, node);

        let Some(first) = node.segments.first() else {
            return;
        };
        let Some(syn::GenericArgument::Type(Type::Path(TypePath {
            qself: None,
            path: replacement,
        }))) = self.subst.get(&first.ident)
        else {
            return;
        };

        if node.segments.len() == 1 {
            *node = replacement.clone();
        }
    }

    fn visit_type_mut(&mut self, node: &mut Type) {
        syn::visit_mut::visit_type_mut(self, node);

        if let Type::Path(TypePath { qself: None, path }) = node
            && let Some(first) = path.segments.first()
            && let Some(replacement) = self.subst.get(&first.ident)
        {
            if path.segments.len() == 1 {
                *node = parse_quote!(#replacement);
                return;
            }

            let mut rest = syn::Path {
                leading_colon: None,
                segments: Default::default(),
            };

            for segment in path.segments.iter().skip(1) {
                rest.segments.push(segment.clone());
            }

            *node = parse_quote!(<#replacement>::#rest);
        }
    }

    fn visit_expr_mut(&mut self, node: &mut syn::Expr) {
        syn::visit_mut::visit_expr_mut(self, node);

        if let syn::Expr::Path(syn::ExprPath { path, .. }) = node
            && let Some(ident) = path.get_ident()
            && let Some(syn::GenericArgument::Const(replacement)) = self.subst.get(ident)
        {
            *node = replacement.clone();
        }
    }
}

pub(crate) fn is_drop_impl(impl_: &syn::ItemImpl) -> bool {
    impl_
        .trait_
        .as_ref()
        .is_some_and(|(_, path, _)| path.segments.last().is_some_and(|seg| seg.ident == "Drop"))
}

impl VisitMut for TypeImplTraitResolver {
    fn visit_type_mut(&mut self, node: &mut Type) {
        if let Type::ImplTrait(impl_trait) = node
            && let Some(resolution) = resolve_impl_trait(impl_trait)
        {
            *node = resolution.target;
        }
    }
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
    generic_params: std::collections::BTreeMap<String, String>,
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
        let Some(seg) = path.segments.last() else {
            self.push_atom("Self");
            return;
        };

        let ident = seg.ident.to_string();
        let atom = self.generic_params.get(&ident).cloned().unwrap_or(ident);
        self.push_atom(&atom);
        self.visit_path_arguments(&seg.arguments);
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
        self.push_atom(if ptr.mutability.is_some() {
            "mut_ptr"
        } else {
            "const_ptr"
        });
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
