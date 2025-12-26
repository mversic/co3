use core::str::FromStr as _;

use darling::{
    ast::{Fields, Style},
    util::SpannedValue,
};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Ident, parse_quote};

use crate::{
    emitter::Emitter,
    extern_c::{
        FfiTypeField, FfiTypeVariant,
        repr_c::{gen_data_carrying_repr_c_enum, gen_repr_c_enum_payload_name, gen_repr_c_item_name},
    },
};

pub(super) fn derive_opaque_item(name: &Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics co3::ir::Ir for #name #ty_generics #where_clause {
            type Type = co3::ir::Opaque;
        }

        impl #impl_generics co3::niche::Ir for #name #ty_generics #where_clause {
            type Type = co3::niche::WithCustomNiche;
        }

        impl #impl_generics co3::niche::Niche for #name #ty_generics #where_clause {
            const NICHE_VALUE: *mut Self = core::ptr::null_mut();
        }
    }
}

pub(super) fn derive_no_repr_struct(
    name: &Ident,
    mut generics: syn::Generics,
    fields: &Fields<FfiTypeField>,
    local: bool,
) -> TokenStream {
    let (repr_c_struct_name, repr_c_struct) = gen_repr_c_struct(name, &generics, fields);

    generics.make_where_clause();
    let params = &generics.params;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

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

    let num_fields = fields.len();
    let (rust_store, ffi_store, rust_store_conversion, ffi_store_conversion) =
        gen_store_types(num_fields, field_rust_stores, field_ffi_stores);

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
            let field_indices = (0..num_fields).map(syn::Index::from);

            let field_vars: Vec<_> = (0..num_fields)
                .map(|i| Ident::new(&format!("field_{}", i), Span::call_site()))
                .collect();

            quote! {
                let Self(#(#field_vars),*) = self;

                #repr_c_struct_name(
                    #(co3::Encode::encode(#field_vars, &mut store.#field_indices)),*
                )
            }
        }
        Style::Unit => quote! { #repr_c_struct_name },
    };

    let decode_impl = match &fields.style {
        Style::Struct => {
            let field_names: Vec<_> = fields.iter().filter_map(|f| f.ident.as_ref()).collect();
            let field_indices = (0..field_names.len()).map(syn::Index::from);

            quote! {
                Ok(Self {
                    #(#field_names: co3::Decode::decode(source.#field_names, &mut store.#field_indices)?),*
                })
            }
        }
        Style::Tuple => {
            let field_indices = (0..num_fields).map(syn::Index::from);

            quote! {
                Ok(Self(
                    #(co3::Decode::decode(source.#field_indices, &mut store.#field_indices)?),*
                ))
            }
        }
        Style::Unit => quote! { Ok(Self) },
    };

    let non_locality =
        local.then(|| gen_out_ptr_impls(name, &generics, fields.iter().map(|f| f.ty.clone())));

    let niche_ir_without = {
        let mut without_niche_where_clause = where_clause.unwrap().clone();

        for ty in fields.iter().map(|f| &f.ty) {
            without_niche_where_clause
                .predicates
                .push(parse_quote! { #ty: co3::niche::Ir<Type = co3::niche::WithoutNiche> });
        }

        quote! {
            impl #impl_generics co3::niche::Ir for #name #ty_generics #without_niche_where_clause {
                type Type = co3::niche::WithoutNiche;
            }
        }
    };

    let niche_ir_with = quote! {
        impl #impl_generics co3::niche::Ir for #name #ty_generics #where_clause {
            type Type = co3::niche::WithCustomNiche;
        }

        impl #impl_generics co3::niche::Niche for #name #ty_generics #where_clause {
            const NICHE_VALUE: #repr_c_struct_name = unsafe { core::mem::zeroed() };
        }
    };

    let basic_impls = gen_basic_trait_impls(name, &generics);

    quote! {
        #repr_c_struct

        #basic_impls

        #niche_ir_without
        #niche_ir_with

        impl #impl_generics co3::ExternC for #name #ty_generics #where_clause {
            type CType = #repr_c_struct_name #ty_generics;
        }
        impl #impl_generics co3::Encode for #name #ty_generics #where_clause {
            type Store = #rust_store;

            fn encode<'_išč>(self, store: &'_išč mut Self::Store) -> <Self as co3::ExternC>::CType where Self: '_išč {
                #rust_store_conversion

                #encode_impl
            }
        }

        impl<'_dšč, #params> co3::Decode<'_dšč> for #name #ty_generics #where_clause {
            type Store = #ffi_store;

            unsafe fn decode<'_išč: '_dšč>(source: <Self as co3::ExternC>::CType, store: &'_išč mut Self::Store) -> co3::Result<Self> {
                #ffi_store_conversion

                #decode_impl
            }
        }

        #non_locality
    }
}

