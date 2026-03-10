use proc_macro2::{Literal, TokenStream};
use quote::{format_ident, quote};
use syn::visit::Visit;

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
