use core::str::FromStr as _;

use darling::util::SpannedValue;
use proc_macro2::TokenStream;
use quote::quote;

use crate::{
    emitter::Emitter,
    extern_c::{FfiTypeInput, FfiTypeVariant, no_repr::variant_mapper, verify_is_non_owning},
};

pub(crate) fn derive_repr_c_item(emitter: &mut Emitter, input: &FfiTypeInput) -> TokenStream {
    verify_is_non_owning(emitter, &input.data);

    let item_name = &input.ident;
    let (impl_generics, ty_generics, _) = input.generics.split_for_impl();
    let params = &input.generics.params;
    let predicates = input
        .generics
        .where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    match &input.data {
        darling::ast::Data::Struct(fields) => {
            let repr_c_predicates = fields.iter().map(|field| {
                let field_ty = &field.ty;
                quote! { #field_ty: co3::ReprC }
            });

            quote! {
                // TODO: Also generate conversion for robust transparent fields, not just robust fields
                unsafe impl #impl_generics co3::ReprC for #item_name #ty_generics where #(#repr_c_predicates,)* #predicates {}

                co3::mineral! {
                    impl(#params) Robust for #item_name #ty_generics where (Self: co3::ReprC, #predicates) {}
                }
            }
        }
        darling::ast::Data::Enum(variants) => {
            let len = TokenStream::from_str(&format!("{}", variants.len())).expect("Valid");

            let (repr_c_enum_name, repr_c_enum) =
                gen_data_carrying_repr_c_enum(emitter, item_name, &input.generics, variants);

            quote! {
                #repr_c_enum

                co3::mineral! {
                    unsafe impl(#params) Transparent for #item_name #ty_generics where (#predicates) {
                        type Target = #repr_c_enum_name #ty_generics;

                        fn is_valid(target: &Self::Target) -> bool {
                            // TODO: Can it be less than 0?
                            // Depends on the c type used
                            target.tag <= #len
                        }
                    }
                }
            }
        }
    }
}

pub(super) fn gen_data_carrying_repr_c_enum(
    emitter: &mut Emitter,
    enum_name: &syn::Ident,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> (syn::Ident, TokenStream) {
    let (payload_name, payload) =
        gen_data_carrying_enum_payload(emitter, enum_name, generics, variants);

    let doc = format!(" [`ReprC`] equivalent of [`{enum_name}`]");
    let repr_c_enum_name = gen_repr_c_item_name(enum_name);
    // FIXME: What is the correct repr here?
    let tag_type = quote! { core::ffi::c_uint };

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let params = &generics.params;
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let repr_c_enum = quote! {
        #payload

        #[repr(C)]
        #[doc = #doc]
        #[derive(Clone)]
        struct #repr_c_enum_name #impl_generics #where_clause {
            tag: #tag_type, payload: #payload_name #ty_generics,
        }

        impl #impl_generics Copy for #repr_c_enum_name #ty_generics #where_clause {}
        unsafe impl #impl_generics co3::ReprC for #repr_c_enum_name #ty_generics #where_clause {}

        co3::mineral! {
            impl(#params) Robust for #repr_c_enum_name where (#predicates) {}
        }
    };

    (repr_c_enum_name, repr_c_enum)
}

pub(crate) fn gen_data_carrying_enum_payload(
    emitter: &mut Emitter,
    enum_name: &syn::Ident,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> (syn::Ident, TokenStream) {
    let payload_name = gen_repr_c_enum_payload_name(enum_name);
    let repr_c_enum_name = gen_repr_c_item_name(enum_name);

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let field_names = variants.iter().map(|variant| &variant.ident);
    let doc = format!(" Payload of [`{repr_c_enum_name}`]");

    let field_tys = variants.iter().map(|variant| {
        variant_mapper(
            emitter,
            variant,
            || quote! {()},
            |field| {
                let field_ty = &field.ty;
                quote! {<#field_ty as co3::ExternC>::CType}
            },
        )
    });

    let payload = quote! {
        #[repr(C)]
        #[doc = #doc]
        #[derive(Clone)]
        #[expect(non_snake_case)]
        union #payload_name #impl_generics #where_clause {
            #(#field_names: #field_tys),*
        }

        impl #impl_generics Copy for #payload_name #ty_generics #where_clause {}
        unsafe impl #impl_generics co3::ReprC for #payload_name #ty_generics #where_clause {}
    };

    (payload_name, payload)
}

pub(super) fn gen_repr_c_item_name(enum_name: &syn::Ident) -> syn::Ident {
    syn::Ident::new(
        &format!("__co3__ReprC{enum_name}"),
        proc_macro2::Span::call_site(),
    )
}

pub(super) fn gen_repr_c_enum_payload_name(enum_name: &syn::Ident) -> syn::Ident {
    syn::Ident::new(
        &format!("__co3__{enum_name}Payload"),
        proc_macro2::Span::call_site(),
    )
}
