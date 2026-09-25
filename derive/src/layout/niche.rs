use proc_macro2::{Literal, TokenStream};
use quote::quote;

use crate::{
    layout::{
        attr::ReprKind,
        borrow::gen_view_owner_name,
        ctype::{gen_ctype_name, gen_extern_c_bounds_for_ctype},
        enum_tag_type, generic_param_idents, is_exhaustive_enum, is_transparent_enum_repr,
        is_type_parametrized,
    },
    utils::build_extern_c_type_tuple,
};

pub fn gen_view_niche_ir(view_name: &syn::Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, view_ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);
    let has_view_lifetime = matches!(
        generics.params.first(),
        Some(syn::GenericParam::Lifetime(param)) if param.lifetime.ident == "_dšč"
    );
    let owner_name = gen_view_owner_name(view_name);
    let owner_generics =
        generic_param_idents(generics.params.iter().skip(has_view_lifetime as usize))
            .collect::<Vec<_>>();
    let owner_ty = quote! { #owner_name <#(#owner_generics),*> };

    quote! {
        impl #impl_generics co3::niche::Niche for #view_name #view_ty_generics where
            #owner_ty: co3::niche::Niche,
            <#owner_ty as co3::ReprC>::CType: co3::borrow::BorrowCast<
                AsConst = <Self as co3::ReprC>::CType
            >,
            #predicates
        {
            const NICHE_VALUE: Self::CType = co3::borrow::borrow_cast(
                <#owner_ty as co3::niche::Niche>::NICHE_VALUE
            );
        }
    }
}