pub(super) fn derive_no_repr_data_carrying_enum(
    emitter: &mut Emitter,
    enum_name: &Ident,
    mut generics: syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
    local: bool,
) -> TokenStream {
    let len = TokenStream::from_str(&format!("{}", variants.len())).expect("Valid");

    let (repr_c_enum_name, repr_c_enum) =
        gen_data_carrying_repr_c_enum(emitter, enum_name, &generics, variants);

    generics.make_where_clause();
    let params = &generics.params;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let variant_rust_stores = variants
        .iter()
        .map(|variant| {
            variant_mapper(
                emitter,
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
                emitter,
                variant,
                || quote! { () },
                |field| {
                    let ty = &field.ty;
                    quote! { <#ty as co3::Decode<'_dšč>>::Store }
                },
            )
        })
        .collect::<Vec<_>>();

    let variants_into_ffi = variants
        .iter()
        .enumerate()
        .map(|(i, variant)| {
            let idx = TokenStream::from_str(&format!("{i}")).expect("Valid");
            let payload_name = gen_repr_c_enum_payload_name(enum_name);
            let variant_name = &variant.ident;

            variant_mapper(
                emitter,
                variant,
                || {
                    quote! { Self::#variant_name => #repr_c_enum_name {
                        tag: #idx, payload: #payload_name {#variant_name: ()}
                    }}
                },
                |_| {
                    quote! {
                        Self::#variant_name(payload) => {
                            let payload = #payload_name {
                                #variant_name: co3::Encode::encode(payload, &mut store.#idx)
                            };

                            #repr_c_enum_name { tag: #idx, payload }
                        }
                    }
                },
            )
        })
        .collect::<Vec<_>>();

    let variants_decode = variants
        .iter()
        .enumerate()
        .map(|(i, variant)| {
            let idx = TokenStream::from_str(&format!("{i}")).expect("Valid");
            let variant_name = &variant.ident;

            variant_mapper(
                emitter,
                variant,
                || quote! { #idx => Ok(Self::#variant_name) },
                |_| {
                    quote! {
                        #idx => {
                            let payload = source.payload.#variant_name;
                            co3::Decode::decode(payload, &mut store.#idx).map(Self::#variant_name)
                        }
                    }
                },
            )
        })
        .collect::<Vec<_>>();

    let (rust_store, ffi_store, rust_store_conversion, ffi_store_conversion) =
        gen_store_types(variants.len(), variant_rust_stores, variant_ffi_stores);

    let non_locality = local.then(|| {
        gen_out_ptr_impls(
            enum_name,
            &generics,
            variants.iter().filter_map(|variant| {
                variant_mapper(emitter, variant, || None, |field| Some(field.ty.clone()))
            }),
        )
    });

    let has_discriminant_niche = variants.len() < (u32::MAX as usize);
    let (niche_ir_without, niche_ir_with) = if has_discriminant_niche {
        (
            quote! {},
            quote! {
                impl #impl_generics co3::niche::Ir for #enum_name #ty_generics #where_clause {
                    type Type = co3::niche::WithCustomNiche;
                }

                impl #impl_generics co3::niche::Niche for #enum_name #ty_generics #where_clause {
                    const NICHE_VALUE: #repr_c_enum_name = #repr_c_enum_name {
                        tag: #len,
                        // FIXME: This likely leads to UB
                        payload: unsafe { core::mem::zeroed() }
                    };
                }
            },
        )
    } else {
        let mut without_niche_where_clause = where_clause.unwrap().clone();

        let mut variant_field_types = Vec::new();
        for variant in variants {
            if let Some(ty) =
                variant_mapper(emitter, variant, || None, |field| Some(field.ty.clone()))
            {
                variant_field_types.push(ty);
            }
        }

        for ty in &variant_field_types {
            without_niche_where_clause
                .predicates
                .push(parse_quote! { #ty: co3::niche::Ir<Type = co3::niche::WithoutNiche> });
        }

        (
            quote! {
                impl #impl_generics co3::niche::Ir for #enum_name #ty_generics #without_niche_where_clause {
                    type Type = co3::niche::WithoutNiche;
                }
            },
            quote! {
                impl #impl_generics co3::niche::Ir for #enum_name #ty_generics #where_clause {
                    type Type = co3::niche::WithCustomNiche;
                }

                impl #impl_generics co3::niche::Niche for #enum_name #ty_generics #where_clause {
                    const NICHE_VALUE: #repr_c_enum_name = #repr_c_enum_name {
                        tag: #len,
                        // FIXME: This likely leads to UB
                        payload: unsafe { core::mem::zeroed() }
                    };
                }
            },
        )
    };

    let basic_impls = gen_basic_trait_impls(enum_name, &generics);

    quote! {
        #repr_c_enum

        #basic_impls

        #niche_ir_without
        #niche_ir_with

        impl #impl_generics co3::ExternC for #enum_name #ty_generics #where_clause {
            type CType = #repr_c_enum_name #ty_generics;
        }
        impl #impl_generics co3::Encode for #enum_name #ty_generics #where_clause {
            type Store = #rust_store;

            fn encode<'_išč>(self, store: &'_išč mut Self::Store) -> <Self as co3::ExternC>::CType where Self: '_išč {
                #ffi_store_conversion

                match self {
                    #(#variants_into_ffi,)*
                }
            }
        }

        impl<'_dšč, #params> co3::Decode<'_dšč> for #enum_name #ty_generics #where_clause {
            type Store = #ffi_store;

            unsafe fn decode<'_išč: '_dšč>(source: <Self as co3::ExternC>::CType, store: &'_išč mut Self::Store) -> co3::Result<Self> {
                #rust_store_conversion

                match source.tag {
                    #(#variants_decode,)*
                    _ => Err(co3::FfiReturn::TrapRepresentation)
                }
            }
        }

        // TODO: This type can utilize niche optimization in some cases. For instance:
        // enum Kita {
        //     A(bool),
        //     B,
        //     C,
        // }
        // assert!(core::mem::size_of::<#enum_name #ty_generics>() == 1);

        #non_locality
    }
}

