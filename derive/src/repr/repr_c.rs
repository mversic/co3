use darling::util::SpannedValue;
use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::{Ident, parse_quote, visit::Visit};

use crate::{
    attr::repr::ReprPrimitive,
    repr::{
        FfiTypeField, FfiTypeVariant, is_type_parameterized,
        niche::{gen_enum_niche_ir, gen_struct_niche_ir},
        no_repr::{gen_data_enum_borrow_ir, gen_struct_borrow_ir, variant_mapper},
    },
};

pub(super) fn derive_repr_c_struct<const NEEDS_DROP: bool>(
    struct_name: &Ident,
    generics: &syn::Generics,
    fields: &darling::ast::Fields<FfiTypeField>,
) -> TokenStream {
    let (repr_c_struct_name, repr_c_struct) =
        gen_repr_c_struct::<NEEDS_DROP>(struct_name, generics, fields);
    let field_types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();
    let size_family_impl = if !NEEDS_DROP {
        gen_sized_family(struct_name, generics)
    } else {
        gen_struct_size_family(struct_name, generics, &field_types)
    };

    let is_valid = match fields.style {
        darling::ast::Style::Tuple => {
            let validations = fields.iter().enumerate().map(|(i, field)| {
                let field_index = syn::Index::from(i);
                let field_ty = &field.ty;

                quote! {
                    <#field_ty as co3::transmute::FlatTransmute>::is_valid(&target.#field_index)
                }
            });

            quote! { #(#validations)&&* }
        }
        darling::ast::Style::Struct => {
            let validations = fields.iter().map(|field| {
                let field_name = &field.ident;
                let field_ty = &field.ty;

                quote! {
                    <#field_ty as co3::transmute::FlatTransmute>::is_valid(&target.#field_name)
                }
            });

            quote! { #(#validations)&&* }
        }
        darling::ast::Style::Unit => unreachable!("ZSTs are not FFI safe"),
    };

    let transparent_impl = gen_transparent_impl(
        struct_name,
        generics,
        &repr_c_struct_name,
        is_valid,
        fields.iter(),
    );

    let niche_ir = gen_struct_niche_ir(struct_name, generics, fields);
    let borrow_ir = gen_struct_borrow_ir::<NEEDS_DROP>(
        Some(quote! { #[repr(C)] }),
        struct_name,
        generics,
        fields,
    );

    quote! {
        #size_family_impl
        #repr_c_struct
        #transparent_impl
        #borrow_ir
        #niche_ir
    }
}

pub(super) fn derive_repr_c_data_enum<const NEEDS_DROP: bool>(
    repr: ReprPrimitive,
    enum_name: &Ident,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> TokenStream {
    let size_family_impl = gen_sized_family(enum_name, generics);
    let (repr_c_enum_name, repr_c_enum) = gen_repr_c_data_enum(enum_name, generics, repr, variants);

    let is_valid = {
        let mut match_arms = Vec::new();

        for (idx, variant) in variants.iter().enumerate() {
            let variant_name = &variant.ident;

            let validation = variant_mapper(
                variant,
                || quote! { true },
                |field| {
                    let field_ty = &field.ty;
                    quote! {
                        <#field_ty as co3::transmute::FlatTransmute>::is_valid(
                            unsafe { &target.payload.#variant_name }
                        )
                    }
                },
            );

            match_arms.push(quote! {
                #idx => #validation
            });
        }

        quote! {
            match target.tag as usize {
                #(#match_arms,)*
                _ => false,
            }
        }
    };

    let fields = variants.iter().flat_map(|variant| variant.fields.iter());
    let niche_ir = gen_enum_niche_ir(repr, enum_name, generics, variants);
    let borrow_ir = gen_data_enum_borrow_ir::<NEEDS_DROP>(
        Some(quote! { #[repr(C, #repr)] }),
        enum_name,
        generics,
        variants,
    );

    let transparent_impl =
        gen_transparent_impl(enum_name, generics, &repr_c_enum_name, is_valid, fields);

    quote! {
        #size_family_impl
        #repr_c_enum
        #transparent_impl
        #niche_ir
        #borrow_ir
    }
}

pub(super) fn derive_data_enum<const NEEDS_DROP: bool>(
    repr: ReprPrimitive,
    enum_name: &Ident,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> TokenStream {
    let size_family_impl = gen_sized_family(enum_name, generics);
    let (union_name, union_and_helpers) = gen_data_enum(enum_name, generics, repr, variants);

    let is_valid = {
        let mut match_arms = Vec::new();

        for (idx, variant) in variants.iter().enumerate() {
            let variant_name = &variant.ident;

            let validation = variant_mapper(
                variant,
                || quote! { true },
                |field| {
                    let field_ty = &field.ty;
                    quote! {
                        <#field_ty as co3::transmute::FlatTransmute>::is_valid(
                            unsafe { &target.#variant_name.value}
                        )
                    }
                },
            );

            match_arms.push(quote! {
                #idx => #validation
            });
        }

        quote! {
            // SAFETY: All variant structs have tag as first field at offset 0
            // We can safely read it by casting the union pointer to the repr type
            match unsafe { *core::ptr::from_ref(target).cast::<#repr>() } as usize {
                #(#match_arms,)*
                _ => false,
            }
        }
    };

    let fields = variants.iter().flat_map(|variant| variant.fields.iter());
    let niche_ir = gen_enum_niche_ir(repr, enum_name, generics, variants);

    let transparent_impl = gen_transparent_impl(enum_name, generics, &union_name, is_valid, fields);
    let borrow_ir = gen_data_enum_borrow_ir::<NEEDS_DROP>(
        Some(quote! { #[repr(#repr)] }),
        enum_name,
        generics,
        variants,
    );

    quote! {
        #size_family_impl
        #union_and_helpers
        #transparent_impl
        #borrow_ir
        #niche_ir
    }
}

pub(crate) fn derive_fieldless_enum(
    repr: ReprPrimitive,
    enum_name: &Ident,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let size_family_impl = gen_sized_family(enum_name, generics);
    let niche_ir = gen_enum_niche_ir(repr, enum_name, generics, variants);

    let niche_value = proc_macro2::Literal::usize_unsuffixed(variants.len());
    let (target_arg, is_valid) = if is_exhaustive_enum(variants.len(), repr) {
        (quote! { _ }, quote! { true })
    } else {
        let is_valid = match repr {
            ReprPrimitive::U8 | ReprPrimitive::U16 | ReprPrimitive::U32 | ReprPrimitive::U64 => {
                quote! { (*target as usize) < #niche_value }
            }
            ReprPrimitive::I8 | ReprPrimitive::I16 | ReprPrimitive::I32 | ReprPrimitive::I64 => {
                quote! { *target >= 0 && (*target as usize) < #niche_value }
            }
        };

        (quote! { target }, is_valid)
    };

    let borrow_ir = gen_fieldless_enum_drop_ir(enum_name, generics);

    quote! {
        #size_family_impl
        impl #impl_generics co3::ir::ReprFamily for #enum_name #ty_generics #where_clause {
            type Kind = co3::ir::Transmuted;
        }

        #borrow_ir

        unsafe #impl_generics impl co3::transmute::CheckedTransmute for #enum_name #ty_generics #where_clause {
            type Target = #repr;

            #[inline(always)]
            fn is_valid(#target_arg: &Self::Target) -> bool {
                #is_valid
            }
        }

        unsafe #impl_generics impl co3::transmute::EncodeTransmuted<false> for #enum_name #ty_generics #where_clause {
            type Store = <Self::Target as co3::EncodeWithStore<false>>::Store;
        }

        #niche_ir
    }
}

pub(crate) fn gen_fieldless_enum_drop_ir(name: &Ident, generics: &syn::Generics) -> TokenStream {
    let (_, ty_generics, where_clause) = generics.split_for_impl();

    let mut params = generics.params.clone();
    params.push(parse_quote!(const IN_STRUCT: bool));

    let drop_impl_assert = assert_drop_impl(generics, name);

    quote! {
        impl<#params> co3::borrow::Borrow<IN_STRUCT> for #name #ty_generics #where_clause {
            type Borrowed<'_išč> = Self
            where
                Self: '_išč;

            type Store = ();

            #[inline(always)]
            fn borrow<'_išč>(self, (): &mut ()) -> Self::Borrowed<'_išč> where Self: '_išč {
                #drop_impl_assert
                self
            }
        }

        impl<'_ršč, #params> co3::borrow::ToOwned<'_ršč, IN_STRUCT> for #name #ty_generics #where_clause {
            #[inline(always)]
            fn to_owned(borrowed: Self::Borrowed<'_ršč>) -> Self {
                borrowed
            }
        }
    }
}

pub(super) fn assert_drop_impl(generics: &syn::Generics, ident: &syn::Ident) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        const _: () = {
            #[allow(dead_code)]
            trait AssertNoDrop {
                fn assert_no_drop();
            }

            impl #impl_generics AssertNoDrop for #ident #ty_generics #where_clause {
                fn assert_no_drop() {
                    const {
                        assert!(co3::impls!(Self: !Drop));
                    }
                }
            }
        };
    }
}

pub(super) fn gen_repr_c_struct<const NEEDS_DROP: bool>(
    struct_name: &syn::Ident,
    generics: &syn::Generics,
    fields: &darling::ast::Fields<FfiTypeField>,
) -> (syn::Ident, TokenStream) {
    let repr_c_struct_name = gen_repr_c_item_name(struct_name);

    let field_types: Vec<_> = fields.iter().map(|field| &field.ty).collect();

    let fields_code = match fields.style {
        darling::ast::Style::Struct => {
            let field_names = fields.iter().map(|field| &field.ident);
            let field_tys = fields.iter().map(|field| {
                let field_ty = &field.ty;
                quote! {<#field_ty as co3::ExternC>::CType}
            });

            quote! { #(#field_names: #field_tys),* }
        }
        darling::ast::Style::Tuple => {
            let field_tys = fields.iter().map(|field| {
                let field_ty = &field.ty;
                quote! { <#field_ty as co3::ExternC>::CType }
            });

            quote! { #(#field_tys),* }
        }
        darling::ast::Style::Unit => unreachable!("ZSTs are not FFI safe"),
    };

    let size_family_impl = if NEEDS_DROP {
        gen_struct_size_family_with_bounds(
            &repr_c_struct_name,
            generics,
            &field_types,
            gen_extern_c_bounds(&field_types, generics),
        )
    } else {
        gen_sized_family_with_bounds(
            &repr_c_struct_name,
            generics,
            gen_extern_c_bounds(&field_types, generics),
        )
    };

    let repr_c_struct = gen_repr_c_type::<false, true>(
        format!(" FFI-safe equivalent of [`{struct_name}`]"),
        repr_c_struct_name.clone(),
        generics,
        fields.style,
        fields_code,
        &field_types,
    );

    (
        repr_c_struct_name,
        quote! { #repr_c_struct #size_family_impl },
    )
}

pub(crate) fn gen_struct_size_family(
    struct_name: &Ident,
    generics: &syn::Generics,
    field_types: &[&syn::Type],
) -> TokenStream {
    gen_struct_size_family_with_bounds(struct_name, generics, field_types, quote! {})
}

fn gen_struct_size_family_with_bounds(
    struct_name: &Ident,
    generics: &syn::Generics,
    field_types: &[&syn::Type],
    extra_bounds: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let Some(last_field) = field_types.last() else {
        return gen_sized_family_with_bounds(struct_name, generics, quote! {});
    };

    let for_dummy =
        (!is_type_parameterized(last_field, generics)).then_some(quote! { for<'_dummy> });

    quote! {
        impl #impl_generics co3::dst::DstFamily for #struct_name #ty_generics
        where
            #for_dummy #last_field: co3::dst::DstFamily,
            #extra_bounds
            #predicates
        {
            type Kind = <#last_field as co3::dst::DstFamily>::Kind;
        }

        // FIXME:
        //impl #impl_generics co3::dst::Dst for #struct_name #ty_generics
        //where
        //    #for_dummy #last_field: co3::dst::DstFamily,
        //    #extra_bounds
        //    #predicates
        //{
        //    type Preamble = #kita;
        //    type Payload = #last_field;
        //}
    }
}

pub(crate) fn gen_sized_family(type_name: &Ident, generics: &syn::Generics) -> TokenStream {
    gen_sized_family_with_bounds(type_name, generics, quote! {})
}

fn gen_sized_family_with_bounds(
    type_name: &Ident,
    generics: &syn::Generics,
    extra_bounds: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    quote! {
        impl #impl_generics co3::dst::DstFamily for #type_name #ty_generics
        where
            #extra_bounds
            #predicates
        {
            type Kind = co3::dst::Sized_;
        }
    }
}

pub(super) fn gen_data_enum(
    enum_name: &syn::Ident,
    generics: &syn::Generics,
    repr: ReprPrimitive,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> (syn::Ident, TokenStream) {
    let union_name = gen_repr_c_item_name(enum_name);

    let mut all_field_types = Vec::new();
    for variant in variants {
        for field in variant.fields.iter() {
            all_field_types.push(&field.ty);
        }
    }

    let mut variant_structs = Vec::new();
    let mut variant_ty_generics = Vec::new();

    for variant in variants {
        let variant_name = &variant.ident;
        let variant_struct_name = gen_data_enum_variant_name(enum_name, variant_name);

        let variant_field_types: Vec<_> = variant.fields.iter().map(|f| &f.ty).collect();

        let filtered_generics = filter_generics(&variant_field_types, generics);
        let (_, var_ty_generics, _) = filtered_generics.split_for_impl();
        variant_ty_generics.push(var_ty_generics.into_token_stream());

        let fields = variant_mapper(
            variant,
            || quote! { tag: #repr },
            |field| {
                let field_ty = &field.ty;
                quote! {
                    tag: #repr,
                    value: <#field_ty as co3::ExternC>::CType
                }
            },
        );

        let size_family_impl = gen_sized_family_with_bounds(
            &variant_struct_name,
            &filtered_generics,
            gen_extern_c_bounds(&variant_field_types, &filtered_generics),
        );
        let struct_ = gen_repr_c_type::<false, true>(
            format!(" Variant struct for [`{enum_name}::{variant_name}`]"),
            variant_struct_name,
            &filtered_generics,
            darling::ast::Style::Struct,
            fields,
            &variant_field_types,
        );

        variant_structs.push(quote! {#size_family_impl #struct_});
    }

    let union_fields = variants
        .iter()
        .zip(variant_ty_generics.iter())
        .map(|(variant, ty_gen)| {
            let variant_name = &variant.ident;
            let variant_struct_name = gen_data_enum_variant_name(enum_name, variant_name);
            quote! { #variant_name: #variant_struct_name #ty_gen }
        });

    let union_def = gen_repr_c_type::<true, true>(
        format!(" FFI-safe equivalent of [`{enum_name}`]"),
        union_name.clone(),
        generics,
        darling::ast::Style::Struct,
        quote! { #(#union_fields),* },
        &all_field_types,
    );

    let size_family_impl = gen_sized_family_with_bounds(
        &union_name,
        generics,
        gen_extern_c_bounds(&all_field_types, generics),
    );
    let all_code = quote! {
        #(#variant_structs)*
        #size_family_impl
        #union_def
    };

    (union_name, all_code)
}

fn gen_repr_c_data_enum(
    enum_name: &syn::Ident,
    generics: &syn::Generics,
    repr: ReprPrimitive,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> (syn::Ident, TokenStream) {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let params = &generics.params;
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let (payload_name, payload) = gen_data_enum_payload(enum_name, generics, variants);

    let doc = format!(" FFI-safe equivalent of [`{enum_name}`]");
    let repr_c_enum_name = gen_repr_c_item_name(enum_name);

    let mut field_types = Vec::new();
    for variant in variants {
        for field in variant.fields.iter() {
            field_types.push(&field.ty);
        }
    }
    let extern_c_bounds = gen_extern_c_bounds(&field_types, generics);
    let size_family_impl =
        gen_sized_family_with_bounds(&repr_c_enum_name, generics, extern_c_bounds.clone());

    let repr_c_enum = quote! {
        #payload

        #[repr(C)]
        #[doc = #doc]
        #[doc(hidden)]
        pub struct #repr_c_enum_name #impl_generics where #extern_c_bounds #predicates {
            tag: #repr, payload: #payload_name #ty_generics,
        }

        impl #impl_generics Clone for #repr_c_enum_name #ty_generics where #extern_c_bounds #predicates {
            fn clone(&self) -> Self { *self }
        }

        impl #impl_generics Copy for #repr_c_enum_name #ty_generics where #extern_c_bounds #predicates {}
        co3::reprC! { unsafe impl(#params) Robust for #repr_c_enum_name #ty_generics where (#extern_c_bounds #predicates) {} }

        #size_family_impl
    };

    (repr_c_enum_name, repr_c_enum)
}

fn gen_repr_c_type<const IS_UNION: bool, const IS_PUBLIC: bool>(
    doc: String,
    ident: syn::Ident,
    generics: &syn::Generics,
    style: darling::ast::Style,
    fields_code: TokenStream,
    field_types: &[&syn::Type],
) -> TokenStream {
    let visibility = IS_PUBLIC.then_some(quote! {pub});

    let extern_c_bounds = gen_extern_c_bounds(field_types, generics);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let params = &generics.params;
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let type_def = if IS_UNION {
        quote! {
            #[repr(C)]
            #[doc = #doc]
            #[doc(hidden)]
            #[allow(non_snake_case)]
            #visibility union #ident #impl_generics where #extern_c_bounds #predicates {
                #fields_code
            }
        }
    } else {
        match style {
            darling::ast::Style::Tuple => quote! {
                #[repr(C)]
                #[doc = #doc]
                #[doc(hidden)]
                #visibility struct #ident #impl_generics (#fields_code) where #extern_c_bounds #predicates;
            },
            darling::ast::Style::Struct => quote! {
                #[repr(C)]
                #[doc = #doc]
                #[doc(hidden)]
                #visibility struct #ident #impl_generics where #extern_c_bounds #predicates {
                    #fields_code
                }
            },
            darling::ast::Style::Unit => unreachable!("ZSTs are not FFI safe"),
        }
    };

    quote! {
        #type_def

        impl #impl_generics Clone for #ident #ty_generics where #extern_c_bounds #predicates {
            fn clone(&self) -> Self { *self }
        }

        impl #impl_generics Copy for #ident #ty_generics where #extern_c_bounds #predicates {}
        co3::reprC! { unsafe impl(#params) Robust for #ident #ty_generics where (#extern_c_bounds #predicates) {} }
    }
}

fn gen_data_enum_payload(
    enum_name: &syn::Ident,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> (syn::Ident, TokenStream) {
    let payload_name = gen_data_enum_payload_name(enum_name);

    let mut field_types = Vec::new();
    for variant in variants {
        for field in variant.fields.iter() {
            field_types.push(&field.ty);
        }
    }

    let field_names = variants.iter().map(|variant| &variant.ident);
    let field_tys = variants.iter().map(|variant| {
        variant_mapper(
            variant,
            || quote! {()},
            |field| {
                let field_ty = &field.ty;
                quote! {<#field_ty as co3::ExternC>::CType}
            },
        )
    });

    let size_family_impl = gen_sized_family_with_bounds(
        &payload_name,
        generics,
        gen_extern_c_bounds(&field_types, generics),
    );
    let payload = gen_repr_c_type::<true, false>(
        format!(" Payload of [`{}`]", gen_repr_c_item_name(enum_name)),
        payload_name.clone(),
        generics,
        darling::ast::Style::Struct,
        quote! { #(#field_names: #field_tys),* },
        &field_types,
    );

    (payload_name, quote! { #payload #size_family_impl })
}

fn gen_transparent_impl<'a>(
    item_name: &Ident,
    generics: &syn::Generics,
    target: &syn::Ident,
    is_valid_body: TokenStream,
    fields: impl IntoIterator<Item = &'a FfiTypeField>,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let field_types = fields.into_iter().map(|f| &f.ty);
    let flat_transmute_bounds = gen_flat_transmute_bounds(field_types, generics);

    quote! {
        impl #impl_generics co3::ir::ReprFamily for #item_name #ty_generics #where_clause {
            type Kind = co3::ir::Transmuted;
        }

        unsafe impl #impl_generics co3::transmute::CheckedTransmute for #item_name #ty_generics
        where
            #flat_transmute_bounds
            #predicates
        {
            type Target = #target #ty_generics;

            #[inline(always)]
            fn is_valid(target: &Self::Target) -> bool {
                #is_valid_body
            }
        }

        unsafe impl #impl_generics co3::transmute::EncodeTransmuted<false> for #item_name #ty_generics where
            // FIXME:
            #target #ty_generics: co3::EncodeWithStore<false>,
            #flat_transmute_bounds
            #predicates
        {
            type Store = <Self::Target as co3::EncodeWithStore<false>>::Store;
        }
    }
}

/// Checks if an enum exhausts all possible values of its repr type
pub(super) fn is_exhaustive_enum(num_variants: usize, repr: ReprPrimitive) -> bool {
    let max_values = match repr {
        ReprPrimitive::U8 | ReprPrimitive::I8 => 1u64 << 8,
        ReprPrimitive::U16 | ReprPrimitive::I16 => 1u64 << 16,
        ReprPrimitive::U32 | ReprPrimitive::I32 => 1u64 << 32,
        ReprPrimitive::U64 | ReprPrimitive::I64 => return false,
    };

    num_variants as u64 == max_values
}

pub(super) fn gen_repr_c_item_name(item_name: &syn::Ident) -> syn::Ident {
    syn::Ident::new(&format!("C{item_name}"), proc_macro2::Span::call_site())
}

pub(super) fn gen_data_enum_payload_name(enum_name: &syn::Ident) -> syn::Ident {
    syn::Ident::new(
        &format!("C{enum_name}Payload"),
        proc_macro2::Span::call_site(),
    )
}

pub(super) fn gen_data_enum_variant_name(
    enum_name: &syn::Ident,
    variant_name: &syn::Ident,
) -> syn::Ident {
    syn::Ident::new(
        &format!("C{enum_name}Variant{variant_name}"),
        proc_macro2::Span::call_site(),
    )
}

pub(super) fn gen_extern_c_bounds(fields: &[&syn::Type], generics: &syn::Generics) -> TokenStream {
    let parameterized_field_types = fields
        .iter()
        .filter(|ty| is_type_parameterized(ty, generics));

    quote! { #(#parameterized_field_types: co3::ExternC,)* }
}

fn gen_flat_transmute_bounds<'a>(
    fields: impl Iterator<Item = &'a syn::Type>,
    generics: &syn::Generics,
) -> TokenStream {
    let parameterized_field_types = fields.filter(|&ty| is_type_parameterized(ty, generics));

    quote! { #(
        #parameterized_field_types: co3::ExternC + co3::transmute::FlatTransmute<
            Target = <#parameterized_field_types as co3::ExternC>::CType
        >,)*
    }
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

fn filter_generics(field_types: &[&syn::Type], generics: &syn::Generics) -> syn::Generics {
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
