use std::collections::HashSet;

use proc_macro2::{Literal, TokenStream};
use quote::{format_ident, quote};
use syn::{parse_quote, visit::Visit};

use crate::layout::{
    attr::ReprKind,
    infer_repr, is_phantom_data, is_transparent_enum_repr, is_type_parametrized,
    primitive_tag_type,
    wide::{
        data_bound_ty, gen_alloc_methods, gen_data_ctype_bounds, gen_data_struct_name,
        gen_dst_methods, last_field, wide_predicate,
    },
};
use crate::utils::co3_path;

fn lowered_field_ty(field_ty: &syn::Type) -> TokenStream {
    if is_phantom_data(field_ty) {
        return quote!(#field_ty);
    }

    quote!(<#field_ty as co3::ExternC>::CType)
}

/// Generates the C carrier for a fieldless enum. Unlike a data enum's C
/// union, this is always a transparent newtype over its discriminant.
pub(super) fn gen_fieldless_enum_ctype(
    tag_type: &syn::Type,
    alignment: Option<&syn::LitInt>,
    vis: &syn::Visibility,
    name: &syn::Ident,
    generics: &syn::Generics,
) -> TokenStream {
    let ctype_name = gen_ctype_name(name);
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);
    // `repr(transparent)` cannot be combined with `align`, so use the
    // equivalent single-field C layout when an explicit alignment is present.
    let repr_kind = if alignment.is_none() {
        &ReprKind::Transparent
    } else {
        &ReprKind::C(None)
    };
    let repr = gen_ctype_repr_attr(Some(repr_kind), alignment);
    let ctype: syn::ItemStruct = parse_quote! {
        #[doc(hidden)]
        #repr
        #vis struct #ctype_name #impl_generics(pub #tag_type)
        where
            #predicates;
    };
    // Keep the normal non-ZST CFnArg size guard. In particular, a one-variant
    // fieldless enum is represented by `CEnum(())` and must not become a CFnArg.
    let impls = gen_struct_ctype_impls::<false, true>(&ctype, false, alignment);
    let fields = ctype
        .fields
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();
    let type_spec_impl = gen_type_spec(&ctype.ident, &ctype.generics, &fields, alignment);
    let borrow_cast_impl = gen_identity_borrow_cast_impl(&ctype.ident, &ctype.generics);

    quote! {
        #ctype
        #impls
        #type_spec_impl
        #borrow_cast_impl
    }
}

pub(super) fn gen_item_ctype(
    repr: Option<&ReprKind>,
    alignment: Option<&syn::LitInt>,
    input: &syn::DeriveInput,
    generate_views_and_spec: bool,
) -> TokenStream {
    let vis = &input.vis;
    let name = &input.ident;
    let generics = &input.generics;
    match &input.data {
        syn::Data::Struct(data) => derive_ctype_struct::<false>(
            repr,
            alignment,
            vis,
            name,
            generics,
            &data.fields,
            generate_views_and_spec,
            generate_views_and_spec,
        ),
        syn::Data::Enum(data) if is_transparent_enum_repr(repr, &data.variants) => {
            let Some(first_variant) = data.variants.first() else {
                return quote! {};
            };

            derive_ctype_struct::<true>(
                repr,
                alignment,
                vis,
                name,
                generics,
                &first_variant.fields,
                generate_views_and_spec,
                false,
            )
        }
        syn::Data::Enum(data) if repr.is_none() => {
            let tag_type = infer_repr(data.variants.len());
            derive_data_enum_ctype(tag_type, alignment, vis, name, generics, &data.variants)
        }
        syn::Data::Enum(data) if let Some(ReprKind::Primitive(repr)) = repr => {
            derive_data_enum_ctype(
                (**repr).clone(),
                alignment,
                vis,
                name,
                generics,
                &data.variants,
            )
        }
        syn::Data::Enum(data) if matches!(repr, Some(&ReprKind::C(Some(_)))) => {
            let tag_type = primitive_tag_type(repr.expect("C primitive representation")).clone();
            derive_repr_c_data_enum_ctype(tag_type, alignment, vis, name, generics, &data.variants)
        }
        syn::Data::Union(_) | syn::Data::Enum(_) => {
            unreachable!()
        }
    }
}

fn derive_data_enum_ctype(
    tag_type: syn::Type,
    alignment: Option<&syn::LitInt>,
    vis: &syn::Visibility,
    name: &syn::Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> TokenStream {
    let union_name = gen_ctype_name(name);
    let (union_def, variant_structs) = gen_data_enum_union(
        Some(tag_type.clone()),
        alignment,
        vis,
        &union_name,
        name,
        generics,
        variants,
    );
    let union_impls = gen_union_ctype_impls(&union_def, alignment);
    let partial_eq_impl = gen_tagged_union_partial_eq(&union_def, &tag_type, variants);
    quote! { #(#variant_structs)* #union_def #union_impls #partial_eq_impl }
}

fn gen_tagged_union_partial_eq(
    union: &syn::ItemUnion,
    tag_type: &syn::Type,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> TokenStream {
    let name = &union.ident;
    let (impl_generics, ty_generics, where_clause) = union.generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|clause| &clause.predicates);
    let bounds = union.fields.named.iter().map(|field| {
        let ty = &field.ty;
        quote!(#ty: core::cmp::PartialEq)
    });
    let arms = variants.iter().enumerate().map(|(index, variant)| {
        let tag = Literal::usize_unsuffixed(index);
        let member = &variant.ident;
        quote!(#tag => unsafe { self.#member == other.#member })
    });

    quote! {
        impl #impl_generics core::cmp::PartialEq for #name #ty_generics
        where
            #(#bounds,)*
            #predicates
        {
            fn eq(&self, other: &Self) -> bool {
                let self_tag = unsafe { *core::ptr::from_ref(self).cast::<#tag_type>() };
                let other_tag = unsafe { *core::ptr::from_ref(other).cast::<#tag_type>() };
                self_tag == other_tag && match self_tag as usize {
                    #(#arms,)*
                    _ => true,
                }
            }
        }
    }
}

#[expect(clippy::too_many_arguments)]
fn derive_ctype_struct<const ADD_COPY: bool>(
    repr: Option<&ReprKind>,
    alignment: Option<&syn::LitInt>,
    vis: &syn::Visibility,
    name: &syn::Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
    generate_views_and_spec: bool,
    generate_wide: bool,
) -> TokenStream {
    let ctype_def = gen_ctype_struct_item::<ADD_COPY>(repr, alignment, vis, name, generics, fields);
    let ctype_impls =
        gen_struct_ctype_impls::<ADD_COPY, true>(&ctype_def, generate_views_and_spec, alignment);
    let wide_impl = generate_wide.then(|| gen_ctype_wide_impl(repr, &ctype_def, fields, generics));

    quote! {
        #ctype_def
        #ctype_impls
        #wide_impl
    }
}

fn gen_ctype_wide_impl(
    repr: Option<&ReprKind>,
    ctype: &syn::ItemStruct,
    source_fields: &syn::Fields,
    source_generics: &syn::Generics,
) -> TokenStream {
    let is_transparent = matches!(repr, Some(ReprKind::Transparent));

    if source_fields
        .iter()
        .next_back()
        .is_some_and(|field| is_phantom_data(&field.ty))
    {
        return quote! {};
    }

    if source_fields.len() == 1 && !is_transparent {
        return quote! {};
    }

    let Some((field, field_ref, field_member)) = last_field(&ctype.fields) else {
        return quote! {};
    };

    let field_ty = &field.ty;
    let name = &ctype.ident;

    let source_name = name.to_string();
    let source_name = source_name.strip_prefix('C').unwrap();
    let source_name = syn::Ident::new(source_name, name.span());

    let (impl_generics, ty_generics, where_clause) = ctype.generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let data_name = gen_data_struct_name(&source_name);
    let data_ctype_name = gen_ctype_name(&data_name);

    let ctype_wide_predicate = wide_predicate(field_ty, &ctype.generics);
    let methods = gen_dst_methods(is_transparent, field_ty, field_ref, field_member);

    let alloc_methods = gen_alloc_methods();
    let Some((source_last, ..)) = last_field(source_fields) else {
        return quote! {};
    };
    let source_field_ty = &source_last.ty;

    let source_wide_predicate = wide_predicate(&source_last.ty, source_generics);
    let source_data_ctype_bounds = gen_data_ctype_bounds(source_fields, source_generics);
    let source_data_ty = data_bound_ty(&source_last.ty, true);
    let source_data_ctype_sized_bound = if is_type_parametrized(&source_data_ty, source_generics) {
        quote!(#source_data_ty: co3::ExternC<CType: Sized>)
    } else {
        quote!(for<'__dummy> #source_data_ty: co3::ExternC<CType: Sized>)
    };

    let for_dummy = (ctype.generics.type_params().count() == 0).then_some(quote! {
        for<'_dummy>
    });

    let data_bound = (!is_transparent).then(|| {
        quote! {
            #for_dummy #data_name #ty_generics: co3::ExternC<CType = #data_ctype_name #ty_generics>,
        }
    });
    let data_ty = if is_transparent {
        quote!(<<#source_field_ty as co3::wide::Wide>::Data as co3::ExternC>::CType)
    } else {
        quote!(<#data_name #ty_generics as co3::ExternC>::CType)
    };

    quote! {
        impl #impl_generics co3::wide::Wide for #name #ty_generics
        where
            #source_wide_predicate,
            #(#source_data_ctype_bounds,)*
            #data_bound
            #source_data_ctype_sized_bound,
            #ctype_wide_predicate,
            #predicates
        {
            type Data = #data_ty;
            type Metadata = <#field_ty as co3::wide::Wide>::Metadata;

            #methods
            #alloc_methods
        }
    }
}

fn derive_repr_c_data_enum_ctype(
    tag_type: syn::Type,
    alignment: Option<&syn::LitInt>,
    vis: &syn::Visibility,
    name: &syn::Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> TokenStream {
    let payload_name = format_ident!("{name}Payload");

    let (payload_def, variant_structs) =
        gen_data_enum_union(None, None, vis, &payload_name, name, generics, variants);

    let payload_impls = gen_union_ctype_impls(&payload_def, None);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let ctype_name = gen_ctype_name(name);
    let ctype_bounds = gen_union_extern_c_bounds(generics, variants);

    let repr = gen_ctype_repr_attr(Some(&ReprKind::C(None)), alignment);
    let ctype_def: syn::ItemStruct = parse_quote! {
        #[doc(hidden)]
        #repr
        #vis struct #ctype_name #impl_generics
        where
            #(#ctype_bounds,)*
            #predicates
        {
            tag: #tag_type,
            payload: #payload_name #ty_generics,
        }
    };

    let ctype_impls = gen_struct_ctype_impls::<true, false>(&ctype_def, true, alignment);
    let partial_eq_impl = gen_tagged_payload_partial_eq(&ctype_def, &payload_def, variants);

    quote! {
        #(#variant_structs)*

        #payload_def
        #payload_impls

        #ctype_def
        #ctype_impls
        #partial_eq_impl
    }
}

fn gen_tagged_payload_partial_eq(
    ctype: &syn::ItemStruct,
    payload: &syn::ItemUnion,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> TokenStream {
    let name = &ctype.ident;
    let (impl_generics, ty_generics, where_clause) = ctype.generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|clause| &clause.predicates);
    let bounds = payload.fields.named.iter().map(|field| {
        let ty = &field.ty;
        quote!(#ty: core::cmp::PartialEq)
    });
    let arms = variants.iter().enumerate().map(|(index, variant)| {
        let tag = Literal::usize_unsuffixed(index);
        let member = &variant.ident;
        quote!(#tag => unsafe { self.payload.#member == other.payload.#member })
    });

    quote! {
        impl #impl_generics core::cmp::PartialEq for #name #ty_generics
        where
            #(#bounds,)*
            #predicates
        {
            fn eq(&self, other: &Self) -> bool {
                self.tag == other.tag && match self.tag as usize {
                    #(#arms,)*
                    _ => true,
                }
            }
        }
    }
}

fn gen_ctype_struct_item<const ADD_COPY: bool>(
    repr: Option<&ReprKind>,
    alignment: Option<&syn::LitInt>,
    vis: &syn::Visibility,
    name: &syn::Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
) -> syn::ItemStruct {
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let ctype_name = gen_ctype_name(name);
    let repr = gen_ctype_repr_attr(repr, alignment);

    let field_types = fields.iter().map(|f| &f.ty).collect::<Vec<_>>();
    let extern_c_bounds = gen_extern_c_bounds_for_ctype::<ADD_COPY>(generics, &field_types);

    let field_c_tys = fields
        .iter()
        .map(|field| lowered_field_ty(&field.ty))
        .collect::<Vec<_>>();

    let ctype = match fields {
        syn::Fields::Named(_) | syn::Fields::Unit => {
            let field_names = fields.iter().map(|field| {
                field
                    .ident
                    .as_ref()
                    .expect("named CType field must have an identifier")
            });

            quote! {
                struct #ctype_name #impl_generics
                where
                    #(#extern_c_bounds,)*
                    #predicates
                {
                    #(pub #field_names: #field_c_tys),*
                }
            }
        }
        syn::Fields::Unnamed(_) => quote! {
            struct #ctype_name #impl_generics (#(pub #field_c_tys),*)
            where
                #(#extern_c_bounds,)*
                #predicates;
        },
    };

    parse_quote! {
        #[doc(hidden)]
        #repr
        #vis #ctype
    }
}

fn gen_data_enum_union(
    variant_tag: Option<syn::Type>,
    alignment: Option<&syn::LitInt>,
    vis: &syn::Visibility,
    union_name: &syn::Ident,
    enum_name: &syn::Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> (syn::ItemUnion, Vec<TokenStream>) {
    let (variant_generics, union_fields) = gen_union_fields(enum_name, generics, variants);

    let variant_structs = variants
        .iter()
        .zip(&variant_generics)
        .map(|(variant, generics)| {
            gen_variant_struct(variant_tag.as_ref(), vis, enum_name, generics, variant)
        });

    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let union_extern_c_bounds = gen_union_extern_c_bounds(generics, variants);

    let repr = gen_ctype_repr_attr(Some(&ReprKind::C(None)), alignment);
    let union_def = parse_quote! {
        #[expect(non_snake_case)]
        #[doc(hidden)]
        #repr
        #vis union #union_name #impl_generics
        where
            #(#union_extern_c_bounds,)*
            #predicates
        #union_fields
    };

    (union_def, variant_structs.collect())
}

fn gen_variant_struct(
    tag_type: Option<&syn::Type>,
    vis: &syn::Visibility,
    enum_name: &syn::Ident,
    generics: &syn::Generics,
    variant: &syn::Variant,
) -> TokenStream {
    let name = format_ident!("{enum_name}{}", &variant.ident);

    let repr = Some(&ReprKind::C(None));
    let mut fields = variant.fields.clone();

    if let Some(tag_type) = tag_type {
        match &mut fields {
            syn::Fields::Unit => {
                fields = syn::Fields::Named(parse_quote!({ tag: #tag_type }));
            }
            syn::Fields::Named(fields) => {
                fields.named.insert(0, parse_quote!(tag: #tag_type));
            }
            syn::Fields::Unnamed(fields) => {
                fields.unnamed.insert(0, parse_quote!(#tag_type));
            }
        }
    }

    let ctype = gen_ctype_struct_item::<true>(repr, None, vis, &name, generics, &fields);
    let ctype_impls = gen_struct_ctype_impls::<true, true>(&ctype, true, None);
    quote! { #ctype #ctype_impls }
}

fn gen_union_fields(
    name: &syn::Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> (Vec<syn::Generics>, syn::FieldsNamed) {
    let filtered_generics = variants
        .iter()
        .map(|variant| {
            let variant_fields = variant.fields.iter().map(|f| &f.ty).collect::<Vec<_>>();
            filter_generics(generics, &variant_fields)
        })
        .collect::<Vec<_>>();

    let named =
        variants
            .iter()
            .zip(&filtered_generics)
            .map(|(variant, variant_generics)| -> syn::Field {
                let variant_name = &variant.ident;
                let (_, ty_generics, _) = variant_generics.split_for_impl();
                let variant_struct_name = gen_variant_struct_name(name, variant_name);
                parse_quote!(#variant_name: #variant_struct_name #ty_generics)
            });

    let fields = syn::FieldsNamed {
        brace_token: Default::default(),
        named: named.collect(),
    };

    (filtered_generics, fields)
}

fn gen_union_extern_c_bounds(
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> impl Iterator<Item = syn::WherePredicate> {
    let mut seen = HashSet::new();

    let unique_types = variants
        .iter()
        .flat_map(|v| v.fields.iter().map(|f| &f.ty))
        .filter(|&ty| seen.insert(ty))
        .collect::<Vec<_>>();

    gen_extern_c_bounds_for_ctype::<true>(generics, &unique_types)
        .into_iter()
        .map(|p| {
            let predicate: syn::WherePredicate = parse_quote!(#p);
            predicate
        })
}

fn gen_struct_ctype_impls<const ADD_COPY: bool, const GEN_PARTIAL_EQ: bool>(
    ctype: &syn::ItemStruct,
    generate_views_and_spec: bool,
    alignment: Option<&syn::LitInt>,
) -> TokenStream {
    let fields = ctype.fields.iter().map(|f| &f.ty).collect::<Vec<_>>();
    let all_phantom_data = !fields.is_empty() && fields.iter().all(|ty| is_phantom_data(ty));
    let generate_views = generate_views_and_spec && !all_phantom_data;

    let copy_impls = gen_copy_impls::<ADD_COPY>(&ctype.ident, &ctype.generics, &fields);
    let partial_eq_impl = GEN_PARTIAL_EQ.then(|| gen_struct_partial_eq(ctype));
    let default_impl = gen_default_impl::<ADD_COPY>(&ctype.ident, &ctype.generics, &fields);
    let robust_impls = gen_robust_impls::<ADD_COPY>(&ctype.ident, &ctype.generics, &fields);
    let type_spec_impl = generate_views_and_spec
        .then(|| gen_type_spec(&ctype.ident, &ctype.generics, &fields, alignment));
    let const_view =
        generate_views.then(|| gen_ctype_struct_view::<ADD_COPY>(ctype.clone(), false, alignment));
    let mut_view =
        generate_views.then(|| gen_ctype_struct_view::<ADD_COPY>(ctype.clone(), true, alignment));

    let borrow_cast_impl = if generate_views {
        gen_borrow_cast_impl::<ADD_COPY>(&ctype.ident, &ctype.generics, &fields)
    } else if all_phantom_data {
        gen_identity_borrow_cast_impl(&ctype.ident, &ctype.generics)
    } else {
        quote! {}
    };

    quote! {
        #copy_impls
        #partial_eq_impl
        #default_impl
        #robust_impls
        #type_spec_impl

        #const_view
        #mut_view

        #borrow_cast_impl
    }
}

fn gen_struct_partial_eq(ctype: &syn::ItemStruct) -> TokenStream {
    let name = &ctype.ident;
    let (impl_generics, ty_generics, where_clause) = ctype.generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|clause| &clause.predicates);
    let bounds = ctype.fields.iter().flat_map(|field| {
        let ty = &field.ty;
        if is_type_parametrized(ty, &ctype.generics) {
            vec![parse_quote!(#ty: core::cmp::PartialEq)]
        } else {
            gen_hrtb_projection_bounds(ty, &ctype.generics, quote!(core::cmp::PartialEq))
        }
    });
    let comparisons = ctype
        .fields
        .members()
        .map(|member| quote!(self.#member == other.#member));

    quote! {
        impl #impl_generics core::cmp::PartialEq for #name #ty_generics
        where
            #(#bounds,)*
            #predicates
        {
            fn eq(&self, other: &Self) -> bool {
                true #(&& #comparisons)*
            }
        }
    }
}

fn gen_union_ctype_impls(ctype: &syn::ItemUnion, alignment: Option<&syn::LitInt>) -> TokenStream {
    let fields = ctype.fields.named.iter().map(|f| &f.ty).collect::<Vec<_>>();

    let copy_impls = gen_copy_impls::<true>(&ctype.ident, &ctype.generics, &fields);
    let default_impl = gen_default_impl::<true>(&ctype.ident, &ctype.generics, &fields);
    let robust_impls = gen_robust_impls::<true>(&ctype.ident, &ctype.generics, &fields);
    let type_spec_impl = gen_type_spec(&ctype.ident, &ctype.generics, &fields, alignment);
    let const_view = gen_ctype_union_view(ctype.clone(), false, alignment);
    let mut_view = gen_ctype_union_view(ctype.clone(), true, alignment);

    let borrow_cast_impl = gen_borrow_cast_impl::<true>(&ctype.ident, &ctype.generics, &fields);

    quote! {
        #copy_impls
        #default_impl
        #robust_impls
        #type_spec_impl

        #const_view
        #mut_view

        #borrow_cast_impl
    }
}

fn gen_robust_impls<const ADD_COPY: bool>(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> TokenStream {
    let co3 = co3_path();
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let mut c_fn_generics = generics.clone();
    c_fn_generics.params.insert(0, parse_quote!(__Co3Abi));
    let (c_fn_impl_generics, _, _) = c_fn_generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let copy_bounds = gen_copy_bounds::<ADD_COPY>(generics, fields);
    let codec_impls = gen_identity_codec_impls::<ADD_COPY>(ident, generics, fields, true);

    let for_dummy = (generics.type_params().count() == 0).then_some(quote! { for<'_dummy> });

    let type_spec_bound = (!ADD_COPY).then(|| {
        quote! { #for_dummy Self: #co3::rust_spec::RustSpec<Size = #co3::rust_spec::size::Sized<#co3::rust_spec::Gt<#co3::rust_spec::Zero>>>, }
    });

    quote! {
        unsafe impl #impl_generics #co3::ReprC for #ident #ty_generics #where_clause {}

        unsafe impl #c_fn_impl_generics #co3::CFnArg<__Co3Abi> for #ident #ty_generics
        where
            #type_spec_bound
            #(#copy_bounds,)*
            #predicates
        {}
        unsafe impl #c_fn_impl_generics #co3::CFnReturn<__Co3Abi> for #ident #ty_generics
        where
            #type_spec_bound
            #(#copy_bounds,)*
            #predicates
        {}

        unsafe impl #impl_generics #co3::transmute::CheckedTransmute for #ident #ty_generics #where_clause {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        #codec_impls
    }
}

// TODO: This is duplicated from rust-spec. There is some complicated logic here.
// Maybe we can use some derive macro attribute in the aformentioned crate to dedup?
fn gen_type_spec(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
    repr_alignment: Option<&syn::LitInt>,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let (size, size_bounds) = gen_field_family(
        generics,
        fields,
        quote! { core::ops::Add },
        quote! { Size },
        quote! { co3::rust_spec::size::Sized<co3::rust_spec::Zero> },
    );
    let (mut alignment, alignment_bounds) = gen_field_family(
        generics,
        fields,
        quote! { co3::rust_spec::Max },
        quote! { Alignment },
        quote! { co3::rust_spec::One },
    );
    if repr_alignment.is_some_and(|alignment| alignment.base10_digits() != "1") {
        alignment = quote!(co3::rust_spec::Gt<co3::rust_spec::One>);
    }
    let rust_spec_bounds = fields
        .iter()
        .filter(|field| !is_type_parametrized(field, generics))
        .flat_map(|field| {
            gen_hrtb_projection_bounds(field, generics, quote! { co3::rust_spec::RustSpec })
        });

    quote! {
        unsafe impl #impl_generics co3::rust_spec::RustSpec for #ident #ty_generics
        where
            #(#rust_spec_bounds,)*
            #(#size_bounds,)*
            #(#alignment_bounds,)*
            #predicates
        {
            type Layout = co3::rust_spec::Stable;
            type Size = #size;
            type Alignment = #alignment;
            type Trap = co3::rust_spec::layout::Robust;
            type Niche = co3::rust_spec::niche::WithoutNiche;
            type Mutability = co3::rust_spec::mutability::Exclusive;
            type __IndirectTrap = co3::rust_spec::layout::Robust;
        }
    }
}

fn gen_field_family(
    generics: &syn::Generics,
    fields: &[&syn::Type],
    operator: TokenStream,
    axis: TokenStream,
    identity: TokenStream,
) -> (TokenStream, Vec<TokenStream>) {
    let (concrete_family, parametrized_family, mut bounds) =
        gen_field_family_parts(operator.clone(), axis, generics, fields);

    let family = match (concrete_family, parametrized_family) {
        (Some(concrete), Some(parametrized)) => {
            bounds.push(quote! { #concrete: #operator<#parametrized> });
            quote! { <#concrete as #operator<#parametrized>>::Output }
        }
        (Some(concrete), None) => concrete,
        (None, Some(parametrized)) => parametrized,
        (None, None) => identity,
    };

    (family, bounds)
}

fn gen_field_family_parts(
    operator: TokenStream,
    axis: TokenStream,
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> (Option<TokenStream>, Option<TokenStream>, Vec<TokenStream>) {
    let mut bounds = Vec::new();
    let (concrete_fields, parametrized_fields): (Vec<_>, Vec<_>) = fields
        .iter()
        .copied()
        .partition(|field| !is_type_parametrized(field, generics));

    let mut concrete_fields = concrete_fields.into_iter();
    let concrete_family = concrete_fields.next().map(|first| {
        let mut family = quote! { <#first as co3::rust_spec::RustSpec>::#axis };

        for field in concrete_fields {
            let field_family = quote! { <#field as co3::rust_spec::RustSpec>::#axis };
            family = quote! { <#family as #operator<#field_family>>::Output };
        }

        family
    });

    let mut parametrized_fields = parametrized_fields.into_iter();
    let parametrized_family = parametrized_fields.next().map(|first| {
        bounds.push(quote! { #first: co3::rust_spec::RustSpec });
        let mut family = quote! { <#first as co3::rust_spec::RustSpec>::#axis };

        for field in parametrized_fields {
            let field_family = quote! { <#field as co3::rust_spec::RustSpec>::#axis };
            bounds.push(quote! { #field: co3::rust_spec::RustSpec });
            bounds.push(quote! { #family: #operator<#field_family> });
            family = quote! { <#family as #operator<#field_family>>::Output };
        }

        family
    });

    (concrete_family, parametrized_family, bounds)
}

fn gen_identity_codec_impls<const ADD_COPY: bool>(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
    require_copy: bool,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let mut decode_generics = generics.clone();
    decode_generics.params.insert(0, parse_quote!('d));
    let (decode_impl_generics, _, _) = decode_generics.split_for_impl();

    let copy_bounds = if require_copy {
        gen_copy_bounds::<ADD_COPY>(generics, fields)
    } else {
        Vec::new()
    };
    let sized_bound = (!require_copy).then(|| quote!(for<'_dummy> Self: Sized,));

    quote! {
        impl #impl_generics co3::ExternC for #ident #ty_generics
        where
            #predicates
        {
            type CType = Self;
        }
        unsafe impl #impl_generics co3::stored::EncodeOwned for #ident #ty_generics
        where
            #sized_bound
            #(#copy_bounds,)*
            #predicates
        {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                self
            }
        }
        unsafe impl #decode_impl_generics co3::stored::DecodeOwned<'d> for #ident #ty_generics
        where
            #sized_bound
            #(#copy_bounds,)*
            #predicates
        {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Some(source)
            }
        }

        impl #impl_generics co3::Encode for #ident #ty_generics
        where
            #sized_bound
            #(#copy_bounds,)*
            #predicates
        {}
        impl #impl_generics co3::Decode<'_> for #ident #ty_generics
        where
            #sized_bound
            #(#copy_bounds,)*
            #predicates
        {}
    }
}

pub(super) fn gen_identity_repr_c_impls(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let mut c_fn_generics = generics.clone();
    c_fn_generics.params.insert(0, parse_quote!(__Co3Abi));
    let (c_fn_impl_generics, _, _) = c_fn_generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|clause| &clause.predicates);
    let codec_impls = gen_identity_codec_impls::<false>(ident, generics, fields, false);

    quote! {
        unsafe impl #impl_generics co3::ReprC for #ident #ty_generics #where_clause {}

        unsafe impl #c_fn_impl_generics co3::CFnArg<__Co3Abi> for #ident #ty_generics
        where
            for<'_dummy> Self: Copy + co3::rust_spec::RustSpec<
                Size = co3::rust_spec::size::Sized<
                    co3::rust_spec::Gt<co3::rust_spec::Zero>
                >
            >,
            #predicates
        {}
        unsafe impl #c_fn_impl_generics co3::CFnReturn<__Co3Abi> for #ident #ty_generics
        where
            for<'_dummy> Self: Copy + co3::rust_spec::RustSpec<
                Size = co3::rust_spec::size::Sized<
                    co3::rust_spec::Gt<co3::rust_spec::Zero>
                >
            >,
            #predicates
        {}

        unsafe impl #impl_generics co3::transmute::CheckedTransmute for #ident #ty_generics
        #where_clause
        {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        #codec_impls
    }
}

fn gen_borrow_cast_impl<const ADD_COPY: bool>(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let const_view_name = gen_const_view_name(ident);
    let mut_view_name = gen_mut_view_name(ident);

    let const_bounds = gen_ctype_borrow_cast_bounds::<ADD_COPY>(generics, fields, false);
    let mut_bounds = gen_ctype_borrow_cast_bounds::<ADD_COPY>(generics, fields, true);

    quote! {
        unsafe impl #impl_generics co3::borrow::BorrowCast for #ident #ty_generics
        where
            #(#const_bounds,)*
            #predicates
        {
            type AsConst = #const_view_name #ty_generics;
        }

        unsafe impl #impl_generics co3::borrow::BorrowCastMut for #ident #ty_generics
        where
            #(#mut_bounds,)*
            #predicates
        {
            type AsMut = #mut_view_name #ty_generics;
        }
    }
}

pub(super) fn gen_identity_borrow_cast_impl(
    ident: &syn::Ident,
    generics: &syn::Generics,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        unsafe impl #impl_generics co3::borrow::BorrowCast for #ident #ty_generics #where_clause {
            type AsConst = Self;
        }

        unsafe impl #impl_generics co3::borrow::BorrowCastMut for #ident #ty_generics #where_clause {
            type AsMut = Self;
        }
    }
}

fn gen_ctype_repr_attr(repr: Option<&ReprKind>, alignment: Option<&syn::LitInt>) -> TokenStream {
    let kind = if repr == Some(&ReprKind::Transparent) {
        quote!(transparent)
    } else {
        quote!(C)
    };
    let alignment = alignment.map(|alignment| quote!(, align(#alignment)));
    quote! { #[repr(#kind #alignment)] }
}

fn gen_ctype_struct_view<const ADD_COPY: bool>(
    mut ctype: syn::ItemStruct,
    is_mut: bool,
    alignment: Option<&syn::LitInt>,
) -> TokenStream {
    rewrite_ctype_view_struct::<ADD_COPY>(&mut ctype, is_mut);

    let fields = ctype
        .fields
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();

    let copy_impls = gen_copy_impls::<ADD_COPY>(&ctype.ident, &ctype.generics, &fields);
    let default_impl = gen_default_impl::<ADD_COPY>(&ctype.ident, &ctype.generics, &fields);
    let robust_impls = gen_robust_impls::<ADD_COPY>(&ctype.ident, &ctype.generics, &fields);
    let type_spec_impl = gen_type_spec(&ctype.ident, &ctype.generics, &fields, alignment);

    quote! {
        #ctype
        #copy_impls
        #default_impl
        #robust_impls
        #type_spec_impl
    }
}

fn gen_ctype_union_view(
    mut ctype: syn::ItemUnion,
    is_mut: bool,
    alignment: Option<&syn::LitInt>,
) -> TokenStream {
    rewrite_ctype_view_union(&mut ctype, is_mut);

    let fields = ctype
        .fields
        .named
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();

    let copy_impls = gen_copy_impls::<true>(&ctype.ident, &ctype.generics, &fields);
    let default_impl = gen_default_impl::<true>(&ctype.ident, &ctype.generics, &fields);
    let robust_impls = gen_robust_impls::<true>(&ctype.ident, &ctype.generics, &fields);
    let type_spec_impl = gen_type_spec(&ctype.ident, &ctype.generics, &fields, alignment);

    quote! {
        #ctype
        #copy_impls
        #default_impl
        #robust_impls
        #type_spec_impl
    }
}

fn rewrite_ctype_view_struct<const ADD_COPY: bool>(ctype: &mut syn::ItemStruct, is_mut: bool) {
    rewrite_ctype_view_name(&mut ctype.ident, is_mut);
    rewrite_ctype_view_generics::<ADD_COPY>(
        &mut ctype.generics,
        ctype.fields.iter_mut().map(|field| &mut field.ty),
        is_mut,
    );
}

fn rewrite_ctype_view_union(ctype: &mut syn::ItemUnion, is_mut: bool) {
    rewrite_ctype_view_name(&mut ctype.ident, is_mut);
    rewrite_ctype_view_generics::<true>(
        &mut ctype.generics,
        ctype.fields.named.iter_mut().map(|field| &mut field.ty),
        is_mut,
    );
}

fn rewrite_ctype_view_name(ident: &mut syn::Ident, is_mut: bool) {
    *ident = if is_mut {
        gen_mut_view_name(ident)
    } else {
        gen_const_view_name(ident)
    };
}

fn rewrite_ctype_view_generics<'a, const ADD_COPY: bool>(
    generics: &mut syn::Generics,
    fields: impl Iterator<Item = &'a mut syn::Type>,
    is_mut: bool,
) {
    let field_tys = rewrite_ctype_view_field_tys(fields, is_mut);
    let field_tys = field_tys.iter().collect::<Vec<_>>();

    for bound in gen_ctype_borrow_cast_bounds::<ADD_COPY>(generics, &field_tys, is_mut) {
        generics.make_where_clause().predicates.push(parse_quote! {
            #bound
        });
    }
}

fn rewrite_ctype_view_field_tys<'a>(
    fields: impl Iterator<Item = &'a mut syn::Type>,
    is_mut: bool,
) -> Vec<syn::Type> {
    fields
        .map(|field_ty| {
            let ty = field_ty.clone();

            if is_phantom_data(&ty) {
                return ty;
            }

            let (borrow_cast_trait, view_ty) = if is_mut {
                (quote!(co3::borrow::BorrowCastMut), quote!(AsMut))
            } else {
                (quote!(co3::borrow::BorrowCast), quote!(AsConst))
            };
            *field_ty = parse_quote! { <#ty as #borrow_cast_trait>::#view_ty };
            ty
        })
        .collect()
}

fn gen_copy_impls<const ADD_COPY: bool>(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let copy_bounds = gen_copy_bounds::<ADD_COPY>(generics, fields);

    quote! {
        impl #impl_generics Clone for #ident #ty_generics
        where
            #(#copy_bounds,)*
            #predicates
        {
            fn clone(&self) -> Self {
                *self
            }
        }

        impl #impl_generics Copy for #ident #ty_generics
        where
            #(#copy_bounds,)*
            #predicates
        {}
    }
}

fn gen_copy_bounds<const ADD_COPY: bool>(
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> Vec<TokenStream> {
    let Some((last, fields)) = fields.split_last() else {
        return Vec::new();
    };

    let mut predicates = fields
        .iter()
        .filter(|ty| is_type_parametrized(ty, generics))
        .map(|&ty| parse_quote! { #ty: Copy })
        .collect::<Vec<_>>();

    if is_type_parametrized(last, generics) {
        predicates.push(quote!(#last: Copy));
    } else if !ADD_COPY {
        predicates.push(quote!(for<'_dummy> #last: Copy));
    }

    predicates
}

fn gen_default_impl<const ADD_COPY: bool>(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let copy_bounds = gen_copy_bounds::<ADD_COPY>(generics, fields);

    quote! {
        impl #impl_generics Default for #ident #ty_generics
        where
            #(#copy_bounds,)*
            #predicates
        {
            #[inline(always)]
            fn default() -> Self {
                unsafe { core::mem::zeroed() }
            }
        }
    }
}

pub(super) fn gen_ctype_name(item_name: &syn::Ident) -> syn::Ident {
    format_ident!("C{item_name}")
}

fn gen_const_view_name(item_name: &syn::Ident) -> syn::Ident {
    format_ident!("{item_name}ConstView")
}

fn gen_mut_view_name(item_name: &syn::Ident) -> syn::Ident {
    format_ident!("{item_name}MutView")
}

pub(super) fn gen_variant_struct_name(
    enum_name: &syn::Ident,
    variant_name: &syn::Ident,
) -> syn::Ident {
    format_ident!("C{enum_name}{variant_name}")
}

struct UsedGenericsVisitor<'a> {
    generics: &'a syn::Generics,
    used_lifetimes: std::collections::HashSet<&'a syn::Ident>,
    used_type_params: std::collections::HashSet<&'a syn::Ident>,
    used_const_params: std::collections::HashSet<&'a syn::Ident>,
}

impl<'a> UsedGenericsVisitor<'a> {
    fn new(generics: &'a syn::Generics) -> Self {
        Self {
            generics,
            used_lifetimes: std::collections::HashSet::new(),
            used_type_params: std::collections::HashSet::new(),
            used_const_params: std::collections::HashSet::new(),
        }
    }
}

impl<'a> Visit<'_> for UsedGenericsVisitor<'a> {
    fn visit_lifetime(&mut self, lifetime: &syn::Lifetime) {
        for lt in self.generics.lifetimes() {
            if lt.lifetime.ident == lifetime.ident {
                self.used_lifetimes.insert(&lt.lifetime.ident);
            }
        }

        syn::visit::visit_lifetime(self, lifetime);
    }

    fn visit_type_path(&mut self, type_path: &syn::TypePath) {
        if let Some(ident) = type_path.path.get_ident() {
            for tp in self.generics.type_params() {
                if &tp.ident == ident {
                    self.used_type_params.insert(&tp.ident);
                }
            }
            for cp in self.generics.const_params() {
                if &cp.ident == ident {
                    self.used_const_params.insert(&cp.ident);
                }
            }
        }

        syn::visit::visit_type_path(self, type_path);
    }

    fn visit_expr_path(&mut self, expr_path: &syn::ExprPath) {
        if let Some(ident) = expr_path.path.get_ident() {
            for cp in self.generics.const_params() {
                if &cp.ident == ident {
                    self.used_const_params.insert(&cp.ident);
                }
            }
        }

        syn::visit::visit_expr_path(self, expr_path);
    }
}

pub(super) fn filter_generics(
    generics: &syn::Generics,
    field_types: &[&syn::Type],
) -> syn::Generics {
    let mut visitor = UsedGenericsVisitor::new(generics);

    for ty in field_types {
        visitor.visit_type(ty);
    }

    let mut filtered = syn::Generics {
        params: generics
            .params
            .iter()
            .filter(|param| match param {
                syn::GenericParam::Lifetime(lt) => {
                    visitor.used_lifetimes.contains(&lt.lifetime.ident)
                }
                syn::GenericParam::Type(tp) => visitor.used_type_params.contains(&tp.ident),
                syn::GenericParam::Const(cp) => visitor.used_const_params.contains(&cp.ident),
            })
            .cloned()
            .collect(),
        ..Default::default()
    };

    if let Some(where_clause) = &generics.where_clause {
        let retained_lifetimes = filtered
            .lifetimes()
            .map(|lt| lt.lifetime.ident.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let retained_type_params = filtered
            .type_params()
            .map(|tp| tp.ident.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let retained_const_params = filtered
            .const_params()
            .map(|cp| cp.ident.clone())
            .collect::<std::collections::BTreeSet<_>>();

        let predicates = where_clause
            .predicates
            .iter()
            .filter(|predicate| {
                let mut predicate_visitor = UsedGenericsVisitor::new(generics);
                predicate_visitor.visit_where_predicate(predicate);

                predicate_visitor
                    .used_lifetimes
                    .into_iter()
                    .all(|ident| retained_lifetimes.contains(ident))
                    && predicate_visitor
                        .used_type_params
                        .into_iter()
                        .all(|ident| retained_type_params.contains(ident))
                    && predicate_visitor
                        .used_const_params
                        .into_iter()
                        .all(|ident| retained_const_params.contains(ident))
            })
            .cloned()
            .collect::<syn::punctuated::Punctuated<_, syn::token::Comma>>();

        filtered.where_clause = (!predicates.is_empty()).then_some(syn::WhereClause {
            where_token: where_clause.where_token,
            predicates,
        });
    }

    filtered
}

pub(super) fn gen_extern_c_bounds_for_ctype<const ADD_COPY: bool>(
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> Vec<syn::WherePredicate> {
    let gen_bound = |ty: &syn::Type, ctype_bound: TokenStream| {
        if is_phantom_data(ty) {
            return vec![];
        }

        if is_type_parametrized(ty, generics) {
            vec![parse_quote! { #ty: co3::ExternC #ctype_bound }]
        } else {
            gen_hrtb_projection_bounds(ty, generics, quote! { co3::ExternC #ctype_bound })
        }
    };

    let Some((last, fields)) = fields.split_last() else {
        return Vec::new();
    };

    let (field_ctype_bound, last_ctype_bound) = if ADD_COPY {
        (quote!(<CType: Copy>), quote!(<CType: Copy>))
    } else {
        (quote!(<CType: Sized>), quote!())
    };

    fields
        .iter()
        .flat_map(|&ty| gen_bound(ty, field_ctype_bound.clone()))
        .chain(gen_bound(last, last_ctype_bound))
        .collect::<Vec<_>>()
}

fn gen_hrtb_projection_bounds(
    ty: &syn::Type,
    generics: &syn::Generics,
    bound: TokenStream,
) -> Vec<syn::WherePredicate> {
    #[derive(Default)]
    struct ProjectionVisitor<'a> {
        projections: Vec<&'a syn::TypePath>,
    }

    impl<'ast> Visit<'ast> for ProjectionVisitor<'ast> {
        fn visit_type_path(&mut self, type_path: &'ast syn::TypePath) {
            if type_path.qself.is_some() {
                self.projections.push(type_path);
                for segment in &type_path.path.segments {
                    self.visit_path_arguments(&segment.arguments);
                }
                return;
            }
            syn::visit::visit_type_path(self, type_path);
        }
    }

    let mut visitor = ProjectionVisitor::default();
    visitor.visit_type(ty);

    let mut bounds = Vec::new();
    for projection in visitor.projections {
        let Some(qself) = projection.qself.as_ref() else {
            continue;
        };
        let trait_path = syn::Path {
            leading_colon: projection.path.leading_colon,
            segments: projection
                .path
                .segments
                .iter()
                .take(qself.position)
                .cloned()
                .collect(),
        };
        for predicate in generics
            .where_clause
            .iter()
            .flat_map(|where_clause| &where_clause.predicates)
        {
            let syn::WherePredicate::Type(predicate) = predicate else {
                continue;
            };
            let Some(lifetimes) = &predicate.lifetimes else {
                continue;
            };
            let bounded_ty = &predicate.bounded_ty;
            let projected_ty = &qself.ty;
            if quote!(#bounded_ty).to_string() != quote!(#projected_ty).to_string() {
                continue;
            }

            for prerequisite in &predicate.bounds {
                let syn::TypeParamBound::Trait(trait_bound) = prerequisite else {
                    continue;
                };
                let predicate_trait_path = &trait_bound.path;
                if quote!(#predicate_trait_path).to_string() != quote!(#trait_path).to_string() {
                    continue;
                }

                let generated: syn::WherePredicate =
                    parse_quote! { #lifetimes #projection: #bound };
                bounds.push(generated);
            }
        }
    }

    bounds
}

fn gen_ctype_borrow_cast_bounds<const ADD_COPY: bool>(
    generics: &syn::Generics,
    fields: &[&syn::Type],
    is_mut: bool,
) -> Vec<TokenStream> {
    let (borrow_cast_trait, assoc_type) = if is_mut {
        (quote! { co3::borrow::BorrowCastMut }, quote!(AsMut))
    } else {
        (quote! { co3::borrow::BorrowCast }, quote!(AsConst))
    };

    let Some((last, fields)) = fields.split_last() else {
        return Vec::new();
    };

    let mut predicates = fields
        .iter()
        .filter(|ty| !is_phantom_data(ty) && is_type_parametrized(ty, generics))
        .map(|&ty| {
            let ctype_bound = if ADD_COPY {
                quote! { <#assoc_type: Copy> }
            } else {
                quote! { <#assoc_type: Sized> }
            };

            parse_quote! { #ty: #borrow_cast_trait #ctype_bound }
        })
        .collect::<Vec<_>>();

    if !is_phantom_data(last) && is_type_parametrized(last, generics) {
        let copy_bound = ADD_COPY.then(|| quote! { <#assoc_type: Copy> });
        predicates.push(parse_quote! { #last: #borrow_cast_trait #copy_bound });
    }

    predicates
}
