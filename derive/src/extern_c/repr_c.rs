use darling::util::SpannedValue;
use manyhow::emit;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, visit::Visit};

use crate::{
    attr_parse::repr::ReprPrimitive,
    emitter::Emitter,
    extern_c::{FfiTypeField, FfiTypeKindFieldAttribute, FfiTypeVariant, no_repr::variant_mapper},
};

/// Visitor to check if a type contains any of the specified type parameters
struct TypeParamVisitor<'a> {
    type_params: &'a [&'a syn::Ident],
    is_generic: bool,
}

impl<'ast> Visit<'ast> for TypeParamVisitor<'_> {
    fn visit_type_path(&mut self, type_path: &'ast syn::TypePath) {
        if let Some(ident) = type_path.path.get_ident()
            && self.type_params.contains(&ident)
        {
            self.is_generic = true;
        }

        syn::visit::visit_type_path(self, type_path);
    }
}

/// Check if a type contains any of the type parameters from generics
fn is_type_parameterized(ty: &syn::Type, generics: &syn::Generics) -> bool {
    let type_param_idents: Vec<_> = generics.type_params().map(|tp| &tp.ident).collect();

    let mut visitor = TypeParamVisitor {
        type_params: &type_param_idents,
        is_generic: false,
    };
    visitor.visit_type(ty);
    visitor.is_generic
}

