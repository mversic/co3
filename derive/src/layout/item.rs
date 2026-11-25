use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, parse_quote};

use crate::{
    codec::Conversion,
    layout::{
        ReprCAttrs, VariantReprCAttrs,
        attr::ReprKind,
        borrow::gen_identity_borrow_impls,
        ctype::{
            classify_tail, gen_ctype_name, gen_fieldless_enum_ctype, gen_identity_repr_c_impls,
            gen_item_ctype, gen_variant_struct_name,
        },
        enum_tag_type, enum_variant_tag_exprs,
        fields::{FieldType, field_types},
        infer_repr,
        inference::{RecursiveCompatibility, RecursiveFields},
        is_exhaustive_enum, is_phantom_data, is_transparent_enum_repr,
        niche::{gen_enum_niche_ir, gen_struct_niche_ir},
        primitive_tag_type,
        reference::{RefEncoder, encoder_output},
    },
    utils::speculative_marker,
};

pub(super) fn derive_item(
    repr: Option<&ReprKind>,
    alignment: Option<&syn::LitInt>,
    input: &syn::DeriveInput,
    attrs: &ReprCAttrs,
    variant_attrs: &[VariantReprCAttrs],
    recursion: &RecursiveFields,
) -> TokenStream {
    if matches!(
        &input.data,
        syn::Data::Enum(data)
            if matches!(repr, Some(ReprKind::Transparent)) && data.variants.len() != 1
    ) {
        return quote! {};
    }

    let is_wide_data = attrs.is_wide_data;
    let has_unsized_tail = matches!(&input.data, syn::Data::Struct(data)
        if data.fields.iter().next_back().is_some_and(|field|
            classify_tail(&field.ty, &input.generics).is_known_unsized()));
    let sized_decode_impls = (!is_wide_data && !has_unsized_tail).then(|| {
        super::reference::gen_sized_decode_impls(
            input,
            repr,
            attrs,
            variant_attrs,
            &recursion.equalities,
        )
    });
    if attrs.is_identity {
        let fields = recursion.types();
        let name = &input.ident;
        let mut identity_generics = input.generics.clone();
        for field in &fields {
            identity_generics
                .make_where_clause()
                .predicates
                .push(parse_quote!(#field: co3::CType));
        }
        let repr_c_impls = gen_identity_repr_c_impls(name, &identity_generics, &fields);
        let borrow_impls = gen_identity_borrow_impls(name, &identity_generics);
        let mut stable_input = input.clone();
        stable_input.generics = identity_generics.clone();
        let stable_mut_impls = super::reference::gen_stable_direct_mut_impls(&stable_input);

        return quote! {
            #repr_c_impls
            #borrow_impls
            #stable_mut_impls
            #sized_decode_impls
        };
    }

    let mut bounded_impl_input = input.clone();
    if is_wide_data {
        let syn::Data::Struct(data) = &input.data else {
            unreachable!("wide data is a struct")
        };
        let header_ty = &data
            .fields
            .iter()
            .next_back()
            .expect("wide data has a tail")
            .ty;
        let bound =
            FieldType::new(header_ty, &input.generics).bound(quote!(co3::ReprC<CType: Sized>));
        bounded_impl_input
            .generics
            .make_where_clause()
            .predicates
            .push(bound);
    }
    let is_move = !is_wide_data
        && !matches!(&input.data, syn::Data::Struct(data) if data.fields.iter().next_back().is_some_and(
            |last| matches!(classify_tail(&last.ty, &input.generics), super::ctype::TailClass::Unsized(_))
        ));
    let ctype_input = if is_wide_data {
        &bounded_impl_input
    } else {
        input
    };
    let ctype_def = gen_item_ctype(
        repr,
        alignment,
        ctype_input,
        !is_wide_data,
        !is_wide_data,
        recursion,
    );
    // Apply the same source prerequisites used to verify an inferred carrier.
    // Keep them off the companion, whose layout must be independent of the source.
    if !recursion.source_bounds.is_empty() {
        bounded_impl_input
            .generics
            .make_where_clause()
            .predicates
            .extend(recursion.source_bounds.iter().cloned());
    }
    let reference_input = if is_wide_data {
        input
    } else {
        &bounded_impl_input
    };
    let codec_impls = gen_item_codec_impls(
        repr,
        &bounded_impl_input,
        reference_input,
        attrs,
        variant_attrs,
        is_move,
        recursion,
    );
    let niche_impls = gen_item_niche_impls(repr, &bounded_impl_input, attrs, recursion);
    let repr_c_impls = repr
        .is_some()
        .then(|| gen_item_repr_c_impls(repr, &bounded_impl_input, attrs, variant_attrs, recursion));

    quote! {
        #ctype_def

        #codec_impls
        #sized_decode_impls
        #niche_impls

        #repr_c_impls
    }
}

fn gen_item_niche_impls(
    repr: Option<&ReprKind>,
    input: &syn::DeriveInput,
    attrs: &ReprCAttrs,
    recursion: &RecursiveFields,
) -> TokenStream {
    match &input.data {
        syn::Data::Struct(data) => gen_struct_niche_ir(
            &input.ident,
            &input.generics,
            &data.fields,
            attrs.niche_value.as_ref(),
            &recursion.equalities,
        ),
        syn::Data::Enum(data) => gen_enum_niche_ir(
            repr,
            &input.ident,
            &input.generics,
            &data.variants,
            false,
            &recursion.equalities,
        ),
        syn::Data::Union(_) => unreachable!(),
    }
}

fn gen_item_codec_impls(
    repr: Option<&ReprKind>,
    input: &syn::DeriveInput,
    reference_input: &syn::DeriveInput,
    attrs: &ReprCAttrs,
    variant_attrs: &[VariantReprCAttrs],
    is_move: bool,
    recursion: &RecursiveFields,
) -> TokenStream {
    use super::aggregate::{Aggregate, Operation::*};
    let aggregate = Aggregate::new(input, repr, attrs, variant_attrs, recursion);
    let repr_impl = aggregate.repr();
    let mut impls = TokenStream::new();
    // The synthetic wide header has extra value bounds. Ordinary aggregates
    // use the same field analysis for values, borrowed carriers and references.
    let wide_reference = attrs
        .is_wide_data
        .then(|| Aggregate::new(reference_input, repr, attrs, variant_attrs, recursion));
    let reference = wide_reference.as_ref().unwrap_or(&aggregate);
    let borrowed = !attrs.is_wide_data
        && recursion.header_prefix.is_none()
        && !matches!(&reference_input.data,syn::Data::Struct(s) if s.fields.iter().any(|f|classify_tail(&f.ty,&reference_input.generics).is_known_unsized()));
    for contract in Conversion::derived(quote!(#input)) {
        for op in [Encode, Decode] {
            impls.extend(aggregate.emit(op, &contract, is_move));
        }
        if borrowed {
            for op in [BorrowedEncode, BorrowedDecode] {
                impls.extend(reference.emit(op, &contract, is_move));
            }
        }
    }
    let values = super::borrow::non_custom_drop_scope(impls);
    if recursion.header_prefix.is_some() {
        return quote!(#repr_impl #values);
    }
    let delegate = matches!(repr, Some(ReprKind::Transparent))
        && attrs.is_valid.is_none()
        && variant_attrs.iter().all(|a| a.is_valid.is_none());
    let field = if delegate {
        match &reference_input.data {
            syn::Data::Struct(s) if s.fields.len() == 1 => {
                Some((s.fields.iter().next().unwrap(), None))
            }
            syn::Data::Enum(e) if e.variants.len() == 1 && e.variants[0].fields.len() == 1 => {
                Some((
                    e.variants[0].fields.iter().next().unwrap(),
                    Some(&e.variants[0].ident),
                ))
            }
            _ => None,
        }
    } else {
        None
    };
    let references = if let Some((field, variant)) = field {
        super::reference::gen_transparent_ref_impls(reference_input, field, variant)
    } else {
        let helpers = super::reference::gen_shared_field_encoders();
        let shared = Conversion::derived(quote!(#reference_input))
            .into_iter()
            .map(|c| reference.reference(&c));
        let mutable =
            (!delegate).then(|| super::reference::gen_stable_direct_mut_impls(reference_input));
        quote!(const _: () = { #helpers #(#shared)* #mutable };)
    };
    quote!(#repr_impl #values #references)
}

fn gen_item_repr_c_impls(
    repr: Option<&ReprKind>,
    input: &syn::DeriveInput,
    attrs: &ReprCAttrs,
    variant_attrs: &[VariantReprCAttrs],
    recursion: &RecursiveFields,
) -> TokenStream {
    let [contract, _] = Conversion::derived(quote!(#input));
    match &input.data {
        syn::Data::Struct(data) => gen_repr_c_struct_impls(
            &contract,
            &input.ident,
            &input.generics,
            &data.fields,
            attrs.is_valid.as_ref(),
            attrs.niche_value.as_ref(),
            recursion,
        ),
        syn::Data::Enum(data) if is_transparent_enum_repr(repr, &data.variants) => {
            let variant = &data.variants[0];
            let is_valid = variant_attrs[0].is_valid.as_ref();
            let mut generics = input.generics.clone();
            if let Some(last) = variant.fields.iter().next_back() {
                let field = FieldType::new(&last.ty, &generics);
                if !field.phantom {
                    let bound = field.bound(quote!(co3::ReprC<CType: Sized>));
                    generics.make_where_clause().predicates.push(bound);
                }
            }

            gen_repr_c_struct_impls(
                &contract,
                &input.ident,
                &generics,
                &variant.fields,
                is_valid,
                None,
                recursion,
            )
        }
        syn::Data::Enum(data) => gen_repr_c_data_enum_impls(
            &contract,
            repr,
            &input.ident,
            &input.generics,
            &data.variants,
            variant_attrs,
            recursion,
        ),
        syn::Data::Union(_) => unreachable!(),
    }
}

fn gen_repr_c_struct_impls(
    contract: &Conversion,
    name: &Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
    is_valid: Option<&syn::ExprClosure>,
    niche_value: Option<&syn::Expr>,
    recursion: &RecursiveFields,
) -> TokenStream {
    let field_types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();
    let field_vars = field_vars(fields);

    let ctype_name = gen_ctype_name(name);
    let niche_validation = niche_value.map(|niche_value| {
        let niche_fields = match fields {
            syn::Fields::Named(_) | syn::Fields::Unit => fields
                .iter()
                .filter_map(|field| field.ident.as_ref())
                .map(|field_name| quote! { __co3_niche_value.#field_name })
                .collect::<Vec<_>>(),
            syn::Fields::Unnamed(_) => (0..fields.iter().count())
                .map(|idx| {
                    let idx = syn::Index::from(idx);
                    quote! { __co3_niche_value.#idx }
                })
                .collect::<Vec<_>>(),
        };

        quote! { && {
            let __co3_niche_value = #niche_value;
            #(*#field_vars != #niche_fields)||*
        }}
    });

    let destructure_target = gen_fields_destructure(fields);
    let is_valid_body = gen_record_is_valid(
        contract,
        &field_vars,
        &field_types,
        is_valid,
        niche_validation,
    );

    let is_valid_impl = quote! {
        let #ctype_name #destructure_target = target;
        #is_valid_body
    };

    gen_repr_c_impls(
        contract,
        name,
        generics,
        &field_types,
        recursion,
        is_valid_impl,
    )
}

fn gen_repr_c_data_enum_impls(
    contract: &Conversion,
    repr: Option<&ReprKind>,
    name: &Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
    variant_attrs: &[VariantReprCAttrs],
    recursion: &RecursiveFields,
) -> TokenStream {
    let tag_type = enum_tag_type(repr, variants.len());
    let tag_values = enum_variant_tag_exprs(variants, &tag_type);

    let fields = variants
        .iter()
        .flat_map(|variant| variant.fields.iter().map(|field| &field.ty))
        .collect::<Vec<_>>();

    let variant_body = variants.iter().enumerate().map(|(variant_idx, variant)| {
        let tag_value = &tag_values[variant_idx];

        let variant_name = &variant.ident;
        let variant_struct_name = gen_variant_struct_name(name, variant_name);

        let variant_fields = variant
            .fields
            .iter()
            .map(|field| &field.ty)
            .collect::<Vec<_>>();
        let variant_struct_type = quote! { #variant_struct_name };

        let field_names = field_vars(&variant.fields);
        let destructure_target = match repr {
            Some(ReprKind::C(Some(_))) => match &variant.fields {
                syn::Fields::Named(_) | syn::Fields::Unit => quote! {
                    let #variant_struct_type { #(#field_names),* } = unsafe {
                        &target.payload.assume_init_ref().#variant_name
                    };
                },
                syn::Fields::Unnamed(_) => quote! {
                    let #variant_struct_type(#(#field_names),*) = unsafe {
                        &target.payload.assume_init_ref().#variant_name
                    };
                },
            },
            _ => match &variant.fields {
                syn::Fields::Named(_) => quote! {
                    let #variant_struct_type { tag: _, #(#field_names),* } = unsafe {
                        &target.#variant_name
                    };
                },
                syn::Fields::Unnamed(_) => quote! {
                    let #variant_struct_type(_, #(#field_names),*) = unsafe {
                        &target.#variant_name
                    };
                },
                syn::Fields::Unit => quote! {
                    let #variant_struct_type { tag: _ } = unsafe {
                        &target.#variant_name
                    };
                },
            },
        };

        let is_valid = variant_attrs[variant_idx].is_valid.as_ref();
        let is_valid_body =
            gen_record_is_valid(contract, &field_names, &variant_fields, is_valid, None);
        let unwrap_fields = if matches!(repr, Some(ReprKind::C(Some(_)))) {
            Vec::new()
        } else {
            field_names
                .iter()
                .map(|field| {
                    quote! {
                        let #field = unsafe { #field.assume_init_ref() };
                    }
                })
                .collect::<Vec<_>>()
        };

        quote! {
            value if value == #tag_value => {
                #destructure_target
                #(#unwrap_fields)*
                #is_valid_body
            }
        }
    });

    let is_valid_impl = quote! {
        let repr_value = <*const _>::cast::<#tag_type>(core::ptr::from_ref(target));

        match unsafe { *repr_value } {
            #(#variant_body,)*
            _ => {  false },
        }
    };

    let mut bounded_generics = generics.clone();
    let predicates = &mut bounded_generics.make_where_clause().predicates;
    predicates.extend(gen_enum_union_copy_bounds(name, generics, variants));

    gen_repr_c_impls(
        contract,
        name,
        &bounded_generics,
        &fields,
        recursion,
        is_valid_impl,
    )
}

fn gen_enum_union_copy_bounds(
    enum_name: &Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> Vec<syn::WherePredicate> {
    super::ctype::gen_union_extern_c_bounds(enum_name, generics, variants)
}

fn gen_repr_c_impls(
    contract: &Conversion,
    name: &Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
    recursion: &RecursiveFields,
    is_valid_body: TokenStream,
) -> TokenStream {
    let g = contract.generics(generics);
    let (checked_generics, _, _) = g.split_for_impl();
    let lang = contract.language();
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let own = syn::parse_quote!(#name #ty_generics);
    let mut recursive_fields = Vec::new();
    let mut checked_transmute_bounds = Vec::new();
    let mut validation_bounds = Vec::new();
    for (index, field) in field_types(fields, generics).enumerate() {
        let ty = field.ty;
        if field.phantom {
            continue;
        }
        if let Some(residual) = &recursion.fields[index].compatibility {
            match residual {
                RecursiveCompatibility::Vector => {
                    // The stable vector depends on this impl. A native vector
                    // retains the full field bound.
                    let ty = parse_quote!(<#ty as __Co3SelectVec>::AbiRequirement);
                    let requirement = FieldType::new(&ty, generics);
                    checked_transmute_bounds
                        .push(requirement.bound(quote!(co3::marker::AbiCompatible)));
                    validation_bounds
                        .push(requirement.bound(quote!(co3::convert::AbiValidate<#lang>)));
                    if !field.concrete {
                        let mutability: syn::WherePredicate = parse_quote!(#own: co3::rust_spec::RustSpec<
                            Mutability: co3::rust_spec::mutability::PointerMutability>);
                        checked_transmute_bounds.push(mutability.clone());
                        validation_bounds.push(mutability);
                    }
                }
                RecursiveCompatibility::Bounds(bounds) => {
                    checked_transmute_bounds.extend(bounds.iter().cloned());
                    validation_bounds.extend(bounds.iter().cloned());
                }
            }
            recursive_fields.push(ty);
            continue;
        }
        // A delegated validator takes the source field's projected CType.
        // Its input must match the carrier stored in the companion.
        if let Some(Some(carrier)) = recursion.carriers.get(index) {
            validation_bounds.push(field.bound(quote!(co3::ReprC<CType = #carrier>)));
        }
        let sized = (index + 1 < fields.len()).then(|| quote!(<CType: Sized>));
        checked_transmute_bounds.push(field.bound(quote!(co3::marker::AbiCompatible #sized)));
        let sized = (index + 1 < fields.len()).then(|| quote!(, CType: Sized));
        validation_bounds.push(field.bound(quote!(co3::convert::AbiValidate<#lang #sized>)));
    }
    let recursive_layout_bound = (!recursive_fields.is_empty()).then(|| {
        let bound = FieldType::new(&own, generics).bound(quote!(
            co3::rust_spec::RustSpec<Layout = co3::rust_spec::Specified>
        ));
        quote!(#bound,)
    });
    // Check the actual wrappers after the record impl is available. Putting
    // this obligation on the impl itself would require proving it recursively.
    let check_recursive_fields = (!recursive_fields.is_empty()).then(|| {
        quote! {
            const _: () = {
                #[expect(dead_code)]
                trait CheckRecursiveFields { fn check(); }
                impl #impl_generics CheckRecursiveFields for #name #ty_generics where
                    #recursive_layout_bound
                    #(#checked_transmute_bounds,)*
                    #predicates
                {
                    fn check() {
                        fn check<T: co3::marker::AbiCompatible + ?Sized>() {}
                        #(check::<#recursive_fields>();)*
                    }
                }
            };
        }
    });
    quote! {
        #check_recursive_fields
        unsafe impl #impl_generics co3::marker::AbiCompatible for #name #ty_generics
        where
            #recursive_layout_bound
            #(#checked_transmute_bounds,)*

            #predicates
        {}
        unsafe impl #checked_generics co3::convert::AbiValidate<#lang> for #name #ty_generics
        where
            #recursive_layout_bound
            #(#validation_bounds,)*

            #predicates
        {
            #[inline(always)]
            unsafe fn is_valid(target: &Self::CType) -> bool {
                #is_valid_body
            }
        }
    }
}

fn gen_record_is_valid(
    contract: &Conversion,
    field_names: &[Ident],
    field_types: &[&syn::Type],
    is_valid: Option<&syn::ExprClosure>,
    niche_validation: Option<TokenStream>,
) -> TokenStream {
    let custom_validation = is_valid.map(|is_valid| {
        let args = field_names
            .iter()
            .map(|field| quote!(#field))
            .collect::<Vec<_>>();
        let call = super::gen_is_valid_call(is_valid, field_types, &args);
        quote! { && { #(
            let #field_names = core::ptr::from_ref(#field_names).cast();
            let #field_names = unsafe {&*#field_names}; )*

            #call
        }}
    });

    let lang = contract.language();
    let field_validation = field_names
        .iter()
        .zip(field_types)
        .filter(|(_, field_ty)| !is_phantom_data(field_ty))
        .map(|(field_name, field_ty)| {
            quote! {
                if !unsafe { <#field_ty as co3::convert::AbiValidate<#lang>>::is_valid(#field_name)} {
                    return false;
                }
            }
        });

    quote! { #(#field_validation)*

        true #niche_validation #custom_validation
    }
}

pub(super) fn derive_fieldless_enum(
    repr: Option<&ReprKind>,
    alignment: Option<&syn::LitInt>,
    input: &syn::DeriveInput,
    is_move: bool,
) -> TokenStream {
    let syn::Data::Enum(data) = &input.data else {
        unreachable!()
    };
    let vis = &input.vis;
    let name = &input.ident;
    let generics = &input.generics;
    let variants = &data.variants;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let ctype_name = gen_ctype_name(name);
    let carrier = quote!(#ctype_name #ty_generics);
    let reference_repr = quote!(co3::ReprC<CType = #carrier>);
    let source_generics = generics.clone();

    let tag_type = match repr {
        None if variants.len() == 1 => None,
        None => Some(infer_repr(variants.len())),
        Some(ReprKind::Transparent) => None,
        Some(repr @ (ReprKind::C(Some(_)) | ReprKind::Primitive(_))) => {
            Some(primitive_tag_type(repr).clone())
        }
        Some(ReprKind::C(None)) => unreachable!(),
    };

    // Anchor implicit tags at the last explicit tag to avoid a recursive chain of constants.
    let mut previous_tag = None;
    let mut implicit_offset = 0;
    let tag_consts = tag_type
        .as_ref()
        .map(|tag_ty| {
            variants
                .iter()
                .map(|variant| {
                    let tag_name = format_ident!("__CO3_TAG_{}", variant.ident);
                    let tag_value = if let Some((_, value)) = &variant.discriminant {
                        previous_tag = Some(tag_name.clone());
                        implicit_offset = 0;
                        quote! { (#value) as #tag_ty }
                    } else if let Some(previous) = &previous_tag {
                        implicit_offset += 1;
                        let offset = proc_macro2::Literal::usize_unsuffixed(implicit_offset);
                        quote! { Self::#previous.wrapping_add(#offset as #tag_ty) }
                    } else {
                        let value = proc_macro2::Literal::usize_unsuffixed(implicit_offset);
                        implicit_offset += 1;
                        quote! { #value }
                    };
                    (tag_name, tag_value)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let variant_tags = tag_consts
        .iter()
        .map(|(tag_name, _)| quote! { #name::#tag_name })
        .collect::<Vec<_>>();

    let tag_ctype: syn::Type = tag_type.clone().unwrap_or_else(|| parse_quote!(()));
    let exhaustive = tag_type
        .as_ref()
        .is_some_and(|tag| is_exhaustive_enum(variants.len(), tag));
    let tag_from_ctype = quote! {
        // Both carriers start with the tag; the wrapper may have extra alignment.
        unsafe { core::ptr::from_ref(&source).cast::<#tag_ctype>().read() }
    };
    let checked_transmute_impl = repr.map(|repr| {
        let valid = if matches!(repr, ReprKind::Transparent) || exhaustive {
            quote!(true)
        } else {
            quote! {
                let tag = unsafe { core::ptr::from_ref(_source).cast::<#tag_ctype>().read() };
                false #(|| tag == #variant_tags)*
            }
        };
        let validator = crate::codec::validator(name, generics, valid);
        quote! {
            unsafe impl #impl_generics co3::marker::AbiCompatible for #name #ty_generics #where_clause {}
            #validator
        }
    });

    let variants_decode = variants.iter().zip(&variant_tags).map(|(variant, tag)| {
        let variant_name = &variant.ident;
        quote! { value if value == #tag => Some(Self::#variant_name) }
    });
    let decode_unchecked = if tag_type.is_some() {
        let variants = variants.iter().zip(&variant_tags).map(|(variant, tag)| {
            let variant_name = &variant.ident;
            quote! { value if value == #tag => Self::#variant_name }
        });
        quote! {
            match #tag_from_ctype {
                #(#variants,)*
                _ => unsafe { core::hint::unreachable_unchecked() },
            }
        }
    } else {
        let transparent_variant = &variants[0].ident;
        quote! {
            let #ctype_name(()) = source;
            Self::#transparent_variant
        }
    };

    let ctype_ty = quote!(#ctype_name #ty_generics);
    let ctype_def = gen_fieldless_enum_ctype(&tag_ctype, alignment, vis, name, generics, variants);
    let variant_ctors = variants.iter().enumerate().map(|(index, variant)| {
        let variant_name = &variant.ident;
        let value = if tag_type.is_none() {
            quote!(())
        } else {
            variant_tags[index].clone()
        };

        quote! {
            #[expect(non_snake_case)]
            #vis const fn #variant_name() -> Self { Self(#value) }
        }
    });
    let encode_body = tag_type
        .as_ref()
        .map(|_| {
            if exhaustive {
                let variant_tags = variants.iter().zip(&variant_tags).map(|(variant, tag)| {
                    let variant_name = &variant.ident;
                    quote! { #name::#variant_name => #tag }
                });
                quote! {
                    let tag: #tag_ctype = match &self { #(#variant_tags,)* };
                    let mut carrier = core::mem::MaybeUninit::<#ctype_ty>::zeroed();
                    unsafe {
                        carrier.as_mut_ptr().cast::<#tag_ctype>().write(tag);
                        carrier.assume_init()
                    }
                }
            } else {
                let variant_tags = variants.iter().zip(&variant_tags).map(|(variant, tag)| {
                    let variant_name = &variant.ident;
                    quote! { #name::#variant_name => #ctype_name(#tag) }
                });
                quote! { match &self { #(#variant_tags,)* } }
            }
        })
        .unwrap_or_else(|| {
            quote! { #ctype_name(()) }
        });
    let moved_encode_body = if is_move {
        quote! {
            let encoded = { #encode_body };
            let _ = core::mem::ManuallyDrop::new(self);
            encoded
        }
    } else {
        encode_body.clone()
    };
    let ref_encode_body = encode_body;
    let decode_body = if tag_type.is_some() {
        quote! {
            match #tag_from_ctype {
                #(#variants_decode,)*
                _ => {  None }
            }
        }
    } else {
        let transparent_variant = &variants[0].ident;

        quote! {
            let #ctype_name(()) = source;
            Some(Self::#transparent_variant)
        }
    };
    let niche_impl = tag_type
        .is_some()
        .then(|| gen_enum_niche_ir(repr, name, &source_generics, variants, false, &[]));

    let borrow_impls = super::borrow::fieldless_borrow_impls(input);
    let tag_const_defs = tag_consts.iter().map(|(tag_name, value)| {
        quote! {
            #[allow(non_upper_case_globals, reason = "generated tag names follow the source variants")]
            const #tag_name: #tag_ctype = #value;
        }
    });
    let tag_const_impl = tag_type.as_ref().map(|_| {
        quote! {
            impl #impl_generics #name #ty_generics #where_clause {
                #(#tag_const_defs)*
            }
        }
    });

    let repr_c_impl = quote! {
        impl #impl_generics co3::ReprC for #name #ty_generics #where_clause {
            type CType = #carrier;
        }
    };

    let ctype_variant_ctors = {
        quote! {
            impl #impl_generics #ctype_name #ty_generics #where_clause {
                #(#variant_ctors)*
            }
        }
    };

    let mut ref_generics = generics.clone();
    ref_generics.params.insert(0, parse_quote!('_ršč));
    let stable_mut_impls = super::reference::gen_stable_direct_mut_impls(input);

    let drop_speculative = speculative_marker(generics.params.is_empty());
    let mut marker_generics = source_generics;
    if !is_move {
        marker_generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(Self: co3::rust_spec::RustSpec));
        marker_generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(
                #drop_speculative <Self as co3::rust_spec::RustSpec>::Drop: __Co3NonCustomDrop
            ));
    }

    let predicates = input
        .generics
        .where_clause
        .as_ref()
        .map(|clause| &clause.predicates);
    let shared_ref_impls = Conversion::derived(quote!(#input)).into_iter().map(|contract| {
        let unchecked = !contract.checked();
        let g = contract.generics(&ref_generics);
        let (ref_impl_generics,_,_) = g.split_for_impl();
        let enc = RefEncoder::new(false,&contract);
        let unsafe_fn = unchecked.then(|| quote!(unsafe));
        let RefEncoder {name:trait_name,store:store_name,method,args,..} = &enc;
        let output = encoder_output(quote!(&'_ršč Self));
        let abi_bound = contract.compatibility();
        let validation = (!unchecked).then(|| quote! {
            debug_assert!(unsafe { <&'_ršč Self as  #abi_bound>::is_valid(&encoded) }, "encoding failed AbiValidate::is_valid");
        });
        let validation_bound = (!unchecked).then(|| quote! {
            &'_ršč Self:  #abi_bound,
        });
        enc.group(quote! {
            unsafe impl #ref_impl_generics #trait_name #args for #name #ty_generics
            where
                Self: #reference_repr,
                &'_ršč Self: co3::ReprC<CType = co3::reference::abi::SoftRef<<Self as co3::ReprC>::CType>>,
                #predicates
            {
                type #store_name = co3::sync::RefSizedEncodeStore<Self, ()>;
                #unsafe_fn fn #method<'itm>(&'_ršč self, store: &'itm mut Self::#store_name) -> #output where '_ršč: 'itm {
                    store.encode(|_| { #ref_encode_body })
                }
            }
            unsafe impl #ref_impl_generics #trait_name #args for #name #ty_generics
            where
                #drop_speculative <Self as co3::ReprC>::CType: Sized,
                #drop_speculative Self: #abi_bound + '_ršč,
                &'_ršč Self: co3::ReprC<CType = co3::reference::abi::Ref<'_ršč, <Self as co3::ReprC>::CType>>,
                #validation_bound
                #predicates
            {
                type #store_name = ();
                #unsafe_fn fn #method<'itm>(&'_ršč self, _: &'itm mut ()) -> #output where '_ršč: 'itm {
                    let encoded = unsafe { co3::reference::abi::Ref::from_raw(core::ptr::from_ref(self).cast()) };
                    #validation
                    encoded
                }
            }
        },true)
    });
    let values = super::borrow::non_custom_drop_scope(crate::codec::stateless(
        name,
        generics,
        &marker_generics,
        moved_encode_body,
        decode_body,
        decode_unchecked,
    ));
    quote! {
        #tag_const_impl
        #ctype_def
        #niche_impl
        #borrow_impls

        #ctype_variant_ctors

        #repr_c_impl
        #stable_mut_impls
        #(#shared_ref_impls)*

        #values
        #checked_transmute_impl
    }
}

pub(crate) fn field_vars(fields: &syn::Fields) -> Vec<Ident> {
    let fields_cnt = fields.iter().count();

    match fields {
        syn::Fields::Named(_) | syn::Fields::Unit => {
            fields.iter().filter_map(|f| f.ident.clone()).collect()
        }
        syn::Fields::Unnamed(_) => (0..fields_cnt).map(|i| format_ident!("_{i}")).collect(),
    }
}

pub fn gen_fields_destructure(fields: &syn::Fields) -> TokenStream {
    let field_names = field_vars(fields);

    match fields {
        syn::Fields::Named(_) | syn::Fields::Unit => quote! {{ #(#field_names),* }},
        syn::Fields::Unnamed(_) => quote!((#(#field_names),*)),
    }
}
