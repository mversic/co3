//! Reference encoder generation for transparent wrappers and aggregate structs.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, parse_quote};

use super::is_parametrized;
use crate::utils::speculative_marker;

/// Select the reference trait, value trait, store, and their encoding methods.
pub(super) fn encoder_names(
    mutable: bool,
    unchecked: bool,
) -> (
    TokenStream,
    TokenStream,
    TokenStream,
    TokenStream,
    TokenStream,
) {
    let borrow = if mutable { "Mut" } else { "Ref" };
    let suffix = if unchecked { "Unchecked" } else { "" };
    let trait_name = format_ident!("Encode{borrow}{suffix}");
    let move_trait = format_ident!("EncodeMove{suffix}");
    let store = if unchecked {
        quote!(SoftStore)
    } else {
        quote!(Store)
    };
    let borrow = if mutable { "mut" } else { "ref" };
    let suffix = if unchecked { "_unchecked" } else { "" };
    let method = format_ident!("encode_{borrow}{suffix}");
    let move_method = format_ident!("soft_encode{suffix}");
    (
        quote!(co3::stored::#trait_name),
        quote!(co3::stored::#move_trait),
        store,
        quote!(#method),
        quote!(#move_method),
    )
}

fn encoding_call(call: TokenStream, unchecked: bool) -> TokenStream {
    if unchecked {
        quote!(unsafe { #call })
    } else {
        call
    }
}

fn field_member(index: usize, field: &syn::Field) -> syn::Member {
    field
        .ident
        .clone()
        .map(syn::Member::Named)
        .unwrap_or_else(|| syn::Member::Unnamed(syn::Index::from(index)))
}

/// Keep the size marker and dispatch helpers inside an unnameable scope.
pub(super) fn group_aggregate_ref_impls(
    implementations: TokenStream,
    dispatch: bool,
) -> TokenStream {
    let extern_thin = (!dispatch).then(|| {
        quote! {
            impl Thin for co3::rust_spec::size::ExternTypeLike {}
        }
    });
    let thin = quote! {
        trait Thin {}
        impl<K: co3::rust_spec::size::SizedKind> Thin for co3::rust_spec::size::Sized<K> {}
        #extern_thin
    };
    if !dispatch {
        return quote!(const _: () = { #thin #implementations };);
    }
    let file: syn::File = syn::parse2(implementations).expect("reference encoder impls");
    let mut groups: [Vec<TokenStream>; 4] = Default::default();
    for item in file.items {
        let syn::Item::Impl(mut implementation) = item else {
            unreachable!("reference generators emit impls");
        };
        let path = &mut implementation.trait_.as_mut().expect("trait impl").0;
        let segment = path.segments.last().unwrap().clone();
        let index = match segment.ident.to_string().as_str() {
            "EncodeRefUnchecked" => 0,
            "EncodeRef" => 1,
            "EncodeMutUnchecked" => 2,
            "EncodeMut" => 3,
            _ => unreachable!("reference encoder trait"),
        };
        *path = parse_quote!(#segment);
        groups[index].push(quote!(#implementation));
    }
    let blocks = groups.into_iter().enumerate().filter_map(|(index, implementations)| {
        if implementations.is_empty() {
            return None;
        }
        let mutable = index >= 2;
        let unchecked = index % 2 == 0;
        let (trait_path, _, store_name, method, _) = encoder_names(mutable, unchecked);
        let path: syn::Path = syn::parse2(trait_path.clone()).expect("encoder path");
        let name = &path.segments.last().unwrap().ident;
        if implementations.len() == 1 {
            return Some(quote! {
                use #trait_path;
                #(#implementations)*
            });
        }
        let mutability = mutable.then(|| quote!(mut));
        let unsafe_fn = unchecked.then(|| quote!(unsafe));
        let supertrait = match (mutable, unchecked) {
            (_, true) => quote!(co3::ReprC + 'a),
            (false, false) => quote!(co3::stored::EncodeRefUnchecked<'a>),
            (true, false) => quote!(co3::stored::EncodeMutUnchecked<'a>),
        };
        Some(quote! {
            use #trait_path;
            co3::disjoint_impls! {
                #[disjoint_impls(remote)]
                pub unsafe trait #name<'a>: #supertrait {
                    type #store_name: co3::stored::Store + Default;
                    #unsafe_fn fn #method(&'a #mutability self, store: &mut Self::#store_name) -> <&'a #mutability Self as co3::ReprC>::CType
                    where &'a #mutability Self: co3::ReprC<CType: Sized>;
                }
                #(#implementations)*
            }
        })
    });
    quote! {
        const _: () = {
            #thin
            #(#blocks)*
        };
    }
}

/// Transparent wrappers delegate reference encoding to the borrowed inner field.
pub(super) fn gen_field_ref_impls(
    input: &syn::DeriveInput,
    field: &syn::Field,
    variant: Option<&Ident>,
) -> TokenStream {
    let name = &input.ident;
    let field_ty = &field.ty;
    let member = field_member(0, field);
    let mut generics = input.generics.clone();
    generics.params.insert(0, parse_quote!('_ršč));
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let (_, ty_generics, _) = input.generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|clause| &clause.predicates);
    let speculative = speculative_marker(input.generics.params.is_empty());
    let field_speculative = speculative_marker(!is_parametrized(field_ty, &input.generics));

    let mut implementations = Vec::new();
    for mutable in [false, true] {
        for unchecked in [true, false] {
            let (trait_name, move_trait, store_name, method, encode) =
                encoder_names(mutable, unchecked);
            let mutability = mutable.then(|| quote!(mut));
            let unsafe_fn = unchecked.then(|| quote!(unsafe));
            let borrow = match (variant, &field.ident) {
                (None, _) => quote!(&#mutability self.#member),
                (Some(variant), Some(ident)) => {
                    quote!(match self { Self::#variant { #ident: field } => field })
                }
                (Some(variant), None) => quote!(match self { Self::#variant(field) => field }),
            };
            let call = encoding_call(quote!(#move_trait::#encode(#borrow, store)), unchecked);
            implementations.push(quote! {
                unsafe impl #impl_generics #trait_name<'_ršč> for #name #ty_generics
                where
                    #speculative Self: co3::ReprC<CType = <#field_ty as co3::ReprC>::CType> + '_ršč,
                    #field_speculative #field_ty: co3::ReprC + '_ršč,
                    #field_speculative &'_ršč #mutability #field_ty: #move_trait + co3::ReprC<CType: Sized>,
                    #speculative &'_ršč #mutability Self: co3::ReprC<CType = <&'_ršč #mutability #field_ty as co3::ReprC>::CType>,
                    #predicates
                {
                    type #store_name = <&'_ršč #mutability #field_ty as #move_trait>::#store_name;
                    #unsafe_fn fn #method(&'_ršč #mutability self, store: &mut Self::#store_name) -> <&'_ršč #mutability Self as co3::ReprC>::CType {
                        #call
                    }
                }
            });
        }
    }
    quote!(#(#implementations)*)
}
