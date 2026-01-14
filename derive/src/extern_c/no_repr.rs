use core::str::FromStr as _;

use darling::{
    ast::{Fields, Style},
    util::SpannedValue,
};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Ident, parse_quote};

use crate::{
    attr_parse::repr::ReprPrimitive,
    extern_c::{
        FfiTypeField, FfiTypeVariant, is_type_parameterized,
        niche::{gen_enum_niche_ir, gen_struct_niche_ir},
        repr_c::{
            gen_data_enum, gen_data_enum_variant_name, gen_extern_c_bounds, gen_repr_c_struct,
        },
    },
};

pub(super) fn derive_opaque_item(name: &Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics co3::ir::ReprFamily for #name #ty_generics #where_clause {
            type Kind = co3::ir::Opaque;
        }

        impl #impl_generics co3::niche::NicheFamily for #name #ty_generics #where_clause {
            type Kind = co3::niche::WithCustomNiche;
        }

        impl #impl_generics co3::niche::Niche for #name #ty_generics #where_clause {
            const NICHE_VALUE: *mut Self = core::ptr::null_mut();
        }
    }
}

pub(super) fn derive_no_repr_struct(
    name: &Ident,
    generics: &syn::Generics,
    fields: &Fields<FfiTypeField>,
    local: bool,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let (repr_c_struct_name, repr_c_struct) = gen_repr_c_struct(name, generics, fields);
    let field_types = fields.iter().map(|f| &f.ty).collect::<Vec<_>>();

    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let field_rust_stores = fields
        .iter()
        .map(|field| {
            let ty = &field.ty;
            quote! { <#ty as co3::Encode>::Store }
        })
        .collect::<Vec<_>>();

    let field_ffi_stores = fields
        .iter()
        .map(|field| {
            let ty = &field.ty;
            quote! { <#ty as co3::Decode<'_dšč>>::Store }
        })
        .collect::<Vec<_>>();

    let basic_impls = gen_ir_impl(name, &repr_c_struct_name, &field_types, generics);
    let (rust_store, ffi_store, store_init) =
        gen_store_types(fields.len(), field_rust_stores, field_ffi_stores);

    let encode_impl = match &fields.style {
        Style::Struct => {
            let field_names: Vec<_> = fields.iter().filter_map(|f| f.ident.as_ref()).collect();
            let field_indices = (0..field_names.len()).map(syn::Index::from);

            quote! {
                let Self { #(#field_names),* } = self;

                #repr_c_struct_name {
                    #(#field_names: co3::Encode::encode(#field_names, &mut store.#field_indices)),*
                }
            }
        }
        Style::Tuple => {
            let field_indices = (0..fields.len()).map(syn::Index::from);

            let field_vars: Vec<_> = (0..fields.len())
                .map(|i| Ident::new(&format!("_{}", i), Span::call_site()))
                .collect();

            quote! {
                let Self(#(#field_vars),*) = self;

                #repr_c_struct_name(
                    #(co3::Encode::encode(#field_vars, &mut store.#field_indices)),*
                )
            }
        }
        Style::Unit => unreachable!("ZSTs are not FFI safe"),
    };

    let decode_impl = match &fields.style {
        Style::Struct => {
            let field_names: Vec<_> = fields.iter().filter_map(|f| f.ident.as_ref()).collect();
            let field_indices = (0..field_names.len()).map(syn::Index::from);

            quote! {
                Some(Self {
                    #(#field_names: unsafe { co3::Decode::decode(source.#field_names, &mut store.#field_indices)? }),*
                })
            }
        }
        Style::Tuple => {
            let field_indices = (0..fields.len()).map(syn::Index::from);

            quote! {
                Some(Self(
                    #(unsafe { co3::Decode::decode(source.#field_indices, &mut store.#field_indices)? }),*
                ))
            }
        }
        Style::Unit => unreachable!("ZSTs are not FFI safe"),
    };

    let params = generics.params.clone();
    let encode_bounds = gen_encode_bounds(&field_types, generics);
    let decode_bounds = gen_decode_bounds(&field_types, generics);
    let niche_ir = gen_struct_niche_ir(name, generics, fields);
    let non_locality =
        (!local).then(|| gen_out_ptr_impls(name, generics, fields.iter().map(|f| f.ty.clone())));

    quote! {
        #repr_c_struct

        #basic_impls
        #niche_ir

        impl #impl_generics co3::Encode for #name #ty_generics where #encode_bounds #predicates {
            type Store = #rust_store;

            fn encode<'_išč>(self, store: &'_išč mut Self::Store) -> <Self as co3::ExternC>::CType where Self: '_išč {
                #store_init
                #encode_impl
            }
        }

        impl<'_dšč, #params> co3::Decode<'_dšč> for #name #ty_generics where #repr_c_struct_name #ty_generics: '_dšč, #decode_bounds #predicates {
            type Store = #ffi_store;

            unsafe fn decode<'_išč: '_dšč>(source: <Self as co3::ExternC>::CType, store: &'_išč mut Self::Store) -> Option<Self> {
                #store_init
                #decode_impl
            }
        }

        #non_locality
    }
}