pub(super) fn derive_repr_c_struct(
    emitter: &mut Emitter,
    struct_name: &Ident,
    generics: &syn::Generics,
    fields: &darling::ast::Fields<FfiTypeField>,
) -> TokenStream {
    let (repr_c_struct_name, repr_c_struct) =
        gen_repr_c_struct(emitter, struct_name, generics, fields);

    let is_valid = match fields.style {
        darling::ast::Style::Unit => unreachable!(),
        darling::ast::Style::Tuple => {
            let validations = fields.iter().enumerate().map(|(i, field)| {
                let field_name =
                    syn::Ident::new(&format!("_{}", i), proc_macro2::Span::call_site());
                let field_ty = &field.ty;

                quote! {
                    <#field_ty as co3::transmute::FlatTransmute>::is_valid(&target.#field_name)
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
    };

    let transparent_impl = gen_transparent_impl(
        struct_name,
        generics,
        quote! { #repr_c_struct_name },
        is_valid,
        fields.iter().map(|f| &f.ty),
    );

    quote! {
        #repr_c_struct
        #transparent_impl
    }
}

pub(super) fn derive_repr_c_data_enum(
    emitter: &mut Emitter,
    repr: ReprPrimitive,
    enum_name: &Ident,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> TokenStream {
    let (repr_c_enum_name, repr_c_enum) =
        gen_repr_c_data_enum(emitter, enum_name, generics, repr, variants);

    let is_valid = {
        let mut match_arms = Vec::new();

        for (idx, variant) in variants.iter().enumerate() {
            let variant_name = &variant.ident;

            let validation = variant_mapper(
                emitter,
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

    let field_types = variants
        .iter()
        .flat_map(|variant| variant.fields.iter().map(|f| &f.ty));

    let transparent_impl = gen_transparent_impl(
        enum_name,
        generics,
        quote! { #repr_c_enum_name },
        is_valid,
        field_types,
    );

    quote! {
        #repr_c_enum
        #transparent_impl
    }
}

pub(super) fn derive_data_enum(
    emitter: &mut Emitter,
    repr: ReprPrimitive,
    enum_name: &Ident,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> TokenStream {
    verify_variants_non_owning(emitter, variants);

    let (union_name, union_and_helpers) =
        gen_data_enum(emitter, enum_name, generics, repr, variants);

    let is_valid = {
        let mut match_arms = Vec::new();

        for (idx, variant) in variants.iter().enumerate() {
            let variant_name = &variant.ident;

            let validation = variant_mapper(
                emitter,
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

    let field_types = variants
        .iter()
        .flat_map(|variant| variant.fields.iter().map(|f| &f.ty));

    let transparent_impl = gen_transparent_impl(
        enum_name,
        generics,
        quote! { #union_name },
        is_valid,
        field_types,
    );

    quote! {
        #union_and_helpers
        #transparent_impl
    }
}

pub(crate) fn derive_fieldless_enum(
    emitter: &mut Emitter,
    repr: ReprPrimitive,
    enum_name: &Ident,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> TokenStream {
    for variant in variants {
        if variant.discriminant.is_some() {
            emit!(
                emitter,
                variant.span(),
                // TODO: Support explicit discriminants later on
                "Fieldless enums with explicit discriminants are prohibited"
            );
        }
    }

    let (repr_c_impl, is_valid, niche_value) = if is_exhaustive_enum(variants.len(), repr) {
        let repr_c_impl = quote! { unsafe impl co3::ReprC for #enum_name {} };
        (Some(repr_c_impl), quote! {true}, None)
    } else {
        let niche_value = proc_macro2::Literal::usize_unsuffixed(variants.len());

        let is_valid = match repr {
            ReprPrimitive::U8 | ReprPrimitive::U16 | ReprPrimitive::U32 | ReprPrimitive::U64 => {
                quote! { (*target as usize) < #niche_value }
            }
            ReprPrimitive::I8 | ReprPrimitive::I16 | ReprPrimitive::I32 | ReprPrimitive::I64 => {
                quote! { *target >= 0 && (*target as usize) < #niche_value }
            }
        };

        (None, is_valid, Some(niche_value))
    };

    quote! {
        #repr_c_impl

        co3::mineral! {
            unsafe impl Transparent for #enum_name {
                type Target = #repr;

                const NICHE_VALUE: Self::CType = #niche_value;
                fn is_valid(target: &Self::Target) -> bool {
                    #is_valid
                }
            }
        }
    }
}

fn gen_transparent_impl<'a>(
    item_name: &Ident,
    generics: &syn::Generics,
    target_type: TokenStream,
    is_valid_body: TokenStream,
    field_types: impl IntoIterator<Item = &'a syn::Type>,
) -> TokenStream {
    let (_, ty_generics, _) = generics.split_for_impl();
    let params = &generics.params;
    let predicates = generics
        .where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let parameterized_field_types = field_types
        .into_iter()
        .filter(|ty| is_type_parameterized(ty, generics));
    let flat_transmute_bounds =
        quote! { #(#parameterized_field_types: co3::transmute::FlatTransmute,)* };

    quote! {
        co3::mineral! {
            unsafe impl(#params) Transparent for #item_name #ty_generics where (#flat_transmute_bounds #predicates) {
                type Target = #target_type #ty_generics;

                fn is_valid(target: &Self::Target) -> bool {
                    #is_valid_body
                }
            }
        }
    }
}

/// Checks if an enum exhausts all possible values of its repr type
fn is_exhaustive_enum(num_variants: usize, repr: ReprPrimitive) -> bool {
    let max_values = match repr {
        ReprPrimitive::U8 | ReprPrimitive::I8 => 1u64 << 8,
        ReprPrimitive::U16 | ReprPrimitive::I16 => 1u64 << 16,
        ReprPrimitive::U32 | ReprPrimitive::I32 => 1u64 << 32,
        ReprPrimitive::U64 | ReprPrimitive::I64 => return false, // Can't have 2^64 variants
    };

    num_variants as u64 == max_values
}

fn gen_repr_c_type<const IS_STRUCT: bool, const IS_PUBLIC: bool>(
    doc: String,
    ident: syn::Ident,
    generics: &syn::Generics,
    fields_code: TokenStream,
    field_types: &[&syn::Type],
) -> TokenStream {
    let visibility = IS_PUBLIC.then_some(quote! {pub});

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let params = &generics.params;
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let parameterized_field_types: Vec<_> = field_types
        .iter()
        .filter(|ty| is_type_parameterized(ty, generics))
        .collect();

    let extern_c_bounds = quote! { #(#parameterized_field_types: co3::ExternC,)* };

    let kind = if IS_STRUCT {
        quote! { struct }
    } else {
        quote! { union }
    };

    quote! {
        #[repr(C)]
        #[doc = #doc]
        #[doc(hidden)]
        #[expect(non_camel_case_types)]
        #[allow(non_snake_case)]
        #visibility #kind #ident #impl_generics where #extern_c_bounds #predicates {
            #fields_code
        }

        impl #impl_generics Clone for #ident #ty_generics where #extern_c_bounds #predicates {
            fn clone(&self) -> Self { *self }
        }

        impl #impl_generics Copy for #ident #ty_generics where #extern_c_bounds #predicates {}
        unsafe impl #impl_generics co3::ReprC for #ident #ty_generics where #extern_c_bounds #predicates {}
        co3::mineral! { impl(#params) Robust for #ident #ty_generics where (#extern_c_bounds #predicates) {} }
    }
}

pub(super) fn gen_repr_c_struct(
    emitter: &mut Emitter,
    struct_name: &syn::Ident,
    generics: &syn::Generics,
    fields: &darling::ast::Fields<FfiTypeField>,
) -> (syn::Ident, TokenStream) {
    let repr_c_struct_name = gen_repr_c_item_name(struct_name);

    verify_fields_non_owning(emitter, fields);

    // Collect field types for bound generation
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
            let field_names = (0..fields.len())
                .map(|i| syn::Ident::new(&format!("_{}", i), proc_macro2::Span::call_site()));
            let field_tys = fields.iter().map(|field| {
                let field_ty = &field.ty;
                quote! {<#field_ty as co3::ExternC>::CType}
            });

            quote! { #(#field_names: #field_tys),* }
        }
        darling::ast::Style::Unit => unreachable!(),
    };

    let repr_c_struct = gen_repr_c_type::<true, true>(
        format!(" [`ReprC`] equivalent of [`{struct_name}`]"),
        repr_c_struct_name.clone(),
        generics,
        fields_code,
        &field_types,
    );

    (repr_c_struct_name, repr_c_struct)
}

pub(super) fn gen_repr_c_data_enum(
    emitter: &mut Emitter,
    enum_name: &syn::Ident,
    generics: &syn::Generics,
    repr: ReprPrimitive,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> (syn::Ident, TokenStream) {
    verify_variants_non_owning(emitter, variants);

    let (payload_name, payload) = gen_data_enum_payload(emitter, enum_name, generics, variants);

    let doc = format!(" [`ReprC`] equivalent of [`{enum_name}`]");
    let repr_c_enum_name = gen_repr_c_item_name(enum_name);

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let params = &generics.params;
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    // Collect all field types from variants for bound generation
    let mut field_types = Vec::new();
    for variant in variants {
        for field in variant.fields.iter() {
            field_types.push(&field.ty);
        }
    }
    let parameterized_field_types: Vec<_> = field_types
        .iter()
        .filter(|ty| is_type_parameterized(ty, generics))
        .collect();
    let extern_c_bounds = quote! { #(#parameterized_field_types: co3::ExternC,)* };

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
        unsafe impl #impl_generics co3::ReprC for #repr_c_enum_name #ty_generics where #extern_c_bounds #predicates {}
        co3::mineral! { impl(#params) Robust for #repr_c_enum_name #ty_generics where (#extern_c_bounds #predicates) {} }
    };

    (repr_c_enum_name, repr_c_enum)
}

fn gen_data_enum_payload(
    emitter: &mut Emitter,
    enum_name: &syn::Ident,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> (syn::Ident, TokenStream) {
    let payload_name = gen_data_enum_payload_name(enum_name);
    let repr_c_enum_name = gen_repr_c_item_name(enum_name);

    // Collect all field types from variants for bound generation
    let mut field_types = Vec::new();
    for variant in variants {
        for field in variant.fields.iter() {
            field_types.push(&field.ty);
        }
    }

    let field_names = variants.iter().map(|variant| &variant.ident);
    let field_tys = variants.iter().map(|variant| {
        variant_mapper(
            emitter,
            variant,
            || quote! {()},
            |field| {
                let field_ty = &field.ty;
                quote! {<#field_ty as co3::ExternC>::CType}
            },
        )
    });

    let payload = gen_repr_c_type::<false, false>(
        format!(" Payload of [`{repr_c_enum_name}`]"),
        payload_name.clone(),
        generics,
        quote! { #(#field_names: #field_tys),* },
        &field_types,
    );

    (payload_name, payload)
}

// NOTE: Except for the raw pointers there should be no other type
// that is at the same time Robust and also transfers ownership
/// Verifies each field's pointer types are marked as non-owning
fn verify_field_non_owning(emitter: &mut Emitter, field: &FfiTypeField) {
    use syn::visit::Visit;

    if field.ffi_type_attr.kind == Some(FfiTypeKindFieldAttribute::UnsafeNonOwning) {
        return;
    }

    struct PtrVisitor<'a> {
        emitter: &'a mut Emitter,
    }
    impl Visit<'_> for PtrVisitor<'_> {
        fn visit_type_ptr(&mut self, node: &syn::TypePtr) {
            emit!(
                self.emitter,
                node,
                "Raw pointer found. If the pointer doesn't own the data, attach `#[mineral(unsafe(non_owning))` to the field. Otherwise, mark the entire type as opaque with `#[mineral(opaque)]`"
            );
        }
    }

    let mut ptr_visitor = PtrVisitor { emitter };
    ptr_visitor.visit_type(&field.ty);
}

/// Verifies each field in a struct's fields are non-owning
fn verify_fields_non_owning(emitter: &mut Emitter, fields: &darling::ast::Fields<FfiTypeField>) {
    for field in fields.iter() {
        verify_field_non_owning(emitter, field);
    }
}

/// Verifies each field in enum variants are non-owning
fn verify_variants_non_owning(emitter: &mut Emitter, variants: &[SpannedValue<FfiTypeVariant>]) {
    for variant in variants {
        for field in variant.fields.iter() {
            verify_field_non_owning(emitter, field);
        }
    }
}

pub(super) fn gen_repr_c_item_name(item_name: &syn::Ident) -> syn::Ident {
    syn::Ident::new(
        &format!("__co3__ReprC{item_name}"),
        proc_macro2::Span::call_site(),
    )
}

pub(super) fn gen_data_enum_payload_name(enum_name: &syn::Ident) -> syn::Ident {
    syn::Ident::new(
        &format!("__co3__{enum_name}Payload"),
        proc_macro2::Span::call_site(),
    )
}

fn gen_data_enum_variant_name(enum_name: &syn::Ident, variant_name: &syn::Ident) -> syn::Ident {
    syn::Ident::new(
        &format!("__co3__{enum_name}Variant{variant_name}"),
        proc_macro2::Span::call_site(),
    )
}

fn gen_data_enum(
    emitter: &mut Emitter,
    enum_name: &syn::Ident,
    generics: &syn::Generics,
    repr: ReprPrimitive,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> (syn::Ident, TokenStream) {
    let union_name = gen_repr_c_item_name(enum_name);

    // Collect all field types from all variants for bound generation
    let mut all_field_types = Vec::new();
    for variant in variants {
        for field in variant.fields.iter() {
            all_field_types.push(&field.ty);
        }
    }

    let variant_structs: Vec<_> = variants
        .iter()
        .map(|variant| {
            let variant_name = &variant.ident;
            let variant_struct_name = gen_data_enum_variant_name(enum_name, variant_name);

            // Collect field types for this variant
            let variant_field_types: Vec<_> = variant.fields.iter().map(|f| &f.ty).collect();

            let fields = variant_mapper(
                emitter,
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

            gen_repr_c_type::<true, true>(
                format!(" Variant struct for [`{enum_name}::{variant_name}`]"),
                variant_struct_name,
                generics,
                fields,
                &variant_field_types,
            )
        })
        .collect();

    // Generate the union
    let union_variant_names = variants.iter().map(|v| &v.ident);
    let union_variant_struct_names = variants
        .iter()
        .map(|v| gen_data_enum_variant_name(enum_name, &v.ident));

    let (_, ty_generics, _) = generics.split_for_impl();

    let union_def = gen_repr_c_type::<false, true>(
        format!(" [`ReprC`] equivalent of [`{enum_name}`]"),
        union_name.clone(),
        generics,
        quote! { #(#union_variant_names: #union_variant_struct_names #ty_generics),* },
        &all_field_types,
    );

    let all_code = quote! {
        #(#variant_structs)*
        #union_def
    };

    (union_name, all_code)
}
