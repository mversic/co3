use std::collections::BTreeSet;

use proc_macro2::{Literal, TokenStream};
use quote::{format_ident, quote};
use syn::{
    GenericParam, Generics, Lifetime, LifetimeParam, Type, WherePredicate, visit::Visit,
    visit_mut::VisitMut,
};

use crate::Arg;

const MAX_TUPLE_ARITY: usize = 12;

struct FfiTypeResolver<'itm>(&'itm syn::Ident, TokenStream);

impl<'itm> Visit<'itm> for FfiTypeResolver<'itm> {
    fn visit_trait_bound(&mut self, i: &'itm syn::TraitBound) {
        let trait_ = i.path.segments.last().expect("Defined");

        let arg_name = self.0;
        if trait_.ident == "IntoIterator" || trait_.ident == "ExactSizeIterator" {
            self.1 = quote! { let #arg_name: Vec<_> = #arg_name.into_iter().collect(); };
        } else if trait_.ident == "Into" {
            self.1 = quote! { let #arg_name = #arg_name.into(); };
        } else if trait_.ident == "AsRef" {
            self.1 = quote! { let #arg_name = #arg_name.as_ref(); };
        }
    }
}

pub fn gen_store_name(arg_name: &syn::Ident) -> syn::Ident {
    syn::Ident::new(&format!("{arg_name}_store"), proc_macro2::Span::call_site())
}

pub fn gen_resolve_type(arg: &Arg) -> TokenStream {
    let (arg_name, src_type) = (arg.name(), arg.src_type());

    if unwrap_result_type(src_type).is_some() {
        return quote! {
            let #arg_name = if let Ok(ok) = #arg_name {
                ok
            } else {
                // TODO: Implement error handling (https://github.com/hyperledger/iroha/issues/2252)
                return Err(co3::FfiReturn::ExecutionFail);
            };
        };
    }

    let mut type_resolver = FfiTypeResolver(arg_name, quote! {});
    type_resolver.visit_type(src_type);
    type_resolver.1
}

pub fn unwrap_result_type(node: &syn::Type) -> Option<(&syn::Type, &syn::Type)> {
    if let syn::Type::Path(type_) = node {
        let last_seg = type_.path.segments.last().expect("Defined");

        if last_seg.ident == "Result"
            && let syn::PathArguments::AngleBracketed(args) = &last_seg.arguments
            && let (syn::GenericArgument::Type(ok), syn::GenericArgument::Type(err)) =
                (&args.args[0], &args.args[1])
        {
            return Some((ok, err));
        }
    }

    None
}

#[derive(Default)]
pub struct SignatureLifetimeBuilder {
    used_names: BTreeSet<String>,
    extra_lifetimes: Vec<LifetimeParam>,
    extra_where_predicates: Vec<WherePredicate>,
    counter: usize,
}

impl SignatureLifetimeBuilder {
    pub fn new(generics: &[&Generics]) -> Self {
        let mut out = Self::default();
        for generics in generics {
            for param in &generics.params {
                if let GenericParam::Lifetime(param) = param {
                    out.used_names.insert(param.lifetime.ident.to_string());
                }
            }
        }
        out
    }

    pub fn borrowed_src_type(&mut self, mut ty: Type) -> TokenStream {
        let seen = self.name_lifetimes_in_type(&mut ty);
        let borrow_lifetime = if seen.len() == 1 {
            seen.into_iter().next().expect("checked len")
        } else {
            let lifetime = self.fresh_lifetime();
            self.extra_where_predicates
                .push(syn::parse_quote!(#ty: #lifetime));
            lifetime
        };

        quote!(<#ty as co3::borrow::Borrow>::Borrowed<#borrow_lifetime>)
    }

    pub fn build_generics(&self, generics: &[&Generics]) -> Generics {
        let mut params = syn::punctuated::Punctuated::new();

        for generics in generics {
            for param in &generics.params {
                if matches!(param, GenericParam::Lifetime(_)) {
                    params.push(param.clone());
                }
            }
        }
        for lifetime in &self.extra_lifetimes {
            params.push(GenericParam::Lifetime(lifetime.clone()));
        }
        for generics in generics {
            for param in &generics.params {
                if !matches!(param, GenericParam::Lifetime(_)) {
                    params.push(param.clone());
                }
            }
        }

        let has_params = !params.is_empty();
        let mut out = Generics {
            lt_token: has_params.then_some(Default::default()),
            params,
            gt_token: has_params.then_some(Default::default()),
            where_clause: None,
        };

        for generics in generics {
            if let Some(where_clause) = &generics.where_clause {
                out.make_where_clause()
                    .predicates
                    .extend(where_clause.predicates.clone());
            }
        }
        if !self.extra_where_predicates.is_empty() {
            out.make_where_clause()
                .predicates
                .extend(self.extra_where_predicates.clone());
        }

        out
    }

    pub fn split_for_signature(
        &self,
        generics: &[&Generics],
    ) -> (TokenStream, Option<syn::WhereClause>) {
        let generics = self.build_generics(generics);
        let (impl_generics, _, where_clause) = generics.split_for_impl();
        (quote!(#impl_generics), where_clause.cloned())
    }

    fn fresh_lifetime(&mut self) -> Lifetime {
        loop {
            let ident = format!("__co3_{}", self.counter);
            self.counter += 1;
            if self.used_names.insert(ident.clone()) {
                let lifetime = Lifetime::new(&format!("'{ident}"), proc_macro2::Span::call_site());
                self.extra_lifetimes
                    .push(LifetimeParam::new(lifetime.clone()));
                return lifetime;
            }
        }
    }

    fn name_lifetimes_in_type(&mut self, ty: &mut Type) -> BTreeSet<Lifetime> {
        struct LifetimeCollector<'a> {
            builder: &'a mut SignatureLifetimeBuilder,
            seen: BTreeSet<Lifetime>,
        }

        impl VisitMut for LifetimeCollector<'_> {
            fn visit_type_reference_mut(&mut self, node: &mut syn::TypeReference) {
                if node.lifetime.is_none() {
                    node.lifetime = Some(self.builder.fresh_lifetime());
                }
                syn::visit_mut::visit_type_reference_mut(self, node);
            }

            fn visit_lifetime_mut(&mut self, lifetime: &mut Lifetime) {
                if lifetime.ident == "_" {
                    *lifetime = self.builder.fresh_lifetime();
                }
                self.seen.insert(lifetime.clone());
            }
        }

        let mut collector = LifetimeCollector {
            builder: self,
            seen: BTreeSet::new(),
        };
        collector.visit_type_mut(ty);
        collector.seen
    }
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

pub fn build_type_tuple(types: &[&syn::Type]) -> (TokenStream, TokenStream, Vec<TokenStream>) {
    let depth = calculate_tuple_depth(types.len());
    build_type_tuple_at_depth(types, depth)
}

fn build_type_tuple_at_depth(
    types: &[&syn::Type],
    depth: usize,
) -> (TokenStream, TokenStream, Vec<TokenStream>) {
    if depth == 1 {
        let c_types = types.iter().map(|ty| quote!(<#ty as co3::ExternC>::CType));
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
        let (sub_tuple, sub_c_tuple, sub_accessors) = build_type_tuple_at_depth(chunk, depth - 1);
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
