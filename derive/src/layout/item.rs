use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, parse_quote};

use crate::{
    layout::{
        ReprCAttrs, VariantReprCAttrs,
        attr::ReprKind,
        borrow::{
            either_variant_name, gen_either_name, gen_identity_borrow_impls, gen_item_borrow_impls,
        },
        ctype::{
            c_fn_last_field_bounds, classify_tail, gen_ctype_name, gen_extern_c_bounds_for_ctype,
            gen_fieldless_enum_ctype, gen_identity_repr_c_impls, gen_item_ctype,
            gen_variant_struct_name,
        },
        enum_tag_type, enum_variant_tag_exprs,
        fields::{FieldType, field_types},
        infer_repr, is_exhaustive_enum, is_phantom_data, is_transparent_enum_repr,
        niche::{gen_enum_niche_ir, gen_struct_niche_ir},
        primitive_tag_type,
        reference::{encoder_names, encoder_output, group_aggregate_ref_impls},
    },
    utils::speculative_marker,
};

pub(super) fn derive_item(
    repr: Option<&ReprKind>,
    alignment: Option<&syn::LitInt>,
    input: &syn::DeriveInput,
    attrs: &ReprCAttrs,
    variant_attrs: &[VariantReprCAttrs],
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
    let sized_decode_impls = (!is_wide_data && !has_unsized_tail)
        .then(|| super::reference::gen_sized_decode_impls(input, repr, attrs, variant_attrs));
    if attrs.is_identity {
        let fields = match &input.data {
            syn::Data::Struct(data) => data
                .fields
                .iter()
                .map(|field| &field.ty)
                .collect::<Vec<_>>(),
            syn::Data::Enum(data) => data
                .variants
                .iter()
                .flat_map(|variant| variant.fields.iter().map(|field| &field.ty))
                .collect::<Vec<_>>(),
            syn::Data::Union(_) => unreachable!(),
        };
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
    let impl_input = if is_wide_data {
        &bounded_impl_input
    } else {
        input
    };
    let ctype_def = Some({
        let ctype_input = if is_wide_data { impl_input } else { input };
        gen_item_ctype(repr, alignment, ctype_input, !is_wide_data, !is_wide_data)
    });

    let codec_impls = gen_item_codec_impls(repr, impl_input, attrs, variant_attrs);
    let niche_impls = gen_item_niche_impls(repr, input, attrs);

    let repr_c_impls = repr
        .is_some()
        .then(|| gen_item_repr_c_impls(repr, impl_input, attrs, variant_attrs));

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
) -> TokenStream {
    match &input.data {
        syn::Data::Struct(data) => gen_struct_niche_ir(
            &input.ident,
            &input.generics,
            &data.fields,
            attrs.niche_value.as_ref(),
        ),
        syn::Data::Enum(data) => {
            gen_enum_niche_ir(repr, &input.ident, &input.generics, &data.variants, false)
        }
        syn::Data::Union(_) => unreachable!(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EncodeMode {
    Owned,
    Ref,
}

fn gen_item_codec_impls(
    repr: Option<&ReprKind>,
    input: &syn::DeriveInput,
    attrs: &ReprCAttrs,
    variant_attrs: &[VariantReprCAttrs],
) -> TokenStream {
    let is_move = attrs.is_move;

    let generate = |mode| match &input.data {
        syn::Data::Struct(data) => gen_struct_codec_impls(
            mode,
            is_move,
            &input.ident,
            &input.generics,
            &data.fields,
            attrs.is_valid.as_ref(),
        ),
        syn::Data::Enum(data) => gen_enum_codec_impls(
            mode,
            attrs,
            repr,
            &input.ident,
            &input.generics,
            &data.variants,
            variant_attrs,
        ),
        syn::Data::Union(_) => unreachable!(),
    };
    let owned = generate(EncodeMode::Owned);
    let borrowed = (!attrs.is_wide_data).then(|| gen_item_borrow_impls(input, owned.clone()));
    let delegate_ref = matches!(repr, Some(ReprKind::Transparent))
        && attrs.is_valid.is_none()
        && variant_attrs.iter().all(|attrs| attrs.is_valid.is_none());
    let references = if delegate_ref {
        match &input.data {
            syn::Data::Struct(data) if data.fields.len() == 1 => {
                super::reference::gen_transparent_ref_impls(
                    input,
                    data.fields.iter().next().unwrap(),
                    None,
                )
            }
            syn::Data::Enum(data)
                if data.variants.len() == 1 && data.variants[0].fields.len() == 1 =>
            {
                super::reference::gen_transparent_ref_impls(
                    input,
                    data.variants[0].fields.iter().next().unwrap(),
                    Some(&data.variants[0].ident),
                )
            }
            _ => group_aggregate_ref_impls(generate(EncodeMode::Ref), true),
        }
    } else {
        group_aggregate_ref_impls(generate(EncodeMode::Ref), true)
    };
    let stable_mut = (!delegate_ref).then(|| super::reference::gen_stable_direct_mut_impls(input));
    quote! { #owned #borrowed #references #stable_mut }
}

fn gen_item_repr_c_impls(
    repr: Option<&ReprKind>,
    input: &syn::DeriveInput,
    attrs: &ReprCAttrs,
    variant_attrs: &[VariantReprCAttrs],
) -> TokenStream {
    match &input.data {
        syn::Data::Struct(data) => gen_repr_c_struct_impls(
            &input.ident,
            &input.generics,
            &data.fields,
            attrs.is_valid.as_ref(),
            attrs.niche_value.as_ref(),
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

            gen_repr_c_struct_impls(&input.ident, &generics, &variant.fields, is_valid, None)
        }
        syn::Data::Enum(data) => gen_repr_c_data_enum_impls(
            repr,
            &input.ident,
            &input.generics,
            &data.variants,
            variant_attrs,
        ),
        syn::Data::Union(_) => unreachable!(),
    }
}

fn gen_struct_codec_impls(
    mode: EncodeMode,
    is_move: bool,
    name: &Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
    is_valid: Option<&syn::ExprClosure>,
) -> TokenStream {
    let field_types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();

    let encode_store = encode_store_type(fields, mode);
    let direct_store = direct_store_type(fields, mode);
    let decode_store = decode_store_type(fields);

    let codec_impls = {
        let fields_destructure = gen_fields_destructure(fields);

        let ctype_name = gen_ctype_name(name);

        let (encode_body, direct_encode_body, decode_body, decode_unchecked_body) =
            gen_record_conversion(mode, None, quote!(Self), fields, is_valid, false);

        let field_vars = field_vars(fields);
        let encode_validation = gen_encode_is_valid_assert(fields, is_valid);
        let move_fields = (mode == EncodeMode::Owned)
            .then(|| quote! { #(let #field_vars = unsafe { core::ptr::read(#field_vars) };)* });
        let encode_destructure = if mode != EncodeMode::Owned {
            quote! { let Self #fields_destructure = self; }
        } else {
            quote! {
                let __co3_owned = core::mem::ManuallyDrop::new(self);
                let Self #fields_destructure = &*__co3_owned;
                #move_fields
            }
        };

        CodecImpls {
            encode_store,
            direct_store,
            decode_store,
            requires_nonrobust: is_valid.is_some(),
            repr_c_bounds: Vec::new(),
            encode_impl: quote! {
                #encode_destructure
                #encode_validation
                #ctype_name #encode_body
            },
            direct_encode_impl: quote! {
                #encode_destructure
                #ctype_name #direct_encode_body
            },
            decode_impl: quote! {
                let #ctype_name #fields_destructure = source;
                #decode_body
            },
            decode_unchecked_impl: quote! {
                let #ctype_name #fields_destructure = source;
                #decode_unchecked_body
            },
        }
    };

    gen_codec_impls(mode, is_move, name, generics, &field_types, codec_impls)
}

fn gen_enum_codec_impls(
    mode: EncodeMode,
    attrs: &ReprCAttrs,
    repr: Option<&ReprKind>,
    name: &Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
    variant_attrs: &[VariantReprCAttrs],
) -> TokenStream {
    let is_move = attrs.is_move;
    let ctype_name = gen_ctype_name(name);

    if is_transparent_enum_repr(repr, variants) {
        return gen_transparent_enum_codec_impls(
            mode,
            is_move,
            name,
            generics,
            variants,
            variant_attrs,
        );
    }
    let tag_type = enum_tag_type(repr, variants.len());
    let tag_values = enum_variant_tag_exprs(variants, &tag_type);

    let payload_name = format_ident!("{name}Payload");

    let fields = variants
        .iter()
        .flat_map(|variant| variant.fields.iter().map(|field| &field.ty))
        .collect::<Vec<_>>();

    let union_copy_bounds = gen_enum_union_copy_bounds(name, generics, variants);

    let (encode_store, direct_store, decode_store) = {
        let either_name = gen_either_name(variants.len());

        let field_encode_stores = variants.iter().map(|v| encode_store_type(&v.fields, mode));
        let field_direct_stores = variants.iter().map(|v| direct_store_type(&v.fields, mode));
        let field_decode_stores = variants.iter().map(|v| decode_store_type(&v.fields));

        (
            quote! { core::option::Option<co3::either::#either_name<#(#field_encode_stores),*>> },
            quote! { core::option::Option<co3::either::#either_name<#(#field_direct_stores),*>> },
            quote! { core::option::Option<co3::either::#either_name<#(#field_decode_stores),*>> },
        )
    };

    let variants: Vec<_> = variants
        .iter()
        .enumerate()
        .map(|(idx, variant)| {
            let either_ty = gen_either_name(variants.len());

            let variant_name = &variant.ident;
            let variant_struct_name = gen_variant_struct_name(name, variant_name);

            let store_variant = either_variant_name(idx);
            let variant_struct = quote! { #variant_struct_name };
            let tag_value = &tag_values[idx];

            let destructure_fields = gen_fields_destructure(&variant.fields);
            let move_vars = field_vars(&variant.fields);
            let move_fields = (mode == EncodeMode::Owned).then(|| quote! { #(let #move_vars = unsafe { core::ptr::read(#move_vars) };)* });
            let custom_is_valid = variant_attrs[idx].is_valid.as_ref();
            let encode_validation = gen_encode_is_valid_assert(&variant.fields, custom_is_valid);
            let variant_tag = match repr {
                None | Some(ReprKind::Primitive(_)) => {
                    // The integer discriminant is already valid; preserve its ReprC projection
                    // without requiring checked-encoding bounds on payload fields.
                    Some(quote!(unsafe { co3::soft_encode_unchecked(#tag_value as #tag_type, &mut ()) }))
                }
                Some(ReprKind::Transparent | ReprKind::C(Some(_))) => None,
                Some(ReprKind::C(None)) => unreachable!(),
            };

            let (encode_body, direct_encode_body, decode_body, decode_unchecked_body) = gen_record_conversion(
                mode,
                variant_tag,
                quote!(Self::#variant_name),
                &variant.fields,
                custom_is_valid,
                !matches!(repr, Some(ReprKind::C(Some(_)))),
            );

            let decode_destructure = match repr {
                Some(ReprKind::Transparent) => quote! {
                    let #ctype_name::#variant_name #destructure_fields = source else {
                        return None;
                    };
                },
                Some(ReprKind::C(Some(_))) => {
                    let field_vars = field_vars(&variant.fields);

                    match &variant.fields {
                        syn::Fields::Named(_) | syn::Fields::Unit => quote! {
                            let #variant_struct { #(#field_vars),* } = source;
                        },
                        syn::Fields::Unnamed(_) => quote! {
                            let #variant_struct(#(#field_vars),*) = source;
                        },
                    }
                },
                None | Some(ReprKind::Primitive(_)) => {
                    let field_vars = field_vars(&variant.fields);
                    let unwrap_fields = field_vars.iter().map(|field| quote! {
                        let #field = unsafe { #field.assume_init() };
                    }).collect::<Vec<_>>();

                    match &variant.fields {
                        syn::Fields::Named(_) => quote! {
                            let #variant_struct { tag: _, #(#field_vars),* } = source;
                            #(#unwrap_fields)*
                        },
                        syn::Fields::Unnamed(_) => quote! {
                            let #variant_struct(_, #(#field_vars),*) = source;
                            #(#unwrap_fields)*
                        },
                        syn::Fields::Unit => quote! {
                            let #variant_struct { tag: _ } = source;
                        },
                    }
                },
                Some(ReprKind::C(None)) => unreachable!(),
            };

            let encode_variant = match repr {
                Some(ReprKind::C(Some(_))) => {
                    quote! {
                        #ctype_name {
                            tag: #tag_value as #tag_type,
                            payload: core::mem::MaybeUninit::new(#payload_name { #variant_name: #variant_struct_name #encode_body }),
                        }
                    }
                }
                None | Some(ReprKind::Primitive(_)) => quote! {
                    #ctype_name {
                        #variant_name: #variant_struct_name #encode_body
                    }
                },
                _ => unreachable!(),
            };
            let direct_encode_variant = match repr {
                Some(ReprKind::C(Some(_))) => quote! {
                    #ctype_name {
                        tag: #tag_value as #tag_type,
                        payload: core::mem::MaybeUninit::new(#payload_name { #variant_name: #variant_struct_name #direct_encode_body }),
                    }
                },
                None | Some(ReprKind::Primitive(_)) => quote! {
                    #ctype_name {
                        #variant_name: #variant_struct_name #direct_encode_body
                    }
                },
                _ => unreachable!(),
            };

            let decode_source = match repr {
                Some(ReprKind::C(Some(_))) => quote! { unsafe { source.payload.assume_init().#variant_name } },
                None | Some(ReprKind::Primitive(_)) => quote! { unsafe { source.#variant_name } },
                _ => unreachable!(),
            };
            let encode_store_init = match repr {
                None | Some(ReprKind::C(Some(_)) | ReprKind::Primitive(_)) => quote! {
                    let co3::either::#either_ty::#store_variant(store) =
                        store.insert(co3::either::#either_ty::#store_variant(Default::default()))
                    else {
                        unreachable!()
                    };
                },
                _ => unreachable!(),
            };
            let decode_store_init = quote! {
                let co3::either::#either_ty::#store_variant(store) =
                    store.insert(co3::either::#either_ty::#store_variant(Default::default()))
                else {
                    unreachable!()
                };
            };

            let decode_variant = quote! {
                {
                    let source = #decode_source;

                    #decode_destructure
                    #decode_store_init

                    #decode_body
                }
            };
            let decode_variant_unchecked = quote! {
                {
                    let source = #decode_source;

                    #decode_destructure
                    #decode_store_init

                    #decode_unchecked_body
                }
            };

            (
                quote! {
                    Self::#variant_name #destructure_fields => {
                        #move_fields
                        #encode_validation
                        #encode_store_init
                        #encode_variant
                    }
                },
                quote! {
                    Self::#variant_name #destructure_fields => {
                        #move_fields
                        #encode_store_init
                        #direct_encode_variant
                    }
                },
                quote! { value if value == #tag_value => #decode_variant },
                quote! { value if value == #tag_value => #decode_variant_unchecked },
            )
        })
        .collect();
    let variants_encode = variants
        .iter()
        .map(|(encode, _, _, _)| encode.clone())
        .collect::<Vec<_>>();
    let variants_direct_encode = variants
        .iter()
        .map(|(_, encode, _, _)| encode.clone())
        .collect::<Vec<_>>();
    let variants_decode = variants
        .iter()
        .map(|(_, _, decode, _)| decode.clone())
        .collect::<Vec<_>>();
    let variants_decode_unchecked = variants
        .iter()
        .map(|(_, _, _, decode)| decode.clone())
        .collect::<Vec<_>>();

    let decode_impl = match repr {
        Some(ReprKind::C(Some(_))) => quote! {
            match source.tag {
                #(#variants_decode,)*
                _ => None,
            }
        },
        None | Some(ReprKind::Primitive(_)) => quote! {
            {
                let repr_value = <*const _>::cast::<#tag_type>(core::ptr::from_ref(&source));
                match unsafe { *repr_value } {
                    #(#variants_decode,)*
                    _ => None,
                }
            }
        },
        _ => unreachable!(),
    };
    let decode_unchecked_impl = match repr {
        Some(ReprKind::C(Some(_))) => quote! {
            match source.tag {
                #(#variants_decode_unchecked,)*
                _ => unsafe { core::hint::unreachable_unchecked() },
            }
        },
        None | Some(ReprKind::Primitive(_)) => quote! {
            {
                let repr_value = <*const _>::cast::<#tag_type>(core::ptr::from_ref(&source));
                match unsafe { *repr_value } {
                    #(#variants_decode_unchecked,)*
                    _ => unsafe { core::hint::unreachable_unchecked() },
                }
            }
        },
        _ => unreachable!(),
    };

    let encode_head = if mode != EncodeMode::Owned {
        quote!(match self)
    } else {
        quote!(match &*__co3_owned)
    };
    let encode_owner = (mode == EncodeMode::Owned)
        .then(|| quote!(let __co3_owned = core::mem::ManuallyDrop::new(self);));
    let codec_impls = CodecImpls {
        encode_store,
        direct_store,
        decode_store,
        requires_nonrobust: variant_attrs.iter().any(|attrs| attrs.is_valid.is_some()),
        repr_c_bounds: union_copy_bounds,
        encode_impl: quote! {
            #encode_owner
            #encode_head {
                #(#variants_encode,)*
            }
        },
        direct_encode_impl: quote! {
            #encode_owner
            #encode_head {
                #(#variants_direct_encode,)*
            }
        },
        decode_impl,
        decode_unchecked_impl,
    };

    gen_codec_impls(mode, is_move, name, generics, &fields, codec_impls)
}

fn gen_transparent_enum_codec_impls(
    mode: EncodeMode,
    is_move: bool,
    name: &Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
    variant_attrs: &[VariantReprCAttrs],
) -> TokenStream {
    if variants.len() > 1 {
        return quote! {};
    }
    let Some(variant) = variants.iter().next() else {
        return quote! {};
    };

    let ctype_name = gen_ctype_name(name);

    let variant_name = &variant.ident;
    let field_types = variant
        .fields
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();
    let destructure_fields = gen_fields_destructure(&variant.fields);
    let field_vars = field_vars(&variant.fields);
    let move_fields = (mode == EncodeMode::Owned)
        .then(|| quote! { #(let #field_vars = unsafe { core::ptr::read(#field_vars) };)* });
    let custom_is_valid = variant_attrs
        .first()
        .and_then(|attrs| attrs.is_valid.as_ref());
    let encode_validation = gen_encode_is_valid_assert(&variant.fields, custom_is_valid);
    let encode_store = encode_store_type(&variant.fields, mode);
    let direct_store = direct_store_type(&variant.fields, mode);
    let decode_store = decode_store_type(&variant.fields);
    let (encode_body, direct_encode_body, decode_body, decode_unchecked_body) =
        gen_record_conversion(
            mode,
            None,
            quote!(Self::#variant_name),
            &variant.fields,
            custom_is_valid,
            false,
        );

    let mut bounded_generics = generics.clone();
    if let Some(last) = field_types.last().filter(|ty| !is_phantom_data(ty)) {
        let bound = FieldType::new(last, generics).bound(quote!(co3::ReprC<CType: Sized>));
        bounded_generics.make_where_clause().predicates.push(bound);
    }

    let encode_head = if mode != EncodeMode::Owned {
        quote!(match self)
    } else {
        quote!(match &*__co3_owned)
    };
    let encode_owner = (mode == EncodeMode::Owned)
        .then(|| quote!(let __co3_owned = core::mem::ManuallyDrop::new(self);));

    gen_codec_impls(
        mode,
        is_move,
        name,
        &bounded_generics,
        &field_types,
        CodecImpls {
            encode_store,
            direct_store,
            decode_store,
            requires_nonrobust: variant_attrs[0].is_valid.is_some(),
            repr_c_bounds: Vec::new(),
            encode_impl: quote! {
                #encode_owner
                #encode_head {
                    Self::#variant_name #destructure_fields => {
                        #move_fields
                        #encode_validation
                        #ctype_name #encode_body
                    }
                }
            },
            direct_encode_impl: quote! {
                #encode_owner
                #encode_head {
                    Self::#variant_name #destructure_fields => {
                        #move_fields
                        #ctype_name #direct_encode_body
                    }
                }
            },
            decode_impl: quote! {
                let #ctype_name #destructure_fields = source;
                #decode_body
            },
            decode_unchecked_impl: quote! {
                let #ctype_name #destructure_fields = source;
                #decode_unchecked_body
            },
        },
    )
}

fn gen_repr_c_struct_impls(
    name: &Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
    is_valid: Option<&syn::ExprClosure>,
    niche_value: Option<&syn::Expr>,
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
    let is_valid_body = gen_record_is_valid(&field_vars, &field_types, is_valid, niche_validation);

    let is_valid_impl = quote! {
        let #ctype_name #destructure_target = target;
        #is_valid_body
    };

    gen_repr_c_impls(name, generics, &field_types, is_valid_impl)
}

fn gen_repr_c_data_enum_impls(
    repr: Option<&ReprKind>,
    name: &Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
    variant_attrs: &[VariantReprCAttrs],
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
        let is_valid_body = gen_record_is_valid(&field_names, &variant_fields, is_valid, None);
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
            _ => false,
        }
    };

    let mut bounded_generics = generics.clone();
    let predicates = &mut bounded_generics.make_where_clause().predicates;
    predicates.extend(gen_enum_union_copy_bounds(name, generics, variants));

    gen_repr_c_impls(name, &bounded_generics, &fields, is_valid_impl)
}

fn gen_enum_union_copy_bounds(
    enum_name: &Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> Vec<syn::WherePredicate> {
    super::ctype::gen_union_extern_c_bounds(enum_name, generics, variants)
}

fn gen_repr_c_impls(
    name: &Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
    is_valid_body: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let checked_transmute_bounds = gen_checked_transmute_bounds(generics, fields, false);
    let validation_bounds = gen_checked_transmute_bounds(generics, fields, true);

    quote! {
        unsafe impl #impl_generics co3::transmute::AbiCompatibleValidate for #name #ty_generics
        where
            #(#validation_bounds,)*

            #predicates
        {
            #[inline(always)]
            unsafe fn is_valid(target: &Self::CType) -> bool {
                #is_valid_body
            }
        }
        unsafe impl #impl_generics co3::transmute::AbiCompatible for #name #ty_generics
        where
            #(#checked_transmute_bounds,)*

            #predicates
        {}
    }
}

fn gen_checked_transmute_bounds(
    generics: &syn::Generics,
    fields: &[&syn::Type],
    validate: bool,
) -> Vec<syn::WherePredicate> {
    let Some((last, fields)) = fields.split_last() else {
        return Vec::new();
    };

    let trait_ = if validate {
        quote!(co3::transmute::AbiCompatibleValidate)
    } else {
        quote!(co3::transmute::AbiCompatible)
    };
    let mut predicates = field_types(fields, generics)
        .filter(|field| !field.phantom)
        .map(|field| field.bound(quote!(#trait_<CType: Sized>)))
        .collect::<Vec<_>>();
    let last = FieldType::new(last, generics);
    if !last.phantom {
        predicates.push(last.bound(trait_));
    }

    predicates
}

fn gen_record_is_valid(
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

    let field_validation = field_names
        .iter()
        .zip(field_types)
        .filter(|(_, field_ty)| !is_phantom_data(field_ty))
        .map(|(field_name, field_ty)| {
            quote! {
                if !unsafe { <#field_ty as co3::transmute::AbiCompatibleValidate>::is_valid(#field_name)} {
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
    let mut decode_generics = generics.clone();
    decode_generics.params.insert(0, parse_quote!('_dšč));
    let (decode_impl_generics, _, _) = decode_generics.split_for_impl();

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
    let checked_transmute_method = match repr {
        None => quote! {},
        Some(ReprKind::C(None)) => unreachable!(),
        Some(ReprKind::Transparent) => {
            quote! { unsafe fn is_valid(_: &Self::CType) -> bool { true } }
        }
        Some(ReprKind::C(Some(repr)) | ReprKind::Primitive(repr))
            if is_exhaustive_enum(variants.len(), repr) =>
        {
            quote! { unsafe fn is_valid(_: &Self::CType) -> bool { true } }
        }
        Some(ReprKind::C(Some(_)) | ReprKind::Primitive(_)) => {
            let variant_tags = &variant_tags;

            quote! {
                unsafe fn is_valid(target: &Self::CType) -> bool {
                    let tag = unsafe { core::ptr::from_ref(target).cast::<#tag_ctype>().read() };
                    false #(|| tag == #variant_tags)*
                }
            }
        }
    };

    let checked_transmute_impl = match repr {
        None => quote! {},
        Some(ReprKind::C(None)) => unreachable!(),
        Some(ReprKind::Transparent | ReprKind::C(Some(_)) | ReprKind::Primitive(_)) => {
            quote! {
                unsafe impl #impl_generics co3::transmute::AbiCompatibleValidate for #name #ty_generics #where_clause {
                    #[inline(always)]
                    #checked_transmute_method
                }
                unsafe impl #impl_generics co3::transmute::AbiCompatible for #name #ty_generics #where_clause {}
            }
        }
    };

    let ctype_name = gen_ctype_name(name);
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
                    let mut carrier = core::mem::MaybeUninit::<Self::CType>::zeroed();
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
            core::mem::forget(self);
            encoded
        }
    } else {
        encode_body.clone()
    };
    let decode_body = if tag_type.is_some() {
        quote! {
            match #tag_from_ctype {
                #(#variants_decode,)*
                _ => None
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
        .then(|| gen_enum_niche_ir(repr, name, generics, variants, false));

    let borrow_impls = gen_item_borrow_impls(input, TokenStream::new());
    let tag_const_defs = tag_consts.iter().map(|(tag_name, value)| {
        quote! {
            #[allow(non_upper_case_globals)]
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
            type CType = #ctype_ty;
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
    let (ref_impl_generics, _, _) = ref_generics.split_for_impl();
    let stable_mut_impls = super::reference::gen_stable_direct_mut_impls(input);

    let drop_speculative = speculative_marker(generics.params.is_empty());
    let mut marker_generics = generics.clone();
    marker_generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(
            #drop_speculative Self: co3::rust_spec::RustSpec
        ));
    if !is_move {
        marker_generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(
                #drop_speculative <Self as co3::rust_spec::RustSpec>::Drop: __Co3NonCustomDrop
            ));
    }
    let (_, _, marker_where_clause) = marker_generics.split_for_impl();

    let predicates = input
        .generics
        .where_clause
        .as_ref()
        .map(|clause| &clause.predicates);
    let shared_ref_impls = [true, false].into_iter().map(|unchecked| {
        let (trait_name, _, store_name, method, _) = encoder_names(false, unchecked);
        let output = encoder_output(quote!(&'_ršč Self));
        let unsafe_fn = unchecked.then(|| quote!(unsafe));
        let abi_bound = if unchecked {
            quote!(co3::transmute::AbiCompatible)
        } else {
            quote!(co3::transmute::AbiCompatibleValidate)
        };
        let validation = (!unchecked).then(|| quote! {
            debug_assert!(unsafe { <&'_ršč Self as co3::transmute::AbiCompatibleValidate>::is_valid(&encoded) }, "encoding failed AbiCompatibleValidate::is_valid");
        });
        let validation_bound = (!unchecked).then(|| quote! {
            &'_ršč Self: co3::transmute::AbiCompatibleValidate,
        });
        quote! {
            unsafe impl #ref_impl_generics #trait_name<'_ršč> for #name #ty_generics
            where
                #drop_speculative Self: co3::ReprC<CType = #ctype_ty>
                    + co3::rust_spec::RustSpec<Layout = co3::rust_spec::Unstable>,
                &'_ršč Self: co3::ReprC<CType = co3::reference::CSoftRef<Self>>,
                #predicates
            {
                type #store_name = co3::sync::RefSizedEncodeStore<Self, ()>;
                #unsafe_fn fn #method<'itm>(&'_ršč self, store: &'itm mut Self::#store_name) -> #output where '_ršč: 'itm {
                    store.encode(|_| { #encode_body })
                }
            }
            unsafe impl #ref_impl_generics #trait_name<'_ršč> for #name #ty_generics
            where
                #drop_speculative Self: co3::ReprC<CType: co3::CType + Sized>
                    + #abi_bound
                    + co3::rust_spec::RustSpec<Layout = co3::rust_spec::Stable> + '_ršč,
                &'_ršč Self: co3::ReprC<CType = co3::reference::CRef<'_ršč, Self>>,
                #validation_bound
                #predicates
            {
                type #store_name = ();
                #unsafe_fn fn #method<'itm>(&'_ršč self, _: &'itm mut ()) -> #output where '_ršč: 'itm {
                    let encoded = unsafe { co3::reference::CRef::from_raw(core::ptr::from_ref(self).cast()) };
                    #validation
                    encoded
                }
            }
        }
    });
    let shared_ref_impls = group_aggregate_ref_impls(quote!(#(#shared_ref_impls)*), false);

    quote! {
        #tag_const_impl
        #ctype_def
        #niche_impl
        #borrow_impls

        #ctype_variant_ctors

        #repr_c_impl
        #stable_mut_impls
        #shared_ref_impls

        unsafe impl #impl_generics co3::stored::EncodeMove for #name #ty_generics #where_clause {
            type Store = ();

            fn soft_encode<'_išč>(self, (): &mut ()) -> Self::CType
            where
                Self: '_išč,
            {
                use co3::stored::EncodeMoveUnchecked as _;
                unsafe { co3::stored::EncodeMoveUnchecked::soft_encode_unchecked(self, &mut ()) }
            }
        }
        unsafe impl #impl_generics co3::stored::EncodeMoveUnchecked for #name #ty_generics #where_clause {
            type SoftStore = ();

            unsafe fn soft_encode_unchecked<'itm>(self, (): &'itm mut ()) -> Self::CType where Self: 'itm {
                #moved_encode_body
            }
        }

        unsafe impl #decode_impl_generics co3::stored::DecodeMove<'_dšč> for #name #ty_generics #where_clause {
            type Store = ();

            unsafe fn soft_decode<'_išč: '_dšč>(source: Self::CType, (): &mut ()) -> Option<Self> {
                #decode_body
            }

            unsafe fn soft_decode_unchecked<'_išč: '_dšč>(source: Self::CType, (): &mut ()) -> Self {
                #decode_unchecked
            }
        }

        const _: () = {
            trait __Co3NonCustomDrop {}
            impl __Co3NonCustomDrop for co3::rust_spec::drop::NoDrop {}
            impl __Co3NonCustomDrop for co3::rust_spec::drop::AutoDrop {}

            impl #impl_generics co3::Encode for #name #ty_generics #marker_where_clause {}
            impl #impl_generics co3::EncodeUnchecked for #name #ty_generics #marker_where_clause {}
            impl #impl_generics co3::Decode<'_> for #name #ty_generics #marker_where_clause {}
        };

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

pub fn tuple_field_exprs(len: usize) -> Vec<TokenStream> {
    (0..len)
        .map(|i| {
            let i = syn::Index::from(i);
            quote! { &mut store.#i }
        })
        .collect()
}

fn gen_encode_is_valid_assert(
    fields: &syn::Fields,
    is_valid: Option<&syn::ExprClosure>,
) -> TokenStream {
    let Some(is_valid) = is_valid else {
        return quote! {};
    };
    let field_vars = field_vars(fields);
    let field_types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();
    let args = field_vars
        .iter()
        .map(|field| quote!(&#field))
        .collect::<Vec<_>>();
    let call = super::gen_is_valid_call(is_valid, &field_types, &args);
    quote! { debug_assert!(#call, "encoding failed custom is_valid"); }
}

fn gen_record_conversion(
    mode: EncodeMode,
    tag: Option<TokenStream>,
    source_head: TokenStream,
    fields: &syn::Fields,
    is_valid: Option<&syn::ExprClosure>,
    wrap_fields: bool,
) -> (TokenStream, TokenStream, TokenStream, TokenStream) {
    let (encode_fn, direct_fn) = match mode {
        EncodeMode::Ref => (
            quote!(__Co3EncodeField::encode_field),
            quote!(__Co3EncodeFieldUnchecked::encode_field_unchecked),
        ),
        EncodeMode::Owned => (
            quote!(co3::stored::EncodeMove::soft_encode),
            quote!(co3::stored::EncodeMoveUnchecked::soft_encode_unchecked),
        ),
    };
    let store_vars = tuple_field_exprs(fields.len());
    let field_vars = field_vars(fields);
    let field_types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();
    let encoded = field_vars
        .iter()
        .zip(&store_vars)
        .zip(&field_types)
        .map(|((field, store), ty)| {
            let value = if mode == EncodeMode::Ref && is_phantom_data(ty) {
                quote!(core::marker::PhantomData)
            } else {
                quote!(#encode_fn(#field, #store))
            };
            if wrap_fields {
                quote!(core::mem::MaybeUninit::new(#value))
            } else {
                value
            }
        })
        .collect::<Vec<_>>();
    let direct_encoded = field_vars
        .iter()
        .zip(&store_vars)
        .zip(&field_types)
        .map(|((field, store), ty)| {
            let value = if mode == EncodeMode::Ref && is_phantom_data(ty) {
                quote!(core::marker::PhantomData)
            } else {
                quote!(unsafe { #direct_fn(#field, #store) })
            };
            if wrap_fields {
                quote!(core::mem::MaybeUninit::new(#value))
            } else {
                value
            }
        })
        .collect::<Vec<_>>();

    let tag_field = tag.as_ref().map(|tag| quote! { tag: #tag, });
    let tag_element = tag.as_ref().map(|tag| quote! { #tag, });

    let custom_validation = is_valid.map(|is_valid| {
        let args = field_vars
            .iter()
            .map(|field| quote!(&#field))
            .collect::<Vec<_>>();
        let call = super::gen_is_valid_call(is_valid, &field_types, &args);
        quote! {
            if !#call {
                return None;
            }
        }
    });

    match fields {
        syn::Fields::Named(_) | syn::Fields::Unit => (
            quote! {
                {
                    #tag_field
                    #(#field_vars: #encoded),*
                }
            },
            quote! {
                {
                    #tag_field
                    #(#field_vars: #direct_encoded),*
                }
            },
            quote! { #(
                let #field_vars = unsafe {
                    co3::stored::DecodeMove::soft_decode(#field_vars, #store_vars)?
                }; )*

                #custom_validation
                Some(#source_head {
                    #(#field_vars),*
                })
            },
            quote! { #(
                let #field_vars = unsafe {
                    co3::stored::DecodeMove::soft_decode_unchecked(#field_vars, #store_vars)
                }; )*

                #source_head {
                    #(#field_vars),*
                }
            },
        ),
        syn::Fields::Unnamed(_) => (
            quote! {
                (
                    #tag_element
                    #(#encoded),*
                )
            },
            quote! {
                (
                    #tag_element
                    #(#direct_encoded),*
                )
            },
            quote! { #(
                let #field_vars = unsafe {
                    co3::stored::DecodeMove::soft_decode(#field_vars, #store_vars)?
                }; )*

                #custom_validation
                Some(#source_head(
                    #(#field_vars),*
                ))
            },
            quote! { #(
                let #field_vars = unsafe {
                    co3::stored::DecodeMove::soft_decode_unchecked(#field_vars, #store_vars)
                }; )*

                #source_head(
                    #(#field_vars),*
                )
            },
        ),
    }
}

struct CodecImpls {
    encode_store: TokenStream,
    direct_store: TokenStream,
    decode_store: TokenStream,
    requires_nonrobust: bool,
    repr_c_bounds: Vec<syn::WherePredicate>,
    encode_impl: TokenStream,
    direct_encode_impl: TokenStream,
    decode_impl: TokenStream,
    decode_unchecked_impl: TokenStream,
}

fn gen_codec_impls(
    mode: EncodeMode,
    is_move: bool,
    name: &Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
    codec_impls: CodecImpls,
) -> TokenStream {
    let CodecImpls {
        encode_store,
        direct_store,
        decode_store,
        requires_nonrobust,
        repr_c_bounds,
        encode_impl,
        direct_encode_impl,
        decode_impl,
        decode_unchecked_impl,
    } = codec_impls;

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let validity_bound = requires_nonrobust.then(|| {
        quote! {
            #name #ty_generics: co3::rust_spec::RustSpec<Trap = co3::rust_spec::layout::NonRobust>,
        }
    });
    let leading_predicates = where_clause.as_ref().map(|w| {
        let predicates = w.predicates.iter();
        quote!(#(#predicates,)*)
    });
    let drop_speculative = speculative_marker(generics.params.is_empty());
    let direct_drop_bound = (!is_move).then(|| {
        quote!(
            #drop_speculative Self: co3::rust_spec::RustSpec,
            #drop_speculative <Self as co3::rust_spec::RustSpec>::Drop: __Co3NonCustomDrop,
        )
    });
    let ctype_name = gen_ctype_name(name);
    let ctype_ty_generics = quote!(#ty_generics);
    let extern_c_bounds = gen_extern_c_bounds_for_ctype(generics, fields);

    let encode_move_bounds =
        gen_field_encode_bounds(generics, fields, quote! { co3::stored::EncodeMove });
    let direct_encode_bounds = gen_field_encode_bounds(
        generics,
        fields,
        quote! { co3::stored::EncodeMoveUnchecked },
    );
    let decode_move_bounds =
        gen_field_decode_bounds(generics, fields, quote! { co3::stored::DecodeMove });

    let encode_bounds = gen_field_encode_bounds(generics, fields, quote! { co3::Encode });
    let direct_bounds = gen_field_encode_bounds(generics, fields, quote! { co3::EncodeUnchecked });
    let decode_bounds = gen_field_decode_bounds(generics, fields, quote! { co3::Decode });

    let (sized_bound, omit_sized_impls) = c_fn_last_field_bounds(fields, generics);
    let mut decode_generics = generics.clone();
    decode_generics.params.insert(0, parse_quote!('_dšč));
    let (decode_impl_generics, _, _) = decode_generics.split_for_impl();

    if mode == EncodeMode::Ref {
        if omit_sized_impls {
            return quote! {};
        }
        let mut ref_generics = generics.clone();
        ref_generics.params.insert(0, parse_quote!('_ršč));
        let (ref_impl_generics, _, _) = ref_generics.split_for_impl();
        let speculative = speculative_marker(generics.params.is_empty());
        let implementations = [true, false].into_iter().map(|unchecked| {
            let (trait_name, _, store_name, method, _) = encoder_names(false, unchecked);
            let output = encoder_output(quote!(&'_ršč Self));
            let unsafe_fn = unchecked.then(|| quote!(unsafe));
            let abi_bound = if unchecked {
                quote!(co3::transmute::AbiCompatible)
            } else {
                quote!(co3::transmute::AbiCompatibleValidate)
            };
            let validation_bound = (!unchecked).then(|| quote! {
                &'_ršč Self: co3::transmute::AbiCompatibleValidate,
            });
            let validation = (!unchecked).then(|| quote! {
                debug_assert!(unsafe { <&'_ršč Self as co3::transmute::AbiCompatibleValidate>::is_valid(&encoded) }, "encoding failed AbiCompatibleValidate::is_valid");
            });
            let (field_store, encode_body) = if unchecked {
                (&direct_store, &direct_encode_impl)
            } else {
                (&encode_store, &encode_impl)
            };
            let field_trait = if unchecked { quote!(__Co3EncodeFieldUnchecked) } else { quote!(__Co3EncodeField) };
            let field_bounds = field_types(fields, generics)
                .filter(|field| !field.phantom)
                .map(|field| field.bound(quote!(
                    co3::ReprC<CType: co3::CType + Copy> + #field_trait<'_ršč>
                )));
            quote! {
                unsafe impl #ref_impl_generics #trait_name<'_ršč> for #name #ty_generics
                where
                    #sized_bound
                    #(#repr_c_bounds,)*
                    #(#field_bounds,)*
                    #leading_predicates
                    #speculative Self: co3::ReprC<CType = #ctype_name #ctype_ty_generics>
                        + co3::rust_spec::RustSpec<Layout = co3::rust_spec::Unstable, Size: Thin> + '_ršč,
                    &'_ršč Self: co3::ReprC<CType = co3::reference::CSoftRef<Self>>,

                {
                    type #store_name = co3::sync::RefSizedEncodeStore<Self, #field_store>;
                    #unsafe_fn fn #method<'itm>(&'_ršč self, store: &'itm mut Self::#store_name) -> #output where '_ršč: 'itm {
                        store.encode(|store| { #encode_body })
                    }
                }
                unsafe impl #ref_impl_generics #trait_name<'_ršč> for #name #ty_generics
                where
                    #sized_bound
                    #leading_predicates
                    #(#repr_c_bounds,)*
                    #speculative Self: co3::ReprC<CType: co3::CType + Sized>
                        + #abi_bound
                        + co3::rust_spec::RustSpec<Layout = co3::rust_spec::Stable, Size: Thin>
                        + Sized + '_ršč,
                    &'_ršč Self: co3::ReprC<CType = co3::reference::CRef<'_ršč, Self>>,
                    #validation_bound
                {
                    type #store_name = ();
                    #unsafe_fn fn #method<'itm>(&'_ršč self, _: &'itm mut ()) -> #output where '_ršč: 'itm {
                        let encoded = unsafe { co3::reference::CRef::from_raw(core::ptr::from_ref(self).cast()) };
                        #validation
                        encoded
                    }
                }
            }
        });
        return quote!(#(#implementations)*);
    }

    let sized_impls = (!omit_sized_impls).then(|| quote! {
        unsafe impl #impl_generics co3::stored::EncodeMove for #name #ty_generics
        where
            #sized_bound

            #(#repr_c_bounds,)*
            #(#encode_move_bounds,)*

            #predicates
        {
            type Store = #encode_store;

            fn soft_encode<'_išč>(self, store: &'_išč mut Self::Store) -> Self::CType where Self: '_išč {
                #encode_impl
            }

        }
        unsafe impl #impl_generics co3::stored::EncodeMoveUnchecked for #name #ty_generics
        where
            #sized_bound

            #(#repr_c_bounds,)*
            #(#direct_encode_bounds,)*

            #predicates
        {
            type SoftStore = #direct_store;

            unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut <Self as co3::stored::EncodeMoveUnchecked>::SoftStore) -> Self::CType where Self: 'itm {
                #direct_encode_impl
            }
        }
        unsafe impl #decode_impl_generics co3::stored::DecodeMove<'_dšč> for #name #ty_generics
        where
            #sized_bound

            #(#repr_c_bounds,)*
            #(#decode_move_bounds,)*

            #predicates
        {
            type Store = #decode_store;

            unsafe fn soft_decode<'_išč: '_dšč>(source: Self::CType, store: &'_išč mut Self::Store) -> Option<Self> {
                #decode_impl
            }

            unsafe fn soft_decode_unchecked<'_išč: '_dšč>(source: Self::CType, store: &'_išč mut Self::Store) -> Self {
                #decode_unchecked_impl
            }
        }

        impl #impl_generics co3::Encode for #name #ty_generics where
            #direct_drop_bound
            #sized_bound

            #(#repr_c_bounds,)*
            #(#encode_bounds,)*

            #predicates
        {}
        impl #impl_generics co3::EncodeUnchecked for #name #ty_generics where
            #direct_drop_bound
            #sized_bound

            #(#repr_c_bounds,)*
            #(#direct_bounds,)*

            #predicates
        {}
        impl #decode_impl_generics co3::Decode<'_dšč> for #name #ty_generics where
            #direct_drop_bound
            #sized_bound

            #(#repr_c_bounds,)*
            #(#decode_bounds,)*

            #predicates
        {}
    });

    quote! {
        impl #impl_generics co3::ReprC for #name #ty_generics where

            #validity_bound
            #(#repr_c_bounds,)*
            #(#extern_c_bounds,)*
            #predicates
        {
            type CType = #ctype_name #ctype_ty_generics;
        }

        const _: () = {
            trait __Co3NonCustomDrop {}
            impl __Co3NonCustomDrop for co3::rust_spec::drop::NoDrop {}
            impl __Co3NonCustomDrop for co3::rust_spec::drop::AutoDrop {}

            #sized_impls
        };
    }
}

fn gen_field_encode_bounds<'a>(
    generics: &'a syn::Generics,
    fields: &'a [&syn::Type],
    bound: TokenStream,
) -> impl Iterator<Item = syn::WherePredicate> + use<'a> {
    field_types(fields, generics)
        .filter(|field| !field.phantom)
        .map(move |field| field.bound(&bound))
}

fn gen_field_decode_bounds<'a>(
    generics: &'a syn::Generics,
    fields: &'a [&syn::Type],
    bound: TokenStream,
) -> impl Iterator<Item = syn::WherePredicate> + use<'a> {
    gen_field_encode_bounds(generics, fields, quote!(#bound<'_dšč>))
}

fn encode_store_type(fields: &syn::Fields, mode: EncodeMode) -> TokenStream {
    let fields = fields.iter().map(|syn::Field { ty, .. }| {
        if mode == EncodeMode::Ref && is_phantom_data(ty) {
            quote!(())
        } else if mode == EncodeMode::Ref {
            quote! { <#ty as __Co3EncodeField<'_ršč>>::Store }
        } else {
            quote! { <#ty as co3::stored::EncodeMove>::Store }
        }
    });

    quote!((#(#fields,)*))
}

fn direct_store_type(fields: &syn::Fields, mode: EncodeMode) -> TokenStream {
    let fields = fields.iter().map(|syn::Field { ty, .. }| {
        if mode == EncodeMode::Ref && is_phantom_data(ty) {
            quote!(())
        } else if mode == EncodeMode::Ref {
            quote! { <#ty as __Co3EncodeFieldUnchecked<'_ršč>>::SoftStore }
        } else {
            quote! { <#ty as co3::stored::EncodeMoveUnchecked>::SoftStore }
        }
    });

    quote!((#(#fields,)*))
}

fn decode_store_type(fields: &syn::Fields) -> TokenStream {
    let fields = fields.iter().map(|syn::Field { ty, .. }| {
        quote! { <#ty as co3::stored::DecodeMove<'_dšč>>::Store }
    });

    quote!((#(#fields,)*))
}