pub(super) fn derive_no_repr_data_enum(
    enum_name: &Ident,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
    local: bool,
) -> TokenStream {
    let inferred_repr = infer_repr(variants.len());

    let (repr_c_enum_name, repr_c_enum) =
        gen_data_enum(enum_name, generics, inferred_repr, variants);

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let params = generics.params.clone();

    let variant_rust_stores = variants
        .iter()
        .map(|variant| {
            variant_mapper(
                variant,
                || quote! { () },
                |field| {
                    let ty = &field.ty;
                    quote! { <#ty as co3::Encode>::Store }
                },
            )
        })
        .collect::<Vec<_>>();

    let variant_ffi_stores = variants
        .iter()
        .map(|variant| {
            variant_mapper(
                variant,
                || quote! { () },
                |field| {
                    let ty = &field.ty;
                    quote! { <#ty as co3::Decode<'_dšč>>::Store }
                },
            )
        })
        .collect::<Vec<_>>();

    let mut field_types = Vec::new();
    for variant in variants {
        for field in variant.fields.iter() {
            field_types.push(&field.ty);
        }
    }
    let basic_impls = gen_ir_impl(enum_name, &repr_c_enum_name, &field_types, generics);
    let (rust_store, ffi_store, store_init) =
        gen_store_types(variants.len(), variant_rust_stores, variant_ffi_stores);

    let variants_into_ffi = variants.iter().enumerate().map(|(i, variant)| {
        let idx = TokenStream::from_str(&format!("{i}")).expect("Valid");
        let variant_name = &variant.ident;

        let variant_struct_name = gen_data_enum_variant_name(enum_name, variant_name);

        variant_mapper(
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
                                value: co3::Encode::encode(payload, &mut store.#idx)
                            }
                        }
                    }
                }
            },
        )
    });

    let variants_decode = variants.iter().enumerate().map(|(i, variant)| {
        let idx = TokenStream::from_str(&format!("{i}")).expect("Valid");
        let variant_name = &variant.ident;

        variant_mapper(
            variant,
            || quote! { #idx => Some(Self::#variant_name) },
            |_| {
                quote! {
                    #idx => {
                        let value = unsafe { source.#variant_name.value };
                        unsafe { co3::Decode::decode(value, &mut store.#idx).map(Self::#variant_name) }
                    }
                }
            },
        )
    });

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
        // SAFETY: All variant structs have tag as first field at offset 0
        // We can safely read it by casting the union pointer to the repr type
        unsafe { *core::ptr::from_ref(&source).cast::<#inferred_repr>() }
    };

    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let encode_bounds = gen_encode_bounds(&field_types, generics);
    let decode_bounds = gen_decode_bounds(&field_types, generics);

    quote! {
        #repr_c_enum

        #basic_impls
        #niche_ir

        impl #impl_generics co3::Encode for #enum_name #ty_generics where #encode_bounds #predicates {
            type Store = #rust_store;

            fn encode<'_išč>(self, store: &'_išč mut Self::Store) -> <Self as co3::ExternC>::CType where Self: '_išč {
                #store_init

                match self {
                    #(#variants_into_ffi,)*
                }
            }
        }

        impl<'_dšč, #params> co3::Decode<'_dšč> for #enum_name #ty_generics where #repr_c_enum_name #ty_generics: '_dšč, #decode_bounds #predicates {
            type Store = #ffi_store;

            unsafe fn decode<'_išč: '_dšč>(source: <Self as co3::ExternC>::CType, store: &'_išč mut Self::Store) -> Option<Self> {
                #store_init

                match #decode_match_expr {
                    #(#variants_decode,)*
                    _ => None
                }
            }
        }

        #non_locality
    }
}