fn gen_repr_c_struct(
    name: &syn::Ident,
    generics: &syn::Generics,
    fields: &Fields<FfiTypeField>,
) -> (syn::Ident, TokenStream) {
    let doc = format!(" [`ReprC`] equivalent of [`{name}`]");
    let repr_c_struct_name = gen_repr_c_item_name(name);

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let params = &generics.params;
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let fields = fields.iter().map(|field| {
        let field_ty = &field.ty;

        if let Some(field_ident) = &field.ident {
            quote! { #field_ident: <#field_ty as co3::ExternC>::CType }
        } else {
            quote! { <#field_ty as co3::ExternC>::CType }
        }
    });

    let repr_c_struct = quote! {
        #[repr(C)]
        #[doc = #doc]
        #[derive(Clone)]
        struct #repr_c_struct_name #impl_generics #where_clause {
            #(#fields),*
        }

        impl #impl_generics Copy for #repr_c_struct_name #ty_generics #where_clause {}
        unsafe impl #impl_generics co3::ReprC for #repr_c_struct_name #ty_generics #where_clause {}

        co3::mineral! {
            impl(#params) Robust for #repr_c_struct_name where (#predicates) {}
        }
    };

    (repr_c_struct_name, repr_c_struct)
}

fn gen_out_ptr_impls(
    type_name: &Ident,
    generics: &syn::Generics,
    types: impl Iterator<Item = syn::Type>,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let mut non_local_where_clause = where_clause.unwrap().clone();

    for ty in types {
        non_local_where_clause
            .predicates
            .push(parse_quote! {for<'_dummy> #ty: co3::out_ptr::NonLocal});
    }

    quote! {
        unsafe impl #impl_generics co3::out_ptr::NonLocal for #type_name #ty_generics #non_local_where_clause {}

        impl #impl_generics co3::out_ptr::OutPtr for #type_name #ty_generics #non_local_where_clause {
            type OutPtr = Self::CType;
        }
        impl #impl_generics co3::out_ptr::OutPtrWrite for #type_name #ty_generics #non_local_where_clause {
            unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
                let mut store = Default::default();
                let encoded = co3::Encode::encode(self, &mut store);
                unsafe { out_ptr.write(encoded); }
            }
        }
        impl #impl_generics co3::out_ptr::OutPtrRead for #type_name #ty_generics #non_local_where_clause {
            unsafe fn try_read_out(out_ptr: Self::OutPtr) -> co3::Result<Self> {
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

pub fn gen_basic_trait_impls(type_name: &Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics co3::ir::Cloned for #type_name #ty_generics #where_clause {}

        impl #impl_generics co3::ir::Ir for #type_name #ty_generics #where_clause {
            type Type = Self;
        }
    }
}

pub fn gen_store_types(
    count: usize,
    rust_stores: Vec<TokenStream>,
    ffi_stores: Vec<TokenStream>,
) -> (TokenStream, TokenStream, TokenStream, TokenStream) {
    if count > 12 {
        (
            quote! { Option<(#( #rust_stores, )*)> },
            quote! { Option<(#( #ffi_stores, )*)> },
            quote! { let store = store.insert(Default::default()); },
            quote! { let store = store.insert(Default::default()); },
        )
    } else {
        (
            quote! { (#( #rust_stores, )*) },
            quote! { (#( #ffi_stores, )*) },
            quote! {},
            quote! {},
        )
    }
}

pub(super) fn variant_mapper<T: Sized, F0: FnOnce() -> T, F1: FnOnce(&FfiTypeField) -> T>(
    emitter: &mut Emitter,
    variant: &SpannedValue<FfiTypeVariant>,
    unit_mapper: F0,
    field_mapper: F1,
) -> T {
    use manyhow::emit;

    match &variant.fields.style {
        Style::Tuple if variant.fields.fields.len() == 1 => field_mapper(&variant.fields.fields[0]),
        Style::Tuple => {
            emit!(
                emitter,
                variant.span(),
                "Only unit or single unnamed field variants supported"
            );
            unit_mapper()
        }
        Style::Struct => {
            emit!(
                emitter,
                variant.span(),
                "Only unit or single unnamed field variants supported"
            );
            unit_mapper()
        }
        Style::Unit => unit_mapper(),
    }
}
