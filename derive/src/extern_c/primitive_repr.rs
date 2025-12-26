use darling::util::SpannedValue;
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Ident, parse_quote};

use crate::{attr_parse::repr::ReprPrimitive, extern_c::FfiTypeVariant};

/// Derives FFI type for fieldless enums with primitive repr.
pub(crate) fn derive_fieldless_enum(
    repr: ReprPrimitive,
    enum_name: &Ident,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> TokenStream {
    let tag_type = parse_quote!(#repr);

    // FIXME: I think this doesn't actually require variant names, just using a range would suffice
    // (note that we don't support custom discriminants)
    let (discriminants, discriminant_decls) = gen_discriminants(enum_name, variants, &tag_type);

    let len = variants.len();
    let match_ = if discriminants.is_empty() {
        quote! {false}
    } else {
        quote! {
            match *target {
                #( | #discriminants )* => true,
                _ => false,
            }
        }
    };

    quote! {
        co3::mineral! {
            unsafe impl Transparent for #enum_name {
                type Target = #tag_type;

                const NICHE_VALUE: <Self as co3::ExternC>::CType = #len as <Self as co3::ExternC>::CType;
                fn is_valid(target: &Self::Target) -> bool {
                    #(#discriminant_decls)*

                    #match_
                }
            }
        }

        // TODO: Only applicable if number of variants fills out entire discriminant domain space
        //unsafe impl co3::ReprC for #enum_name {}
    }
}

fn gen_discriminants(
    enum_name: &Ident,
    variants: &[SpannedValue<FfiTypeVariant>],
    tag_type: &syn::Type,
) -> (Vec<Ident>, Vec<TokenStream>) {
    let variant_names = variants.iter().map(|v| &v.ident);
    let discriminant_values = variant_discriminants(variants);

    variant_names.zip(discriminant_values.iter()).fold(
        Default::default(),
        |mut acc, (variant_name, discriminant_value)| {
            let discriminant_name = Ident::new(
                &format!("{enum_name}__{variant_name}").to_uppercase(),
                Span::call_site(),
            );

            acc.1.push(quote! {
                const #discriminant_name: #tag_type = #discriminant_value;
            });
            acc.0.push(discriminant_name);

            acc
        },
    )
}

fn variant_discriminants(variants: &[SpannedValue<FfiTypeVariant>]) -> Vec<proc_macro2::Literal> {
    variants
        .iter()
        .enumerate()
        .map(|(i, _)| proc_macro2::Literal::usize_unsuffixed(i))
        .collect()
}
