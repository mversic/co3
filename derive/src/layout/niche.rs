use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::{
    layout::{
        attr::ReprKind,
        ctype::{classify_tail, gen_copy_extern_c_bounds_for_ctype, gen_ctype_name},
        enum_tag_type, enum_variant_tag_exprs, is_exhaustive_enum, is_parametrized,
        is_phantom_data, is_transparent_enum_repr,
    },
    utils::speculative_marker,
};

pub fn gen_niche_value_ir(
    name: &syn::Ident,
    generics: &syn::Generics,
    value: TokenStream,
    detector: TokenStream,
    bounds: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|clause| &clause.predicates);
    quote! {
        impl #impl_generics co3::niche::Niche for #name #ty_generics where
            #bounds
            #predicates
        {
            const NICHE: Self::CType = #value;
            fn is_niche(value: &Self::CType) -> bool { #detector }
        }
    }
}

pub fn gen_struct_niche_ir(
    struct_name: &syn::Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
    niche_value: Option<&syn::Expr>,
    equalities: &[syn::WherePredicate],
) -> TokenStream {
    if fields
        .iter()
        .next_back()
        .is_some_and(|field| classify_tail(&field.ty, generics).is_known_unsized())
    {
        return quote! {};
    }

    let (_, ty_generics, _) = generics.split_for_impl();

    let ctype_name = gen_ctype_name(struct_name);
    let for_dummy = speculative_marker(generics.params.is_empty());

    let types = fields.iter().map(|f| &f.ty).collect::<Vec<_>>();
    let copy_bounds = types
        .iter()
        .filter(|ty| !is_phantom_data(ty))
        .map(|ty| {
            let speculative = speculative_marker(!is_parametrized(ty, generics));
            quote!(#speculative #ty: co3::ReprC<CType: Copy>,)
        })
        .collect::<Vec<_>>();
    let (last_field_sized, _) = super::ctype::c_fn_last_field_bounds(&types, generics);
    let extern_c_bounds = gen_copy_extern_c_bounds_for_ctype(generics, &types);
    let tuple = FieldTuple::new(&types, fields.members());
    let fields_tuple = &tuple.rust;

    let (niche_value, detector, inferred_niche_bounds, custom_niche_bounds) =
        if let Some(niche_value) = niche_value {
            let custom_niche_bounds = quote! {
                #struct_name #ty_generics: co3::rust_spec::RustSpec<
                    Niche = co3::rust_spec::niche::WithNiche<__Co3Unstable>
                >,
                #fields_tuple: co3::rust_spec::RustSpec<
                    Niche = co3::rust_spec::niche::WithoutNiche
                >,
                #for_dummy #ctype_name #ty_generics: PartialEq,
            };

            (
                quote! { #niche_value },
                quote! {{
                    let niche = #niche_value;
                    debug_assert!(niche == niche, "custom niche must compare equal to itself");
                    *value == niche
                }},
                quote! {},
                custom_niche_bounds,
            )
        } else {
            let c_fields_tuple = &tuple.carrier;
            let tuple_value = &tuple.value;
            let niche_field_values = tuple
                .paths
                .iter()
                .map(|path| quote!(<#fields_tuple as co3::niche::Niche>::NICHE #(.0.#path)*));
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
                quote!(<#fields_tuple as co3::niche::Niche>::is_niche(&#tuple_value)),
                quote! { #for_dummy #fields_tuple: co3::niche::Niche<CType = #c_fields_tuple>, },
                quote! {},
            )
        };

    gen_niche_value_ir(
        struct_name,
        generics,
        niche_value,
        detector,
        quote! {
            #for_dummy #ctype_name #ty_generics: Copy,
            #last_field_sized
            #inferred_niche_bounds
            #(#copy_bounds)*
            #(#equalities,)*
            #custom_niche_bounds
            #(#extern_c_bounds,)*
        },
    )
}

/// The Rust type, native carrier and value all follow the same tuple layout.
/// Keep each leaf's indices so a niche can be projected back onto the record.
struct FieldTuple {
    rust: TokenStream,
    carrier: TokenStream,
    value: TokenStream,
    paths: Vec<Vec<syn::Index>>,
}

impl FieldTuple {
    fn new(types: &[&syn::Type], members: impl Iterator<Item = syn::Member>) -> Self {
        const ARITY: usize = 12;
        let mut fields = types.iter().zip(members).map(|(ty, member)| Self {
            rust: quote!(#ty),
            carrier: quote!(<#ty as co3::ReprC>::CType),
            value: quote!(value.#member),
            paths: vec![Vec::new()],
        });
        let mut level = Vec::new();
        loop {
            let group = fields.by_ref().take(ARITY).collect::<Vec<_>>();
            if group.is_empty() {
                break;
            }
            level.push(Self::pack(group));
        }
        while level.len() > 1 {
            let mut nodes = level.into_iter();
            level = Vec::new();
            loop {
                let mut group = nodes.by_ref().take(ARITY).collect::<Vec<_>>();
                match group.len() {
                    0 => break,
                    1 => level.push(group.pop().unwrap()),
                    _ => level.push(Self::pack(group)),
                }
            }
        }
        level.pop().unwrap_or_else(|| Self {
            rust: quote!(()),
            carrier: quote!(()),
            value: quote!(()),
            paths: Vec::new(),
        })
    }

    fn pack(children: Vec<Self>) -> Self {
        let ident = format_ident!("Tuple{}", children.len());
        let rust = children.iter().map(|child| &child.rust);
        let carriers = children.iter().map(|child| &child.carrier);
        let values = children.iter().map(|child| &child.value);
        Self {
            rust: quote!((#(#rust,)*)),
            carrier: quote!(__Co3Native<co3::tuple::abi::#ident<#(#carriers),*>>),
            value: quote!(__Co3Native(co3::tuple::abi::#ident(#(#values),*))),
            paths: children
                .into_iter()
                .enumerate()
                .flat_map(|(i, child)| {
                    child
                        .paths
                        .into_iter()
                        .map(move |path| std::iter::once(i.into()).chain(path).collect())
                })
                .collect(),
        }
    }
}

pub(super) fn gen_enum_niche_tag(
    enum_name: &syn::Ident,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
    tag_ty: &syn::Type,
    is_fieldless: bool,
) -> TokenStream {
    if variants
        .iter()
        .any(|variant| variant.discriminant.is_some())
    {
        let tag_values = enum_variant_tag_exprs(variants, tag_ty);
        let variant_tags = variants.iter().zip(&tag_values).map(|(variant, value)| {
            if is_fieldless {
                let tag = format_ident!("__CO3_TAG_{}", variant.ident);
                quote! { __co3_niche_tag != #enum_name::#tag }
            } else {
                quote! { __co3_niche_tag != #value }
            }
        });
        quote! {{
            let mut __co3_niche_tag: #tag_ty = 0;
            loop {
                if true #(&& #variant_tags)* {
                    break __co3_niche_tag;
                }
                __co3_niche_tag = __co3_niche_tag.wrapping_sub(1);
            }
        }}
    } else if variants.is_empty() {
        quote! { 0 as #tag_ty }
    } else {
        quote! { (0 as #tag_ty).wrapping_sub(1) }
    }
}

pub fn gen_enum_niche_ir(
    repr: Option<&ReprKind>,
    enum_name: &syn::Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
    transparent: bool,
    equalities: &[syn::WherePredicate],
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    if variants.len() == 1 && matches!(variants[0].fields, syn::Fields::Unit) {
        return quote! {};
    }

    if is_transparent_enum_repr(repr, variants) {
        let Some(variant) = variants.first() else {
            return quote! {};
        };
        if variant.fields.is_empty() {
            return quote! {};
        }

        return gen_struct_niche_ir(enum_name, generics, &variant.fields, None, equalities);
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
    let extern_c_bounds = gen_copy_extern_c_bounds_for_ctype(generics, &variant_field_types);

    let niche_value = if is_fieldless {
        if transparent {
            gen_enum_niche_tag(enum_name, variants, &tag_ty, true)
        } else {
            let ctype_name = gen_ctype_name(enum_name);
            quote! { <#ctype_name #ty_generics>::niche() }
        }
    } else {
        let ctype_name = gen_ctype_name(enum_name);
        quote! { <#ctype_name #ty_generics>::niche() }
    };
    let detector = if is_fieldless && transparent {
        quote!(*value == Self::NICHE)
    } else {
        quote!(value.is_niche())
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
            const NICHE: <Self as co3::ReprC>::CType = #niche_value;
            fn is_niche(value: &Self::CType) -> bool { #detector }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FieldTuple;

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
        let tuple = FieldTuple::new(&refs, (0..count).map(|i| syn::Member::Unnamed(i.into())));
        let (rust_tuple, c_tuple, accessors) = (tuple.rust, tuple.carrier, tuple.paths);
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
            assert_eq!(
                accessor
                    .iter()
                    .map(|i| i.index as usize)
                    .collect::<Vec<_>>(),
                path
            );
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
                let ident = quote::format_ident!("T{index}");
                let projected: syn::Type = syn::parse_quote!(<#ident as co3::ReprC>::CType);
                assert_eq!(ty, &projected);
            }
            ExpectedNode::Tuple(children) if children.is_empty() => {
                let syn::Type::Tuple(tuple) = ty else {
                    panic!("expected empty C tuple, got {ty:?}");
                };
                assert!(tuple.elems.is_empty());
            }
            ExpectedNode::Tuple(children) => {
                let syn::Type::Path(wrapper) = ty else {
                    panic!("expected native tuple modifier, got {ty:?}");
                };
                let segment = wrapper.path.segments.last().expect("modifier path");
                assert_eq!(segment.ident, "__Co3Native");
                let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
                    panic!("expected modifier argument");
                };
                let Some(syn::GenericArgument::Type(ty)) = arguments.args.first() else {
                    panic!("expected carrier type");
                };
                let syn::Type::Path(ty) = ty else {
                    panic!("expected Tuple{} type, got {ty:?}", children.len());
                };
                assert!(ty.qself.is_none());
                let segment = ty.path.segments.last().expect("Tuple path");
                let expected_name = format!("Tuple{}", children.len());
                assert_eq!(
                    ["co3", "tuple", "abi", expected_name.as_str()],
                    ty.path
                        .segments
                        .iter()
                        .map(|part| part.ident.to_string())
                        .collect::<Vec<_>>()
                        .as_slice()
                );

                let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
                    panic!("expected Tuple type arguments");
                };
                assert_eq!(children.len(), arguments.args.len());
                for (argument, expected_child) in arguments.args.iter().zip(children) {
                    let syn::GenericArgument::Type(child) = argument else {
                        panic!("expected Tuple type argument, got {argument:?}");
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
        let tuple = FieldTuple::new(&refs, (0..count).map(|i| syn::Member::Unnamed(i.into())));
        let path = &tuple.paths[index];
        assert_eq!(expected, syn::parse_quote!(value #(.#path)*));
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