pub fn gen_struct_niche_ir(
    struct_name: &syn::Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
    niche_value: Option<&syn::Expr>,
) -> TokenStream {
    let (impl_generics, ty_generics, _) = generics.split_for_impl();

    let predicates = generics
        .where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let ctype_name = gen_ctype_name(struct_name);
    let for_dummy = (generics.type_params().count() == 0).then_some(quote! {
        for<'_dummy>
    });

    let last_field_sized = fields.iter().next_back().map(|field| {
        let ty = &field.ty;

        let for_dummy = (!is_type_parametrized(ty, generics)).then_some(quote! {
            for<'_dummy>
        });

        quote! { #for_dummy #ty: Sized, }
    });
    let types = fields.iter().map(|f| &f.ty).collect::<Vec<_>>();
    let extern_c_bounds = gen_extern_c_bounds_for_ctype::<true>(generics, &types);
    let (fields_tuple, c_fields_tuple, accessors) = build_extern_c_type_tuple(&types);

    let (niche_value, inferred_niche_bounds, custom_niche_bounds) =
        if let Some(niche_value) = niche_value {
            let custom_niche_bounds = quote! {
                #struct_name #ty_generics: co3::rust_spec::RustSpec<
                    Niche = co3::rust_spec::niche::WithNiche<co3::rust_spec::Unstable>
                >,
                #fields_tuple: co3::rust_spec::RustSpec<
                    Niche = co3::rust_spec::niche::WithoutNiche
                >,
            };

            (quote! { #niche_value }, quote! {}, custom_niche_bounds)
        } else {
            let niche_field_values = accessors.iter().map(|accessor| {
                quote! { <#fields_tuple as co3::niche::Niche>::NICHE_VALUE.#accessor }
            });
            let niche_value = match fields {
                syn::Fields::Named(_) | syn::Fields::Unit => {
                    let field_names = fields.iter().map(|f| &f.ident);
                    quote! { #ctype_name { #(#field_names: #niche_field_values),* } }
                }
                syn::Fields::Unnamed(_) => {
                    quote! { #ctype_name(#(#niche_field_values),*) }
                }
            };
            (
                niche_value,
                quote! { #for_dummy #fields_tuple: co3::niche::Niche<CType = #c_fields_tuple>, },
                quote! {},
            )
        };

    quote! {
        impl #impl_generics co3::niche::Niche for #struct_name #ty_generics where
            #for_dummy #ctype_name #ty_generics: Copy,
            #last_field_sized
            #inferred_niche_bounds
            #custom_niche_bounds
            #(#extern_c_bounds,)*
            #predicates
        {
            const NICHE_VALUE: Self::CType = #niche_value;
        }
    }
}

pub fn gen_enum_niche_ir(
    repr: Option<&ReprKind>,
    enum_name: &syn::Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    if is_transparent_enum_repr(repr, variants) {
        let Some(variant) = variants.first() else {
            return quote! {};
        };
        if variant.fields.is_empty() {
            return quote! {};
        }

        return gen_struct_niche_ir(enum_name, generics, &variant.fields, None);
    }
    let tag_ty = enum_tag_type(repr, variants.len());
    if is_exhaustive_enum(variants.len(), &tag_ty) {
        return quote! {};
    }

    let is_fieldless = variants
        .iter()
        .all(|v| matches!(v.fields, syn::Fields::Unit));

    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let variant_field_types = variants
        .iter()
        .flat_map(|variant| variant.fields.iter().map(|field| &field.ty))
        .collect::<Vec<_>>();
    let extern_c_bounds = gen_extern_c_bounds_for_ctype::<true>(generics, &variant_field_types);

    let niche_discriminant = Literal::usize_unsuffixed(variants.len());
    let niche_tag_value = quote! { #niche_discriminant as #tag_ty };

    let has_explicit_discriminant = variants
        .iter()
        .any(|variant| variant.discriminant.is_some());
    let niche_value = if is_fieldless && has_explicit_discriminant {
        let ctype_name = gen_ctype_name(enum_name);
        let variant_tags = variants.iter().map(|variant| {
            let variant_name = &variant.ident;
            quote! { __co3_niche_tag != Self::#variant_name as #tag_ty }
        });
        quote! {
            #ctype_name({
                let mut __co3_niche_tag: #tag_ty = 0;
                loop {
                    if true #(&& #variant_tags)* {
                        break __co3_niche_tag;
                    }
                    __co3_niche_tag = __co3_niche_tag.wrapping_add(1);
                }
            })
        }
    } else if is_fieldless {
        let ctype_name = gen_ctype_name(enum_name);
        quote! { #ctype_name(#niche_tag_value) }
    } else {
        match repr {
            Some(ReprKind::C(Some(_))) => quote! {
                Self::CType {
                    tag: #niche_tag_value,
                    payload: unsafe { core::mem::zeroed() },
                }
            },
            None | Some(ReprKind::Primitive(_)) => quote! {{
                let mut value: Self::CType = unsafe { core::mem::zeroed() };

                // SAFETY: The CType is a union of variant structs with the tag as first field.
                unsafe { *<*mut Self::CType>::cast::<#tag_ty>(core::ptr::from_mut(&mut value)) = #niche_tag_value };

                value
            }},
            Some(ReprKind::Transparent) => unreachable!(),
            Some(ReprKind::C(None)) => unreachable!(),
        }
    };

    let self_bounds = generics
        .params
        .is_empty()
        .then(|| quote! { Self: co3::ReprC<CType: Copy>, });

    quote! {
        impl #impl_generics co3::niche::Niche for #enum_name #ty_generics where
            #(#extern_c_bounds,)*
            #self_bounds
            #predicates
        {
            const NICHE_VALUE: <Self as co3::ReprC>::CType = #niche_value;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::build_extern_c_type_tuple;

    const MAX_TUPLE_ARITY: usize = 12;

    fn make_types(count: usize) -> Vec<syn::Type> {
        (0..count)
            .map(|i| {
                let ident = syn::Ident::new(&format!("T{}", i), proc_macro2::Span::call_site());
                syn::parse_quote!(#ident)
            })
            .collect()
    }

    #[derive(Debug)]
    enum ExpectedNode {
        Leaf(usize),
        Tuple(Vec<ExpectedNode>),
    }

    fn expected_tuple_layout(count: usize) -> ExpectedNode {
        if count == 0 {
            return ExpectedNode::Tuple(Vec::new());
        }

        let mut leaves = (0..count).map(ExpectedNode::Leaf);
        let mut nodes = Vec::new();
        loop {
            let chunk = leaves.by_ref().take(MAX_TUPLE_ARITY).collect::<Vec<_>>();
            if chunk.is_empty() {
                break;
            }
            nodes.push(ExpectedNode::Tuple(chunk));
        }

        while nodes.len() > 1 {
            let mut input = nodes.into_iter();
            let mut next = Vec::new();
            loop {
                let mut chunk = input.by_ref().take(MAX_TUPLE_ARITY).collect::<Vec<_>>();
                if chunk.is_empty() {
                    break;
                }
                if chunk.len() == 1 {
                    next.push(chunk.pop().expect("singleton remainder"));
                } else {
                    next.push(ExpectedNode::Tuple(chunk));
                }
            }
            nodes = next;
        }

        nodes.pop().expect("non-empty layout")
    }

    fn assert_tuple_layout(count: usize) {
        let types = make_types(count);
        let refs: Vec<_> = types.iter().collect();
        let (rust_tuple, c_tuple, accessors) = build_extern_c_type_tuple(&refs);

        eprintln!("{count} elements:");
        eprintln!("  Rust tuple: {rust_tuple}");
        eprintln!("  C tuple: {c_tuple}");
        eprintln!(
            "  Accessors: {}",
            accessors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );

        let rust_tuple: syn::Type =
            syn::parse2(rust_tuple).expect("generated Rust tuple must be a type");
        let c_tuple: syn::Type = syn::parse2(c_tuple).expect("generated C tuple must be a type");
        let expected = expected_tuple_layout(count);

        assert_rust_tuple_tree(&rust_tuple, &expected);
        assert_c_tuple_tree(&c_tuple, &expected);

        let mut expected_accessors = Vec::new();
        collect_expected_accessors(&expected, &mut Vec::new(), &mut expected_accessors);
        assert_eq!(expected_accessors.len(), accessors.len());
        for (position, (accessor, (index, path))) in
            accessors.iter().zip(expected_accessors).enumerate()
        {
            assert_eq!(position, index);
            let mut expected = String::from("value");
            for field in path {
                expected.push_str(&format!(".{field}"));
            }
            let expected: syn::Expr = syn::parse_str(&expected).unwrap();

            assert_eq!(expected, syn::parse_quote!(value.#accessor));
        }
    }

    fn assert_rust_tuple_tree(ty: &syn::Type, expected: &ExpectedNode) {
        match expected {
            ExpectedNode::Leaf(index) => {
                let syn::Type::Path(ty) = ty else {
                    panic!("expected leaf T{index}, got {ty:?}");
                };
                let ident = &ty.path.segments.last().expect("type path").ident;
                assert_eq!(format!("T{index}"), ident.to_string());
            }
            ExpectedNode::Tuple(children) => {
                let syn::Type::Tuple(tuple) = ty else {
                    panic!(
                        "expected tuple with {} children, got {ty:?}",
                        children.len()
                    );
                };
                assert_eq!(children.len(), tuple.elems.len());
                for (child, expected_child) in tuple.elems.iter().zip(children) {
                    assert_rust_tuple_tree(child, expected_child);
                }
            }
        }
    }

    fn assert_c_tuple_tree(ty: &syn::Type, expected: &ExpectedNode) {
        match expected {
            ExpectedNode::Leaf(index) => {
                let syn::Type::Path(ty) = ty else {
                    panic!("expected associated CType for T{index}, got {ty:?}");
                };
                let qself = ty.qself.as_ref().expect("CType must be qualified");
                let syn::Type::Path(source) = qself.ty.as_ref() else {
                    panic!("expected source type path, got {:?}", qself.ty);
                };
                let ident = &source.path.segments.last().expect("source type path").ident;
                assert_eq!(format!("T{index}"), ident.to_string());
                assert_eq!(
                    ["co3", "ReprC", "CType"],
                    ty.path
                        .segments
                        .iter()
                        .map(|part| part.ident.to_string())
                        .collect::<Vec<_>>()
                        .as_slice()
                );
            }
            ExpectedNode::Tuple(children) if children.is_empty() => {
                let syn::Type::Tuple(tuple) = ty else {
                    panic!("expected empty C tuple, got {ty:?}");
                };
                assert!(tuple.elems.is_empty());
            }
            ExpectedNode::Tuple(children) => {
                let syn::Type::Path(ty) = ty else {
                    panic!("expected ReprCTuple{} type, got {ty:?}", children.len());
                };
                assert!(ty.qself.is_none());
                let segment = ty.path.segments.last().expect("ReprCTuple path");
                let expected_name = format!("ReprCTuple{}", children.len());
                assert_eq!(
                    ["co3", "tuple", expected_name.as_str()],
                    ty.path
                        .segments
                        .iter()
                        .map(|part| part.ident.to_string())
                        .collect::<Vec<_>>()
                        .as_slice()
                );

                let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
                    panic!("expected ReprCTuple type arguments");
                };
                assert_eq!(children.len(), arguments.args.len());
                for (argument, expected_child) in arguments.args.iter().zip(children) {
                    let syn::GenericArgument::Type(child) = argument else {
                        panic!("expected ReprCTuple type argument, got {argument:?}");
                    };
                    assert_c_tuple_tree(child, expected_child);
                }
            }
        }
    }

    fn collect_expected_accessors(
        node: &ExpectedNode,
        path: &mut Vec<usize>,
        accessors: &mut Vec<(usize, Vec<usize>)>,
    ) {
        match node {
            ExpectedNode::Leaf(index) => accessors.push((*index, path.clone())),
            ExpectedNode::Tuple(children) => {
                for (index, child) in children.iter().enumerate() {
                    path.push(index);
                    collect_expected_accessors(child, path, accessors);
                    path.pop();
                }
            }
        }
    }

    fn assert_accessor(count: usize, index: usize, expected: syn::Expr) {
        let types = make_types(count);
        let refs = types.iter().collect::<Vec<_>>();
        let (_, _, accessors) = build_extern_c_type_tuple(&refs);
        let accessor = &accessors[index];
        assert_eq!(expected, syn::parse_quote!(value.#accessor));
    }

    #[test]
    fn test_empty() {
        assert_tuple_layout(0);
    }

    #[test]
    fn test_base_case_1_element() {
        assert_tuple_layout(1);
    }

    #[test]
    fn test_base_case_12_elements() {
        assert_tuple_layout(12);
    }

    #[test]
    fn test_13_elements() {
        assert_tuple_layout(13);
    }

    #[test]
    fn test_24_elements() {
        assert_tuple_layout(24);
    }

    #[test]
    fn test_25_elements() {
        assert_tuple_layout(25);
    }

    #[test]
    fn test_36_elements() {
        assert_tuple_layout(36);
    }

    #[test]
    fn test_37_elements() {
        assert_tuple_layout(37);
    }

    #[test]
    fn test_144_elements() {
        assert_tuple_layout(144);
    }

    #[test]
    fn test_145_elements() {
        assert_tuple_layout(145);
        assert_accessor(145, 144, syn::parse_quote!(value.1.0));
    }

    #[test]
    fn test_156_elements() {
        assert_tuple_layout(156);
        assert_accessor(156, 144, syn::parse_quote!(value.1.0));
        assert_accessor(156, 155, syn::parse_quote!(value.1.11));
    }
}
