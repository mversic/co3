use darling::util::SpannedValue;
use proc_macro2::TokenStream;
use quote::quote;

use crate::{
    attr_parse::repr::ReprPrimitive,
    extern_c::{
        FfiTypeField, FfiTypeVariant, is_type_parameterized,
        repr_c::{gen_extern_c_bounds, gen_repr_c_item_name, is_exhaustive_enum},
    },
};

fn build_nested_tuple(types: &[&syn::Type]) -> (TokenStream, TokenStream, Vec<TokenStream>) {
    fn build_recursive(
        types: &[&syn::Type],
        prefix: &mut Vec<usize>,
    ) -> (TokenStream, TokenStream, Vec<TokenStream>) {
        let n = types.len();

        if n <= 12 {
            let tuple_type = quote! { (#(#types,)*) };
            let c_tuple_type = quote! { (#(<#types as co3::ExternC>::CType,)*) };

            let accessors = (0..n)
                .map(|i| {
                    let indices = prefix
                        .iter()
                        .copied()
                        .chain(std::iter::once(i))
                        .map(syn::Index::from);

                    quote! { #(#indices).* }
                })
                .collect();

            return (tuple_type, c_tuple_type, accessors);
        }

        let num_positions = n.div_ceil(12).min(12);
        let base_count = n / num_positions;
        let remainder = n % num_positions;

        let mut tuple_elems = Vec::new();
        let mut c_tuple_elems = Vec::new();
        let mut all_accessors = Vec::new();
        let mut offset = 0;

        for i in 0..num_positions {
            let count = if i < remainder {
                base_count + 1
            } else {
                base_count
            };

            let slice = &types[offset..offset + count];
            prefix.push(i);
            let (elem, c_elem, accessors) = build_recursive(slice, prefix);
            prefix.pop();

            tuple_elems.push(elem);
            c_tuple_elems.push(c_elem);
            all_accessors.extend(accessors);

            offset += count;
        }

        (
            quote! { (#(#tuple_elems,)*) },
            quote! { (#(#c_tuple_elems,)*) },
            all_accessors,
        )
    }

    build_recursive(types, &mut Vec::new())
}

pub fn gen_struct_niche_ir(
    struct_name: &syn::Ident,
    generics: &syn::Generics,
    fields: &darling::ast::Fields<FfiTypeField>,
) -> TokenStream {
    let types = fields.iter().map(|f| &f.ty).collect::<Vec<_>>();

    let (impl_generics, ty_generics, _) = generics.split_for_impl();
    let predicates = generics
        .where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let repr_c_struct_name = gen_repr_c_item_name(struct_name);
    let extern_c_bounds = gen_extern_c_bounds(&types, generics);
    let (fields_tuple, c_fields_tuple, accessors) = build_nested_tuple(&types);
    let is_parametrized = types.iter().any(|ty| is_type_parameterized(ty, generics));
    let niche_ir_bound = is_parametrized.then_some(quote! {
        #fields_tuple: co3::niche::NicheFamily,
    });

    let for_dummy = (!is_parametrized).then_some(quote! { for<'_dšč> });
    let niche_field_values = accessors.iter().map(|accessor| {
        quote! { <#fields_tuple as co3::niche::Niche>::NICHE_VALUE.#accessor }
    });
    let niche_value = match fields.style {
        darling::ast::Style::Tuple => {
            quote! { #repr_c_struct_name(#(#niche_field_values),*) }
        }
        darling::ast::Style::Struct => {
            let field_names = fields.iter().map(|f| &f.ident);
            quote! { #repr_c_struct_name { #(#field_names: #niche_field_values),* } }
        }
        darling::ast::Style::Unit => unreachable!("ZSTs are not FFI safe"),
    };

    quote! {
        impl #impl_generics co3::niche::Niche for #struct_name #ty_generics where
            Self: co3::ExternC<CType = #repr_c_struct_name #ty_generics>,
            #for_dummy #fields_tuple: co3::niche::Niche<CType = #c_fields_tuple>,
            #extern_c_bounds
            #predicates
        {
            const NICHE_VALUE: Self::CType = #niche_value;
        }

        impl #impl_generics co3::niche::NicheFamily for #struct_name #ty_generics where #niche_ir_bound #predicates {
            type Kind = <#fields_tuple as co3::niche::NicheFamily>::Kind;
        }
    }
}

pub fn gen_enum_niche_ir(
    repr: ReprPrimitive,
    enum_name: &syn::Ident,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let niche_value = proc_macro2::Literal::usize_unsuffixed(variants.len());
    let is_fieldless = variants.iter().all(|v| v.fields.fields.is_empty());

    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let niche_value = if is_fieldless {
        quote! { #niche_value }
    } else {
        quote! {{
            let mut value: Self::CType = unsafe { core::mem::zeroed() };

            // SAFETY: All variant structs have tag as first field at offset 0
            // We can safely write it by casting the union pointer to the repr type
            unsafe { *core::ptr::from_mut(&mut value).cast::<#repr>() = #niche_value };

            value
        }}
    };

    if is_exhaustive_enum(variants.len(), repr) {
        return quote! {
            impl #impl_generics co3::niche::NicheFamily for #enum_name #ty_generics #where_clause {
                type Kind = co3::niche::WithoutNiche;
            }
        };
    }

    quote! {
        impl #impl_generics co3::niche::Niche for #enum_name #ty_generics where Self: co3::ExternC, #predicates {
            const NICHE_VALUE: <Self as co3::ExternC>::CType = #niche_value;
        }

        impl #impl_generics co3::niche::NicheFamily for #enum_name #ty_generics #where_clause {
            type Kind = co3::niche::WithCustomNiche;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_types(count: usize) -> Vec<syn::Type> {
        (0..count)
            .map(|i| {
                let ident = syn::Ident::new(&format!("T{}", i), proc_macro2::Span::call_site());
                syn::parse_quote!(#ident)
            })
            .collect()
    }

    #[test]
    fn test_base_case_1_element() {
        let types = make_types(1);
        let refs: Vec<_> = types.iter().collect();
        let (result, _, accessors) = build_nested_tuple(&refs);

        let expected: syn::Type = syn::parse_quote! {
            (T0,)
        };
        let expected_accessors: Vec<syn::Expr> = vec![syn::parse_quote! { value.0 }];

        for (accessor, expected_accessor) in accessors.iter().zip(expected_accessors) {
            assert_eq!(expected_accessor, syn::parse_quote!(value.#accessor));
        }

        assert_eq!(expected, syn::parse_quote!(#result));
    }

    #[test]
    fn test_base_case_12_elements() {
        let types = make_types(12);
        let refs: Vec<_> = types.iter().collect();
        let (result, _, _) = build_nested_tuple(&refs);

        let expected: syn::Type = syn::parse_quote! {
            (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11,)
        };

        assert_eq!(expected, syn::parse_quote!(#result));
    }

    #[test]
    fn test_13_elements() {
        let types = make_types(13);
        let refs: Vec<_> = types.iter().collect();
        let (result, _, accessors) = build_nested_tuple(&refs);

        let expected: syn::Type = syn::parse_quote! {
            (
                (T0, T1, T2, T3, T4, T5, T6),
                (T7, T8, T9, T10, T11, T12),
            )
        };

        assert_eq!(expected, syn::parse_quote!(#result));

        assert_eq!(13, accessors.len());
        // Check first few accessors from the nested structure
        let a0 = &accessors[0];
        let a6 = &accessors[6];
        let a7 = &accessors[7];
        let a12 = &accessors[12];
        let expected0: syn::Expr = syn::parse_quote!(value.0.0);
        let expected6: syn::Expr = syn::parse_quote!(value.0.6);
        let expected7: syn::Expr = syn::parse_quote!(value.1.0);
        let expected12: syn::Expr = syn::parse_quote!(value.1.5);
        assert_eq!(expected0, syn::parse_quote!(value #a0));
        assert_eq!(expected6, syn::parse_quote!(value #a6));
        assert_eq!(expected7, syn::parse_quote!(value #a7));
        assert_eq!(expected12, syn::parse_quote!(value #a12));
    }

    #[test]
    fn test_24_elements() {
        let types = make_types(24);
        let refs: Vec<_> = types.iter().collect();
        let (result, _, _) = build_nested_tuple(&refs);

        let expected: syn::Type = syn::parse_quote! {
            (
                (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11),
                (T12, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23),
            )
        };

        assert_eq!(expected, syn::parse_quote!(#result));
    }

    #[test]
    fn test_25_elements() {
        let types = make_types(25);
        let refs: Vec<_> = types.iter().collect();
        let (result, _, _) = build_nested_tuple(&refs);

        let expected: syn::Type = syn::parse_quote! {
            (
                (T0, T1, T2, T3, T4, T5, T6, T7, T8),
                (T9, T10, T11, T12, T13, T14, T15, T16),
                (T17, T18, T19, T20, T21, T22, T23, T24),
            )
        };

        assert_eq!(expected, syn::parse_quote!(#result));
    }

    #[test]
    fn test_36_elements() {
        let types = make_types(36);
        let refs: Vec<_> = types.iter().collect();
        let (result, _, _) = build_nested_tuple(&refs);

        let expected: syn::Type = syn::parse_quote! {
            (
                (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11),
                (T12, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23),
                (T24, T25, T26, T27, T28, T29, T30, T31, T32, T33, T34, T35),
            )
        };

        assert_eq!(expected, syn::parse_quote!(#result));
    }

    #[test]
    fn test_37_elements() {
        let types = make_types(37);
        let refs: Vec<_> = types.iter().collect();
        let (result, _, _) = build_nested_tuple(&refs);

        let expected: syn::Type = syn::parse_quote! {
            (
                (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9),
                (T10, T11, T12, T13, T14, T15, T16, T17, T18),
                (T19, T20, T21, T22, T23, T24, T25, T26, T27),
                (T28, T29, T30, T31, T32, T33, T34, T35, T36),
            )
        };

        assert_eq!(expected, syn::parse_quote!(#result));
    }

    #[test]
    fn test_144_elements() {
        let types = make_types(144);
        let refs: Vec<_> = types.iter().collect();
        let (result, _, _) = build_nested_tuple(&refs);

        let expected: syn::Type = syn::parse_quote! {
            (
                (T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11),
                (T12, T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23),
                (T24, T25, T26, T27, T28, T29, T30, T31, T32, T33, T34, T35),
                (T36, T37, T38, T39, T40, T41, T42, T43, T44, T45, T46, T47),
                (T48, T49, T50, T51, T52, T53, T54, T55, T56, T57, T58, T59),
                (T60, T61, T62, T63, T64, T65, T66, T67, T68, T69, T70, T71),
                (T72, T73, T74, T75, T76, T77, T78, T79, T80, T81, T82, T83),
                (T84, T85, T86, T87, T88, T89, T90, T91, T92, T93, T94, T95),
                (T96, T97, T98, T99, T100, T101, T102, T103, T104, T105, T106, T107),
                (T108, T109, T110, T111, T112, T113, T114, T115, T116, T117, T118, T119),
                (T120, T121, T122, T123, T124, T125, T126, T127, T128, T129, T130, T131),
                (T132, T133, T134, T135, T136, T137, T138, T139, T140, T141, T142, T143),
            )
        };

        assert_eq!(expected, syn::parse_quote!(#result));
    }

    #[test]
    fn test_145_elements() {
        let types = make_types(145);
        let refs: Vec<_> = types.iter().collect();
        let (result, _, _) = build_nested_tuple(&refs);

        let expected: syn::Type = syn::parse_quote! {
            (
                (
                    (T0, T1, T2, T3, T4, T5, T6),
                    (T7, T8, T9, T10, T11, T12),
                ),
                (T13, T14, T15, T16, T17, T18, T19, T20, T21, T22, T23, T24),
                (T25, T26, T27, T28, T29, T30, T31, T32, T33, T34, T35, T36),
                (T37, T38, T39, T40, T41, T42, T43, T44, T45, T46, T47, T48),
                (T49, T50, T51, T52, T53, T54, T55, T56, T57, T58, T59, T60),
                (T61, T62, T63, T64, T65, T66, T67, T68, T69, T70, T71, T72),
                (T73, T74, T75, T76, T77, T78, T79, T80, T81, T82, T83, T84),
                (T85, T86, T87, T88, T89, T90, T91, T92, T93, T94, T95, T96),
                (T97, T98, T99, T100, T101, T102, T103, T104, T105, T106, T107, T108),
                (T109, T110, T111, T112, T113, T114, T115, T116, T117, T118, T119, T120),
                (T121, T122, T123, T124, T125, T126, T127, T128, T129, T130, T131, T132),
                (T133, T134, T135, T136, T137, T138, T139, T140, T141, T142, T143, T144),
            )
        };

        assert_eq!(expected, syn::parse_quote!(#result));
    }

    #[test]
    fn test_156_elements() {
        let types = make_types(156);
        let refs: Vec<_> = types.iter().collect();
        let (result, _, _) = build_nested_tuple(&refs);

        let expected: syn::Type = syn::parse_quote! {
            (
                (
                    (T0, T1, T2, T3, T4, T5, T6),
                    (T7, T8, T9, T10, T11, T12),
                ),
                (
                    (T13, T14, T15, T16, T17, T18, T19),
                    (T20, T21, T22, T23, T24, T25),
                ),
                (
                    (T26, T27, T28, T29, T30, T31, T32),
                    (T33, T34, T35, T36, T37, T38),
                ),
                (
                    (T39, T40, T41, T42, T43, T44, T45),
                    (T46, T47, T48, T49, T50, T51),
                ),
                (
                    (T52, T53, T54, T55, T56, T57, T58),
                    (T59, T60, T61, T62, T63, T64),
                ),
                (
                    (T65, T66, T67, T68, T69, T70, T71),
                    (T72, T73, T74, T75, T76, T77),
                ),
                (
                    (T78, T79, T80, T81, T82, T83, T84),
                    (T85, T86, T87, T88, T89, T90),
                ),
                (
                    (T91, T92, T93, T94, T95, T96, T97),
                    (T98, T99, T100, T101, T102, T103),
                ),
                (
                    (T104, T105, T106, T107, T108, T109, T110),
                    (T111, T112, T113, T114, T115, T116),
                ),
                (
                    (T117, T118, T119, T120, T121, T122, T123),
                    (T124, T125, T126, T127, T128, T129),
                ),
                (
                    (T130, T131, T132, T133, T134, T135, T136),
                    (T137, T138, T139, T140, T141, T142),
                ),
                (
                    (T143, T144, T145, T146, T147, T148, T149),
                    (T150, T151, T152, T153, T154, T155),
                ),
            )
        };

        assert_eq!(expected, syn::parse_quote!(#result));
    }
}
