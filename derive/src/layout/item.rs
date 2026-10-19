use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, parse_quote};

use crate::utils::speculative_marker;

use crate::layout::{
    ReprCAttrs, VariantReprCAttrs,
    attr::ReprKind,
    borrow::{
        either_variant_name, gen_borrow_cast_eq_bounds, gen_const_view_name, gen_either_name,
        gen_identity_borrow_impls, gen_item_borrow_impls, gen_item_view, gen_view_ctype_name,
        gen_view_owner_name,
    },
    ctype::{
        c_fn_last_field_bounds, gen_ctype_name, gen_extern_c_bounds_for_ctype,
        gen_fieldless_enum_ctype, gen_identity_borrow_cast_impl, gen_identity_repr_c_impls,
        gen_item_ctype, gen_union_fields, gen_variant_struct_name,
    },
    enum_tag_type, generic_param_idents, infer_repr, is_exhaustive_enum, is_parametrized,
    is_phantom_data, is_transparent_enum_repr,
    niche::{gen_enum_niche_ir, gen_struct_niche_ir, gen_view_niche_ir},
    primitive_tag_type,
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

    let is_view = attrs.is_view;
    let is_wide_data = attrs.is_wide_data;
    if attrs.is_identity {
        let syn::Data::Struct(data) = &input.data else {
            return quote! {};
        };
        let name = &input.ident;
        let fields = data
            .fields
            .iter()
            .map(|field| &field.ty)
            .collect::<Vec<_>>();
        let mut identity_generics = input.generics.clone();
        for field in &fields {
            identity_generics
                .make_where_clause()
                .predicates
                .push(parse_quote!(#field: co3::CType));
        }
        let repr_c_impls = gen_identity_repr_c_impls(name, &identity_generics, &fields);
        let borrow_impls = gen_identity_borrow_impls(name, &identity_generics);
        let borrow_cast_impls = gen_identity_borrow_cast_impl(name, &identity_generics);

        return quote! {
            #repr_c_impls
            #borrow_impls
            #borrow_cast_impls
        };
    }

    let view_def =
        (!is_view && !is_wide_data).then(|| gen_item_view(input, attrs, variant_attrs, repr));

    let mut bounded_impl_input = input.clone();
    if is_view {
        let owner_name = gen_view_owner_name(&input.ident);
        let has_view_lifetime = matches!(
            input.generics.params.first(),
            Some(syn::GenericParam::Lifetime(param)) if param.lifetime.ident == "_dšč"
        );
        let owner_args = generic_param_idents(
            input
                .generics
                .params
                .iter()
                .skip(has_view_lifetime as usize),
        )
        .collect::<Vec<_>>();
        let owner_type = if owner_args.is_empty() {
            quote!(#owner_name)
        } else {
            quote!(#owner_name <#(#owner_args),*>)
        };
        // The generated view lifetime is not an argument of the owner type.
        let speculative = speculative_marker(owner_args.is_empty());
        bounded_impl_input
            .generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(
                #speculative #owner_type:
                    co3::rust_spec::RustSpec<Drop = co3::rust_spec::drop::AutoDrop>
            ));
    } else if is_wide_data {
        let syn::Data::Struct(data) = &input.data else {
            unreachable!("wide data is a struct")
        };
        let header_ty = &data
            .fields
            .iter()
            .next_back()
            .expect("wide data has a tail")
            .ty;
        let speculative = speculative_marker(!is_parametrized(header_ty, &input.generics));
        bounded_impl_input
            .generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#speculative #header_ty: co3::ReprC<CType: Sized>));
    }
    let impl_input = if is_view || is_wide_data {
        &bounded_impl_input
    } else {
        input
    };
    let ctype_def = (!is_view).then(|| {
        let ctype_input = if is_wide_data { impl_input } else { input };
        gen_item_ctype(repr, alignment, ctype_input, !is_wide_data)
    });

    let borrow_impls = (!is_view && !is_wide_data).then(|| gen_item_borrow_impls(input));
    let codec_impls = gen_item_codec_impls(repr, impl_input, attrs, variant_attrs);
    let niche_impls = if is_view {
        attrs
            .niche_value
            .is_some()
            .then(|| gen_view_niche_ir(&input.ident, &impl_input.generics))
    } else {
        Some(gen_item_niche_impls(repr, input, attrs))
    };

    let repr_c_impls = repr
        .is_some()
        .then(|| gen_item_repr_c_impls(repr, impl_input, attrs, variant_attrs));

    quote! {
        #ctype_def
        #view_def

        #borrow_impls
        #codec_impls
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
            gen_enum_niche_ir(repr, &input.ident, &input.generics, &data.variants)
        }
        syn::Data::Union(_) => unreachable!(),
    }
}

fn gen_item_codec_impls(
    repr: Option<&ReprKind>,
    input: &syn::DeriveInput,
    attrs: &ReprCAttrs,
    variant_attrs: &[VariantReprCAttrs],
) -> TokenStream {
    let is_view = attrs.is_view;

    match &input.data {
        syn::Data::Struct(data) => gen_struct_codec_impls(
            is_view,
            &input.ident,
            &input.generics,
            &data.fields,
            attrs.is_valid.as_ref(),
        ),
        syn::Data::Enum(data) => gen_enum_codec_impls(
            is_view,
            repr,
            &input.ident,
            &input.generics,
            &data.variants,
            variant_attrs,
        ),
        syn::Data::Union(_) => unreachable!(),
    }
}

fn gen_item_repr_c_impls(
    repr: Option<&ReprKind>,
    input: &syn::DeriveInput,
    attrs: &ReprCAttrs,
    variant_attrs: &[VariantReprCAttrs],
) -> TokenStream {
    let is_view = attrs.is_view;

    match &input.data {
        syn::Data::Struct(data) => gen_repr_c_struct_impls(
            is_view,
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
            if !is_view && let Some(last) = variant.fields.iter().next_back() {
                let ty = &last.ty;
                if !is_phantom_data(ty) {
                    let speculative = speculative_marker(!is_parametrized(ty, &generics));
                    generics
                        .make_where_clause()
                        .predicates
                        .push(parse_quote!(#speculative #ty: co3::ReprC<CType: Sized>));
                }
            }

            gen_repr_c_struct_impls(
                is_view,
                &input.ident,
                &generics,
                &variant.fields,
                is_valid,
                None,
            )
        }
        syn::Data::Enum(data) => gen_repr_c_data_enum_impls(
            is_view,
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
    is_view: bool,
    name: &Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
    is_valid: Option<&syn::ExprClosure>,
) -> TokenStream {
    let field_types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();

    let encode_store = encode_store_type(fields);
    let direct_store = direct_store_type(fields);
    let decode_store = decode_store_type(fields);

    let codec_impls = {
        let fields_destructure = gen_fields_destructure(fields);

        let ctype_name = if is_view {
            gen_view_ctype_name(name)
        } else {
            gen_ctype_name(name)
        };

        let (encode_body, direct_encode_body, decode_body, decode_unchecked_body) =
            gen_record_conversion(None, quote!(Self), fields, is_valid);

        let field_vars = field_vars(fields);
        let encode_destructure = quote! {
            let __co3_owned = core::mem::ManuallyDrop::new(self);
            let Self #fields_destructure = &*__co3_owned;
            #(let #field_vars = unsafe { core::ptr::read(#field_vars) };)*
        };

        CodecImpls {
            encode_store,
            direct_store,
            decode_store,
            repr_c_bounds: Vec::new(),
            encode_impl: quote! {
                #encode_destructure
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

    gen_codec_impls(is_view, name, generics, &field_types, codec_impls)
}

fn gen_enum_codec_impls(
    is_view: bool,
    repr: Option<&ReprKind>,
    name: &Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
    variant_attrs: &[VariantReprCAttrs],
) -> TokenStream {
    let view_owner_name = is_view.then(|| gen_view_owner_name(name));

    let ctype_name = if is_view {
        gen_view_ctype_name(name)
    } else {
        gen_ctype_name(name)
    };

    if is_transparent_enum_repr(repr, variants) {
        return gen_transparent_enum_codec_impls(is_view, name, generics, variants, variant_attrs);
    }
    let tag_type = enum_tag_type(repr, variants.len());

    let payload_name = if let Some(owner_name) = &view_owner_name {
        format_ident!("{owner_name}Payload")
    } else {
        format_ident!("{name}Payload")
    };
    let payload_name = if is_view {
        gen_const_view_name(&payload_name)
    } else {
        payload_name
    };

    let fields = variants
        .iter()
        .flat_map(|variant| variant.fields.iter().map(|field| &field.ty))
        .collect::<Vec<_>>();

    let mut union_copy_bounds = gen_enum_union_copy_bounds(name, generics, variants, is_view);
    if is_view {
        // Enum payload fields must have sized C representations, including the
        // final field which may be unsized in an ordinary struct view.
        let source_fields = fields
            .iter()
            .filter_map(|field| match field {
                syn::Type::Path(syn::TypePath {
                    qself: Some(qself), ..
                }) => Some(&*qself.ty),
                _ => None,
            })
            .filter(|field| is_parametrized(field, generics))
            .collect::<Vec<_>>();
        let mut source_bounds = source_fields
            .into_iter()
            .map(|field| {
                parse_quote!(#field: co3::ReprC<CType: co3::borrow::BorrowCast<AsConst: Sized> + Sized>)
            })
            .collect::<Vec<syn::WherePredicate>>();
        source_bounds.append(&mut union_copy_bounds);
        union_copy_bounds = source_bounds;
    }

    let (encode_store, direct_store, decode_store) = {
        let either_name = gen_either_name(variants.len());

        let field_encode_stores = variants.iter().map(|v| encode_store_type(&v.fields));
        let field_direct_stores = variants.iter().map(|v| direct_store_type(&v.fields));
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
            let variant_struct_name = if let Some(owner_name) = &view_owner_name {
                gen_const_view_name(&gen_variant_struct_name(owner_name, variant_name))
            } else {
                gen_variant_struct_name(name, variant_name)
            };

            let store_variant = either_variant_name(idx);
            let variant_struct = quote! { #variant_struct_name };
            let tag_value = proc_macro2::Literal::usize_unsuffixed(idx);

            let destructure_fields = gen_fields_destructure(&variant.fields);
            let move_vars = field_vars(&variant.fields);
            let move_fields = quote! { #(let #move_vars = unsafe { core::ptr::read(#move_vars) };)* };
            let custom_is_valid = variant_attrs[idx].is_valid.as_ref();
            let variant_tag = match repr {
                None | Some(ReprKind::Primitive(_)) => {
                    Some(quote!(#tag_value as #tag_type))
                }
                Some(ReprKind::Transparent | ReprKind::C(Some(_))) => None,
                Some(ReprKind::C(None)) => unreachable!(),
            };

            let (encode_body, direct_encode_body, decode_body, decode_unchecked_body) = gen_record_conversion(
                variant_tag,
                quote!(Self::#variant_name),
                &variant.fields,
                custom_is_valid,
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

                    match &variant.fields {
                        syn::Fields::Named(_) => quote! {
                            let #variant_struct { tag: _, #(#field_vars),* } = source;
                        },
                        syn::Fields::Unnamed(_) => quote! {
                            let #variant_struct(_, #(#field_vars),*) = source;
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
                            payload: #payload_name { #variant_name: #variant_struct_name #encode_body },
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
                        payload: #payload_name { #variant_name: #variant_struct_name #direct_encode_body },
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
                Some(ReprKind::C(Some(_))) => quote! { unsafe { source.payload.#variant_name } },
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
                quote! { #tag_value => #decode_variant },
                quote! { #tag_value => #decode_variant_unchecked },
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

    let codec_impls = CodecImpls {
        encode_store,
        direct_store,
        decode_store,
        repr_c_bounds: union_copy_bounds,
        encode_impl: quote! {
            let __co3_owned = core::mem::ManuallyDrop::new(self);
            match &*__co3_owned {
                #(#variants_encode,)*
            }
        },
        direct_encode_impl: quote! {
            let __co3_owned = core::mem::ManuallyDrop::new(self);
            match &*__co3_owned {
                #(#variants_direct_encode,)*
            }
        },
        decode_impl,
        decode_unchecked_impl,
    };

    gen_codec_impls(is_view, name, generics, &fields, codec_impls)
}

fn gen_transparent_enum_codec_impls(
    is_view: bool,
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

    let ctype_name = if is_view {
        gen_view_ctype_name(name)
    } else {
        gen_ctype_name(name)
    };

    let variant_name = &variant.ident;
    let field_types = variant
        .fields
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();
    let destructure_fields = gen_fields_destructure(&variant.fields);
    let field_vars = field_vars(&variant.fields);
    let move_fields = quote! { #(let #field_vars = unsafe { core::ptr::read(#field_vars) };)* };
    let custom_is_valid = variant_attrs
        .first()
        .and_then(|attrs| attrs.is_valid.as_ref());
    let encode_store = encode_store_type(&variant.fields);
    let direct_store = direct_store_type(&variant.fields);
    let decode_store = decode_store_type(&variant.fields);
    let (encode_body, direct_encode_body, decode_body, decode_unchecked_body) =
        gen_record_conversion(
            None,
            quote!(Self::#variant_name),
            &variant.fields,
            custom_is_valid,
        );

    let mut bounded_generics = generics.clone();
    if !is_view && let Some(last) = field_types.last().filter(|ty| !is_phantom_data(ty)) {
        let speculative = speculative_marker(!is_parametrized(last, generics));
        bounded_generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#speculative #last: co3::ReprC<CType: Sized>));
    }

    gen_codec_impls(
        is_view,
        name,
        &bounded_generics,
        &field_types,
        CodecImpls {
            encode_store,
            direct_store,
            decode_store,
            repr_c_bounds: Vec::new(),
            encode_impl: quote! {
                let __co3_owned = core::mem::ManuallyDrop::new(self);
                match &*__co3_owned {
                    Self::#variant_name #destructure_fields => {
                        #move_fields
                        #ctype_name #encode_body
                    }
                }
            },
            direct_encode_impl: quote! {
                let __co3_owned = core::mem::ManuallyDrop::new(self);
                match &*__co3_owned {
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
    is_view: bool,
    name: &Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
    is_valid: Option<&syn::ExprClosure>,
    niche_value: Option<&syn::Expr>,
) -> TokenStream {
    let field_types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();
    let field_vars = field_vars(fields);

    let ctype_name = if is_view {
        gen_view_ctype_name(name)
    } else {
        gen_ctype_name(name)
    };
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

    gen_repr_c_impls(is_view, name, generics, &field_types, is_valid_impl)
}

fn gen_repr_c_data_enum_impls(
    is_view: bool,
    repr: Option<&ReprKind>,
    name: &Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
    variant_attrs: &[VariantReprCAttrs],
) -> TokenStream {
    let tag_type = enum_tag_type(repr, variants.len());

    let fields = variants
        .iter()
        .flat_map(|variant| variant.fields.iter().map(|field| &field.ty))
        .collect::<Vec<_>>();

    let variant_body = variants.iter().enumerate().map(|(variant_idx, variant)| {
        let variant_idx_lit = proc_macro2::Literal::usize_unsuffixed(variant_idx);

        let variant_name = &variant.ident;
        let variant_struct_name = if is_view {
            let view_owner_name = gen_view_owner_name(name);
            gen_const_view_name(&gen_variant_struct_name(&view_owner_name, variant_name))
        } else {
            gen_variant_struct_name(name, variant_name)
        };

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
                        &target.payload.#variant_name
                    };
                },
                syn::Fields::Unnamed(_) => quote! {
                    let #variant_struct_type(#(#field_names),*) = unsafe {
                        &target.payload.#variant_name
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

        quote! {
            #variant_idx_lit => {
                #destructure_target
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
    if is_view {
        let source_fields = fields
            .iter()
            .filter_map(|field| match field {
                syn::Type::Path(syn::TypePath {
                    qself: Some(qself), ..
                }) => Some(&*qself.ty),
                _ => None,
            })
            .filter(|field| is_parametrized(field, generics));
        predicates.extend(source_fields.map(|field| -> syn::WherePredicate {
            parse_quote!(#field: co3::ReprC<CType: co3::borrow::BorrowCast<AsConst: Sized> + Sized>)
        }));
    }
    predicates.extend(gen_enum_union_copy_bounds(
        name, generics, variants, is_view,
    ));

    gen_repr_c_impls(is_view, name, &bounded_generics, &fields, is_valid_impl)
}

fn gen_enum_union_copy_bounds(
    enum_name: &Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
    is_view: bool,
) -> Vec<syn::WherePredicate> {
    let (owner_name, owner_generics) = if is_view {
        let owner_name = gen_view_owner_name(enum_name);
        let mut owner_generics = generics.clone();
        if matches!(
            owner_generics.params.first(),
            Some(syn::GenericParam::Lifetime(param)) if param.lifetime.ident == "_dšč"
        ) {
            owner_generics.params = owner_generics.params.into_iter().skip(1).collect();
        }
        (owner_name, owner_generics)
    } else {
        (enum_name.clone(), generics.clone())
    };

    let (_, fields) = gen_union_fields(&owner_name, &owner_generics, variants);
    let field_types = variants
        .iter()
        .flat_map(|variant| variant.fields.iter().map(|field| &field.ty))
        .collect::<Vec<_>>();
    let mut bounds = if is_view {
        Vec::new()
    } else {
        super::ctype::gen_sized_extern_c_bounds_for_ctype(&owner_generics, &field_types)
    };
    bounds.reserve(fields.named.len() * if is_view { 2 } else { 1 });
    for field in fields.named {
        let original_ty = field.ty;

        if is_view {
            bounds.push(parse_quote!(#original_ty: Copy));
            let mut view_ty = original_ty.clone();
            if let syn::Type::Path(path) = &mut view_ty
                && let Some(segment) = path.path.segments.last_mut()
            {
                segment.ident = gen_const_view_name(&segment.ident);
                bounds.push(parse_quote!(#view_ty: Copy));
            }
        } else {
            bounds.push(parse_quote!(#original_ty: Copy));
        }
    }
    bounds
}

fn gen_repr_c_impls(
    is_view: bool,
    name: &Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
    is_valid_body: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let checked_transmute_bounds = gen_checked_transmute_bounds(generics, fields);
    let (borrow_cast_bounds, cast_eq_bounds) = if is_view {
        (
            gen_borrow_cast_view_bounds(generics, fields),
            gen_borrow_cast_eq_bounds(fields),
        )
    } else {
        (vec![], quote! {})
    };

    quote! {
        unsafe impl #impl_generics co3::transmute::CheckedTransmute for #name #ty_generics
        where
            #(#checked_transmute_bounds,)*
            #(#borrow_cast_bounds,)*
            #cast_eq_bounds
            #predicates
        {
            #[inline(always)]
            unsafe fn is_valid(target: &Self::CType) -> bool {
                #is_valid_body
            }
        }
    }
}

fn gen_checked_transmute_bounds(
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> Vec<syn::WherePredicate> {
    let Some((last, fields)) = fields.split_last() else {
        return Vec::new();
    };

    let mut predicates = fields
        .iter()
        .filter(|ty| !is_phantom_data(ty))
        .map(|&ty| {
            let for_dummy = speculative_marker(!is_parametrized(ty, generics));

            parse_quote! { #for_dummy #ty: co3::transmute::CheckedTransmute<CType: Sized> }
        })
        .collect::<Vec<_>>();

    if !is_phantom_data(last) {
        let for_dummy = speculative_marker(!is_parametrized(last, generics));
        predicates.push(parse_quote! {
            #for_dummy #last: co3::transmute::CheckedTransmute
        });
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
                    if !unsafe { <#field_ty as co3::transmute::CheckedTransmute>::is_valid(#field_name)} {
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
                    false #(|| target.0 == #variant_tags)*
                }
            }
        }
    };

    let checked_transmute_impl = match repr {
        None => quote! {},
        Some(ReprKind::C(None)) => unreachable!(),
        Some(ReprKind::Transparent | ReprKind::C(Some(_)) | ReprKind::Primitive(_)) => {
            quote! {
                unsafe impl #impl_generics co3::transmute::CheckedTransmute for #name #ty_generics #where_clause {
                    #[inline(always)]
                    #checked_transmute_method
                }
            }
        }
    };

    let tag_ctype: syn::Type = tag_type.clone().unwrap_or_else(|| parse_quote!(()));
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
            match source.0 {
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
    let ctype_def = gen_fieldless_enum_ctype(&tag_ctype, alignment, vis, name, generics);
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
            let variant_tags = variants.iter().zip(&variant_tags).map(|(variant, tag)| {
                let variant_name = &variant.ident;
                quote! { #name::#variant_name => #ctype_name(#tag) }
            });
            quote! { match &self { #(#variant_tags,)* } }
        })
        .unwrap_or_else(|| quote! { #ctype_name(()) });
    let decode_body = if tag_type.is_some() {
        quote! {
            match source.0 {
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
        .then(|| gen_enum_niche_ir(repr, name, generics, variants));

    let borrow_impls = gen_item_borrow_impls(input);
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

    quote! {
        #tag_const_impl
        #ctype_def
        #niche_impl
        #borrow_impls

        impl #impl_generics #ctype_name #ty_generics #where_clause {
            #(#variant_ctors)*
        }

        impl #impl_generics co3::ReprC for #name #ty_generics #where_clause {
            type CType = #ctype_ty;
        }
        unsafe impl #impl_generics co3::stored::EncodeOwned for #name #ty_generics #where_clause {
            type Store = ();

            fn soft_encode<'_išč>(self, (): &mut ()) -> Self::CType
            where
                Self: '_išč,
            {
                use co3::stored::DirectEncodeOwned as _;
                unsafe { self.soft_encode_unchecked(&mut ()) }
            }
        }
        unsafe impl #impl_generics co3::stored::DirectEncodeOwned for #name #ty_generics #where_clause {
            type SoftStore = ();

            unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
                #encode_body
            }
        }

        unsafe impl #decode_impl_generics co3::stored::DecodeOwned<'_dšč> for #name #ty_generics #where_clause {
            type Store = ();

            unsafe fn soft_decode<'_išč: '_dšč>(source: Self::CType, (): &mut ()) -> Option<Self> {
                #decode_body
            }

            unsafe fn soft_decode_unchecked<'_išč: '_dšč>(source: Self::CType, (): &mut ()) -> Self {
                #decode_unchecked
            }
        }

        impl #impl_generics co3::Encode for #name #ty_generics #where_clause {}
        impl #impl_generics co3::DirectEncode for #name #ty_generics #where_clause {}
        impl #impl_generics co3::Decode<'_> for #name #ty_generics #where_clause {}

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

fn gen_record_conversion(
    tag: Option<TokenStream>,
    source_head: TokenStream,
    fields: &syn::Fields,
    is_valid: Option<&syn::ExprClosure>,
) -> (TokenStream, TokenStream, TokenStream, TokenStream) {
    let store_vars = tuple_field_exprs(fields.len());
    let field_vars = field_vars(fields);
    let field_types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();

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
                    #(#field_vars: co3::stored::EncodeOwned::soft_encode(#field_vars, #store_vars)),*
                }
            },
            quote! {
                {
                    #tag_field
                    #(#field_vars: unsafe { co3::stored::DirectEncodeOwned::soft_encode_unchecked(#field_vars, #store_vars) }),*
                }
            },
            quote! { #(
                let #field_vars = unsafe {
                    co3::stored::DecodeOwned::soft_decode(#field_vars, #store_vars)?
                }; )*

                #custom_validation
                Some(#source_head {
                    #(#field_vars),*
                })
            },
            quote! { #(
                let #field_vars = unsafe {
                    co3::stored::DecodeOwned::soft_decode_unchecked(#field_vars, #store_vars)
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
                    #(co3::stored::EncodeOwned::soft_encode(#field_vars, #store_vars)),*
                )
            },
            quote! {
                (
                    #tag_element
                    #(unsafe { co3::stored::DirectEncodeOwned::soft_encode_unchecked(#field_vars, #store_vars) }),*
                )
            },
            quote! { #(
                let #field_vars = unsafe {
                    co3::stored::DecodeOwned::soft_decode(#field_vars, #store_vars)?
                }; )*

                #custom_validation
                Some(#source_head(
                    #(#field_vars),*
                ))
            },
            quote! { #(
                let #field_vars = unsafe {
                    co3::stored::DecodeOwned::soft_decode_unchecked(#field_vars, #store_vars)
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
    repr_c_bounds: Vec<syn::WherePredicate>,
    encode_impl: TokenStream,
    direct_encode_impl: TokenStream,
    decode_impl: TokenStream,
    decode_unchecked_impl: TokenStream,
}

fn gen_codec_impls(
    is_view: bool,
    name: &Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
    codec_impls: CodecImpls,
) -> TokenStream {
    let CodecImpls {
        encode_store,
        direct_store,
        decode_store,
        repr_c_bounds,
        encode_impl,
        direct_encode_impl,
        decode_impl,
        decode_unchecked_impl,
    } = codec_impls;

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let has_view_lifetime = is_view
        && matches!(
            generics.params.first(),
            Some(syn::GenericParam::Lifetime(param)) if param.lifetime.ident == "_dšč"
        );

    let ctype_name = if is_view {
        gen_view_ctype_name(name)
    } else {
        gen_ctype_name(name)
    };

    let ctype_ty_generics = if is_view {
        let ty_generics =
            generic_param_idents(generics.params.iter().skip(has_view_lifetime as usize))
                .collect::<Vec<_>>();

        if ty_generics.is_empty() {
            quote! {}
        } else {
            quote! { <#(#ty_generics),*> }
        }
    } else {
        quote! { #ty_generics }
    };

    let extern_c_bounds = if is_view {
        vec![parse_quote!(#ctype_name #ctype_ty_generics: co3::CType)]
    } else {
        gen_extern_c_bounds_for_ctype(generics, fields)
    };

    let encode_owned_bounds =
        gen_field_encode_bounds(generics, fields, quote! { co3::stored::EncodeOwned });
    let direct_encode_bounds =
        gen_field_encode_bounds(generics, fields, quote! { co3::stored::DirectEncodeOwned });
    let decode_owned_bounds =
        gen_field_decode_bounds(generics, fields, quote! { co3::stored::DecodeOwned });

    let encode_bounds = gen_field_encode_bounds(generics, fields, quote! { co3::Encode });
    let direct_bounds = gen_field_encode_bounds(generics, fields, quote! { co3::DirectEncode });
    let decode_bounds = gen_field_decode_bounds(generics, fields, quote! { co3::Decode });

    let (sized_bound, omit_sized_impls) = if is_view {
        (quote! {}, false)
    } else {
        c_fn_last_field_bounds(fields, generics)
    };

    let (borrow_cast_bounds, cast_eq_bounds) = if is_view {
        (
            gen_borrow_cast_view_bounds(generics, fields),
            gen_borrow_cast_eq_bounds(fields),
        )
    } else {
        (vec![], quote! {})
    };

    let mut decode_generics = generics.clone();
    if !has_view_lifetime {
        decode_generics.params.insert(0, parse_quote!('_dšč));
    }
    let (decode_impl_generics, _, _) = decode_generics.split_for_impl();

    let sized_impls = (!omit_sized_impls).then(|| quote! {
        unsafe impl #impl_generics co3::stored::EncodeOwned for #name #ty_generics
        where
            #sized_bound
            #(#borrow_cast_bounds,)*
            #(#repr_c_bounds,)*
            #(#encode_owned_bounds,)*
            #cast_eq_bounds
            #predicates
        {
            type Store = #encode_store;

            fn soft_encode<'_išč>(self, store: &'_išč mut Self::Store) -> Self::CType where Self: '_išč {
                #encode_impl
            }

        }
        unsafe impl #impl_generics co3::stored::DirectEncodeOwned for #name #ty_generics
        where
            #sized_bound
            #(#borrow_cast_bounds,)*
            #(#repr_c_bounds,)*
            #(#direct_encode_bounds,)*
            #cast_eq_bounds
            #predicates
        {
            type SoftStore = #direct_store;

            unsafe fn soft_encode_unchecked(self, store: &mut <Self as co3::stored::DirectEncodeOwned>::SoftStore) -> Self::CType {
                #direct_encode_impl
            }
        }
        unsafe impl #decode_impl_generics co3::stored::DecodeOwned<'_dšč> for #name #ty_generics
        where
            #sized_bound
            #(#borrow_cast_bounds,)*
            #(#repr_c_bounds,)*
            #(#decode_owned_bounds,)*
            #cast_eq_bounds
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
            #sized_bound
            #(#borrow_cast_bounds,)*
            #(#repr_c_bounds,)*
            #(#encode_bounds,)*
            #cast_eq_bounds
            #predicates
        {}
        impl #impl_generics co3::DirectEncode for #name #ty_generics where
            #sized_bound
            #(#borrow_cast_bounds,)*
            #(#repr_c_bounds,)*
            #(#direct_bounds,)*
            #cast_eq_bounds
            #predicates
        {}
        impl #decode_impl_generics co3::Decode<'_dšč> for #name #ty_generics where
            #sized_bound
            #(#borrow_cast_bounds,)*
            #(#repr_c_bounds,)*
            #(#decode_bounds,)*
            #cast_eq_bounds
            #predicates
        {}
    });

    quote! {
        impl #impl_generics co3::ReprC for #name #ty_generics where
            #(#borrow_cast_bounds,)*
            #(#repr_c_bounds,)*
            #(#extern_c_bounds,)*
            #predicates
        {
            type CType = #ctype_name #ctype_ty_generics;
        }

        #sized_impls
    }
}

fn gen_field_encode_bounds<'a>(
    generics: &'a syn::Generics,
    fields: &'a [&syn::Type],
    bound: TokenStream,
) -> impl Iterator<Item = syn::WherePredicate> + use<'a> {
    fields
        .iter()
        .filter(move |ty| !is_phantom_data(ty))
        .map(move |ty| {
            let for_dummy = speculative_marker(!is_parametrized(ty, generics));
            parse_quote! { #for_dummy #ty: #bound }
        })
}

fn gen_field_decode_bounds<'a>(
    generics: &'a syn::Generics,
    fields: &'a [&syn::Type],
    bound: TokenStream,
) -> impl Iterator<Item = syn::WherePredicate> + use<'a> {
    fields
        .iter()
        .filter(move |ty| !is_phantom_data(ty))
        .map(move |ty| {
            let for_dummy = speculative_marker(!is_parametrized(ty, generics));
            parse_quote! { #for_dummy #ty: #bound<'_dšč> }
        })
}

fn encode_store_type(fields: &syn::Fields) -> TokenStream {
    let fields = fields.iter().map(|syn::Field { ty, .. }| {
        quote! { <#ty as co3::stored::EncodeOwned>::Store }
    });

    quote!((#(#fields,)*))
}

fn direct_store_type(fields: &syn::Fields) -> TokenStream {
    let fields = fields.iter().map(|syn::Field { ty, .. }| {
        quote! { <#ty as co3::stored::DirectEncodeOwned>::SoftStore }
    });

    quote!((#(#fields,)*))
}

fn decode_store_type(fields: &syn::Fields) -> TokenStream {
    let fields = fields.iter().map(|syn::Field { ty, .. }| {
        quote! { <#ty as co3::stored::DecodeOwned<'_dšč>>::Store }
    });

    quote!((#(#fields,)*))
}

fn gen_borrow_cast_view_bounds(
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> Vec<TokenStream> {
    let Some((last, fields)) = fields.split_last() else {
        return vec![];
    };

    fn borrow_ty(field_ty: &syn::Type) -> &syn::Type {
        match field_ty {
            syn::Type::Path(syn::TypePath {
                qself: Some(syn::QSelf { ty, .. }),
                ..
            }) => ty,
            _ => unreachable!(),
        }
    }

    let mut predicates = fields
        .iter()
        .filter(|ty| !is_phantom_data(ty))
        .map(|ty| borrow_ty(ty))
        .filter(|ty| is_parametrized(ty, generics))
        .map(
            |ty| quote! { #ty: co3::ReprC<CType: co3::borrow::BorrowCast<AsConst: Sized> + Sized> },
        )
        .collect::<Vec<_>>();

    if !is_phantom_data(last) {
        let last = borrow_ty(last);
        if is_parametrized(last, generics) {
            predicates.push(quote!(#last: co3::ReprC<CType: co3::borrow::BorrowCast>));
        }
    }

    predicates
}
