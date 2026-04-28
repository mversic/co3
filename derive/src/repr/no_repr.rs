use core::str::FromStr as _;
use std::collections::BTreeSet;

use darling::{
    ast::{Fields, Style},
    util::SpannedValue,
};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Ident, parse_quote, visit::Visit};

use crate::{
    attr::repr::ReprPrimitive,
    repr::{
        FfiTypeField, FfiTypeVariant, derive_extern_c_internal, is_type_parameterized,
        niche::{gen_enum_niche_ir, gen_struct_niche_ir},
        repr_c::{
            assert_drop_impl, gen_data_enum, gen_data_enum_variant_name, gen_extern_c_bounds,
            gen_fieldless_enum_drop_ir, gen_repr_c_struct, gen_sized_size_family,
            gen_struct_size_family,
        },
    },
    utils::build_type_tuple,
};

pub(super) fn derive_no_repr_struct<const NEEDS_DROP: bool>(
    name: &Ident,
    generics: &syn::Generics,
    fields: &Fields<FfiTypeField>,
    local: bool,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let (repr_c_struct_name, repr_c_struct) = gen_repr_c_struct(name, generics, fields);
    let field_types = fields.iter().map(|f| &f.ty).collect::<Vec<_>>();
    let size_family_impl = gen_struct_size_family(name, generics, &field_types);

    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let field_rust_stores = fields.iter().map(|field| {
        let ty = &field.ty;
        quote! { <#ty as co3::EncodeWithStore<false>>::Store }
    });
    let field_ffi_stores = fields.iter().map(|field| {
        let ty = &field.ty;
        quote! { <#ty as co3::DecodeWithStore<'_dšč>>::Store }
    });

    let basic_impls = gen_ir_impl(name, &repr_c_struct_name, &field_types, generics);
    let store_name = gen_store_name(name);
    let rust_store = quote! { #store_name<#(#field_rust_stores),*> };
    let ffi_store = quote! { #store_name<#(#field_ffi_stores),*> };

    let (encode_impl, decode_impl, decode_cloned_impl) = match &fields.style {
        Style::Struct => {
            let field_names: Vec<_> = fields.iter().filter_map(|f| f.ident.as_ref()).collect();
            let field_indices: Vec<_> = (0..field_names.len()).map(syn::Index::from).collect();

            (
                quote! {
                    let Self { #(#field_names),* } = self;

                    #repr_c_struct_name {
                        #(#field_names: co3::EncodeWithStore::encode(#field_names, &mut store.#field_indices)),*
                    }
                },
                quote! {
                    Some(Self {
                        #(#field_names: unsafe { co3::DecodeWithStore::decode(source.#field_names, &mut store.#field_indices)? }),*
                    })
                },
                quote! {
                    Some(Self {
                        #(#field_names: unsafe { co3::cloned::DecodeCloned::decode_cloned(source.#field_names, &mut store.#field_indices)? }),*
                    })
                },
            )
        }
        Style::Tuple => {
            let field_indices: Vec<_> = (0..fields.len()).map(syn::Index::from).collect();
            let field_vars: Vec<_> = (0..fields.len())
                .map(|i| Ident::new(&format!("_{}", i), Span::call_site()))
                .collect();

            (
                quote! {
                    let Self(#(#field_vars),*) = self;

                    #repr_c_struct_name(
                        #(co3::EncodeWithStore::encode(#field_vars, &mut store.#field_indices)),*
                    )
                },
                quote! {
                    Some(Self(
                        #(unsafe { co3::DecodeWithStore::decode(source.#field_indices, &mut store.#field_indices)? }),*
                    ))
                },
                quote! {
                    Some(Self(
                        #(unsafe { co3::cloned::DecodeCloned::decode_cloned(source.#field_indices, &mut store.#field_indices)? }),*
                    ))
                },
            )
        }
        Style::Unit => unreachable!("ZSTs are not FFI safe"),
    };

    let params = &generics.params;
    let decode_params = if generics
        .lifetimes()
        .any(|param| param.lifetime.ident == "_dšč")
    {
        quote! { #params }
    } else {
        quote! { '_dšč, #params }
    };
    let encode_bounds = gen_encode_bounds(&field_types, generics);
    let decode_bounds = gen_decode_bounds(&field_types, generics);
    let decode_cloned_bounds = gen_decode_cloned_bounds(&field_types);

    let borrow_ir = gen_struct_borrow_ir::<NEEDS_DROP>(None, name, generics, fields);
    let store_defs = (!NEEDS_DROP).then_some(gen_custom_store_types(name, &field_types));

    let niche_ir = gen_struct_niche_ir(name, generics, fields);
    let non_locality =
        (!local).then(|| gen_out_ptr_impls(name, generics, fields.iter().map(|f| f.ty.clone())));

    quote! {
        #repr_c_struct

        #size_family_impl
        #basic_impls
        #niche_ir
        #store_defs
        #borrow_ir

        impl #impl_generics co3::EncodeWithStore for #name #ty_generics where #encode_bounds #predicates {
            type Store = #rust_store;

            fn encode<'_išč>(self, store: &'_išč mut Self::Store) -> Self::CType where Self: '_išč {
                #encode_impl
            }
        }
        impl<#decode_params> co3::DecodeWithStore<'_dšč> for #name #ty_generics where
            #repr_c_struct_name #ty_generics: '_dšč,
            #decode_bounds #predicates
        {
            type Store = #ffi_store;

            unsafe fn decode<'_išč: '_dšč>(source: Self::CType, store: &'_išč mut Self::Store) -> Option<Self> {
                #decode_impl
            }
        }
        impl<#decode_params> co3::cloned::DecodeCloned<'_dšč> for #name #ty_generics where
            #repr_c_struct_name #ty_generics: '_dšč,
            #decode_cloned_bounds
            #predicates
        {
            unsafe fn decode_cloned<'_išč: '_dšč>(source: Self::CType, store: &'_išč mut Self::Store) -> Option<Self> {
                #decode_cloned_impl
            }
        }

        #non_locality
    }
}

pub(super) fn derive_no_repr_data_enum<const NEEDS_DROP: bool>(
    enum_name: &Ident,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
    local: bool,
) -> TokenStream {
    let inferred_repr = infer_repr(variants.len());
    let size_family_impl = gen_sized_size_family(enum_name, generics);

    let (repr_c_enum_name, repr_c_enum) =
        gen_data_enum(enum_name, generics, inferred_repr, variants);

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let params = &generics.params;
    let decode_params = if generics
        .lifetimes()
        .any(|param| param.lifetime.ident == "_dšč")
    {
        quote! { #params }
    } else {
        quote! { '_dšč, #params }
    };

    let variant_rust_stores = variants.iter().map(|variant| {
        variant_mapper(
            variant,
            || quote! { () },
            |field| {
                let ty = &field.ty;
                quote! { <#ty as co3::EncodeWithStore<false>>::Store }
            },
        )
    });
    let variant_ffi_stores = variants.iter().map(|variant| {
        variant_mapper(
            variant,
            || quote! { () },
            |field| {
                let ty = &field.ty;
                quote! { <#ty as co3::DecodeWithStore<'_dšč>>::Store }
            },
        )
    });

    let mut field_types = Vec::new();
    for variant in variants.iter() {
        for field in variant.fields.iter() {
            field_types.push(&field.ty);
        }
    }
    let basic_impls = gen_ir_impl(enum_name, &repr_c_enum_name, &field_types, generics);
    let store_name = gen_store_name(enum_name);
    let rust_store = quote! { #store_name<#(#variant_rust_stores),*> };
    let ffi_store = quote! { #store_name<#(#variant_ffi_stores),*> };
    let mut variants_encode = Vec::with_capacity(variants.len());
    let mut variants_decode = Vec::with_capacity(variants.len());
    let mut variants_decode_cloned = Vec::with_capacity(variants.len());
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
                                value: co3::EncodeWithStore::encode(payload, &mut store.#idx)
                            }
                        }
                    }
                }
            },
        ));
        variants_decode.push(variant_mapper(
                variant,
                || quote! { #idx => Some(Self::#variant_name) },
                |_| {
                    quote! {
                        #idx => {
                            let value = unsafe { source.#variant_name.value };
                            unsafe { co3::DecodeWithStore::<'_dšč>::decode(value, &mut store.#idx).map(Self::#variant_name) }
                        }
                    }
                },
            ));
        variants_decode_cloned.push(variant_mapper(
                variant,
                || quote! { #idx => Some(Self::#variant_name) },
                |_| {
                    quote! {
                        #idx => {
                            let value = unsafe { source.#variant_name.value };
                            unsafe { co3::cloned::DecodeCloned::<'_dšč>::decode_cloned(value, &mut store.#idx).map(Self::#variant_name) }
                        }
                    }
                },
            ));
    }

    let non_locality = (!local).then(|| {
        gen_out_ptr_impls(
            enum_name,
            generics,
            variants.iter().filter_map(|variant| {
                variant_mapper(variant, || None, |field| Some(field.ty.clone()))
            }),
        )
    });

    let niche_ir = gen_enum_niche_ir(inferred_repr, enum_name, generics, variants);

    let decode_match_expr = quote! {
        // SAFETY: All variant structs have tag as the first field at offset 0
        unsafe { core::ptr::from_ref(&source).cast::<#inferred_repr>().read() }
    };

    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let encode_bounds = gen_encode_bounds(&field_types, generics);
    let decode_bounds = gen_decode_bounds(&field_types, generics);
    let decode_cloned_bounds = gen_decode_cloned_bounds(&field_types);

    let borrow_ir = gen_data_enum_borrow_ir::<NEEDS_DROP>(None, enum_name, generics, variants);
    let store_defs = (!NEEDS_DROP).then_some(gen_custom_store_types(enum_name, &field_types));

    quote! {
        #repr_c_enum

        #size_family_impl
        #basic_impls
        #niche_ir
        #store_defs
        #borrow_ir

        impl #impl_generics co3::EncodeWithStore for #enum_name #ty_generics where #encode_bounds #predicates {
            type Store = #rust_store;

            fn encode<'_išč>(self, store: &'_išč mut Self::Store) -> Self::CType where Self: '_išč {
                match self {
                    #(#variants_encode,)*
                }
            }
        }

        impl<#decode_params> co3::DecodeWithStore<'_dšč> for #enum_name #ty_generics
        where
            #repr_c_enum_name #ty_generics: '_dšč,
            #decode_bounds
            #predicates
        {
            type Store = #ffi_store;

            unsafe fn decode<'_išč: '_dšč>(source: Self::CType, store: &'_išč mut Self::Store) -> Option<Self> {
                match #decode_match_expr {
                    #(#variants_decode,)*
                    _ => None
                }
            }
        }
        impl<#decode_params> co3::cloned::DecodeCloned<'_dšč> for #enum_name #ty_generics
        where
            #repr_c_enum_name #ty_generics: '_dšč,
            #decode_cloned_bounds
            #predicates
        {
            unsafe fn decode_cloned<'_išč: '_dšč>(source: Self::CType, store: &'_išč mut Self::Store) -> Option<Self> {
                match #decode_match_expr {
                    #(#variants_decode_cloned,)*
                    _ => None
                }
            }
        }

        #non_locality
    }
}

pub(super) fn derive_no_repr_fieldless_enum(
    enum_name: &Ident,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> TokenStream {
    let inferred_repr = infer_repr(variants.len());
    let size_family_impl = gen_sized_size_family(enum_name, generics);

    let basic_impls = gen_ir_impl(enum_name, &parse_quote!( #inferred_repr ), &[], generics);

    let variants_decode = variants.iter().enumerate().map(|(i, variant)| {
        let idx = TokenStream::from_str(&format!("{i}")).expect("Valid");
        let variant_name = &variant.ident;
        quote! { #idx => Some(Self::#variant_name) }
    });

    let niche_ir = gen_enum_niche_ir(inferred_repr, enum_name, generics, variants);

    let nodrop_borrow_ir = gen_fieldless_enum_drop_ir(enum_name, generics);
    let non_locality = gen_out_ptr_impls(
        enum_name,
        generics,
        variants
            .iter()
            .filter_map(|variant| variant_mapper(variant, || None, |field| Some(field.ty.clone()))),
    );

    quote! {
        #size_family_impl
        #basic_impls

        #nodrop_borrow_ir

        impl co3::EncodeWithStore for #enum_name {
            type Store = ();

            fn encode<'_išč>(self, (): &mut ()) -> Self::CType where Self: '_išč {
                self as #inferred_repr
            }
        }

        impl<'_dšč> co3::DecodeWithStore<'_dšč> for #enum_name {
            type Store = ();

            unsafe fn decode<'_išč: '_dšč>(source: Self::CType, (): &mut ()) -> Option<Self> {
                match source {
                    #(#variants_decode,)*
                    _ => None
                }
            }
        }

        impl<'_dšč> co3::cloned::DecodeCloned<'_dšč> for #enum_name {}

        #niche_ir
        #non_locality
    }
}

fn gen_borrow_store_type(name: &Ident, fields: &[&syn::Type]) -> TokenStream {
    let store_name = gen_store_name(name);

    let stores = fields.iter().map(|field| {
        quote! { <#field as co3::borrow::Borrow>::Store }
    });

    quote! { #store_name<#(#stores),*> }
}

pub(super) fn gen_struct_borrow_ir<const NEEDS_DROP: bool>(
    repr_attr: Option<TokenStream>,
    name: &Ident,
    generics: &syn::Generics,
    fields: &Fields<FfiTypeField>,
) -> TokenStream {
    let field_types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();

    let (borrowed_struct, store_defs, borrow_store) = if NEEDS_DROP {
        (
            gen_borrowed_struct(repr_attr, name, generics, fields),
            gen_custom_store_types(name, &field_types),
            gen_borrow_store_type(name, &field_types),
        )
    } else {
        (quote! {}, quote! {}, quote! {})
    };

    let borrowed_struct_name = gen_borrowed_name(name);
    let (borrow_impl, to_owned_impl) = match &fields.style {
        Style::Struct => {
            let field_names: Vec<_> = fields.iter().filter_map(|f| f.ident.as_ref()).collect();
            let field_indices: Vec<_> = (0..field_names.len()).map(syn::Index::from).collect();

            (
                quote! {
                    let Self { #(#field_names),* } = self;

                    #borrowed_struct_name {
                        #(#field_names: co3::borrow::Borrow::borrow(#field_names, &mut store.#field_indices)),*
                    }
                },
                quote! {
                    let #borrowed_struct_name { #(#field_names),* } = borrowed;

                    Self {
                        #(#field_names: co3::borrow::ToOwned::to_owned(#field_names)),*
                    }
                },
            )
        }
        Style::Tuple => {
            let field_indices: Vec<_> = (0..fields.len()).map(syn::Index::from).collect();
            let field_vars: Vec<_> = (0..fields.len())
                .map(|i| Ident::new(&format!("_{}", i), Span::call_site()))
                .collect();

            (
                quote! {
                    let Self(#(#field_vars),*) = self;

                    #borrowed_struct_name(
                        #(co3::borrow::Borrow::borrow(#field_vars, &mut store.#field_indices)),*
                    )
                },
                quote! {
                    let #borrowed_struct_name(#(#field_vars),*) = borrowed;

                    Self(
                        #(co3::borrow::ToOwned::to_owned(#field_vars)),*
                    )
                },
            )
        }
        Style::Unit => unreachable!("ZSTs are not FFI safe"),
    };
    let drop_ir = gen_drop_ir::<NEEDS_DROP>(
        name,
        &field_types,
        generics,
        &borrow_store,
        &borrow_impl,
        &to_owned_impl,
    );

    quote! {
        #borrowed_struct
        #store_defs
        #drop_ir
    }
}

pub(super) fn gen_data_enum_borrow_ir<const NEEDS_DROP: bool>(
    repr_attr: Option<TokenStream>,
    enum_name: &Ident,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> TokenStream {
    let mut field_types = Vec::new();
    let mut variants_borrow = Vec::with_capacity(variants.len());
    let mut variants_to_owned = Vec::with_capacity(variants.len());

    let borrowed_enum_name = gen_borrowed_name(enum_name);
    for (i, variant) in variants.iter().enumerate() {
        let idx = TokenStream::from_str(&format!("{i}")).expect("Valid");
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
        variants_to_owned.push(variant_mapper(
            variant,
            || quote! { #borrowed_enum_name::#variant_name => Self::#variant_name },
            |_| {
                quote! {
                    #borrowed_enum_name::#variant_name(payload) => {
                        Self::#variant_name(co3::borrow::ToOwned::to_owned(payload))
                    }
                }
            },
        ));
    }

    let (borrowed_enum, store_defs, borrow_store) = if NEEDS_DROP {
        (
            gen_borrowed_data_enum(repr_attr, enum_name, generics, variants),
            gen_custom_store_types(enum_name, &field_types),
            gen_borrow_store_type(enum_name, &field_types),
        )
    } else {
        (quote! {}, quote! {}, quote! {})
    };

    let borrow_impl = quote! {
        match self {
            #(#variants_borrow,)*
        }
    };
    let to_owned_impl = quote! {
        match borrowed {
            #(#variants_to_owned,)*
        }
    };
    let drop_ir = gen_drop_ir::<NEEDS_DROP>(
        enum_name,
        &field_types,
        generics,
        &borrow_store,
        &borrow_impl,
        &to_owned_impl,
    );

    quote! {
        #borrowed_enum
        #store_defs
        #drop_ir
    }
}

fn gen_drop_ir<const NEEDS_DROP: bool>(
    name: &syn::Ident,
    types: &[&syn::Type],
    generics: &syn::Generics,
    borrow_store: &TokenStream,
    borrow_impl: &TokenStream,
    to_owned_impl: &TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let (fields_tuple, _, _) = build_type_tuple(types);
    let borrow_view_bounds = gen_borrow_view_bounds(types, generics, false);
    let to_owned_bounds = gen_to_owned_bounds(types, generics);

    if !NEEDS_DROP {
        let no_drop_borrow = no_drop_borrow::<false>(name, generics);
        let no_drop_to_owned = no_drop_to_owned::<false>(name, generics);

        return quote! {
            impl #impl_generics co3::borrow::DropFamily for #name #ty_generics where #predicates {
                type Kind = co3::borrow::NoDrop;
            }

            #no_drop_borrow
            #no_drop_to_owned
        };
    }

    let is_parametrized = types.iter().any(|ty| is_type_parameterized(ty, generics));
    let drop_family_bound = is_parametrized.then_some(quote! {
        #fields_tuple: co3::borrow::DropFamily,
    });

    let no_drop_borrow = no_drop_borrow::<true>(name, generics);
    let no_drop_to_owned = no_drop_to_owned::<true>(name, generics);
    let needs_drop_borrow = needs_drop_borrow::<true>(
        name,
        generics,
        &borrow_view_bounds,
        borrow_store,
        borrow_impl,
    );
    let needs_drop_to_owned =
        needs_drop_to_owned::<true>(name, generics, &to_owned_bounds, to_owned_impl);

    quote! {
        impl #impl_generics co3::borrow::DropFamily for #name #ty_generics
        where
            #drop_family_bound
            #predicates
        {
            type Kind = <#fields_tuple as co3::borrow::DropFamily>::Kind;
        }

        const _: () = {
            use co3::borrow::{Borrow, ToOwned};

            co3::disjoint_impls! {
                #[disjoint_impls(remote)]
                pub trait Borrow: Sized {
                    type Borrowed<'_išč>
                    where
                        Self: '_išč;
                    type Store: Default;

                    fn borrow<'_išč>(self, store: &'_išč mut Self::Store) -> Self::Borrowed<'_išč>
                    where
                        Self: '_išč;
                }

                #needs_drop_borrow
                #no_drop_borrow
            }

            co3::disjoint_impls! {
                #[disjoint_impls(remote)]
                pub trait ToOwned<'_ršč>: Borrow + Sized {
                    fn to_owned(borrowed: Self::Borrowed<'_ršč>) -> Self;
                }

                #needs_drop_to_owned
                #no_drop_to_owned
            }
        };
    }
}

fn no_drop_borrow<const ADD_BOUND: bool>(
    name: &syn::Ident,
    generics: &syn::Generics,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let impl_drop_assert = assert_drop_impl();
    let for_dummy = generics
        .params
        .is_empty()
        .then_some(quote! { for<'_dummy> });

    let bound = ADD_BOUND.then_some(quote! {
        #for_dummy Self: co3::borrow::DropFamily<Kind = co3::borrow::NoDrop>,
    });
    let borrow_trait = if ADD_BOUND {
        quote! { Borrow }
    } else {
        quote! { co3::borrow::Borrow }
    };

    quote! {
        impl #impl_generics #borrow_trait for #name #ty_generics
        where
            #bound
            #for_dummy Self: Sized,
            #predicates
        {
            type Borrowed<'_išč> = Self
            where
                Self: '_išč;

            type Store = ();

            #[inline(always)]
            fn borrow<'_išč>(self, (): &mut ()) -> Self::Borrowed<'_išč>
            where
                Self: '_išč,
            {
                #impl_drop_assert
                self
            }
        }
    }
}

fn needs_drop_borrow<const ADD_BOUND: bool>(
    name: &syn::Ident,
    generics: &syn::Generics,
    borrow_view_bounds: &TokenStream,
    borrow_store: &TokenStream,
    borrow_impl: &TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let for_dummy = generics
        .params
        .is_empty()
        .then_some(quote! { for<'_dummy> });

    let borrowed_name = gen_borrowed_name(name);
    let impl_drop_assert = assert_drop_impl();
    let generic_idents = generics.params.iter().map(|param| match param {
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

    let bound = ADD_BOUND.then_some(quote! {
        #for_dummy Self: co3::borrow::DropFamily<Kind = co3::borrow::NeedsDrop>,
    });

    quote! {
        impl #impl_generics Borrow for #name #ty_generics
        where
            #bound
            #for_dummy Self: Sized,
            #borrow_view_bounds
            #predicates
        {
            type Borrowed<'_išč> = #borrowed_name<'_išč, #(#generic_idents),*>
            where
                Self: '_išč;

            type Store = #borrow_store;

            #[inline(always)]
            fn borrow<'_išč>(self, store: &'_išč mut Self::Store) -> Self::Borrowed<'_išč>
            where
                Self: '_išč,
            {
                #impl_drop_assert
                #borrow_impl
            }
        }
    }
}

fn no_drop_to_owned<const ADD_BOUND: bool>(
    name: &syn::Ident,
    generics: &syn::Generics,
) -> TokenStream {
    let (_, ty_generics, where_clause) = generics.split_for_impl();

    let params = &generics.params;
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let for_dummy = generics
        .params
        .is_empty()
        .then_some(quote! { for<'_dummy> });

    let bound = ADD_BOUND.then_some(quote! {
        #for_dummy Self: co3::borrow::DropFamily<Kind = co3::borrow::NoDrop>,
    });

    let to_owned_trait = if ADD_BOUND {
        quote! { ToOwned<'_ršč> }
    } else {
        quote! { co3::borrow::ToOwned<'_ršč> }
    };

    quote! {
        impl<'_ršč, #params> #to_owned_trait for #name #ty_generics
        where
            #bound
            #for_dummy Self: Sized + '_ršč,
            #predicates
        {
            #[inline(always)]
            fn to_owned(borrowed: Self::Borrowed<'_ršč>) -> Self {
                borrowed
            }
        }
    }
}

fn needs_drop_to_owned<const ADD_BOUND: bool>(
    name: &syn::Ident,
    generics: &syn::Generics,
    to_owned_bounds: &TokenStream,
    to_owned_impl: &TokenStream,
) -> TokenStream {
    let (_, ty_generics, where_clause) = generics.split_for_impl();

    let params = &generics.params;
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let for_dummy = generics
        .params
        .is_empty()
        .then_some(quote! { for<'_dummy> });

    let bound = ADD_BOUND.then_some(quote! {
        #for_dummy Self: co3::borrow::DropFamily<Kind = co3::borrow::NeedsDrop>,
    });

    quote! {
        impl<'_ršč, #params> ToOwned<'_ršč> for #name #ty_generics
        where
            #bound
            #to_owned_bounds
            #for_dummy Self: Sized + '_ršč,
            #predicates
        {
            #[inline(always)]
            fn to_owned(borrowed: Self::Borrowed<'_ršč>) -> Self {
                #to_owned_impl
            }
        }
    }
}

fn gen_store_name(name: &syn::Ident) -> syn::Ident {
    format_ident!("{name}Store")
}

fn gen_borrowed_name(name: &Ident) -> Ident {
    format_ident!("{name}Borrow")
}

fn borrowed_view_lifetime(ty: &syn::Type) -> Option<syn::Lifetime> {
    struct LifetimeCollector {
        lifetimes: BTreeSet<syn::Lifetime>,
    }

    impl<'ast> Visit<'ast> for LifetimeCollector {
        fn visit_lifetime(&mut self, lifetime: &'ast syn::Lifetime) {
            if lifetime.ident != "_" {
                self.lifetimes.insert(lifetime.clone());
            }
        }
    }

    let mut collector = LifetimeCollector {
        lifetimes: BTreeSet::new(),
    };
    collector.visit_type(ty);

    (collector.lifetimes.len() == 1)
        .then(|| collector.lifetimes.into_iter().next().expect("checked len"))
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
    generics: &syn::Generics,
    fields: &Fields<FfiTypeField>,
) -> TokenStream {
    let params = &generics.params;
    let (_, _, where_clause) = generics.split_for_impl();
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let borrowed_name = gen_borrowed_name(name);
    let field_types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();
    let borrow_bounds = gen_borrow_view_bounds(&field_types, generics, true);
    let needs_borrow_lifetime = needs_borrow_lifetime_param(&field_types);
    let borrow_lifetime_param = needs_borrow_lifetime.then_some(quote!('_dšč,));
    let borrowed_field_types = fields
        .iter()
        .map(|field| borrowed_view_type(&field.ty))
        .collect::<Vec<_>>();

    let struct_item = match &fields.style {
        Style::Struct => {
            let field_names = fields.iter().filter_map(|f| f.ident.as_ref());

            parse_quote! {
                #[doc(hidden)]
                #repr_attr
                pub struct #borrowed_name<#borrow_lifetime_param #params>
                where
                    #borrow_bounds
                    #predicates
                {
                    #(#field_names: #borrowed_field_types),*
                }
            }
        }
        Style::Tuple => {
            parse_quote! {
                #[doc(hidden)]
                #repr_attr
                pub struct #borrowed_name<#borrow_lifetime_param #params>(#(#borrowed_field_types),*)
                where
                    #borrow_bounds
                    #predicates
                ;
            }
        }
        Style::Unit => unreachable!("ZSTs are not FFI safe"),
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
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> TokenStream {
    let params = &generics.params;
    let (_, _, where_clause) = generics.split_for_impl();
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
    let borrow_bounds = gen_borrow_view_bounds(&field_types, generics, true);
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
        pub enum #borrowed_name<#borrow_lifetime_param #params>
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
    match derive_extern_c_internal::<false>(item) {
        Ok(derived) => derived,
        Err(err) => err.to_compile_error(),
    }
}

fn gen_out_ptr_impls(
    _type_name: &Ident,
    generics: &syn::Generics,
    types: impl IntoIterator<Item = syn::Type>,
) -> TokenStream {
    let (_, _ty_generics, where_clause) = generics.split_for_impl();
    let _params = &generics.params;
    let _predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let _for_dummy = if types
        .into_iter()
        .all(|ty| !is_type_parameterized(&ty, generics))
    {
        Some(quote! { for<'_dummy> })
    } else {
        None
    };

    quote! {
        //impl<#params> co3::out_ptr::OutPtr for #type_name #ty_generics
        //where
        //    #for_dummy Self: co3::ExternC + co3::out_ptr::NonLocal,
        //    #predicates
        //{
        //    type OutPtr = Self::CType;
        //}
        //impl<#params> co3::out_ptr::OutPtrWrite for #type_name #ty_generics
        //where
        //    #for_dummy Self: co3::Encode<false> + co3::out_ptr::OutPtr + co3::out_ptr::NonLocal,
        //    #predicates
        //{
        //    unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
        //        unimplemented!()
        //        // FIXME:
        //        //let mut store = Default::default();
        //        //let encoded = co3::Encode::encode(self, &mut store);
        //        //unsafe { out_ptr.write(encoded); }
        //    }
        //}
        //impl<'_dšč, #params> co3::out_ptr::OutPtrRead for #type_name #ty_generics where
        //    #for_dummy Self: co3::Decode<'_dšč, false> + co3::out_ptr::OutPtr + co3::out_ptr::NonLocal, #predicates
        //{
        //    unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
        //        unimplemented!()
        //        // FIXME:
        //        //let mut store = Default::default();

        //        //unsafe {
        //        //    // SAFETY: check `NonLocal` for guarantees
        //        //    let store_ref = &mut *(&mut store as *mut _);
        //        //    co3::DecodeWithStore::decode(out_ptr, store_ref)
        //        //}
        //    }
        //}
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

pub fn gen_ir_impl(
    type_name: &Ident,
    repr_c_name: &Ident,
    fields: &[&syn::Type],
    generics: &syn::Generics,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let _field_kinds = fields
        .iter()
        .map(|field| quote!(<#field as co3::ir::ReprFamily>::Kind));

    let extern_c_bounds = gen_extern_c_bounds(fields, generics);
    let params = &generics.params;
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    quote! {
        co3::reprC! {
            impl(#params) Cloned for #type_name #ty_generics where (#predicates) {}
        }

        impl #impl_generics co3::ExternC for #type_name #ty_generics where #extern_c_bounds #predicates {
            type CType = #repr_c_name #ty_generics;
        }
    }
}

pub fn gen_custom_store_types(name: &Ident, field_types: &[&syn::Type]) -> TokenStream {
    let store_name = gen_store_name(name);
    let idxs = (0..field_types.len())
        .map(syn::Index::from)
        .collect::<Vec<_>>();

    let store_defs = {
        let store_params = (0..field_types.len())
            .map(|idx| format_ident!("D{idx}"))
            .collect::<Vec<_>>();

        quote! {
            pub struct #store_name<#(#store_params),*>(#(#store_params),*);

            impl<#(#store_params),*> Default for #store_name<#(#store_params),*> where #(#store_params: Default,)* {
                fn default() -> Self {
                    Self(#(<#store_params as Default>::default()),*)
                }
            }

            impl<#(#store_params),*> co3::Store for #store_name<#(#store_params),*> where #(#store_params: co3::Store,)* {
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

// TODO: Maybe these bounds should use `for <'_dummy>` for concrete types?
fn gen_borrow_view_bounds(
    fields: &[&syn::Type],
    generics: &syn::Generics,
    with_lifetime: bool,
) -> TokenStream {
    let bounds = fields
        .iter()
        .filter(|ty| is_type_parameterized(ty, generics))
        .map(|ty| {
            let lifetime = with_lifetime
                .then(|| borrowed_view_lifetime(ty).map(|lifetime| quote!( + #lifetime)))
                .flatten()
                .unwrap_or_else(|| {
                    with_lifetime
                        .then_some(quote!( + '_dšč))
                        .unwrap_or_default()
                });

            quote! { #ty: co3::borrow::Borrow #lifetime, }
        });

    quote! { #(#bounds)* }
}

fn gen_to_owned_bounds(fields: &[&syn::Type], generics: &syn::Generics) -> TokenStream {
    let parameterized_field_types = fields
        .iter()
        .filter(|ty| is_type_parameterized(ty, generics));

    quote! { #(#parameterized_field_types: co3::borrow::ToOwned<'_ršč>,)* }
}

fn gen_encode_bounds(fields: &[&syn::Type], _generics: &syn::Generics) -> TokenStream {
    quote! { #(#fields: co3::EncodeWithStore<false>,)* }
}

fn gen_decode_bounds(fields: &[&syn::Type], _generics: &syn::Generics) -> TokenStream {
    quote! { #(#fields: co3::DecodeWithStore<'_dšč>,)* }
}

fn gen_decode_cloned_bounds(fields: &[&syn::Type]) -> TokenStream {
    quote! { #(#fields: co3::cloned::DecodeCloned<'_dšč>,)* }
}
