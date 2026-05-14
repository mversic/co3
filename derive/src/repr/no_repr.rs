use core::str::FromStr as _;

use darling::{
    ast::{Fields, Style},
    util::SpannedValue,
};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Ident, parse_quote};

use crate::{
    attr::repr::ReprPrimitive,
    repr::{
        FfiTypeField, FfiTypeVariant, derive_extern_c_internal, gen_sized_family,
        gen_struct_size_family, is_type_parameterized,
        niche::{gen_enum_niche_ir, gen_struct_niche_ir_with_mode},
        repr_c::{
            ReprFamily, assert_no_drop, gen_data_enum, gen_data_enum_variant_name,
            gen_extern_c_bounds, gen_identity_borrow_ir, gen_repr_c_item_name, gen_repr_c_struct,
        },
    },
};

pub(super) fn derive_no_repr_struct<const IS_VIEW: bool>(
    name: &Ident,
    vis: &syn::Visibility,
    generics: &syn::Generics,
    fields: &Fields<FfiTypeField>,
) -> TokenStream {
    let repr_c_struct_name = gen_repr_c_item_name(name);
    let repr_c_struct =
        gen_repr_c_struct::<IS_VIEW>(name, vis, generics, fields, ReprFamily::NoRepr);
    let field_types = fields.iter().map(|f| &f.ty).collect::<Vec<_>>();
    let self_bounds = gen_extern_c_bounds::<false>(&field_types, generics);
    let size_family_impl = gen_struct_size_family(name, generics, &field_types, self_bounds);

    let field_rust_stores = fields.iter().map(|field| {
        let ty = &field.ty;
        quote! { <#ty as co3::stored::SoftEncodeOwned>::Store }
    });
    let field_ffi_stores = fields.iter().map(|field| {
        let ty = &field.ty;
        quote! { <#ty as co3::stored::SoftDecodeOwned<'_dšč>>::Store }
    });
    let store_defs = IS_VIEW.then(|| gen_custom_store_types(name, vis, &field_types));
    let basic_impls = if IS_VIEW {
        gen_view_ir_impl::<false>(name, generics, &field_types)
    } else {
        gen_ir_impl::<false>(name, &repr_c_struct_name, &field_types, generics)
    };
    let store_name = gen_store_name(name);
    let rust_store = quote! { #store_name<#(#field_rust_stores),*> };
    let ffi_store = quote! { #store_name<#(#field_ffi_stores),*> };

    let encode_impl = match &fields.style {
        Style::Struct => {
            let field_names: Vec<_> = fields.iter().filter_map(|f| f.ident.as_ref()).collect();
            let field_indices: Vec<_> = (0..field_names.len()).map(syn::Index::from).collect();

            quote! {
                let Self { #(#field_names),* } = self;

                #repr_c_struct_name {
                    #(#field_names: co3::stored::SoftEncodeOwned::encode(#field_names, &mut store.#field_indices)),*
                }
            }
        }
        Style::Tuple => {
            let field_indices: Vec<_> = (0..fields.len()).map(syn::Index::from).collect();
            let field_vars: Vec<_> = (0..fields.len())
                .map(|i| Ident::new(&format!("_{}", i), Span::call_site()))
                .collect();

            quote! {
                let Self(#(#field_vars),*) = self;

                #repr_c_struct_name(
                    #(co3::stored::SoftEncodeOwned::encode(#field_vars, &mut store.#field_indices)),*
                )
            }
        }
        Style::Unit => unreachable!("ZSTs are not FFI safe"),
    };
    let decode_impl = match &fields.style {
        Style::Struct => {
            let field_names: Vec<_> = fields.iter().filter_map(|f| f.ident.as_ref()).collect();
            let field_indices: Vec<_> = (0..field_names.len()).map(syn::Index::from).collect();

            quote! {
                Some(Self {
                    #(#field_names: unsafe {
                        co3::stored::SoftDecodeOwned::decode(source.#field_names, &mut store.#field_indices)?
                    }),*
                })
            }
        }
        Style::Tuple => {
            let field_indices: Vec<_> = (0..fields.len()).map(syn::Index::from).collect();

            quote! {
                Some(Self(
                    #(unsafe {
                        co3::stored::SoftDecodeOwned::decode(source.#field_indices, &mut store.#field_indices)?
                    }),*
                ))
            }
        }
        Style::Unit => unreachable!("ZSTs are not FFI safe"),
    };

    let conversion_impls = gen_no_repr_conversion_impls::<IS_VIEW, false>(
        name,
        generics,
        &field_types,
        rust_store,
        ffi_store,
        encode_impl,
        decode_impl,
    );
    let niche_ir = gen_struct_niche_ir_with_mode(name, generics, fields, ReprFamily::NoRepr, None);
    let borrow_ir = if IS_VIEW {
        gen_identity_borrow_ir(name, generics)
    } else {
        gen_struct_borrow_ir(None, name, vis, generics, fields)
    };
    quote! {
        #repr_c_struct

        #size_family_impl
        #basic_impls
        #store_defs
        #niche_ir
        #borrow_ir

        #conversion_impls
    }
}

pub(super) fn derive_no_repr_data_enum<const IS_VIEW: bool>(
    enum_name: &Ident,
    vis: &syn::Visibility,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> TokenStream {
    let inferred_repr = infer_repr(variants.len());
    let mut field_types = Vec::new();
    for variant in variants.iter() {
        for field in variant.fields.iter() {
            field_types.push(&field.ty);
        }
    }
    let size_family_impl = gen_sized_family(
        enum_name,
        generics,
        gen_extern_c_bounds::<true>(&field_types, generics),
    );

    let repr_c_enum_name = gen_repr_c_item_name(enum_name);
    let repr_c_enum = gen_data_enum::<IS_VIEW>(
        enum_name,
        vis,
        generics,
        inferred_repr,
        variants,
        ReprFamily::NoRepr,
    );

    let variant_rust_stores = variants.iter().map(|variant| {
        variant_mapper(
            variant,
            || quote! { () },
            |field| {
                let ty = &field.ty;
                quote! { <#ty as co3::stored::SoftEncodeOwned>::Store }
            },
        )
    });
    let variant_ffi_stores = variants.iter().map(|variant| {
        variant_mapper(
            variant,
            || quote! { () },
            |field| {
                let ty = &field.ty;
                quote! { <#ty as co3::stored::SoftDecodeOwned<'_dšč>>::Store }
            },
        )
    });
    let store_defs = IS_VIEW.then(|| gen_custom_store_types(enum_name, vis, &field_types));
    let basic_impls = if IS_VIEW {
        gen_view_ir_impl::<true>(enum_name, generics, &field_types)
    } else {
        gen_ir_impl::<true>(enum_name, &repr_c_enum_name, &field_types, generics)
    };
    let store_name = gen_store_name(enum_name);
    let rust_store = quote! { #store_name<#(#variant_rust_stores),*> };
    let ffi_store = quote! { #store_name<#(#variant_ffi_stores),*> };
    let mut variants_encode = Vec::with_capacity(variants.len());
    for (i, variant) in variants.iter().enumerate() {
        let idx = TokenStream::from_str(&format!("{i}")).expect("Valid");
        let variant_name = &variant.ident;
        let variant_struct_name = gen_data_enum_variant_name(enum_name, variant_name);
        variants_encode.push(variant_mapper(
            variant,
            || {
                quote! { Self::#variant_name => #repr_c_enum_name {
                    #variant_name: #variant_struct_name { tag: #idx }
                }}
            },
            |_| {
                quote! {
                    Self::#variant_name(payload) => {
                        #repr_c_enum_name {
                            #variant_name: #variant_struct_name {
                                tag: #idx,
                                value: co3::stored::SoftEncodeOwned::encode(payload, &mut store.#idx)
                            }
                        }
                    }
                }
            },
        ));
    }
    let enum_encode_impl = quote! {
        match self {
            #(#variants_encode,)*
        }
    };
    let variants_decode = variants.iter().enumerate().map(|(i, variant)| {
        let idx = TokenStream::from_str(&format!("{i}")).unwrap();
        let variant_name = &variant.ident;

        variant_mapper(
            variant,
            || quote! { #idx => Some(Self::#variant_name) },
            |_| {
                quote! {
                    #idx => {
                        let source = unsafe { source.#variant_name };

                        Some(Self::#variant_name(unsafe {
                            co3::stored::SoftDecodeOwned::decode(source.value, &mut store.#idx)?
                        }))
                    }
                }
            },
        )
    });
    let decode_impl = quote! {
        let repr_value = <*const _>::cast::<#inferred_repr>(core::ptr::from_ref(&source));

        match unsafe { *repr_value } {
            #(#variants_decode,)*
            _ => None,
        }
    };

    let niche_ir =
        (!IS_VIEW).then(|| gen_enum_niche_ir(inferred_repr, enum_name, generics, variants));
    let conversion_impls = gen_no_repr_conversion_impls::<IS_VIEW, true>(
        enum_name,
        generics,
        &field_types,
        rust_store,
        ffi_store,
        enum_encode_impl,
        decode_impl,
    );
    let borrow_ir = if IS_VIEW {
        gen_identity_borrow_ir(enum_name, generics)
    } else {
        gen_data_enum_borrow_ir(None, enum_name, vis, generics, variants)
    };

    quote! {
        #repr_c_enum

        #size_family_impl
        #basic_impls
        #store_defs
        #niche_ir
        #borrow_ir

        #conversion_impls
    }
}

fn gen_no_repr_conversion_impls<const IS_VIEW: bool, const ADD_COPY: bool>(
    item_name: &Ident,
    generics: &syn::Generics,
    field_types: &[&syn::Type],
    rust_store: TokenStream,
    ffi_store: TokenStream,
    encode_impl: TokenStream,
    decode_impl: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let repr_c_name = gen_repr_c_item_name(item_name);
    let params = &generics.params;
    let lifetime = if IS_VIEW {
        quote! {}
    } else {
        quote! { '_dšč, }
    };

    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let decode_bounds = gen_decode_bounds(field_types);
    let decode_owned_bounds = gen_decode_owned_bounds::<ADD_COPY>(field_types, generics);
    let encode_bounds = gen_encode_bounds(field_types, generics);
    let encode_owned_bounds = gen_encode_owned_bounds::<ADD_COPY>(field_types, generics);

    let extern_c_bounds = if IS_VIEW {
        let view_bounds = gen_view_bounds::<ADD_COPY>(field_types.iter().copied(), generics);
        let cast_eq_bounds = gen_borrow_cast_eq_bounds(field_types);

        quote! {
            #view_bounds
            #cast_eq_bounds
        }
    } else {
        quote! {}
    };

    let encode_impl = if IS_VIEW {
        quote! {
            let __co3_ctype = {
                #encode_impl
            };

            unsafe { core::mem::transmute_copy::<#repr_c_name #ty_generics, Self::CType>(&__co3_ctype) }
        }
    } else {
        encode_impl
    };
    let decode_impl = if IS_VIEW {
        quote! {
            let source = unsafe {
                core::mem::transmute_copy::<Self::CType, #repr_c_name #ty_generics>(&source)
            };

            #decode_impl
        }
    } else {
        decode_impl
    };
    let is_parametrized = field_types
        .iter()
        .any(|ty| is_type_parameterized(ty, generics));
    let for_dummy = (!is_parametrized).then_some(quote! { for<'_dummy> });

    quote! {
        impl #impl_generics co3::SoftEncode for #item_name #ty_generics where
            #for_dummy Self: co3::stored::SoftEncodeOwned,
            #encode_bounds
            #predicates
        {}

        impl #impl_generics co3::stored::SoftEncodeOwned for #item_name #ty_generics
        where
            #encode_owned_bounds
            #extern_c_bounds
            #predicates
        {
            type Store = #rust_store;

            fn encode<'_išč>(self, store: &'_išč mut Self::Store) -> Self::CType where Self: '_išč {
                #encode_impl
            }
        }

        impl<#lifetime #params> co3::SoftDecode<'_dšč> for #item_name #ty_generics where
            Self: co3::stored::SoftDecodeOwned<'_dšč>,
            #decode_bounds
            #predicates
        {}

        impl<#lifetime #params> co3::stored::SoftDecodeOwned<'_dšč> for #item_name #ty_generics
        where
            #decode_owned_bounds
            #extern_c_bounds
            #predicates
        {
            type Store = #ffi_store;

            unsafe fn decode<'_išč: '_dšč>(source: Self::CType, store: &'_išč mut Self::Store) -> Option<Self> {
                #decode_impl
            }
        }
    }
}

pub(super) fn derive_no_repr_fieldless_enum(
    enum_name: &Ident,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> TokenStream {
    let inferred_repr = infer_repr(variants.len());
    let size_family_impl = gen_sized_family(enum_name, generics, quote! {});

    let basic_impls =
        gen_ir_impl::<false>(enum_name, &parse_quote!( #inferred_repr ), &[], generics);

    let variants_decode = variants.iter().enumerate().map(|(i, variant)| {
        let idx = TokenStream::from_str(&format!("{i}")).expect("Valid");
        let variant_name = &variant.ident;
        quote! { #idx => Some(Self::#variant_name) }
    });

    let niche_ir = gen_enum_niche_ir(inferred_repr, enum_name, generics, variants);

    let borrow_ir = gen_identity_borrow_ir(enum_name, generics);
    quote! {
        #size_family_impl
        #borrow_ir
        #basic_impls

        impl co3::SoftEncode for #enum_name {}
        impl co3::stored::SoftEncodeOwned for #enum_name {
            type Store = ();

            fn encode<'_išč>(self, (): &mut ()) -> Self::CType {
                self as #inferred_repr
            }
        }

        impl<'_dšč> co3::SoftDecode<'_dšč> for #enum_name {}
        impl<'_dšč> co3::stored::SoftDecodeOwned<'_dšč> for #enum_name {
            type Store = ();

            unsafe fn decode<'_išč: '_dšč>(source: Self::CType, (): &mut ()) -> Option<Self> {
                match source {
                    #(#variants_decode,)*
                    _ => None
                }
            }
        }

        #niche_ir
    }
}

fn gen_borrow_store_type(name: &Ident, fields: &[&syn::Type]) -> TokenStream {
    let store_name = gen_store_name(name);

    let stores = fields.iter().map(|field| {
        quote! { <#field as co3::borrow::Borrow>::Owner }
    });

    quote! { #store_name<#(#stores),*> }
}

pub(super) fn gen_struct_borrow_ir(
    repr_attr: Option<TokenStream>,
    name: &Ident,
    vis: &syn::Visibility,
    generics: &syn::Generics,
    fields: &Fields<FfiTypeField>,
) -> TokenStream {
    let field_types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();
    let borrowed_struct_name = gen_borrowed_name(name);
    let borrow_impl = match &fields.style {
        Style::Struct => {
            let field_names: Vec<_> = fields.iter().filter_map(|f| f.ident.as_ref()).collect();
            let field_indices: Vec<_> = (0..field_names.len()).map(syn::Index::from).collect();

            quote! {
                let Self { #(#field_names),* } = self;

                #borrowed_struct_name {
                    #(#field_names: co3::borrow::Borrow::borrow(#field_names, &mut store.#field_indices)),*
                }
            }
        }
        Style::Tuple => {
            let field_indices: Vec<_> = (0..fields.len()).map(syn::Index::from).collect();
            let field_vars: Vec<_> = (0..fields.len())
                .map(|i| Ident::new(&format!("_{}", i), Span::call_site()))
                .collect();

            quote! {
                let Self(#(#field_vars),*) = self;

                #borrowed_struct_name(
                    #(co3::borrow::Borrow::borrow(#field_vars, &mut store.#field_indices)),*
                )
            }
        }
        Style::Unit => unreachable!("ZSTs are not FFI safe"),
    };

    let to_owned_impl = match &fields.style {
        Style::Struct => {
            let field_names: Vec<_> = fields.iter().filter_map(|f| f.ident.as_ref()).collect();

            quote! {
                let #borrowed_struct_name { #(#field_names),* } = source;
                Self { #(#field_names: co3::borrow::ToOwned::to_owned(#field_names)),* }
            }
        }
        Style::Tuple => {
            let field_vars: Vec<_> = (0..fields.len())
                .map(|i| Ident::new(&format!("_{}", i), Span::call_site()))
                .collect();

            quote! {
                let #borrowed_struct_name(#(#field_vars),*) = source;
                Self(#(co3::borrow::ToOwned::to_owned(#field_vars)),*)
            }
        }
        Style::Unit => unreachable!("ZSTs are not FFI safe"),
    };

    gen_borrow::<_, true>(
        name,
        vis,
        generics,
        &field_types,
        || gen_borrowed_struct(repr_attr, name, vis, generics, fields),
        &borrow_impl,
        &to_owned_impl,
    )
}

pub(super) fn gen_data_enum_borrow_ir(
    repr_attr: Option<TokenStream>,
    enum_name: &Ident,
    vis: &syn::Visibility,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> TokenStream {
    let mut field_types = Vec::new();
    let mut variants_borrow = Vec::with_capacity(variants.len());

    let borrowed_enum_name = gen_borrowed_name(enum_name);
    for (i, variant) in variants.iter().enumerate() {
        let idx = TokenStream::from_str(&format!("{i}")).unwrap();

        for field in variant.fields.iter() {
            field_types.push(&field.ty);
        }

        let variant_name = &variant.ident;
        variants_borrow.push(variant_mapper(
            variant,
            || quote! { Self::#variant_name => #borrowed_enum_name::#variant_name },
            |_| {
                quote! {
                    Self::#variant_name(payload) => #borrowed_enum_name::#variant_name(
                        co3::borrow::Borrow::borrow(payload, &mut store.#idx)
                    )
                }
            },
        ));
    }

    let borrow_impl = quote! {
        match self {
            #(#variants_borrow,)*
        }
    };

    let variants_to_owned = variants.iter().map(|variant| {
        let variant_name = &variant.ident;
        variant_mapper(
            variant,
            || quote! { #borrowed_enum_name::#variant_name => Some(Self::#variant_name) },
            |_| {
                quote! {
                    #borrowed_enum_name::#variant_name(payload) => {
                        Self::#variant_name(co3::borrow::ToOwned::to_owned(payload))
                    }
                }
            },
        )
    });
    let to_owned_impl = quote! {
        match source {
            #(#variants_to_owned,)*
        }
    };

    gen_borrow::<_, false>(
        enum_name,
        vis,
        generics,
        &field_types,
        || gen_borrowed_data_enum(repr_attr, enum_name, vis, generics, variants),
        &borrow_impl,
        &to_owned_impl,
    )
}

fn gen_borrow<F, const ADD_SIZED_BOUND: bool>(
    name: &Ident,
    vis: &syn::Visibility,
    generics: &syn::Generics,
    field_types: &[&syn::Type],
    gen_borrowed_item: F,
    borrow_impl: &TokenStream,
    to_owned_impl: &TokenStream,
) -> TokenStream
where
    F: FnOnce() -> TokenStream,
{
    let borrowed_item = gen_borrowed_item();
    let store_defs = gen_custom_store_types(name, vis, field_types);
    let borrow_store = gen_borrow_store_type(name, field_types);

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let params = &generics.params;
    let impl_drop_assert = assert_no_drop(generics, name);
    let borrow_bounds = gen_borrow_bounds::<false>(field_types, generics);
    let to_owned_bounds = gen_to_owned_bounds(field_types, generics);
    let borrowed_name = gen_borrowed_name(name);
    let mut borrowed_ty_args = Vec::new();
    if needs_borrow_lifetime_param(field_types) {
        borrowed_ty_args.push(quote! { '_išč });
    }
    borrowed_ty_args.extend(generics.params.iter().map(|param| match param {
        syn::GenericParam::Lifetime(param) => {
            let lifetime = &param.lifetime;
            quote! { #lifetime }
        }
        syn::GenericParam::Type(param) => {
            let ident = &param.ident;
            quote! { #ident }
        }
        syn::GenericParam::Const(param) => {
            let ident = &param.ident;
            quote! { #ident }
        }
    }));
    let borrowed_ty_generics = if borrowed_ty_args.is_empty() {
        quote! {}
    } else {
        quote! { <#(#borrowed_ty_args),*> }
    };

    let sized_bound = (ADD_SIZED_BOUND && generics.params.is_empty())
        .then_some(quote! { for<'_dummy> Self: Sized, });

    quote! {
        #borrowed_item
        #store_defs

        impl #impl_generics co3::borrow::Borrow for #name #ty_generics
        where
            #borrow_bounds
            #sized_bound
            #predicates
        {
            type Borrowed<'_išč> = #borrowed_name #borrowed_ty_generics
            where
                Self: '_išč;

            type Owner = #borrow_store;

            #[inline(always)]
            fn borrow<'_išč>(self, store: &'_išč mut Self::Owner) -> Self::Borrowed<'_išč>
            where
                Self: '_išč,
            {
                #impl_drop_assert
                #borrow_impl
            }
        }

        impl<'_išč, #params> co3::borrow::ToOwned<'_išč> for #name #ty_generics
        where
            #to_owned_bounds
            #predicates
        {
            #[inline(always)]
            fn to_owned(source: Self::Borrowed<'_išč>) -> Self {
                #to_owned_impl
            }
        }
    }
}

fn gen_store_name(name: &syn::Ident) -> syn::Ident {
    format_ident!("{name}Store")
}

fn gen_borrowed_name(name: &Ident) -> Ident {
    format_ident!("{name}View")
}

fn borrowed_view_lifetime(ty: &syn::Type) -> Option<syn::Lifetime> {
    match ty {
        syn::Type::Reference(reference) => {
            let lifetime = reference.lifetime.as_ref()?;
            (lifetime.ident != "_").then(|| lifetime.clone())
        }
        _ => None,
    }
}

fn borrowed_view_type(ty: &syn::Type) -> TokenStream {
    let lifetime = borrowed_view_lifetime(ty)
        .map(|lifetime| quote!(#lifetime))
        .unwrap_or_else(|| quote!('_dšč));

    quote! { <#ty as co3::borrow::Borrow>::Borrowed<#lifetime> }
}

fn needs_borrow_lifetime_param(fields: &[&syn::Type]) -> bool {
    fields.iter().any(|ty| borrowed_view_lifetime(ty).is_none())
}

fn gen_borrowed_struct(
    repr_attr: Option<TokenStream>,
    name: &Ident,
    vis: &syn::Visibility,
    generics: &syn::Generics,
    fields: &Fields<FfiTypeField>,
) -> TokenStream {
    let (_, _, where_clause) = generics.split_for_impl();

    let params = &generics.params;
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let borrowed_name = gen_borrowed_name(name);
    let field_types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();
    let borrow_bounds = gen_borrow_bounds::<true>(&field_types, generics);
    let needs_borrow_lifetime = needs_borrow_lifetime_param(&field_types);
    let borrow_lifetime_param = needs_borrow_lifetime.then_some(quote!('_dšč,));
    let borrowed_field_types = fields
        .iter()
        .map(|field| borrowed_view_type(&field.ty))
        .collect::<Vec<_>>();

    let struct_item = match &fields.style {
        Style::Struct => {
            let field_names = fields.iter().filter_map(|f| f.ident.as_ref());

            quote! {
                #vis struct #borrowed_name<#borrow_lifetime_param #params>
                where
                    #borrow_bounds
                    #predicates
                {
                    #(#field_names: #borrowed_field_types),*
                }
            }
        }
        Style::Tuple => {
            quote! {
                #vis struct #borrowed_name<#borrow_lifetime_param #params>(#(#borrowed_field_types),*)
                where
                    #borrow_bounds
                    #predicates
                ;
            }
        }
        Style::Unit => unreachable!("ZSTs are not FFI safe"),
    };

    let struct_item = parse_quote! {
        #repr_attr
        #[doc(hidden)]
        #struct_item
    };
    let derived = derive_borrowed_helper(&struct_item);

    quote! {
        #struct_item
        #derived
    }
}

fn gen_borrowed_data_enum(
    repr_attr: Option<TokenStream>,
    enum_name: &Ident,
    vis: &syn::Visibility,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> TokenStream {
    let (_, _, where_clause) = generics.split_for_impl();

    let params = &generics.params;
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);
    let borrowed_name = gen_borrowed_name(enum_name);
    let mut field_types = Vec::new();
    for variant in variants {
        for field in variant.fields.iter() {
            field_types.push(&field.ty);
        }
    }
    let borrow_bounds = gen_borrow_bounds::<true>(&field_types, generics);
    let needs_borrow_lifetime = needs_borrow_lifetime_param(&field_types);
    let borrow_lifetime_param = needs_borrow_lifetime.then_some(quote!('_dšč,));
    let variants = variants.iter().map(|variant| {
        let variant_name = &variant.ident;
        variant_mapper(
            variant,
            || quote! { #variant_name },
            |field| {
                let borrowed_ty = borrowed_view_type(&field.ty);
                quote! { #variant_name(#borrowed_ty) }
            },
        )
    });

    let enum_ = parse_quote! {
        #[doc(hidden)]
        #repr_attr
        #vis enum #borrowed_name<#borrow_lifetime_param #params>
        where
            #borrow_bounds
            #predicates
        {
            #(#variants),*
        }
    };

    let derived = derive_borrowed_helper(&enum_);

    quote! {
        #enum_
        #derived
    }
}

fn derive_borrowed_helper(item: &syn::DeriveInput) -> TokenStream {
    match derive_extern_c_internal::<true>(item) {
        Ok(derived) => derived,
        Err(err) => err.to_compile_error(),
    }
}

fn infer_repr(num_variants: usize) -> ReprPrimitive {
    const U8_MAX: usize = u8::MAX as usize;
    const U16_MAX: usize = u16::MAX as usize;
    const U32_MAX: usize = u32::MAX as usize;
    const U64_MAX: usize = u64::MAX as usize;

    #[expect(clippy::match_overlapping_arm)]
    match num_variants {
        0..=U8_MAX => ReprPrimitive::U8,
        0..=U16_MAX => ReprPrimitive::U16,
        0..=U32_MAX => ReprPrimitive::U32,
        0..=U64_MAX => ReprPrimitive::U64,
        _ => unreachable!(),
    }
}

fn gen_ir_impl<const ADD_COPY: bool>(
    type_name: &Ident,
    repr_c_name: &Ident,
    fields: &[&syn::Type],
    generics: &syn::Generics,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let params = &generics.params;

    let extern_c_bounds = gen_extern_c_bounds::<ADD_COPY>(fields, generics);

    quote! {
        co3::reprC! {
            impl(#params) Stored for #type_name #ty_generics where (#predicates) {}
        }

        impl #impl_generics co3::ExternC for #type_name #ty_generics
        where
            #extern_c_bounds
            #predicates
        {
            type CType = #repr_c_name #ty_generics;
        }

        unsafe impl #impl_generics co3::handle::Erase for #type_name #ty_generics where
            #extern_c_bounds
            #predicates
        {
            // FIXME: The type is not correct, likewise in repr_c. Also fix bounds
            type Erased = <#repr_c_name #ty_generics as co3::handle::Erase>::Erased;
        }
    }
}

pub fn gen_custom_store_types(
    name: &Ident,
    vis: &syn::Visibility,
    field_types: &[&syn::Type],
) -> TokenStream {
    let store_name = gen_store_name(name);
    let idxs = (0..field_types.len())
        .map(syn::Index::from)
        .collect::<Vec<_>>();

    let store_defs = {
        let store_params = (0..field_types.len())
            .map(|idx| format_ident!("D{idx}"))
            .collect::<Vec<_>>();

        quote! {
            #vis struct #store_name<#(#store_params),*>(#(#store_params),*);

            impl<#(#store_params),*> Default for #store_name<#(#store_params),*> where #(#store_params: Default,)* {
                fn default() -> Self {
                    Self(#(<#store_params as Default>::default()),*)
                }
            }

            unsafe impl<#(#store_params),*> co3::out_ptr::Zst for #store_name<#(#store_params),*>
            where
                #(#store_params: co3::out_ptr::Zst,)*
            {}

            impl<#(#store_params),*> co3::stored::Store for #store_name<#(#store_params),*> where #(#store_params: co3::stored::Store,)* {
                fn sync(self) -> Option<()> {
                    #(self.#idxs.sync()?;)*
                    Some(())
                }
            }
        }
    };

    quote! { #store_defs }
}

pub(super) fn variant_mapper<T: Sized, F0: FnOnce() -> T, F1: FnOnce(&FfiTypeField) -> T>(
    variant: &SpannedValue<FfiTypeVariant>,
    unit_mapper: F0,
    field_mapper: F1,
) -> T {
    match &variant.fields.style {
        Style::Tuple if variant.fields.fields.len() == 1 => field_mapper(&variant.fields.fields[0]),
        Style::Unit => unit_mapper(),
        _ => unreachable!(),
    }
}

fn gen_view_ir_impl<const ADD_COPY: bool>(
    view_name: &Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let view_name_str = view_name.to_string();
    let owner_name = view_name_str.strip_suffix("View").unwrap();
    let repr_c_name = Ident::new(&format!("C{owner_name}ConstView"), view_name.span());
    let owner_generics = generics.params.iter().skip(1).map(|param| match param {
        syn::GenericParam::Lifetime(param) => {
            let lifetime = &param.lifetime;
            quote! { #lifetime }
        }
        syn::GenericParam::Type(param) => {
            let ident = &param.ident;
            quote! { #ident }
        }
        syn::GenericParam::Const(param) => {
            let ident = &param.ident;
            quote! { #ident }
        }
    });
    let view_bounds = gen_view_bounds::<ADD_COPY>(fields.iter().copied(), generics);

    let params = &generics.params;
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    quote! {
        co3::reprC! {
            impl(#params) Stored for #view_name #ty_generics where (#predicates) {}
        }

        impl #impl_generics co3::ExternC for #view_name #ty_generics where
            #view_bounds
            #predicates
        {
            type CType = #repr_c_name <#(#owner_generics),*>;
        }
    }
}

pub(super) fn gen_view_bounds<'a, const ADD_COPY: bool>(
    field_types: impl IntoIterator<Item = &'a syn::Type>,
    generics: &syn::Generics,
) -> TokenStream {
    let field_types = field_types
        .into_iter()
        .filter(|ty| is_type_parameterized(ty, generics))
        .map(|field_ty| match field_ty {
            syn::Type::Path(syn::TypePath {
                qself: Some(syn::QSelf { ty, .. }),
                ..
            }) => quote! { #ty },
            _ => quote! { #field_ty },
        });

    if ADD_COPY {
        quote! { #( #field_types: co3::ExternC<CType: co3::borrow::BorrowCast<AsConst: Copy> + Copy>,)* }
    } else {
        quote! { #( #field_types: co3::ExternC<CType: co3::borrow::BorrowCast>,)* }
    }
}

/// This bound is extremely important for the soundness of transmute of owned and borrowed `ReprC` forms
/// If every field is guaranteed transmutable by `BorrowCast` then the whole struct must be as well
pub(super) fn gen_borrow_cast_eq_bounds(fields: &[&syn::Type]) -> TokenStream {
    let bounds = fields.iter().map(|ty| {
        let syn::Type::Path(syn::TypePath {
            qself: Some(syn::QSelf { ty, .. }),
            ..
        }) = ty
        else {
            unreachable!()
        };

        quote! {
            <#ty as co3::borrow::Borrow>::Borrowed<'_dšč>: co3::ExternC<
                CType = <<#ty as co3::ExternC>::CType as co3::borrow::BorrowCast>::AsConst
            >,
        }
    });

    quote! { #(#bounds)* }
}

pub(super) fn gen_borrow_bounds<const WITH_LIFETIME: bool>(
    fields: &[&syn::Type],
    generics: &syn::Generics,
) -> TokenStream {
    // TODO: This fn looks complicated, simplify it
    let bounds = fields
        .iter()
        .filter(|ty| is_type_parameterized(ty, generics))
        .map(|ty| {
            let lifetime_bound = WITH_LIFETIME
                .then(|| borrowed_view_lifetime(ty).map(|lifetime| quote!( + #lifetime)))
                .flatten()
                .unwrap_or_else(|| {
                    WITH_LIFETIME
                        .then_some(quote!( + '_dšč))
                        .unwrap_or_default()
                });

            quote! { #ty: co3::borrow::Borrow #lifetime_bound, }
        });

    quote! { #(#bounds)* }
}

fn gen_to_owned_bounds(fields: &[&syn::Type], generics: &syn::Generics) -> TokenStream {
    let fields = fields
        .iter()
        .copied()
        .filter(|ty| is_type_parameterized(ty, generics));

    quote! { #(#fields: co3::borrow::ToOwned<'_išč>,)* }
}

fn gen_encode_bounds(fields: &[&syn::Type], generics: &syn::Generics) -> TokenStream {
    let bounds = fields.iter().map(|ty| {
        if is_type_parameterized(ty, generics) {
            quote! { #ty: co3::SoftEncode, }
        } else {
            quote! { for<'_dummy> #ty: co3::SoftEncode, }
        }
    });

    quote! { #(#bounds)* }
}

fn gen_decode_bounds(fields: &[&syn::Type]) -> TokenStream {
    let bounds = fields.iter().map(|ty| {
        quote! { #ty: co3::SoftDecode<'_dšč>, }
    });

    quote! { #(#bounds)* }
}

fn gen_encode_owned_bounds<const ADD_COPY: bool>(
    fields: &[&syn::Type],
    generics: &syn::Generics,
) -> TokenStream {
    let fields = fields
        .iter()
        .copied()
        .filter(|ty| is_type_parameterized(ty, generics));

    if ADD_COPY {
        quote! { #(#fields: co3::stored::SoftEncodeOwned<CType: Copy>,)* }
    } else {
        quote! { #(#fields: co3::stored::SoftEncodeOwned,)* }
    }
}

fn gen_decode_owned_bounds<const ADD_COPY: bool>(
    fields: &[&syn::Type],
    generics: &syn::Generics,
) -> TokenStream {
    let fields = fields
        .iter()
        .copied()
        .filter(|ty| is_type_parameterized(ty, generics));

    if ADD_COPY {
        quote! { #(#fields: co3::stored::SoftDecodeOwned<'_dšč, CType: Copy>,)* }
    } else {
        quote! { #(#fields: co3::stored::SoftDecodeOwned<'_dšč>,)* }
    }
}
