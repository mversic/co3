use core::str::FromStr as _;

use darling::{
    ast::{Fields, Style},
    util::SpannedValue,
};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Ident, parse_quote};

use crate::{
    attr_parse::repr::ReprPrimitive,
    repr::{
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
    let (borrowed_struct_name, borrowed_struct) = gen_borrowed_struct(name, generics, fields);
    let borrowed_struct_ty = gen_borrowed_type_use(&borrowed_struct_name, generics);
    let field_types = fields.iter().map(|f| &f.ty).collect::<Vec<_>>();

    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let field_borrow_stores = fields.iter().map(|field| {
        let ty = &field.ty;
        quote! { <#ty as co3::borrow::Borrow>::Store }
    });

    let field_rust_stores = fields.iter().map(|field| {
        let ty = &field.ty;
        quote! { <#ty as co3::Encode<false>>::Store }
    });
    let field_ffi_stores = fields.iter().map(|field| {
        let ty = &field.ty;
        quote! { <#ty as co3::Decode>::Store }
    });

    let basic_impls = gen_ir_impl(name, &repr_c_struct_name, &field_types, generics);
    let (store_defs, rust_store, ffi_store) =
        gen_custom_store_types(name, field_rust_stores, field_ffi_stores, &field_types);
    let borrow_store = gen_borrow_store_type(name, field_borrow_stores);

    let (borrow_impl, encode_impl, decode_impl, decode_cloned_impl) = match &fields.style {
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
                    let Self { #(#field_names),* } = self;

                    #repr_c_struct_name {
                        #(#field_names: co3::Encode::encode(#field_names, &mut store.#field_indices)),*
                    }
                },
                quote! {
                    Some(Self {
                        #(#field_names: unsafe { co3::Decode::decode(source.#field_names, &mut store.#field_indices)? }),*
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

                    #borrowed_struct_name(
                        #(co3::borrow::Borrow::borrow(#field_vars, &mut store.#field_indices)),*
                    )
                },
                quote! {
                    let Self(#(#field_vars),*) = self;

                    #repr_c_struct_name(
                        #(co3::Encode::encode(#field_vars, &mut store.#field_indices)),*
                    )
                },
                quote! {
                    Some(Self(
                        #(unsafe { co3::Decode::decode(source.#field_indices, &mut store.#field_indices)? }),*
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
    let encode_bounds = gen_encode_bounds(&field_types, generics);
    let borrow_bounds = gen_borrow_bounds(&field_types, generics);
    let decode_bounds = gen_decode_bounds(&field_types, generics);
    let decode_cloned_bounds = gen_decode_cloned_bounds(&field_types);

    let niche_ir = gen_struct_niche_ir(name, generics, fields);
    let non_locality =
        (!local).then(|| gen_out_ptr_impls(name, generics, fields.iter().map(|f| f.ty.clone())));

    quote! {
        #borrowed_struct
        #repr_c_struct

        #basic_impls
        #store_defs
        #niche_ir

        impl #impl_generics co3::borrow::Borrow for #name #ty_generics
        where
            #borrow_bounds
            #predicates
        {
            type Store = #borrow_store;

            type Borrowed<'_išč>
                = #borrowed_struct_ty
            where
                Self: '_išč;

            fn borrow<'_išč>(self, store: &'_išč mut Self::Store) -> Self::Borrowed<'_išč> where Self: '_išč {
                #borrow_impl
            }
        }
        impl #impl_generics co3::Encode for #name #ty_generics where #encode_bounds #predicates {
            type Store = #rust_store;

            fn encode<'_išč>(self, store: &'_išč mut Self::Store) -> <Self as co3::ExternC>::CType where Self: '_išč {
                #encode_impl
            }
        }
        impl<'_dšč, #params> co3::Decode<'_dšč> for #name #ty_generics where
            #repr_c_struct_name #ty_generics: '_dšč,
            #decode_bounds #predicates
        {
            type Store = #ffi_store;

            unsafe fn decode<'_išč: '_dšč>(source: <Self as co3::ExternC>::CType, store: &'_išč mut Self::Store) -> Option<Self> {
                #decode_impl
            }
        }
        impl<'_dšč, #params> co3::cloned::DecodeCloned<'_dšč> for #name #ty_generics where
            #decode_cloned_bounds
            #predicates
        {
            unsafe fn decode_cloned<'_išč: '_dšč>(source: <Self as co3::ExternC>::CType, store: &'_išč mut Self::Store) -> Option<Self> {
                #decode_cloned_impl
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
    let (borrowed_enum_name, borrowed_enum) = gen_borrowed_data_enum(enum_name, generics, variants);
    let borrowed_enum_ty = gen_borrowed_type_use(&borrowed_enum_name, generics);

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let params = &generics.params;

    let variant_rust_stores = variants.iter().map(|variant| {
        variant_mapper(
            variant,
            || quote! { () },
            |field| {
                let ty = &field.ty;
                quote! { <#ty as co3::Encode<false>>::Store }
            },
        )
    });
    let variant_borrow_stores = variants.iter().map(|variant| {
        variant_mapper(
            variant,
            || quote! { () },
            |field| {
                let ty = &field.ty;
                quote! { <#ty as co3::borrow::Borrow>::Store }
            },
        )
    });

    let variant_ffi_stores = variants.iter().map(|variant| {
        variant_mapper(
            variant,
            || quote! { () },
            |field| {
                let ty = &field.ty;
                quote! { <#ty as co3::Decode<'_dšč, false>>::Store }
            },
        )
    });

    let mut field_types = Vec::new();
    for variant in variants {
        for field in variant.fields.iter() {
            field_types.push(&field.ty);
        }
    }
    let basic_impls = gen_ir_impl(enum_name, &repr_c_enum_name, &field_types, generics);
    let (store_defs, rust_store, ffi_store) = gen_custom_store_types(
        enum_name,
        variant_rust_stores,
        variant_ffi_stores,
        &field_types,
    );
    let borrow_store = gen_borrow_store_type(enum_name, variant_borrow_stores);

    let mut variants_borrow = Vec::with_capacity(variants.len());
    let mut variants_encode = Vec::with_capacity(variants.len());
    let mut variants_decode = Vec::with_capacity(variants.len());
    let mut variants_decode_cloned = Vec::with_capacity(variants.len());
    for (i, variant) in variants.iter().enumerate() {
        let idx = TokenStream::from_str(&format!("{i}")).expect("Valid");
        let variant_name = &variant.ident;
        let variant_struct_name = gen_data_enum_variant_name(enum_name, variant_name);

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
                                value: co3::Encode::encode(payload, &mut store.#idx)
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
                            unsafe { co3::Decode::<'_dšč, false>::decode(value, &mut store.#idx).map(Self::#variant_name) }
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
                            unsafe { co3::cloned::DecodeCloned::<'_dšč, false>::decode_cloned(value, &mut store.#idx).map(Self::#variant_name) }
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
    let borrow_bounds = gen_borrow_bounds(&field_types, generics);
    let decode_bounds = gen_decode_bounds(&field_types, generics);
    let decode_cloned_bounds = gen_decode_cloned_bounds(&field_types);

    quote! {
        #borrowed_enum
        #repr_c_enum

        #basic_impls
        #store_defs
        #niche_ir

        impl #impl_generics co3::borrow::Borrow for #enum_name #ty_generics
        where
            #borrow_bounds
            #predicates
        {
            type Store = #borrow_store;

            type Borrowed<'_išč>
                = #borrowed_enum_ty
            where
                Self: '_išč;

            fn borrow<'_išč>(self, store: &'_išč mut Self::Store) -> Self::Borrowed<'_išč> where Self: '_išč {
                match self {
                    #(#variants_borrow,)*
                }
            }
        }

        impl #impl_generics co3::Encode for #enum_name #ty_generics where #encode_bounds #predicates {
            type Store = #rust_store;

            fn encode<'_išč>(self, store: &'_išč mut Self::Store) -> <Self as co3::ExternC>::CType where Self: '_išč {
                match self {
                    #(#variants_encode,)*
                }
            }
        }

        impl<'_dšč, #params> co3::Decode<'_dšč, false> for #enum_name #ty_generics
        where
            #repr_c_enum_name #ty_generics: '_dšč,
            #decode_bounds
            #predicates
        {
            type Store = #ffi_store;

            unsafe fn decode<'_išč: '_dšč>(source: <Self as co3::ExternC>::CType, store: &'_išč mut Self::Store) -> Option<Self> {
                match #decode_match_expr {
                    #(#variants_decode,)*
                    _ => None
                }
            }
        }
        impl<'_dšč, #params> co3::cloned::DecodeCloned<'_dšč, false> for #enum_name #ty_generics
        where
            Self: co3::Decode<'_dšč, false>,
            #decode_cloned_bounds
            #predicates
        {
            unsafe fn decode_cloned<'_išč: '_dšč>(source: <Self as co3::ExternC>::CType, store: &'_išč mut Self::Store) -> Option<Self> {
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

    let basic_impls = gen_ir_impl(enum_name, &parse_quote!( #inferred_repr ), &[], generics);

    let variants_decode = variants.iter().enumerate().map(|(i, variant)| {
        let idx = TokenStream::from_str(&format!("{i}")).expect("Valid");
        let variant_name = &variant.ident;
        quote! { #idx => Some(Self::#variant_name) }
    });

    let niche_ir = gen_enum_niche_ir(inferred_repr, enum_name, generics, variants);

    let non_locality = gen_out_ptr_impls(
        enum_name,
        generics,
        variants
            .iter()
            .filter_map(|variant| variant_mapper(variant, || None, |field| Some(field.ty.clone()))),
    );

    quote! {
        #basic_impls

        impl co3::borrow::Borrow for #enum_name {
            type Store = ();

            type Borrowed<'_išč>
                = Self
            where
                Self: '_išč;

            fn borrow<'_išč>(self, (): &'_išč mut ()) -> Self::Borrowed<'_išč> where Self: '_išč {
                self
            }
        }

        impl co3::Encode<false> for #enum_name {
            type Store = ();

            fn encode<'_išč>(self, (): &'_išč mut ()) -> <Self as co3::ExternC>::CType where Self: '_išč {
                self as #inferred_repr
            }
        }

        impl<'_dšč> co3::Decode<'_dšč, false> for #enum_name {
            type Store = ();

            unsafe fn decode<'_išč: '_dšč>(source: <Self as co3::ExternC>::CType, (): &'_išč mut ()) -> Option<Self> {
                match source {
                    #(#variants_decode,)*
                    _ => None
                }
            }
        }

        impl<'_dšč> co3::cloned::DecodeCloned<'_dšč, false> for #enum_name {}

        #niche_ir
        #non_locality
    }
}

fn gen_borrow_store_type(name: &Ident, stores: impl Iterator<Item = TokenStream>) -> TokenStream {
    let store_name = gen_store_name(name);
    quote! { #store_name<#(#stores),*> }
}

fn gen_borrowed_type_use(borrowed_name: &Ident, generics: &syn::Generics) -> TokenStream {
    let params = generics.params.iter().map(|param| match param {
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

    quote! { #borrowed_name<'_išč, #(#params),*> }
}

fn gen_store_name(name: &syn::Ident) -> syn::Ident {
    format_ident!("{name}Store")
}

fn gen_borrowed_name(name: &Ident) -> Ident {
    format_ident!("{name}Borrow")
}

fn gen_borrowed_struct(
    name: &Ident,
    generics: &syn::Generics,
    fields: &Fields<FfiTypeField>,
) -> (syn::Ident, TokenStream) {
    let (_, _, where_clause) = generics.split_for_impl();
    let params = &generics.params;

    let borrowed_name = gen_borrowed_name(name);
    let field_types = fields.iter().map(|field| {
        let ty = &field.ty;
        quote! { <#ty as co3::borrow::Borrow>::Borrowed<'_išč> }
    });

    let struct_item = match &fields.style {
        Style::Struct => {
            let field_names = fields.iter().filter_map(|f| f.ident.as_ref());

            quote! {
                #[doc(hidden)]
                pub struct #borrowed_name<'_išč, #params> #where_clause {
                    #(#field_names: #field_types),*
                }
            }
        }
        Style::Tuple => {
            quote! {
                #[doc(hidden)]
                pub struct #borrowed_name<'_išč, #params>(#(#field_types),*) #where_clause;
            }
        }
        Style::Unit => unreachable!("ZSTs are not FFI safe"),
    };

    (borrowed_name, struct_item)
}

fn gen_borrowed_data_enum(
    enum_name: &Ident,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> (syn::Ident, TokenStream) {
    let params = &generics.params;

    let (_, _, where_clause) = generics.split_for_impl();
    let borrowed_name = gen_borrowed_name(enum_name);
    let variants = variants.iter().map(|variant| {
        let variant_name = &variant.ident;
        variant_mapper(
            variant,
            || quote! { #variant_name },
            |field| {
                let ty = &field.ty;
                quote! { #variant_name(<#ty as co3::borrow::Borrow>::Borrowed<'_išč>) }
            },
        )
    });

    let enum_ = quote! {
        #[doc(hidden)]
        pub enum #borrowed_name<'_išč, #params> #where_clause {
            #(#variants),*
        }
    };

    (borrowed_name, enum_)
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
        //        //    co3::Decode::decode(out_ptr, store_ref)
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

    let extern_c_bounds = gen_extern_c_bounds(fields, generics);
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    quote! {
        impl #impl_generics co3::ir::Cloned for #type_name #ty_generics #where_clause {}

        impl #impl_generics co3::ir::ReprFamily for #type_name #ty_generics #where_clause {
            type Kind = Self;
        }

        impl #impl_generics co3::ExternC for #type_name #ty_generics where #extern_c_bounds #predicates {
            type CType = #repr_c_name #ty_generics;
        }
    }
}

pub fn gen_custom_store_types(
    name: &Ident,
    encode_stores: impl Iterator<Item = TokenStream>,
    decode_stores: impl Iterator<Item = TokenStream>,
    field_types: &[&syn::Type],
) -> (TokenStream, TokenStream, TokenStream) {
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

    (
        quote! { #store_defs },
        quote! { #store_name<#(#encode_stores),*> },
        quote! { #store_name<#(#decode_stores),*> },
    )
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
fn gen_borrow_bounds(fields: &[&syn::Type], generics: &syn::Generics) -> TokenStream {
    let parameterized_field_types = fields
        .iter()
        .filter(|ty| is_type_parameterized(ty, generics));

    quote! { #(#parameterized_field_types: co3::borrow::Borrow,)* }
}

fn gen_encode_bounds(fields: &[&syn::Type], generics: &syn::Generics) -> TokenStream {
    let parameterized_field_types = fields
        .iter()
        .filter(|ty| is_type_parameterized(ty, generics));

    quote! { #(#parameterized_field_types: co3::Encode<false>,)* }
}

fn gen_decode_bounds(fields: &[&syn::Type], generics: &syn::Generics) -> TokenStream {
    let parameterized_field_types = fields
        .iter()
        .filter(|ty| is_type_parameterized(ty, generics));

    quote! { #(#parameterized_field_types: co3::Decode<'_dšč, false>,)* }
}

fn gen_decode_cloned_bounds(fields: &[&syn::Type]) -> TokenStream {
    quote! { #(#fields: co3::cloned::DecodeCloned<'_dšč, false>,)* }
}
