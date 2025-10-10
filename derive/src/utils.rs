use proc_macro2::TokenStream;
use quote::quote;
use syn::visit::Visit;

use crate::Arg;

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
