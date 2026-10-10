use quote::quote;
use syn::{DeriveInput, parse_quote};

use super::ReprCAttrs;

pub(super) fn derive_custom_repr_c(
    input: &DeriveInput,
    attrs: &ReprCAttrs,
) -> proc_macro2::TokenStream {
    let name = &input.ident;
    let intermediate = attrs.as_type.as_ref().expect("validated intermediate type");
    let (_, ty_generics, _) = input.generics.split_for_impl();

    let mut repr_generics = input.generics.clone();
    repr_generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#intermediate: co3::ReprC));
    repr_generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#name #ty_generics: co3::rust_spec::RustSpec));
    // DropKind addition keeps CustomDrop, but collapses NoDrop and AutoDrop to AutoDrop.
    repr_generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(
            <#name #ty_generics as co3::rust_spec::RustSpec>::Drop:
                core::ops::Add<co3::rust_spec::drop::AutoDrop,
                    Output = co3::rust_spec::drop::AutoDrop>
        ));
    let (repr_impl_generics, _, repr_where_clause) = repr_generics.split_for_impl();

    let mut encode_generics = input.generics.clone();
    encode_generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#intermediate: co3::stored::EncodeOwned));
    encode_generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#name #ty_generics: core::convert::Into<#intermediate>));
    encode_generics.make_where_clause().predicates.push(
        parse_quote!(#name #ty_generics: co3::ReprC<CType = <#intermediate as co3::ReprC>::CType>),
    );
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
    decode_generics.make_where_clause().predicates.push(
        parse_quote!(#name #ty_generics: co3::ReprC<CType = <#intermediate as co3::ReprC>::CType>),
    );
    let (decode_impl_generics, _, decode_where_clause) = decode_generics.split_for_impl();

    quote! {
        impl #repr_impl_generics co3::ReprC for #name #ty_generics #repr_where_clause {
            type CType = <#intermediate as co3::ReprC>::CType;
        }

        unsafe impl #encode_impl_generics co3::stored::EncodeOwned
            for #name #ty_generics #encode_where_clause
        {
            type Store = <#intermediate as co3::stored::EncodeOwned>::Store;

            fn soft_encode<'_išč>(self, store: &'_išč mut Self::Store) -> Self::CType
            where
                Self: '_išč,
            {
                let value: #intermediate = core::convert::Into::into(self);
                co3::stored::EncodeOwned::soft_encode(value, store)
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
                let value: #intermediate = unsafe {
                    co3::stored::DecodeOwned::soft_decode(source, store)?
                };
                let value: Self = core::convert::TryInto::try_into(value).ok()?;
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
