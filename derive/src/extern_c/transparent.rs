use proc_macro2::TokenStream;
use quote::quote;

use super::{FfiTypeInput, FfiTypeKindAttribute};

/// Derives FFI type for transparent items.
///
/// Possible transparent items:
///
/// * fieldless structs
/// * one-variant fieldless enums
pub(crate) fn derive_transparent_item(input: &FfiTypeInput) -> TokenStream {
    debug_assert_eq!(
        input.repr_attr.kind.as_deref().copied(),
        Some(crate::attr_parse::repr::ReprKind::Transparent)
    );

    let (_, ty_generics, _) = input.generics.split_for_impl();
    let params = &input.generics.params;
    let predicates = input
        .generics
        .where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let name = &input.ident;
    let target = match &input.data {
        // TODO: We don't check to find which struct/enum field is not a ZST. It is just assumed that it is the first field.
        // I think something can be done inside `co3::repr_C!` through the use of disjoint_impls! or via macro attribute
        darling::ast::Data::Struct(item) => item.fields.first().map(|first_field| &first_field.ty),
        darling::ast::Data::Enum(variants) => variants.first().and_then(|variant| {
            variant
                .fields
                .fields
                .first()
                .map(|first_field| &first_field.ty)
        }),
    };

    if target.is_none() {
        return quote! {};
    };

    let custom_validation = if let Some(FfiTypeKindAttribute::Transparent(niche_value, is_valid)) =
        &input.ffi_type_attr.kind
    {
        let niche_value = niche_value.as_ref().map(|value| {
            quote! { const NICHE_VALUE: <Self as co3::ExternC>::CType = #value; }
        });

        quote! {
            #niche_value

            fn is_valid(target: &Self::Target) -> bool {
                (#is_valid)(target)
            }
        }
    } else {
        quote!()
    };

    let params = if params.is_empty() {
        quote!()
    } else {
        quote!((#params))
    };

    quote! {
        co3::repr_C! {
            // SAFETY: `Self` and `Self::Target` are guaranteed to be transmutable, but the user
            // must make sure the provided validation function does not return false positives
            unsafe impl #params Transparent for #name #ty_generics where (#predicates) {
                type Target = #target;

                #custom_validation
            }
        }
    }
}
