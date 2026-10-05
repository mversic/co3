use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, Fields, parse_quote};

use super::{ReprCAttrs, VariantReprCAttrs, gen_is_valid_call, item::field_vars};

fn validation_arm(head: TokenStream, fields: &Fields, is_valid: &syn::ExprClosure) -> TokenStream {
    let vars = field_vars(fields);
    let types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();
    let args = vars.iter().map(|var| quote!(#var)).collect::<Vec<_>>();
    let call = gen_is_valid_call(is_valid, &types, &args);
    let pattern = match fields {
        Fields::Named(_) => quote!(#head { #(#vars),* }),
        Fields::Unnamed(_) => quote!(#head(#(#vars),*)),
        Fields::Unit => head,
    };

    quote! {
        #pattern => #call
    }
}

fn gen_custom_validation(
    input: &DeriveInput,
    attrs: &ReprCAttrs,
    variant_attrs: &[VariantReprCAttrs],
) -> TokenStream {
    let arms = match &input.data {
        syn::Data::Struct(data) => attrs
            .is_valid
            .as_ref()
            .map(|is_valid| validation_arm(quote!(Self), &data.fields, is_valid))
            .into_iter()
            .collect::<Vec<_>>(),
        syn::Data::Enum(data) => data
            .variants
            .iter()
            .zip(variant_attrs)
            .filter_map(|(variant, attrs)| {
                attrs.is_valid.as_ref().map(|is_valid| {
                    let name = &variant.ident;
                    validation_arm(quote!(Self::#name), &variant.fields, is_valid)
                })
            })
            .collect::<Vec<_>>(),
        syn::Data::Union(_) => unreachable!(),
    };
    if arms.is_empty() {
        return quote! {};
    }

    let fallback = match &input.data {
        syn::Data::Enum(data) if arms.len() < data.variants.len() => Some(quote!(_ => true,)),
        _ => None,
    };
    quote! {
        if !match &value { #(#arms,)* #fallback } {
            return None;
        }
    }
}

pub(super) fn derive_custom_repr_c(
    input: &DeriveInput,
    attrs: &ReprCAttrs,
    variant_attrs: &[VariantReprCAttrs],
) -> TokenStream {
    let name = &input.ident;
    let intermediate = attrs.as_type.as_ref().expect("validated intermediate type");
    let (_, ty_generics, _) = input.generics.split_for_impl();

    let mut repr_generics = input.generics.clone();
    repr_generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#intermediate: co3::ReprC));
    let (repr_impl_generics, _, repr_where_clause) = repr_generics.split_for_impl();

    let niche_bound: Option<syn::WherePredicate> = attrs
        .niche_value
        .as_ref()
        .map(|_| parse_quote!(<#intermediate as co3::ReprC>::CType: Copy + PartialEq));

    let mut encode_generics = input.generics.clone();
    encode_generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#intermediate: co3::stored::EncodeOwned));
    encode_generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#name #ty_generics: core::convert::Into<#intermediate>));
    encode_generics
        .make_where_clause()
        .predicates
        .extend(niche_bound.iter().cloned());
    let (encode_impl_generics, _, encode_where_clause) = encode_generics.split_for_impl();

    let mut decode_generics = input.generics.clone();
    decode_generics.params.insert(0, parse_quote!('_dšč));
    decode_generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#intermediate: co3::stored::DecodeOwned<'_dšč>));
    decode_generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#intermediate: core::convert::TryInto<#name #ty_generics>));
    decode_generics
        .make_where_clause()
        .predicates
        .extend(niche_bound.iter().cloned());
    let (decode_impl_generics, _, decode_where_clause) = decode_generics.split_for_impl();

    let niche_impl = attrs.niche_value.as_ref().map(|niche_value| {
        let mut niche_generics = repr_generics.clone();
        niche_generics
            .make_where_clause()
            .predicates
            .extend(niche_bound.iter().cloned());
        niche_generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(
                #name #ty_generics: co3::rust_spec::RustSpec<
                    Niche = co3::rust_spec::niche::WithNiche<co3::rust_spec::Unstable>
                >
            ));
        let (impl_generics, _, where_clause) = niche_generics.split_for_impl();
        quote! {
            impl #impl_generics co3::niche::Niche for #name #ty_generics #where_clause {
                const NICHE_VALUE: Self::CType = #niche_value;
            }
        }
    });
    let encode_niche_check = attrs.niche_value.as_ref().map(|niche_value| {
        quote! {
            debug_assert!(encoded != #niche_value, "encoding produced the reserved NICHE_VALUE");
        }
    });
    let decode_niche_check = attrs.niche_value.as_ref().map(|niche_value| {
        quote! {
            if source == #niche_value {
                return None;
            }
        }
    });
    let custom_validation = gen_custom_validation(input, attrs, variant_attrs);

    quote! {
        impl #repr_impl_generics co3::ReprC for #name #ty_generics #repr_where_clause {
            type CType = <#intermediate as co3::ReprC>::CType;
        }

        #niche_impl

        unsafe impl #encode_impl_generics co3::stored::EncodeOwned
            for #name #ty_generics #encode_where_clause
        {
            type Store = <#intermediate as co3::stored::EncodeOwned>::Store;

            fn soft_encode<'_išč>(self, store: &'_išč mut Self::Store) -> Self::CType
            where
                Self: '_išč,
            {
                let value: #intermediate = core::convert::Into::into(self);
                let encoded = co3::stored::EncodeOwned::soft_encode(value, store);
                #encode_niche_check
                encoded
            }
        }

        unsafe impl #decode_impl_generics co3::stored::DecodeOwned<'_dšč>
            for #name #ty_generics #decode_where_clause
        {
            type Store = <#intermediate as co3::stored::DecodeOwned<'_dšč>>::Store;

            unsafe fn soft_decode<'_išč: '_dšč>(
                source: Self::CType,
                store: &'_išč mut Self::Store,
            ) -> Option<Self> {
                #decode_niche_check
                let value: #intermediate = unsafe {
                    co3::stored::DecodeOwned::soft_decode(source, store)?
                };
                let value: Self = core::convert::TryInto::try_into(value).ok()?;
                #custom_validation
                Some(value)
            }

            unsafe fn soft_decode_unchecked<'_išč: '_dšč>(
                source: Self::CType,
                store: &'_išč mut Self::Store,
            ) -> Self {
                let value = unsafe {
                    co3::stored::DecodeOwned::soft_decode_unchecked(source, store)
                };
                unsafe {
                    <#intermediate as core::convert::TryInto<Self>>::try_into(value)
                        .ok()
                        .unwrap_unchecked()
                }
            }
        }

        impl #encode_impl_generics co3::Encode for #name #ty_generics #encode_where_clause {}
        impl #decode_impl_generics co3::Decode<'_dšč>
            for #name #ty_generics #decode_where_clause {}
    }
}