pub(super) fn derive_no_repr_fieldless_enum(
    enum_name: &Ident,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> TokenStream {
    let inferred_repr = infer_repr(variants.len());

    let basic_impls = gen_ir_impl(
        enum_name,
        &parse_quote!( #inferred_repr ),
        &[],
        &syn::Generics::default(),
    );

    let variants_decode = variants.iter().enumerate().map(|(i, variant)| {
        let idx = TokenStream::from_str(&format!("{i}")).expect("Valid");
        let variant_name = &variant.ident;
        quote! { #idx => Some(Self::#variant_name) }
    });

    let niche_ir = gen_enum_niche_ir(
        inferred_repr,
        enum_name,
        &syn::Generics::default(),
        variants,
    );

    let mut generics = syn::Generics::default();
    generics.make_where_clause();

    let non_locality = gen_out_ptr_impls(
        enum_name,
        &generics,
        variants
            .iter()
            .filter_map(|variant| variant_mapper(variant, || None, |field| Some(field.ty.clone()))),
    );

    quote! {
        #basic_impls

        impl co3::Encode for #enum_name {
            type Store = ();

            fn encode<'_išč>(self, _store: &'_išč mut Self::Store) -> <Self as co3::ExternC>::CType where Self: '_išč {
                self as #inferred_repr
            }
        }

        impl<'_dšč> co3::Decode<'_dšč> for #enum_name {
            type Store = ();

            unsafe fn decode<'_išč: '_dšč>(source: <Self as co3::ExternC>::CType, _store: &'_išč mut Self::Store) -> Option<Self> {
                match source {
                    #(#variants_decode,)*
                    _ => None
                }
            }
        }

        #niche_ir
        #non_locality
    }
}

fn gen_out_ptr_impls(
    type_name: &Ident,
    generics: &syn::Generics,
    types: impl IntoIterator<Item = syn::Type>,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let for_dummy = if types
        .into_iter()
        .all(|ty| !is_type_parameterized(&ty, generics))
    {
        Some(quote! { for<'_dummy> })
    } else {
        None
    };

    quote! {
        impl #impl_generics co3::out_ptr::OutPtr for #type_name #ty_generics where #for_dummy Self: co3::out_ptr::NonLocal, #predicates {
            type OutPtr = Self::CType;
        }
        impl #impl_generics co3::out_ptr::OutPtrWrite for #type_name #ty_generics where #for_dummy Self: co3::out_ptr::NonLocal, #predicates {
            unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
                let mut store = Default::default();
                let encoded = co3::Encode::encode(self, &mut store);
                unsafe { out_ptr.write(encoded); }
            }
        }
        impl #impl_generics co3::out_ptr::OutPtrRead for #type_name #ty_generics where #for_dummy Self: co3::out_ptr::NonLocal, #predicates {
            unsafe fn try_read_out(out_ptr: Self::OutPtr) -> Option<Self> {
                let mut store = Default::default();

                unsafe {
                    // SAFETY: check `NonLocal` for guarantees
                    let store_ref = &mut *(&mut store as *mut _);
                    co3::Decode::decode(out_ptr, store_ref)
                }
            }
        }
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

    let extern_c_bounds = gen_extern_c_bounds(fields, generics);
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    quote! {
        impl #impl_generics co3::ir::Cloned for #type_name #ty_generics #where_clause {}

        impl #impl_generics co3::ir::ReprFamily for #type_name #ty_generics where #extern_c_bounds #predicates {
            type Kind = Self;
        }

        impl #impl_generics co3::ExternC for #type_name #ty_generics where #extern_c_bounds #predicates {
            type CType = #repr_c_name #ty_generics;
        }
    }
}

pub fn gen_store_types(
    count: usize,
    rust_stores: Vec<TokenStream>,
    ffi_stores: Vec<TokenStream>,
) -> (TokenStream, TokenStream, TokenStream) {
    if count > 12 {
        (
            quote! { Option<(#( #rust_stores, )*)> },
            quote! { Option<(#( #ffi_stores, )*)> },
            quote! { let store = store.insert(Default::default()); },
        )
    } else {
        (
            quote! { (#( #rust_stores, )*) },
            quote! { (#( #ffi_stores, )*) },
            quote! {},
        )
    }
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

fn gen_encode_bounds(fields: &[&syn::Type], generics: &syn::Generics) -> TokenStream {
    let parameterized_field_types = fields
        .iter()
        .filter(|ty| is_type_parameterized(ty, generics));

    quote! { #(#parameterized_field_types: co3::Encode,)* }
}

fn gen_decode_bounds(fields: &[&syn::Type], generics: &syn::Generics) -> TokenStream {
    let parameterized_field_types = fields
        .iter()
        .filter(|ty| is_type_parameterized(ty, generics));

    quote! { #(#parameterized_field_types: co3::Decode<'_dšč>,)* }
}
