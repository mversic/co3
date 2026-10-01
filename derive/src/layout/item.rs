use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, parse_quote};

use crate::layout::{
    ReprCAttrs, VariantReprCAttrs,
    attr::ReprKind,
    borrow::{
        either_variant_name, gen_borrow_cast_eq_bounds, gen_const_view_name, gen_either_name,
        gen_identity_borrow_impls, gen_item_borrow_impls, gen_item_view, gen_view_ctype_name,
        gen_view_owner_name,
    },
    ctype::{
        gen_ctype_name, gen_extern_c_bounds_for_ctype, gen_fieldless_enum_ctype,
        gen_identity_borrow_cast_impl, gen_identity_repr_c_impls, gen_item_ctype,
        gen_variant_struct_name,
    },
    enum_tag_type, generic_param_idents, infer_repr, is_exhaustive_enum, is_phantom_data,
    is_transparent_enum_repr, is_type_parametrized,
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

    let ctype_def = (!is_view).then(|| gen_item_ctype(repr, alignment, input, !is_wide_data));
    let view_def = (!is_view && !is_wide_data).then(|| gen_item_view(input, attrs, variant_attrs));

    let borrow_impls = (!is_view && !is_wide_data).then(|| gen_item_borrow_impls(input));
    let codec_impls = gen_item_codec_impls(repr, input, attrs, variant_attrs);
    let niche_impls = if is_view {
        attrs
            .niche_value
            .is_some()
            .then(|| gen_view_niche_ir(&input.ident, &input.generics))
    } else {
        Some(gen_item_niche_impls(repr, input, attrs))
    };
    let interior_mut_impl = gen_item_interior_mut_impl(repr, input);

    let repr_c_impls = repr
        .is_some()
        .then(|| gen_item_repr_c_impls(repr, input, attrs, variant_attrs));

    quote! {
        #ctype_def
        #view_def

        #borrow_impls
        #codec_impls
        #niche_impls
        #interior_mut_impl

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
        syn::Data::Struct(data) => gen_repr_c_struct_impls::<false>(
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

            gen_repr_c_struct_impls::<true>(
                is_view,
                &input.ident,
                &input.generics,
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

fn gen_item_interior_mut_impl(repr: Option<&ReprKind>, input: &syn::DeriveInput) -> TokenStream {
    match &input.data {
        syn::Data::Struct(data) => {
            gen_struct_interior_mut_impl(&input.ident, &input.generics, &data.fields)
        }
        syn::Data::Enum(data)
            if matches!(repr, None | Some(ReprKind::Transparent)) && data.variants.len() == 1 =>
        {
            gen_enum_interior_mut_impl(&input.ident, &input.generics, &data.variants[0])
        }
        syn::Data::Enum(_) | syn::Data::Union(_) => quote! {},
    }
}

fn gen_struct_interior_mut_impl(
    name: &Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
) -> TokenStream {
    if fields.len() != 1 {
        return quote! {};
    }

    let field = fields.iter().next().expect("single-field struct");
    let field_access = match &field.ident {
        Some(ident) => quote! { &self.#ident },
        None => quote! { &self.0 },
    };

    gen_interior_mut_impl(
        name,
        generics,
        field,
        quote! { co3::cell::InteriorMut::get(#field_access) },
    )
}

fn gen_enum_interior_mut_impl(
    name: &Ident,
    generics: &syn::Generics,
    variant: &syn::Variant,
) -> TokenStream {
    if variant.fields.len() != 1 {
        return quote! {};
    }

    let field = variant.fields.iter().next().expect("single-field variant");
    let variant_name = &variant.ident;
    let (pattern, field_access) = match &field.ident {
        Some(field_name) => (
            quote! { Self::#variant_name { #field_name } },
            quote! { #field_name },
        ),
        None => {
            let field_name = format_ident!("__co3_interior_mut_field");
            (
                quote! { Self::#variant_name(#field_name) },
                quote! { #field_name },
            )
        }
    };

    gen_interior_mut_impl(
        name,
        generics,
        field,
        quote! {
            let #pattern = self;
            co3::cell::InteriorMut::get(#field_access)
        },
    )
}

fn gen_interior_mut_impl(
    name: &Ident,
    generics: &syn::Generics,
    field: &syn::Field,
    get: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);
    let field_ty = &field.ty;
    let for_dummy = (!is_type_parametrized(field_ty, generics)).then_some(quote! { for<'_dummy> });

    quote! {
        unsafe impl #impl_generics co3::cell::InteriorMut for #name #ty_generics
        where
            #for_dummy #field_ty: co3::cell::InteriorMut,
            #predicates
        {
            type Target = <#field_ty as co3::cell::InteriorMut>::Target;

            #[inline(always)]
            fn get(&self) -> *mut Self::Target {
                #get
            }
        }
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
    let decode_store = decode_store_type(fields);

    let codec_impls = {
        let fields_destructure = gen_fields_destructure(fields);

        let ctype_name = if is_view {
            gen_view_ctype_name(name)
        } else {
            gen_ctype_name(name)
        };

        let (encode_body, decode_body) =
            gen_record_conversion(None, quote!(Self), fields, is_valid);

        CodecImpls {
            encode_store,
            decode_store,
            encode_impl: quote! {
                let Self #fields_destructure = self;
                #ctype_name #encode_body
            },
            decode_impl: quote! {
                let #ctype_name #fields_destructure = source;
                #decode_body
            },
        }
    };

    gen_codec_impls::<false>(is_view, name, generics, &field_types, codec_impls)
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

    let (encode_store, decode_store) = {
        let either_name = gen_either_name(variants.len());

        let field_encode_stores = variants.iter().map(|v| encode_store_type(&v.fields));
        let field_decode_stores = variants.iter().map(|v| decode_store_type(&v.fields));

        (
            quote! { core::option::Option<co3::either::#either_name<#(#field_encode_stores),*>> },
            quote! { core::option::Option<co3::either::#either_name<#(#field_decode_stores),*>> },
        )
    };

    let (variants_encode, variants_decode): (Vec<_>, Vec<_>) = variants
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
            let custom_is_valid = variant_attrs[idx].is_valid.as_ref();
            let variant_tag = match repr {
                None | Some(ReprKind::Primitive(_)) => {
                    Some(quote!(#tag_value as #tag_type))
                }
                Some(ReprKind::Transparent | ReprKind::C(Some(_))) => None,
                Some(ReprKind::C(None)) => unreachable!(),
            };

            let (encode_body, decode_body) = gen_record_conversion(
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

            (
                quote! {
                    Self::#variant_name #destructure_fields => {
                        #encode_store_init
                        #encode_variant
                    }
                },
                quote! { #tag_value => #decode_variant },
            )
        })
        .unzip();

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

    let codec_impls = CodecImpls {
        encode_store,
        decode_store,
        encode_impl: quote! {
            match self {
                #(#variants_encode,)*
            }
        },
        decode_impl,
    };

    gen_codec_impls::<true>(is_view, name, generics, &fields, codec_impls)
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
    let custom_is_valid = variant_attrs
        .first()
        .and_then(|attrs| attrs.is_valid.as_ref());
    let encode_store = encode_store_type(&variant.fields);
    let decode_store = decode_store_type(&variant.fields);
    let (encode_body, decode_body) = gen_record_conversion(
        None,
        quote!(Self::#variant_name),
        &variant.fields,
        custom_is_valid,
    );

    gen_codec_impls::<true>(
        is_view,
        name,
        generics,
        &field_types,
        CodecImpls {
            encode_store,
            decode_store,
            encode_impl: quote! {
                match self {
                    Self::#variant_name #destructure_fields => {
                        #ctype_name #encode_body
                    }
                }
            },
            decode_impl: quote! {
                let #ctype_name #destructure_fields = source;
                #decode_body
            },
        },
    )
}

fn gen_repr_c_struct_impls<const ADD_COPY: bool>(
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

    gen_repr_c_impls::<ADD_COPY>(is_view, name, generics, &field_types, is_valid_impl)
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

    gen_repr_c_impls::<true>(is_view, name, generics, &fields, is_valid_impl)
}

fn gen_repr_c_impls<const ADD_COPY: bool>(
    is_view: bool,
    name: &Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
    is_valid_body: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let checked_transmute_bounds = gen_checked_transmute_bounds::<ADD_COPY>(generics, fields);
    let (borrow_cast_bounds, cast_eq_bounds) = if is_view {
        (
            gen_borrow_cast_view_bounds::<ADD_COPY>(generics, fields),
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

fn gen_checked_transmute_bounds<const ADD_COPY: bool>(
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
            let for_dummy = (!is_type_parametrized(ty, generics)).then_some(quote!(for<'_dummy>));

            let ctype_bound = if ADD_COPY {
                quote!(<CType: Copy>)
            } else {
                quote! {<CType: Sized>}
            };

            parse_quote! { #for_dummy #ty: co3::transmute::CheckedTransmute #ctype_bound }
        })
        .collect::<Vec<_>>();

    if !is_phantom_data(last) {
        let for_dummy = (!is_type_parametrized(last, generics)).then_some(quote!(for<'_dummy>));
        let copy_bound = ADD_COPY.then(|| quote! { <CType: Copy> });

        predicates.push(parse_quote! {
            #for_dummy #last: co3::transmute::CheckedTransmute #copy_bound
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
    vis: &syn::Visibility,
    name: &Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> TokenStream {
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
        Some(ReprKind::C(Some(repr)) | ReprKind::Primitive(repr)) => {
            let variant_tags = variants.iter().map(|variant| {
                let variant_name = &variant.ident;
                quote! { target.0 == Self::#variant_name as #repr }
            });

            quote! {
                unsafe fn is_valid(target: &Self::CType) -> bool {
                    false #(|| #variant_tags)*
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
    let variants_decode = variants.iter().map(|variant| {
        let variant_name = &variant.ident;
        quote! { value if value == Self::#variant_name as #tag_ctype => Some(Self::#variant_name) }
    });

    let ctype_name = gen_ctype_name(name);
    let ctype_ty = quote!(#ctype_name #ty_generics);
    let ctype_def = gen_fieldless_enum_ctype(&tag_ctype, alignment, vis, name, generics);
    let encode_impl = tag_type
        .as_ref()
        .map(|repr| quote! { #ctype_name(self as #repr) })
        .unwrap_or_else(|| quote! { #ctype_name(()) });
    let decode_impl = if tag_type.is_some() {
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

    let borrow_impls = gen_identity_borrow_impls(name, generics);

    quote! {
        #ctype_def
        #niche_impl
        #borrow_impls

        impl #impl_generics co3::ReprC for #name #ty_generics #where_clause {
            type CType = #ctype_ty;
        }
        unsafe impl #impl_generics co3::stored::EncodeOwned for #name #ty_generics #where_clause {
            type Store = ();

            fn soft_encode<'_išč>(self, (): &mut ()) -> Self::CType
            where
                Self: '_išč,
            {
                #encode_impl
            }
        }

        unsafe impl #decode_impl_generics co3::stored::DecodeOwned<'_dšč> for #name #ty_generics #where_clause {
            type Store = ();

            unsafe fn soft_decode<'_išč: '_dšč>(source: Self::CType, (): &mut ()) -> Option<Self> {
                #decode_impl
            }
        }

        impl #impl_generics co3::Encode for #name #ty_generics #where_clause {}
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
) -> (TokenStream, TokenStream) {
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
            quote! { #(
                let #field_vars = unsafe {
                    co3::stored::DecodeOwned::soft_decode(#field_vars, #store_vars)?
                }; )*

                #custom_validation
                Some(#source_head {
                    #(#field_vars),*
                })
            },
        ),
        syn::Fields::Unnamed(_) => (
            quote! {
                (
                    #tag_element
                    #(co3::stored::EncodeOwned::soft_encode(#field_vars, #store_vars)),*
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
        ),
    }
}

struct CodecImpls {
    encode_store: TokenStream,
    decode_store: TokenStream,
    encode_impl: TokenStream,
    decode_impl: TokenStream,
}

fn gen_codec_impls<const ADD_COPY: bool>(
    is_view: bool,
    name: &Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
    codec_impls: CodecImpls,
) -> TokenStream {
    let CodecImpls {
        encode_store,
        decode_store,
        encode_impl,
        decode_impl,
    } = codec_impls;

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let extern_c_bounds = if is_view {
        Vec::new()
    } else {
        gen_extern_c_bounds_for_ctype::<ADD_COPY>(generics, fields)
    };

    let encode_owned_bounds =
        gen_field_encode_bounds(generics, fields, quote! { co3::stored::EncodeOwned });
    let decode_owned_bounds =
        gen_field_decode_bounds(generics, fields, quote! { co3::stored::DecodeOwned });

    let encode_bounds = gen_field_encode_bounds(generics, fields, quote! { co3::Encode });
    let decode_bounds = gen_field_decode_bounds(generics, fields, quote! { co3::Decode });

    let sized_bound = if is_view {
        quote! {}
    } else if !is_view && generics.type_params().count() == 0 {
        quote!(for<'_dummy> Self: Sized,)
    } else {
        quote!(Self: Sized,)
    };

    let has_view_lifetime = is_view
        && matches!(
            generics.params.first(),
            Some(syn::GenericParam::Lifetime(param)) if param.lifetime.ident == "_dšč"
        );

    let (borrow_cast_bounds, cast_eq_bounds) = if is_view {
        (
            gen_borrow_cast_view_bounds::<ADD_COPY>(generics, fields),
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

    let ctype_name = if is_view {
        gen_view_ctype_name(name)
    } else {
        gen_ctype_name(name)
    };

    let ctype_ty_generics = if is_view {
        let ty_generics =
            generic_param_idents(generics.params.iter().skip(has_view_lifetime as usize))
                .collect::<Vec<_>>();

        quote! { <#(#ty_generics),*> }
    } else {
        quote! { #ty_generics }
    };

    quote! {
        impl #impl_generics co3::ReprC for #name #ty_generics where
            #(#borrow_cast_bounds,)*
            #(#extern_c_bounds,)*
            #predicates
        {
            type CType = #ctype_name #ctype_ty_generics;
        }

        unsafe impl #impl_generics co3::stored::EncodeOwned for #name #ty_generics
        where
            #sized_bound
            #(#borrow_cast_bounds,)*
            #(#encode_owned_bounds,)*
            #cast_eq_bounds
            #predicates
        {
            type Store = #encode_store;

            fn soft_encode<'_išč>(self, store: &'_išč mut Self::Store) -> Self::CType where Self: '_išč {
                #encode_impl
            }
        }
        unsafe impl #decode_impl_generics co3::stored::DecodeOwned<'_dšč> for #name #ty_generics
        where
            #sized_bound
            #(#borrow_cast_bounds,)*
            #(#decode_owned_bounds,)*
            #cast_eq_bounds
            #predicates
        {
            type Store = #decode_store;

            unsafe fn soft_decode<'_išč: '_dšč>(source: Self::CType, store: &'_išč mut Self::Store) -> Option<Self> {
                #decode_impl
            }
        }

        impl #impl_generics co3::Encode for #name #ty_generics where
            #sized_bound
            #(#borrow_cast_bounds,)*
            #(#encode_bounds,)*
            #cast_eq_bounds
            #predicates
        {}
        impl #decode_impl_generics co3::Decode<'_dšč> for #name #ty_generics where
            #sized_bound
            #(#borrow_cast_bounds,)*
            #(#decode_bounds,)*
            #cast_eq_bounds
            #predicates
        {}
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
            let for_dummy =
                (!is_type_parametrized(ty, generics)).then_some(quote! { for<'_dummy> });
            parse_quote! { #for_dummy #ty: #bound<CType: Copy> }
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
            let for_dummy =
                (!is_type_parametrized(ty, generics)).then_some(quote! { for<'_dummy> });
            parse_quote! { #for_dummy #ty: #bound<'_dšč, CType: Copy> }
        })
}

fn encode_store_type(fields: &syn::Fields) -> TokenStream {
    let fields = fields.iter().map(|syn::Field { ty, .. }| {
        quote! { <#ty as co3::stored::EncodeOwned>::Store }
    });

    quote!((#(#fields,)*))
}

fn decode_store_type(fields: &syn::Fields) -> TokenStream {
    let fields = fields.iter().map(|syn::Field { ty, .. }| {
        quote! { <#ty as co3::stored::DecodeOwned<'_dšč>>::Store }
    });

    quote!((#(#fields,)*))
}

fn gen_borrow_cast_view_bounds<const ADD_COPY: bool>(
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
        .filter(|ty| is_type_parametrized(ty, generics))
        .map(|ty| {
            let ctype_bound = if ADD_COPY {
                quote! { <AsConst: Copy> + Copy }
            } else {
                quote! { <AsConst: Sized> + Sized }
            };

            quote! { #ty: co3::ReprC<CType: co3::borrow::BorrowCast #ctype_bound> }
        })
        .collect::<Vec<_>>();

    if !is_phantom_data(last) {
        let last = borrow_ty(last);
        if is_type_parametrized(last, generics) {
            let ctype_bound = ADD_COPY.then(|| quote! { <AsConst: Copy> + Copy });
            predicates.push(quote!(#last: co3::ReprC<CType: co3::borrow::BorrowCast #ctype_bound>));
        }
    }

    predicates
}
