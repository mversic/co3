use proc_macro2::TokenStream;
use quote::quote;

use crate::repr::repr_c::{assert_no_drop, custom_is_valid};

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
        Some(crate::attr::repr::ReprKind::Transparent)
    );

    let params = &input.generics.params;
    let (_, ty_generics, where_clause) = input.generics.split_for_impl();
    let predicates = where_clause.map(|w| &w.predicates);

    let name = &input.ident;
    let target = match &input.data {
        // TODO: We don't check to find which struct/enum field is not a ZST. It is just assumed that it is the first field.
        // I think something can be done inside `co3::reprC!` through the use of disjoint_impls! or via macro attribute
        darling::ast::Data::Struct(item) => item.fields.first().map(|first_field| &first_field.ty),
        darling::ast::Data::Enum(variants) => variants.first().and_then(|variant| {
            variant
                .fields
                .fields
                .first()
                .map(|first_field| &first_field.ty)
        }),
    };

    let Some(target) = target else {
        return quote! {};
    };

    let custom_validation = if let Some(FfiTypeKindAttribute::Transparent(niche_value, _)) =
        &input.ffi_type_attr.kind
    {
        let niche_value = niche_value.as_ref().map(|value| {
            quote! { const NICHE_VALUE: <Self as co3::ExternC>::CType = #value; }
        });
        let is_valid = custom_is_valid(input.ffi_type_attr.kind.as_ref()).unwrap();

        quote! {
            #niche_value

            fn is_valid(target: &Self::Target) -> bool {
                #is_valid
            }
        }
    } else {
        quote!()
    };

    let impl_drop_assert = assert_no_drop(&input.generics, name);
    let (trait_, impl_drop_assert) = if input.data.is_enum() {
        (quote!(NoDropSizedTransmuted), quote! {})
    } else {
        (quote!(Transmuted), impl_drop_assert)
    };

    quote! {
        #impl_drop_assert

        co3::reprC! {
            // SAFETY: `Self` and `Self::Target` are guaranteed to be transmutable, but the user
            // must make sure the provided validation function does not return false positives
            unsafe impl(#params) #trait_ for #name #ty_generics where (#predicates) {
                type Target = #target;

                #custom_validation
            }
        }

        // FIXME: This is the poorest default implementation
        // Transparent types should delegate to the inner type
        impl<#params> co3::borrow::Borrow for #name #ty_generics #where_clause
        where
            Self: Sized,
        {
            type Borrowed<'itm>
                = &'itm Self
            where
                Self: 'itm;

            type Owner = Option<Self>;

            #[inline(always)]
            fn borrow<'itm>(self, store: &'itm mut Self::Owner) -> Self::Borrowed<'itm>
            where
                Self: 'itm,
            {
                store.insert(self)
            }
        }

        impl<'itm, #params> co3::borrow::ToOwned<'itm> for #name #ty_generics
        where
            Self: Clone,
            #predicates
        {
            #[inline(always)]
            fn to_owned(source: Self::Borrowed<'itm>) -> Self {
                (*source).clone()
            }
        }
    }
}
