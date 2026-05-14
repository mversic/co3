use darling::util::SpannedValue;
use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::{Ident, visit::Visit};

use crate::{
    attr::repr::ReprPrimitive,
    repr::{
        FfiTypeField, FfiTypeKindAttribute, FfiTypeVariant, gen_sized_family,
        gen_struct_size_family, is_type_parameterized,
        niche::{gen_enum_niche_ir_with_mode, gen_struct_niche_ir},
        no_repr::{
            gen_borrow_bounds, gen_borrow_cast_eq_bounds, gen_data_enum_borrow_ir,
            gen_struct_borrow_ir, gen_view_bounds, variant_mapper,
        },
    },
};

#[derive(Clone, Copy)]
pub(super) enum ReprFamily {
    NoRepr,
    ReprC,
}

fn lowered_field_ty(field_ty: &syn::Type, lowering: ReprFamily) -> TokenStream {
    match lowering {
        ReprFamily::ReprC => quote!(<#field_ty as co3::transmute::FlatTransmute>::Target),
        ReprFamily::NoRepr => quote!(<#field_ty as co3::ExternC>::CType),
    }
}

fn lowered_field_copy_bounds(
    field_types: &[&syn::Type],
    generics: &syn::Generics,
    lowering: ReprFamily,
) -> TokenStream {
    let lowered_field_types = field_types
        .iter()
        .filter(|ty| is_type_parameterized(ty, generics))
        .map(|ty| lowered_field_ty(ty, lowering));

    quote! { #(#lowered_field_types: Copy,)* }
}

fn gen_transparent_view_impl<const ADD_COPY: bool>(
    name: &Ident,
    generics: &syn::Generics,
    field_types: &[&syn::Type],
) -> TokenStream {
    fn strip_view_suffix(view_name: &Ident) -> Ident {
        let name = view_name.to_string();
        let owner = name.strip_suffix("View").unwrap_or(&name);
        Ident::new(owner, view_name.span())
    }

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let owner_ty_generics = generics
        .params
        .iter()
        .skip(1)
        .map(|param| match param {
            syn::GenericParam::Lifetime(syn::LifetimeParam { lifetime, .. }) => quote! {#lifetime},
            syn::GenericParam::Type(syn::TypeParam { ident, .. }) => quote! {#ident},
            syn::GenericParam::Const(syn::ConstParam { ident, .. }) => quote! {#ident},
        })
        .collect::<Vec<_>>();

    let owner_name = strip_view_suffix(name);
    let owner_const_view = format_ident!("C{}ConstView", owner_name);
    let owner_target = format_ident!("C{}", owner_name);
    let owner_ty = quote! { #owner_name<#(#owner_ty_generics),*> };

    //let extern_c_bounds = gen_flat_transmute_bounds::<ADD_COPY>(field_types, generics);
    let borrow_cast_equality_bounds = gen_borrow_cast_eq_bounds(field_types);
    let view_bounds = gen_view_bounds::<ADD_COPY>(field_types.iter().copied(), generics);

    quote! {
        impl #impl_generics co3::ir::ReprFamily for #name #ty_generics #where_clause {
            type Kind = #(<#field_types as co3::ir::ReprFamily>::Kind)+*;
        }

        // FIXME:
        //impl #impl_generics co3::ir::EncodeReprFamily for #name #ty_generics
        //where
        //    #owner_const_view<#(#owner_ty_generics),*>: co3::ir::EncodeReprFamily,
        //    #predicates
        //{
        //    type Kind = <#owner_const_view<#(#owner_ty_generics),*> as co3::ir::EncodeReprFamily>::Kind;
        //}

        unsafe impl #impl_generics co3::transmute::CheckedTransmute for #name #ty_generics
        where
            #borrow_cast_equality_bounds
            #view_bounds
            #predicates
        {
            type Target = #owner_const_view<#(#owner_ty_generics),*>;

            #[inline(always)]
            fn is_valid(target: &Self::Target) -> bool {
                let target = <*const _>::cast::<#owner_target<#(#owner_ty_generics),*>>(
                    core::ptr::from_ref(target)
                );

                <#owner_ty as co3::transmute::CheckedTransmute>::is_valid(unsafe {&*target })
            }
        }
    }
}

pub(super) fn custom_is_valid(ffi_type_kind: Option<&FfiTypeKindAttribute>) -> Option<TokenStream> {
    let Some(FfiTypeKindAttribute::Transparent(_, is_valid)) = ffi_type_kind else {
        return None;
    };

    Some(quote! { (#is_valid)(target) })
}

pub(super) fn derive_repr_c_struct<const IS_VIEW: bool>(
    struct_name: &Ident,
    vis: &syn::Visibility,
    generics: &syn::Generics,
    fields: &darling::ast::Fields<FfiTypeField>,
    ffi_type_kind: Option<&FfiTypeKindAttribute>,
) -> TokenStream {
    let repr_c_struct_name = gen_repr_c_item_name(struct_name);
    let repr_c_struct =
        gen_repr_c_struct::<IS_VIEW>(struct_name, vis, generics, fields, ReprFamily::ReprC);
    let field_types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();
    let size_family_impl = gen_struct_size_family(struct_name, generics, &field_types, quote! {});

    let transparent_impl = if IS_VIEW {
        gen_transparent_view_impl::<false>(struct_name, generics, &field_types)
    } else {
        let default_is_valid = match fields.style {
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
            darling::ast::Style::Unit => unreachable!(),
        };
        let is_valid = custom_is_valid(ffi_type_kind).unwrap_or(default_is_valid);

        gen_transparent_impl::<false>(
            struct_name,
            generics,
            &repr_c_struct_name,
            is_valid,
            fields.iter(),
        )
    };

    let niche_ir = gen_struct_niche_ir(struct_name, generics, fields, ffi_type_kind);
    let borrow_ir = if IS_VIEW {
        gen_identity_borrow_ir(struct_name, generics)
    } else {
        gen_struct_borrow_ir(
            Some(quote! { #[repr(C)] }),
            struct_name,
            vis,
            generics,
            fields,
        )
    };

    quote! {
        #repr_c_struct
        #size_family_impl
        #transparent_impl
        #borrow_ir
        #niche_ir
    }
}

pub(super) fn derive_repr_c_data_enum<const IS_VIEW: bool>(
    repr: ReprPrimitive,
    enum_name: &Ident,
    vis: &syn::Visibility,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
    ffi_type_kind: Option<&FfiTypeKindAttribute>,
) -> TokenStream {
    let size_family_impl = gen_sized_family(enum_name, generics, quote! {});
    let (repr_c_enum_name, repr_c_enum) =
        gen_repr_c_data_enum(enum_name, vis, generics, repr, variants);

    let fields = variants
        .iter()
        .flat_map(|variant| variant.fields.iter())
        .collect::<Vec<_>>();
    let field_types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();
    let niche_ir = (!IS_VIEW).then(|| {
        gen_enum_niche_ir_with_mode(
            repr,
            enum_name,
            generics,
            variants,
            ReprFamily::ReprC,
            ffi_type_kind,
        )
    });

    let (transparent_impl, borrow_ir) = if IS_VIEW {
        (
            gen_transparent_view_impl::<true>(enum_name, generics, &field_types),
            gen_identity_borrow_ir(enum_name, generics),
        )
    } else {
        let default_is_valid = {
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
        let is_valid = custom_is_valid(ffi_type_kind).unwrap_or(default_is_valid);

        (
            gen_transparent_impl::<true>(
                enum_name,
                generics,
                &repr_c_enum_name,
                is_valid,
                fields.iter().copied(),
            ),
            gen_data_enum_borrow_ir(
                Some(quote! { #[repr(C, #repr)] }),
                enum_name,
                vis,
                generics,
                variants,
            ),
        )
    };

    quote! {
        #size_family_impl
        #repr_c_enum
        #transparent_impl
        #niche_ir
        #borrow_ir
    }
}

pub(super) fn derive_data_enum<const IS_VIEW: bool>(
    repr: ReprPrimitive,
    enum_name: &Ident,
    vis: &syn::Visibility,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
    ffi_type_kind: Option<&FfiTypeKindAttribute>,
) -> TokenStream {
    let union_name = gen_repr_c_item_name(enum_name);
    let size_family_impl = gen_sized_family(enum_name, generics, quote! {});
    let union_and_helpers =
        gen_data_enum::<IS_VIEW>(enum_name, vis, generics, repr, variants, ReprFamily::ReprC);

    let fields = variants
        .iter()
        .flat_map(|variant| variant.fields.iter())
        .collect::<Vec<_>>();
    let field_types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();
    let niche_ir = (!IS_VIEW).then(|| {
        gen_enum_niche_ir_with_mode(
            repr,
            enum_name,
            generics,
            variants,
            ReprFamily::ReprC,
            ffi_type_kind,
        )
    });

    let (transparent_impl, borrow_ir) = if IS_VIEW {
        (
            gen_identity_borrow_ir(enum_name, generics),
            gen_transparent_view_impl::<true>(enum_name, generics, &field_types),
        )
    } else {
        let default_is_valid = {
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
                match unsafe { *<*const Self::Target>::cast::<#repr>(core::ptr::from_ref(target)) } as usize {
                    #(#match_arms,)*
                    _ => false,
                }
            }
        };
        let is_valid = custom_is_valid(ffi_type_kind).unwrap_or(default_is_valid);

        (
            gen_transparent_impl::<true>(
                enum_name,
                generics,
                &union_name,
                is_valid,
                fields.iter().copied(),
            ),
            gen_data_enum_borrow_ir(
                Some(quote! { #[repr(#repr)] }),
                enum_name,
                vis,
                generics,
                variants,
            ),
        )
    };

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
    let size_family_impl = gen_sized_family(enum_name, generics, quote! {});
    let niche_ir =
        gen_enum_niche_ir_with_mode(repr, enum_name, generics, variants, ReprFamily::ReprC, None);

    let niche_value = proc_macro2::Literal::usize_unsuffixed(variants.len());
    let borrow_ir = gen_identity_borrow_ir(enum_name, generics);
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

    quote! {
        #size_family_impl
        #borrow_ir
        #niche_ir

        impl #impl_generics co3::ir::ReprFamily for #enum_name #ty_generics #where_clause {
            type Kind = co3::ir::Transmuted;
        }

        impl #impl_generics co3::ir::EncodeReprFamily for #enum_name #ty_generics #where_clause {
            type Kind = co3::ir::Transmuted;
        }

        unsafe #impl_generics impl co3::transmute::CheckedTransmute for #enum_name #ty_generics #where_clause {
            type Target = #repr;

            #[inline(always)]
            fn is_valid(#target_arg: &Self::Target) -> bool {
                #is_valid
            }
        }

        unsafe impl #impl_generics co3::handle::Erase for #enum_name #ty_generics #where_clause {
            type Erased = Self;
        }
    }
}

pub(crate) fn gen_identity_borrow_ir(name: &Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let drop_impl_assert = assert_no_drop(generics, name);
    let params = &generics.params;

    quote! {
        #drop_impl_assert

        impl #impl_generics co3::borrow::Borrow for #name #ty_generics #where_clause {
            type Borrowed<'_išč>
                = Self
            where
                Self: '_išč;

            type Owner = ();

            #[inline(always)]
            fn borrow<'_išč>(self, (): &mut ()) -> Self::Borrowed<'_išč>
            where
                Self: '_išč,
            {
                self
            }
        }

        impl<'d, #params> co3::borrow::ToOwned<'d> for #name #ty_generics #where_clause {
            fn to_owned(source: Self) -> Self {
                source
            }
        }
    }
}

pub(super) fn assert_no_drop(generics: &syn::Generics, ident: &syn::Ident) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        const _: () = {
            #[expect(dead_code)]
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

pub(super) fn gen_repr_c_struct<const IS_VIEW: bool>(
    struct_name: &syn::Ident,
    vis: &syn::Visibility,
    generics: &syn::Generics,
    fields: &darling::ast::Fields<FfiTypeField>,
    lowering: ReprFamily,
) -> TokenStream {
    let field_types: Vec<_> = fields.iter().map(|field| &field.ty).collect();
    let repr_c_struct_name = gen_repr_c_item_name(struct_name);

    let field_c_tys = fields
        .iter()
        .map(|field| lowered_field_ty(&field.ty, lowering))
        .collect::<Vec<_>>();

    let fields_code = match fields.style {
        darling::ast::Style::Struct => {
            let field_names = fields.iter().map(|field| &field.ident);

            quote! { #(#field_names: #field_c_tys),* }
        }
        darling::ast::Style::Tuple => {
            quote! { #(#field_c_tys),* }
        }
        darling::ast::Style::Unit => unreachable!(),
    };

    let fields_const_code = match fields.style {
        darling::ast::Style::Struct => {
            let field_names = fields.iter().map(|field| &field.ident);

            quote! { #(#field_names: <#field_c_tys as co3::borrow::BorrowCast>::AsConst),* }
        }
        darling::ast::Style::Tuple => {
            quote! { #(<#field_c_tys as co3::borrow::BorrowCast>::AsConst),* }
        }
        darling::ast::Style::Unit => unreachable!(),
    };

    let fields_mut_code = match fields.style {
        darling::ast::Style::Struct => {
            let field_names = fields.iter().map(|field| &field.ident);

            quote! { #(#field_names: <#field_c_tys as co3::borrow::BorrowCast>::AsMut),* }
        }
        darling::ast::Style::Tuple => {
            quote! { #(<#field_c_tys as co3::borrow::BorrowCast>::AsMut),* }
        }
        darling::ast::Style::Unit => unreachable!(),
    };

    let doc = format!(" FFI-safe equivalent of [`{struct_name}`]");
    let size_family_impl = gen_struct_size_family(
        &repr_c_struct_name,
        generics,
        &field_types,
        gen_field_lowering_bounds::<false>(&field_types, generics, lowering),
    );

    let repr_c_struct = gen_repr_c_type::<false>(
        doc,
        repr_c_struct_name,
        vis,
        generics,
        fields.style,
        fields_code,
        fields_const_code,
        fields_mut_code,
        &field_c_tys,
        &field_types,
        lowering,
    );

    quote! {
        #repr_c_struct
        #size_family_impl
    }
}

pub(super) fn gen_data_enum<const IS_VIEW: bool>(
    enum_name: &syn::Ident,
    vis: &syn::Visibility,
    generics: &syn::Generics,
    repr: ReprPrimitive,
    variants: &[SpannedValue<FfiTypeVariant>],
    lowering: ReprFamily,
) -> TokenStream {
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
        let variant_value_ty = variant_mapper(
            variant,
            || quote! {()},
            |field| lowered_field_ty(&field.ty, lowering),
        );
        let variant_c_tys = variant_mapper(
            variant,
            || vec![quote! { #repr }],
            |_| vec![quote! { #repr }, variant_value_ty.clone()],
        );

        let fields = variant_mapper(
            variant,
            || quote! { tag: #repr },
            |_| {
                quote! {
                    tag: #repr,
                    value: #variant_value_ty
                }
            },
        );
        let fields_const = variant_mapper(
            variant,
            || quote! { tag: <#repr as co3::borrow::BorrowCast>::AsConst },
            |_| {
                quote! {
                    tag: <#repr as co3::borrow::BorrowCast>::AsConst,
                    value: <#variant_value_ty as co3::borrow::BorrowCast>::AsConst
                }
            },
        );
        let fields_mut = variant_mapper(
            variant,
            || quote! { tag: <#repr as co3::borrow::BorrowCast>::AsMut },
            |_| {
                quote! {
                    tag: <#repr as co3::borrow::BorrowCast>::AsMut,
                    value: <#variant_value_ty as co3::borrow::BorrowCast>::AsMut
                }
            },
        );

        let struct_ = gen_repr_c_type::<false>(
            format!(" Variant struct for [`{enum_name}::{variant_name}`]"),
            variant_struct_name,
            vis,
            &filtered_generics,
            darling::ast::Style::Struct,
            fields,
            fields_const,
            fields_mut,
            &variant_c_tys,
            &variant_field_types,
            lowering,
        );

        variant_structs.push(quote! {
            #struct_
        });
    }

    let union_field_tys = variants
        .iter()
        .zip(variant_ty_generics.iter())
        .map(|(variant, ty_gen)| {
            let variant_name = &variant.ident;
            let variant_struct_name = gen_data_enum_variant_name(enum_name, variant_name);
            quote! { #variant_struct_name #ty_gen }
        })
        .collect::<Vec<_>>();
    let union_fields = variants
        .iter()
        .zip(union_field_tys.iter())
        .map(|(variant, ty_gen)| {
            let variant_name = &variant.ident;
            quote! { #variant_name: #ty_gen }
        });
    let union_const_fields = variants
        .iter()
        .zip(union_field_tys.iter())
        .map(|(variant, ty)| {
            let variant_name = &variant.ident;
            quote! { #variant_name: <#ty as co3::borrow::BorrowCast>::AsConst }
        });
    let union_mut_fields = variants
        .iter()
        .zip(union_field_tys.iter())
        .map(|(variant, ty)| {
            let variant_name = &variant.ident;
            quote! { #variant_name: <#ty as co3::borrow::BorrowCast>::AsMut }
        });

    let union_doc = format!(" FFI-safe equivalent of [`{enum_name}`]");
    let union_def = gen_repr_c_type::<true>(
        union_doc,
        union_name.clone(),
        vis,
        generics,
        darling::ast::Style::Struct,
        quote! { #(#union_fields),* },
        quote! { #(#union_const_fields),* },
        quote! { #(#union_mut_fields),* },
        &union_field_tys,
        &all_field_types,
        lowering,
    );

    quote! {
        #(#variant_structs)*
        #union_def
    }
}

fn gen_repr_c_data_enum(
    enum_name: &syn::Ident,
    vis: &syn::Visibility,
    generics: &syn::Generics,
    repr: ReprPrimitive,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> (syn::Ident, TokenStream) {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let params = &generics.params;
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let (payload_name, payload_def) = gen_data_enum_payload(enum_name, vis, generics, variants);

    let doc = format!(" FFI-safe equivalent of [`{enum_name}`]");
    let repr_c_enum_name = gen_repr_c_item_name(enum_name);

    let mut field_types = Vec::new();
    for variant in variants {
        for field in variant.fields.iter() {
            field_types.push(&field.ty);
        }
    }
    let extern_c_bounds = gen_flat_transmute_bounds::<true>(&field_types, generics);
    let wrapper_field_tys = vec![quote! { #repr }, quote! { #payload_name #ty_generics }];
    let borrow_cast_impl = gen_borrow_cast_impl::<false>(
        &repr_c_enum_name,
        vis,
        generics,
        darling::ast::Style::Struct,
        quote! {
            tag: <#repr as co3::borrow::BorrowCast>::AsConst,
            payload: <#payload_name #ty_generics as co3::borrow::BorrowCast>::AsConst
        },
        quote! {
            tag: <#repr as co3::borrow::BorrowCast>::AsMut,
            payload: <#payload_name #ty_generics as co3::borrow::BorrowCast>::AsMut
        },
        &wrapper_field_tys,
        quote! { SizedRobust },
        quote! { #extern_c_bounds },
    );

    let repr_c_enum = quote! {
        #payload_def

        #[repr(C)]
        #[doc = #doc]
        #[doc(hidden)]
        #vis struct #repr_c_enum_name #impl_generics
        where
            #extern_c_bounds
            #predicates
        {
            tag: #repr, payload: #payload_name #ty_generics,
        }

        impl #impl_generics Clone for #repr_c_enum_name #ty_generics
        where
            #extern_c_bounds
            #predicates
        {
            fn clone(&self) -> Self { *self }
        }

        impl #impl_generics Copy for #repr_c_enum_name #ty_generics
        where
            #extern_c_bounds
            #predicates
        {}

        co3::reprC! {
            unsafe impl(#params) SizedRobust for #repr_c_enum_name #ty_generics where (#extern_c_bounds #predicates) {}
        }

        #borrow_cast_impl
    };

    (repr_c_enum_name, repr_c_enum)
}

fn gen_repr_c_type<const IS_UNION: bool>(
    doc: String,
    ident: syn::Ident,
    vis: &syn::Visibility,
    generics: &syn::Generics,
    style: darling::ast::Style,
    fields_code: TokenStream,
    fields_const_code: TokenStream,
    fields_mut_code: TokenStream,
    borrow_cast_field_tys: &[TokenStream],
    field_types: &[&syn::Type],
    lowering: ReprFamily,
) -> TokenStream {
    let type_bounds = gen_field_lowering_bounds::<IS_UNION>(field_types, generics, lowering);
    let copy_bounds = (!IS_UNION)
        .then(|| lowered_field_copy_bounds(field_types, generics, lowering))
        .unwrap_or_default();
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let params = &generics.params;
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let (robust_kind, type_bounds, copy_bounds) = if IS_UNION {
        (
            quote! {SizedRobust},
            type_bounds.clone(),
            type_bounds.clone(),
        )
    } else {
        (
            quote! {Robust},
            type_bounds.clone(),
            quote! { #copy_bounds #type_bounds },
        )
    };

    let type_def = if IS_UNION {
        quote! {
            #[expect(non_snake_case)]
            #vis union #ident #impl_generics where #type_bounds #predicates {
                #fields_code
            }
        }
    } else {
        match style {
            darling::ast::Style::Tuple => quote! {
                #vis struct #ident #impl_generics (#fields_code) where #type_bounds #predicates;
            },
            darling::ast::Style::Struct => quote! {
                #vis struct #ident #impl_generics where #type_bounds #predicates {
                    #fields_code
                }
            },
            darling::ast::Style::Unit => unreachable!(),
        }
    };
    let borrow_cast_impl = gen_borrow_cast_impl::<IS_UNION>(
        &ident,
        vis,
        generics,
        style,
        fields_const_code,
        fields_mut_code,
        borrow_cast_field_tys,
        robust_kind.clone(),
        type_bounds.clone(),
    );

    quote! {
        #[repr(C)]
        #[doc = #doc]
        #[doc(hidden)]
        #type_def

        impl #impl_generics Clone for #ident #ty_generics
        where
            #copy_bounds
            #predicates
        {
            fn clone(&self) -> Self { *self }
        }

        impl #impl_generics Copy for #ident #ty_generics
        where
            #copy_bounds
            #predicates
        {}

        #borrow_cast_impl

        co3::reprC! {
            unsafe impl(#params) #robust_kind for #ident #ty_generics where (#type_bounds #predicates) {}
        }
    }
}

fn gen_borrow_cast_impl<const IS_UNION: bool>(
    ident: &syn::Ident,
    vis: &syn::Visibility,
    generics: &syn::Generics,
    style: darling::ast::Style,
    fields_const_code: TokenStream,
    fields_mut_code: TokenStream,
    borrow_cast_field_tys: &[TokenStream],
    robust_kind: TokenStream,
    bounds: TokenStream,
) -> TokenStream {
    let const_name = format_ident!("{ident}ConstView");
    let mut_name = format_ident!("{ident}MutView");
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let borrow_cast_field_tys = borrow_cast_field_tys
        .iter()
        .filter(|&ty| is_type_parameterized(&syn::parse_quote!(#ty), generics))
        .collect::<Vec<_>>();

    let borrow_cast_bounds = quote! {
        #(#borrow_cast_field_tys: co3::borrow::BorrowCast,)*
    };
    let const_copy_bounds = quote! {
        #(#borrow_cast_field_tys: co3::borrow::BorrowCast<AsConst: Copy>,)*
    };
    let mut_copy_bounds = quote! {
        #(#borrow_cast_field_tys: co3::borrow::BorrowCast<AsMut: Copy>,)*
    };

    let const_view_bounds = if IS_UNION {
        quote! { #bounds #const_copy_bounds }
    } else {
        quote! { #bounds #borrow_cast_bounds }
    };
    let mut_view_bounds = if IS_UNION {
        quote! { #bounds #mut_copy_bounds }
    } else {
        quote! { #bounds #borrow_cast_bounds }
    };
    let borrow_cast_copy_bounds = if IS_UNION {
        quote! {
            #(#borrow_cast_field_tys: co3::borrow::BorrowCast<AsConst: Copy, AsMut: Copy>,)*
            #bounds
        }
    } else {
        quote! { #bounds #borrow_cast_bounds }
    };

    let const_type_def = gen_borrow_cast_type_def::<IS_UNION>(
        &const_name,
        vis,
        generics,
        style,
        fields_const_code,
        &const_view_bounds,
        robust_kind.clone(),
    );
    let mut_type_def = gen_borrow_cast_type_def::<IS_UNION>(
        &mut_name,
        vis,
        generics,
        style,
        fields_mut_code,
        &mut_view_bounds,
        robust_kind,
    );

    quote! {
        #const_type_def
        #mut_type_def

        impl #impl_generics Clone for #const_name #ty_generics
        where
            #const_copy_bounds
            #bounds
            #predicates
        {
            fn clone(&self) -> Self { *self }
        }
        impl #impl_generics Clone for #mut_name #ty_generics
        where
            #mut_copy_bounds
            #bounds
            #predicates
        {
            fn clone(&self) -> Self { *self }
        }

        impl #impl_generics Copy for #const_name #ty_generics
        where
            #const_copy_bounds
            #bounds
            #predicates
        {}
        impl #impl_generics Copy for #mut_name #ty_generics
        where
            #mut_copy_bounds
            #bounds
            #predicates
        {}

        unsafe impl #impl_generics co3::borrow::BorrowCast for #ident #ty_generics
        where
            #borrow_cast_copy_bounds
            #predicates
        {
            type AsConst = #const_name #ty_generics;
            type AsMut = #mut_name #ty_generics;
        }
    }
}

fn gen_borrow_cast_type_def<const IS_UNION: bool>(
    ident: &syn::Ident,
    vis: &syn::Visibility,
    generics: &syn::Generics,
    style: darling::ast::Style,
    fields_code: TokenStream,
    view_bounds: &TokenStream,
    robust_kind: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let params = &generics.params;

    let item = if IS_UNION {
        quote! {
            #[repr(C)]
            #[doc(hidden)]
            #[expect(non_snake_case)]
            #vis union #ident #impl_generics
            where
                #view_bounds
                #predicates
            {
                #fields_code
            }
        }
    } else {
        match style {
            darling::ast::Style::Tuple => quote! {
                #[repr(C)]
                #[doc(hidden)]
                #vis struct #ident #impl_generics (#fields_code)
                where
                    #view_bounds
                    #predicates;
            },
            darling::ast::Style::Struct => quote! {
                #[repr(C)]
                #[doc(hidden)]
                #vis struct #ident #impl_generics
                where
                    #view_bounds
                    #predicates
                {
                    #fields_code
                }
            },
            darling::ast::Style::Unit => unreachable!(),
        }
    };

    quote! {
        #item

        co3::reprC! {
            unsafe impl(#params) #robust_kind for #ident #ty_generics where (
                #view_bounds
                #predicates
            ) {}
        }
    }
}

fn gen_data_enum_payload(
    enum_name: &syn::Ident,
    vis: &syn::Visibility,
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

    let field_names = variants
        .iter()
        .map(|variant| &variant.ident)
        .collect::<Vec<_>>();
    let field_tys = variants
        .iter()
        .map(|variant| {
            variant_mapper(
                variant,
                || quote! {()},
                |field| {
                    let field_ty = &field.ty;
                    quote! {<#field_ty as co3::transmute::FlatTransmute>::Target}
                },
            )
        })
        .collect::<Vec<_>>();
    let field_const_tys = field_tys.iter().map(|field_ty| {
        quote! {
            <#field_ty as co3::borrow::BorrowCast>::AsConst
        }
    });
    let field_mut_tys = field_tys.iter().map(|field_ty| {
        quote! {
            <#field_ty as co3::borrow::BorrowCast>::AsMut
        }
    });

    let payload = gen_repr_c_type::<true>(
        format!(" Payload of [`{}`]", gen_repr_c_item_name(enum_name)),
        payload_name.clone(),
        vis,
        generics,
        darling::ast::Style::Struct,
        quote! { #(#field_names: #field_tys),* },
        quote! { #(#field_names: #field_const_tys),* },
        quote! { #(#field_names: #field_mut_tys),* },
        &field_tys,
        &field_types,
        ReprFamily::ReprC,
    );

    (payload_name, payload)
}

fn gen_transparent_impl<'a, const ADD_COPY: bool>(
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

    let fields = fields.into_iter().map(|f| &f.ty).collect::<Vec<_>>();
    let flat_transmute_bounds = gen_flat_transmute_bounds::<ADD_COPY>(&fields, generics);

    quote! {
        impl #impl_generics co3::ir::ReprFamily for #item_name #ty_generics #where_clause {
            // FIXME:
            //type Kind = #(<#fields as co3::ir::ReprFamily>::Kind)+*;
            type Kind = co3::ir::Transmuted;
        }

        impl #impl_generics co3::ir::EncodeReprFamily for #item_name #ty_generics #where_clause {
            type Kind = #(<#fields as co3::ir::EncodeReprFamily>::Kind)+*;
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

        unsafe impl #impl_generics co3::handle::Erase for #item_name #ty_generics where
            #flat_transmute_bounds
            #predicates
        {
            // FIXME: The type is not correct, likewise in no_repr. Also fix bounds
            type Erased = <#target #ty_generics as co3::handle::Erase>::Erased;
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

fn gen_data_enum_payload_name(enum_name: &syn::Ident) -> syn::Ident {
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

fn gen_flat_transmute_bounds<const ADD_COPY: bool>(
    fields: &[&syn::Type],
    generics: &syn::Generics,
) -> TokenStream {
    let flat_transmute_bound = quote! { co3::transmute::FlatTransmute };

    let parameterized_field_types = fields
        .iter()
        .filter(|&ty| is_type_parameterized(ty, generics));

    if ADD_COPY {
        quote! { #( #parameterized_field_types: #flat_transmute_bound<Target: Copy>,)* }
    } else {
        quote! { #( #parameterized_field_types: #flat_transmute_bound,)* }
    }
}

pub(super) fn gen_extern_c_bounds<const ADD_COPY: bool>(
    fields: &[&syn::Type],
    generics: &syn::Generics,
) -> TokenStream {
    let extern_c_bound = quote! { co3::ExternC };

    let parameterized_field_types = fields
        .iter()
        .filter(|&ty| is_type_parameterized(ty, generics));

    if ADD_COPY {
        quote! { #( #parameterized_field_types: #extern_c_bound<CType: Copy>,)* }
    } else {
        quote! { #( #parameterized_field_types: #extern_c_bound,)* }
    }
}

pub(super) fn gen_field_lowering_bounds<const ADD_COPY: bool>(
    fields: &[&syn::Type],
    generics: &syn::Generics,
    lowering: ReprFamily,
) -> TokenStream {
    match lowering {
        // FIXME: I don't think flat bounds for enums shouldn't have Copy bound
        ReprFamily::NoRepr => gen_extern_c_bounds::<ADD_COPY>(fields, generics),
        ReprFamily::ReprC => gen_flat_transmute_bounds::<ADD_COPY>(fields, generics),
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
