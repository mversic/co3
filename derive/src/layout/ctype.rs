use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_quote, visit::Visit};

use crate::layout::{
    attr::ReprKind,
    infer_repr, is_parametrized, is_phantom_data, is_transparent_enum_repr, primitive_tag_type,
    wide::{
        gen_alloc_methods, gen_data_struct_name, gen_dst_methods, gen_identity_header_impl,
        header_repr_c_predicate, last_field, wide_predicate, wide_usize_predicate,
    },
};
use crate::utils::{co3_path, speculative_marker};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SyntacticSizedness {
    Sized,
    Unsized,
    Unknown,
}

pub(super) fn syntactic_sizedness(ty: &syn::Type, generics: &syn::Generics) -> SyntacticSizedness {
    use SyntacticSizedness::*;

    match ty {
        syn::Type::Slice(_) | syn::Type::TraitObject(_) => Unsized,
        syn::Type::Reference(_)
        | syn::Type::Ptr(_)
        | syn::Type::Array(_)
        | syn::Type::Tuple(_)
        | syn::Type::Never(_) => Sized,
        syn::Type::Paren(ty) => syntactic_sizedness(&ty.elem, generics),
        syn::Type::Group(ty) => syntactic_sizedness(&ty.elem, generics),
        syn::Type::Path(path) => {
            let Some(last) = path.path.segments.last() else {
                return Unknown;
            };
            if last.ident == "CStr" || last.ident == "str" {
                return Unsized;
            }
            if path.qself.is_some() {
                return Unknown;
            }
            if crate::ffi_fn::option_inner_type(ty).is_some() {
                return Sized;
            }
            if path.path.segments.len() == 1 {
                if let Some(param) = generics
                    .type_params()
                    .find(|param| param.ident == last.ident)
                {
                    return if param.bounds.iter().any(|bound| {
                        matches!(bound, syn::TypeParamBound::Trait(bound)
                            if bound.maybe.is_some() && bound.path.is_ident("Sized"))
                    }) {
                        Unknown
                    } else {
                        Sized
                    };
                }
                return if crate::ffi_fn::copy_primitive_type(path).is_some() {
                    Sized
                } else {
                    Unknown
                };
            }
            if crate::ffi_fn::copy_primitive_type(path).is_some() {
                Sized
            } else {
                Unknown
            }
        }
        _ => Unknown,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum WideTail {
    Yes,
    No,
    Unknown,
}

pub(super) fn wide_tail(ty: &syn::Type, generics: &syn::Generics) -> WideTail {
    match ty {
        syn::Type::Slice(_) => WideTail::Yes,
        syn::Type::Path(path)
            if path
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "str") =>
        {
            WideTail::Yes
        }
        syn::Type::Path(path)
            if path
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "CStr") =>
        {
            WideTail::No
        }
        _ => match syntactic_sizedness(ty, generics) {
            SyntacticSizedness::Sized => WideTail::No,
            SyntacticSizedness::Unsized | SyntacticSizedness::Unknown => WideTail::Unknown,
        },
    }
}

pub(super) fn c_fn_last_field_bounds(
    fields: &[&syn::Type],
    generics: &syn::Generics,
) -> (TokenStream, bool) {
    let Some(last) = fields.last() else {
        return (quote! {}, false);
    };

    match syntactic_sizedness(last, generics) {
        SyntacticSizedness::Sized => (quote! {}, false),
        SyntacticSizedness::Unsized => (quote! {}, true),
        SyntacticSizedness::Unknown => {
            let speculative = speculative_marker(!is_parametrized(last, generics));
            (quote!(#speculative #last: Sized,), false)
        }
    }
}

fn lowered_field_ty(field_ty: &syn::Type) -> TokenStream {
    if is_phantom_data(field_ty) {
        return quote!(#field_ty);
    }

    quote!(<#field_ty as co3::ReprC>::CType)
}

/// Generates the wrapper carrier for a fieldless enum. Exhaustive enums
/// without custom Drop use the tag itself as their CType instead.
pub(super) fn gen_fieldless_enum_ctype(
    tag_type: &syn::Type,
    alignment: Option<&syn::LitInt>,
    vis: &syn::Visibility,
    name: &syn::Ident,
    generics: &syn::Generics,
) -> TokenStream {
    let ctype_name = gen_ctype_name(name);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);
    // `repr(transparent)` cannot be combined with `align`, so use the
    // equivalent single-field C layout when an explicit alignment is present.
    let repr_kind = if alignment.is_none() {
        &ReprKind::Transparent
    } else {
        &ReprKind::C(None)
    };
    let repr = gen_ctype_repr_attr(Some(repr_kind), alignment);
    let ctype: syn::ItemStruct = parse_quote! {
        #[doc(hidden)]
        #repr
        #vis struct #ctype_name #impl_generics(#tag_type)
        where
            #predicates;
    };
    // Keep the normal non-ZST CFnArg size guard. In particular, a one-variant
    // fieldless enum is represented by `CEnum(())` and must not become a CFnArg.
    let impls = gen_struct_ctype_impls::<true>(&ctype, false, alignment, None, false);
    let fields = ctype
        .fields
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();
    let type_spec_impl = gen_type_spec(&ctype.ident, &ctype.generics, &fields, alignment);
    let borrow_cast_impl = gen_identity_borrow_cast_impl(&ctype.ident, &ctype.generics);
    let primitive_constructor = match tag_type {
        syn::Type::Path(path) => {
            let primitive = path.path.get_ident().expect("primitive enum tag type");
            let method = format_ident!("from_{primitive}");
            quote! {
                /// Construct a carrier from its primitive discriminant.
                /// Invalid enum tags remain valid carrier values and are rejected by decoding.
                pub const fn #method(tag: #tag_type) -> Self { Self(tag) }
            }
        }
        syn::Type::Tuple(tuple) if tuple.elems.is_empty() => quote! {},
        _ => unreachable!("fieldless enum tag is a primitive or unit type"),
    };

    quote! {
        #ctype
        #impls
        #type_spec_impl
        #borrow_cast_impl
        impl #impl_generics #ctype_name #ty_generics #where_clause {
            #primitive_constructor

            /// Return the raw discriminant.
            pub const fn tag(&self) -> #tag_type { self.0 }
        }
    }
}

pub(super) fn gen_item_ctype(
    repr: Option<&ReprKind>,
    alignment: Option<&syn::LitInt>,
    input: &syn::DeriveInput,
    generate_views_and_spec: bool,
) -> TokenStream {
    let vis = &input.vis;
    let name = &input.ident;
    let generics = &input.generics;
    match &input.data {
        syn::Data::Struct(data) => derive_ctype_struct(
            repr,
            alignment,
            vis,
            name,
            generics,
            &data.fields,
            generate_views_and_spec,
            generate_views_and_spec,
        ),
        syn::Data::Enum(data) if is_transparent_enum_repr(repr, &data.variants) => {
            let Some(first_variant) = data.variants.first() else {
                return quote! {};
            };
            let carrier = derive_ctype_struct(
                repr,
                alignment,
                vis,
                name,
                generics,
                &first_variant.fields,
                generate_views_and_spec,
                false,
            );
            let constructor = gen_transparent_enum_constructor(name, generics, first_variant);
            quote! { #carrier #constructor }
        }
        syn::Data::Enum(data) if repr.is_none() => {
            let tag_type = infer_repr(data.variants.len());
            derive_data_enum_ctype(tag_type, alignment, vis, name, generics, &data.variants)
        }
        syn::Data::Enum(data) if let Some(ReprKind::Primitive(repr)) = repr => {
            derive_data_enum_ctype(
                (**repr).clone(),
                alignment,
                vis,
                name,
                generics,
                &data.variants,
            )
        }
        syn::Data::Enum(data) if matches!(repr, Some(&ReprKind::C(Some(_)))) => {
            let tag_type = primitive_tag_type(repr.expect("C primitive representation")).clone();
            derive_repr_c_data_enum_ctype(tag_type, alignment, vis, name, generics, &data.variants)
        }
        syn::Data::Union(_) | syn::Data::Enum(_) => {
            unreachable!()
        }
    }
}

fn gen_transparent_enum_constructor(
    enum_name: &syn::Ident,
    generics: &syn::Generics,
    variant: &syn::Variant,
) -> TokenStream {
    let ctype_name = gen_ctype_name(enum_name);
    let variant_name = &variant.ident;
    let field_types = variant
        .fields
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();
    let mut bounds = gen_extern_c_bounds_for_ctype(generics, &field_types);
    if let Some(last) = field_types.last().filter(|ty| !is_phantom_data(ty)) {
        let speculative = speculative_marker(!is_parametrized(last, generics));
        bounds.push(parse_quote!(#speculative #last: co3::ReprC<CType: Sized>));
    }
    let mut impl_generics_with_bounds = generics.clone();
    impl_generics_with_bounds
        .make_where_clause()
        .predicates
        .extend(bounds);
    let (impl_generics, ty_generics, where_clause) = impl_generics_with_bounds.split_for_impl();
    let args = variant
        .fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let name = field
                .ident
                .clone()
                .unwrap_or_else(|| format_ident!("field_{index}"));
            (name, lowered_field_ty(&field.ty))
        })
        .collect::<Vec<_>>();
    let arg_names = args.iter().map(|(name, _)| name).collect::<Vec<_>>();
    let arg_types = args.iter().map(|(_, ty)| ty).collect::<Vec<_>>();
    let body = match &variant.fields {
        syn::Fields::Named(_) => quote!(Self { #(#arg_names),* }),
        syn::Fields::Unnamed(_) => quote!(Self(#(#arg_names),*)),
        syn::Fields::Unit => quote!(Self),
    };

    quote! {
        impl #impl_generics #ctype_name #ty_generics #where_clause {
            /// This single-variant representation has no stored discriminant.
            pub const fn tag(&self) {}

            #[expect(non_snake_case)]
            pub fn #variant_name(#(#arg_names: #arg_types),*) -> Self {
                #body
            }
        }
    }
}

fn derive_data_enum_ctype(
    tag_type: syn::Type,
    alignment: Option<&syn::LitInt>,
    vis: &syn::Visibility,
    name: &syn::Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> TokenStream {
    let union_name = gen_ctype_name(name);
    let (union_def, variant_structs) = gen_data_enum_union(
        Some(tag_type.clone()),
        alignment,
        vis,
        &union_name,
        name,
        generics,
        variants,
    );
    let union_impls = gen_union_ctype_impls(&union_def, alignment, Some(name));
    let partial_eq_impl = gen_tagged_union_partial_eq(&union_def, &tag_type, variants);
    let constructors = gen_enum_variant_constructors(
        &union_def.ident,
        &union_def.generics,
        name,
        variants,
        Some(&tag_type),
        None,
    );
    quote! { #(#variant_structs)* #union_def #union_impls #partial_eq_impl #constructors }
}

fn gen_enum_variant_constructors(
    ctype_name: &syn::Ident,
    generics: &syn::Generics,
    enum_name: &syn::Ident,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
    variant_tag: Option<&syn::Type>,
    payload_name: Option<&syn::Ident>,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let tag_type = variant_tag.expect("data enum has a tag type");
    let tag_accessor = if payload_name.is_some() {
        quote! {
            /// Return the raw discriminant.
            pub const fn tag(&self) -> #tag_type { self.tag }
        }
    } else {
        quote! {
            /// Return the raw discriminant.
            pub fn tag(&self) -> #tag_type {
                unsafe { *core::ptr::from_ref(self).cast::<#tag_type>() }
            }
        }
    };
    let tag_values = super::enum_variant_tag_exprs(variants, tag_type);
    let methods = variants.iter().enumerate().map(|(index, variant)| {
        let variant_name = &variant.ident;
        let variant_struct_name = gen_variant_struct_name(enum_name, variant_name);
        let tag = &tag_values[index];
        let args = variant
            .fields
            .iter()
            .enumerate()
            .map(|(index, field)| {
                let name = field
                    .ident
                    .clone()
                    .unwrap_or_else(|| format_ident!("field_{index}"));
                (name, lowered_field_ty(&field.ty))
            })
            .collect::<Vec<_>>();
        let arg_names = args.iter().map(|(name, _)| name).collect::<Vec<_>>();
        let arg_types = args.iter().map(|(_, ty)| ty).collect::<Vec<_>>();
        let tag_value = if payload_name.is_none() {
            variant_tag.map(|tag_type| quote!(#tag as #tag_type))
        } else {
            None
        };
        let body = match (&variant.fields, tag_value.as_ref()) {
            (syn::Fields::Named(_), Some(tag)) => {
                quote!(#variant_struct_name { tag: #tag, #(#arg_names),* })
            }
            (syn::Fields::Unnamed(_), Some(tag)) => {
                quote!(#variant_struct_name(#tag, #(#arg_names),*))
            }
            (syn::Fields::Unit, Some(tag)) => quote!(#variant_struct_name { tag: #tag }),
            (syn::Fields::Named(_), None) => quote!(#variant_struct_name { #(#arg_names),* }),
            (syn::Fields::Unnamed(_), None) => quote!(#variant_struct_name(#(#arg_names),*)),
            (syn::Fields::Unit, None) => quote!(#variant_struct_name {}),
        };
        let result = if let Some(payload_name) = payload_name {
            let tag_type = variant_tag.expect("tagged payload has a tag type");
            quote!(Self {
                tag: #tag as #tag_type,
                payload: #payload_name { #variant_name: #body },
            })
        } else {
            quote!(Self { #variant_name: #body })
        };

        quote! {
            #[expect(non_snake_case)]
            pub fn #variant_name(#(#arg_names: #arg_types),*) -> Self {
                #result
            }
        }
    });

    quote! {
        impl #impl_generics #ctype_name #ty_generics #where_clause {
            #tag_accessor
            #(#methods)*
        }
    }
}

fn gen_tagged_union_partial_eq(
    union: &syn::ItemUnion,
    tag_type: &syn::Type,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> TokenStream {
    let name = &union.ident;
    let (impl_generics, ty_generics, where_clause) = union.generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|clause| &clause.predicates);
    let bounds = union.fields.named.iter().flat_map(|field| {
        gen_conditional_trait_bounds(&field.ty, &union.generics, quote!(core::cmp::PartialEq))
    });
    let tag_values = super::enum_variant_tag_exprs(variants, tag_type);
    let arms = variants.iter().enumerate().map(|(index, variant)| {
        let tag = &tag_values[index];
        let member = &variant.ident;
        quote!(value if value == #tag => unsafe { self.#member == other.#member })
    });

    quote! {
        impl #impl_generics core::cmp::PartialEq for #name #ty_generics
        where
            #(#bounds,)*
            #predicates
        {
            fn eq(&self, other: &Self) -> bool {
                let self_tag = unsafe { *core::ptr::from_ref(self).cast::<#tag_type>() };
                let other_tag = unsafe { *core::ptr::from_ref(other).cast::<#tag_type>() };
                self_tag == other_tag && match self_tag {
                    #(#arms,)*
                    _ => true,
                }
            }
        }
    }
}

#[expect(clippy::too_many_arguments)]
fn derive_ctype_struct(
    repr: Option<&ReprKind>,
    alignment: Option<&syn::LitInt>,
    vis: &syn::Visibility,
    name: &syn::Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
    generate_views_and_spec: bool,
    generate_wide: bool,
) -> TokenStream {
    let ctype_def = gen_ctype_struct_item(repr, alignment, vis, name, generics, fields);
    let known_unsized_tail = fields.iter().next_back().is_some_and(|field| {
        syntactic_sizedness(&field.ty, generics) == SyntacticSizedness::Unsized
    });
    let fields_methods = gen_fields_methods(
        &ctype_def.ident,
        &ctype_def.generics,
        &ctype_def.fields,
        known_unsized_tail,
    );
    let ctype_impls = gen_struct_ctype_impls::<true>(
        &ctype_def,
        generate_views_and_spec,
        alignment,
        Some(name),
        known_unsized_tail,
    );
    let wide_impl = generate_wide.then(|| gen_ctype_wide_impl(repr, &ctype_def, fields, generics));
    let header_impl = (!generate_views_and_spec)
        .then(|| gen_identity_header_impl(&ctype_def.ident, &ctype_def.generics));

    quote! {
        #ctype_def
        #fields_methods
        #ctype_impls
        #wide_impl
        #header_impl
    }
}

pub(super) fn gen_fields_methods(
    name: &syn::Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
    known_unsized_tail: bool,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let args = fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let arg = field
                .ident
                .clone()
                .unwrap_or_else(|| format_ident!("field_{index}"));
            (arg, &field.ty)
        })
        .collect::<Vec<_>>();
    let arg_names = args.iter().map(|(name, _)| name).collect::<Vec<_>>();
    let arg_types = args.iter().map(|(_, ty)| ty).collect::<Vec<_>>();
    let field_types = args.iter().map(|(_, ty)| *ty).collect::<Vec<_>>();
    let (last_field_sized_bound, omit_methods) = c_fn_last_field_bounds(&field_types, generics);
    if known_unsized_tail || omit_methods {
        return quote! {};
    }
    let members = fields.members().collect::<Vec<_>>();
    let body = match fields {
        syn::Fields::Named(_) => quote!(Self { #(#arg_names),* }),
        syn::Fields::Unnamed(_) => quote!(Self(#(#arg_names),*)),
        syn::Fields::Unit => quote!(Self),
    };
    let into_body = if fields.is_empty() {
        quote! { let _this = core::mem::ManuallyDrop::new(self); }
    } else {
        quote! {
            let this = core::mem::ManuallyDrop::new(self);
            ( #(unsafe { core::ptr::read(&this.#members) },)* )
        }
    };

    quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            /// Construct this generated struct from its fields.
            ///
            /// # Safety
            ///
            /// The caller must uphold the validity, ownership, and aliasing
            /// requirements of this generated struct.
            #[allow(clippy::too_many_arguments)]
            pub unsafe fn from_fields(#(#arg_names: #arg_types),*) -> Self
            where
                #last_field_sized_bound
            {
                #body
            }

            /// Consume this generated struct and return its fields in declaration order.
            pub fn into_fields(self) -> (#(#arg_types,)*)
            where
                #last_field_sized_bound
            {
                #into_body
            }
        }
    }
}

fn gen_ctype_wide_impl(
    repr: Option<&ReprKind>,
    ctype: &syn::ItemStruct,
    source_fields: &syn::Fields,
    source_generics: &syn::Generics,
) -> TokenStream {
    let is_transparent = matches!(repr, Some(ReprKind::Transparent));

    if source_fields
        .iter()
        .next_back()
        .is_some_and(|field| is_phantom_data(&field.ty))
    {
        return quote! {};
    }

    if source_fields.len() == 1 && !is_transparent {
        return quote! {};
    }

    let Some((field, ..)) = last_field(&ctype.fields) else {
        return quote! {};
    };

    let field_ty = &field.ty;
    let name = &ctype.ident;

    let source_name = name.to_string();
    let source_name = source_name.strip_prefix('C').unwrap();
    let source_name = syn::Ident::new(source_name, name.span());

    let (impl_generics, ty_generics, where_clause) = ctype.generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let data_name = gen_data_struct_name(&source_name);
    let data_ctype_name = gen_ctype_name(&data_name);

    let ctype_wide_predicate = is_transparent.then(|| {
        let predicate = wide_predicate(field_ty, &ctype.generics);
        quote!(#predicate,)
    });
    let methods = gen_dst_methods(is_transparent, field_ty);

    let alloc_methods = gen_alloc_methods(is_transparent, field_ty);
    let usize_predicate = (!is_transparent).then(|| {
        let predicate = wide_usize_predicate(field_ty, &ctype.generics);
        quote!(#predicate,)
    });
    let Some((source_last, ..)) = last_field(source_fields) else {
        return quote! {};
    };
    let source_field_ty = &source_last.ty;

    if wide_tail(source_field_ty, source_generics) == WideTail::No {
        return quote! {};
    }

    let source_header_predicate = header_repr_c_predicate(&source_last.ty, source_generics);
    let source_tail_ctype_sized = is_implicitly_sized_type_param(source_field_ty, source_generics)
        .then(|| quote!(#source_field_ty: co3::ReprC<CType: Sized>,));
    let source_slice_tail = if let syn::Type::Slice(slice) = source_field_ty {
        let element = &slice.elem;
        Some(quote! {
            #element: co3::ReprC<CType: Sized>,
            [#element]: co3::ReprC<CType = [<#element as co3::ReprC>::CType]>,
        })
    } else {
        None
    };
    let for_dummy = speculative_marker(ctype.generics.params.is_empty());

    let data_bound = (!is_transparent).then(|| {
        quote! {
            #for_dummy #data_name #ty_generics: co3::ReprC<CType = #data_ctype_name #ty_generics>,
        }
    });
    let data_ty = if is_transparent {
        quote!(<<#source_field_ty as co3::wide::Wide>::Header as co3::ReprC>::CType)
    } else {
        quote!(<#data_name #ty_generics as co3::ReprC>::CType)
    };
    let transparent_header_predicate =
        is_transparent.then(|| quote!(#for_dummy #data_ty: co3::wide::WideHeader,));

    quote! {
        unsafe impl #impl_generics co3::wide::Wide for #name #ty_generics
        where
            #source_header_predicate,
            #source_tail_ctype_sized
            #source_slice_tail
            #data_bound
            #transparent_header_predicate
            #ctype_wide_predicate
            #usize_predicate
            #predicates
        {
            type Header = #data_ty;
            type Metadata = <#field_ty as co3::wide::Wide>::Metadata;

            #methods
            #alloc_methods
        }
    }
}

fn is_implicitly_sized_type_param(ty: &syn::Type, generics: &syn::Generics) -> bool {
    let syn::Type::Path(path) = ty else {
        return false;
    };
    let Some(ident) = path
        .qself
        .is_none()
        .then(|| path.path.get_ident())
        .flatten()
    else {
        return false;
    };
    let Some(param) = generics.type_params().find(|param| param.ident == *ident) else {
        return false;
    };
    let maybe_sized = |bound: &syn::TypeParamBound| {
        matches!(bound, syn::TypeParamBound::Trait(trait_bound)
            if trait_bound.maybe.is_some() && trait_bound.path.is_ident("Sized"))
    };
    !param.bounds.iter().any(maybe_sized)
        && !generics
            .where_clause
            .iter()
            .flat_map(|clause| &clause.predicates)
            .any(|predicate| {
                matches!(predicate, syn::WherePredicate::Type(predicate)
                if matches!(&predicate.bounded_ty, syn::Type::Path(path)
                    if path.qself.is_none() && path.path.is_ident(ident))
                    && predicate.bounds.iter().any(maybe_sized))
            })
}

fn derive_repr_c_data_enum_ctype(
    tag_type: syn::Type,
    alignment: Option<&syn::LitInt>,
    vis: &syn::Visibility,
    name: &syn::Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> TokenStream {
    let payload_name = format_ident!("{name}Payload");

    let (payload_def, variant_structs) =
        gen_data_enum_union(None, None, vis, &payload_name, name, generics, variants);

    let payload_impls = gen_union_ctype_impls(&payload_def, None, None);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let ctype_name = gen_ctype_name(name);
    let ctype_bounds = gen_union_extern_c_bounds(name, generics, variants);

    let repr = gen_ctype_repr_attr(Some(&ReprKind::C(None)), alignment);
    let ctype_def: syn::ItemStruct = parse_quote! {
        #[doc(hidden)]
        #repr
        #vis struct #ctype_name #impl_generics
        where
            #(#ctype_bounds,)*
            #predicates
        {
            tag: #tag_type,
            payload: #payload_name #ty_generics,
        }
    };

    let ctype_impls =
        gen_struct_ctype_impls::<false>(&ctype_def, true, alignment, Some(name), false);
    let fields_methods = gen_fields_methods(
        &ctype_def.ident,
        &ctype_def.generics,
        &ctype_def.fields,
        false,
    );
    let partial_eq_impl =
        gen_tagged_payload_partial_eq(&ctype_def, &payload_def, &tag_type, variants);
    let constructors = gen_enum_variant_constructors(
        &ctype_def.ident,
        &ctype_def.generics,
        name,
        variants,
        Some(&tag_type),
        Some(&payload_name),
    );

    quote! {
        #(#variant_structs)*

        #payload_def
        #payload_impls

        #ctype_def
        #fields_methods
        #ctype_impls
        #partial_eq_impl
        #constructors
    }
}

fn gen_tagged_payload_partial_eq(
    ctype: &syn::ItemStruct,
    payload: &syn::ItemUnion,
    tag_type: &syn::Type,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> TokenStream {
    let name = &ctype.ident;
    let (impl_generics, ty_generics, where_clause) = ctype.generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|clause| &clause.predicates);
    let bounds = payload.fields.named.iter().flat_map(|field| {
        gen_conditional_trait_bounds(&field.ty, &ctype.generics, quote!(core::cmp::PartialEq))
    });
    let tag_values = super::enum_variant_tag_exprs(variants, tag_type);
    let arms = variants.iter().enumerate().map(|(index, variant)| {
        let tag = &tag_values[index];
        let member = &variant.ident;
        quote!(value if value == #tag => unsafe { self.payload.#member == other.payload.#member })
    });

    quote! {
        impl #impl_generics core::cmp::PartialEq for #name #ty_generics
        where
            #(#bounds,)*
            #predicates
        {
            fn eq(&self, other: &Self) -> bool {
                self.tag == other.tag && match self.tag {
                    #(#arms,)*
                    _ => true,
                }
            }
        }
    }
}

fn gen_ctype_struct_item(
    repr: Option<&ReprKind>,
    alignment: Option<&syn::LitInt>,
    vis: &syn::Visibility,
    name: &syn::Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
) -> syn::ItemStruct {
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let ctype_name = gen_ctype_name(name);
    let repr = gen_ctype_repr_attr(repr, alignment);

    let field_types = fields.iter().map(|f| &f.ty).collect::<Vec<_>>();
    let extern_c_bounds = gen_extern_c_bounds_for_ctype(generics, &field_types);

    let field_c_tys = fields
        .iter()
        .map(|field| lowered_field_ty(&field.ty))
        .collect::<Vec<_>>();

    let ctype = match fields {
        syn::Fields::Named(_) | syn::Fields::Unit => {
            let field_names = fields.iter().map(|field| {
                field
                    .ident
                    .as_ref()
                    .expect("named CType field must have an identifier")
            });

            quote! {
                struct #ctype_name #impl_generics
                where
                    #(#extern_c_bounds,)*
                    #predicates
                {
                    #(#field_names: #field_c_tys),*
                }
            }
        }
        syn::Fields::Unnamed(_) => quote! {
            struct #ctype_name #impl_generics (#(#field_c_tys),*)
            where
                #(#extern_c_bounds,)*
                #predicates;
        },
    };

    parse_quote! {
        #[doc(hidden)]
        #repr
        #vis #ctype
    }
}

fn gen_data_enum_union(
    variant_tag: Option<syn::Type>,
    alignment: Option<&syn::LitInt>,
    vis: &syn::Visibility,
    union_name: &syn::Ident,
    enum_name: &syn::Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> (syn::ItemUnion, Vec<TokenStream>) {
    let (variant_generics, union_fields) = gen_union_fields(enum_name, generics, variants);

    let variant_structs = variants
        .iter()
        .zip(&variant_generics)
        .map(|(variant, generics)| {
            gen_variant_struct(variant_tag.as_ref(), vis, enum_name, generics, variant)
        });

    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let union_extern_c_bounds = gen_union_extern_c_bounds(enum_name, generics, variants);

    let repr = gen_ctype_repr_attr(Some(&ReprKind::C(None)), alignment);
    let union_def = parse_quote! {
        #[expect(non_snake_case)]
        #[doc(hidden)]
        #repr
        #vis union #union_name #impl_generics
        where
            #(#union_extern_c_bounds,)*
            #predicates
        #union_fields
    };

    (union_def, variant_structs.collect())
}

fn gen_variant_struct(
    tag_type: Option<&syn::Type>,
    vis: &syn::Visibility,
    enum_name: &syn::Ident,
    generics: &syn::Generics,
    variant: &syn::Variant,
) -> TokenStream {
    let name = format_ident!("{enum_name}{}", &variant.ident);

    let repr = Some(&ReprKind::C(None));
    let mut fields = variant.fields.clone();

    if let Some(tag_type) = tag_type {
        match &mut fields {
            syn::Fields::Unit => {
                fields = syn::Fields::Named(parse_quote!({ tag: #tag_type }));
            }
            syn::Fields::Named(fields) => {
                fields.named.insert(0, parse_quote!(tag: #tag_type));
            }
            syn::Fields::Unnamed(fields) => {
                fields.unnamed.insert(0, parse_quote!(#tag_type));
            }
        }
    }

    let field_types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();
    let mut variant_generics = generics.clone();
    variant_generics
        .make_where_clause()
        .predicates
        .extend(gen_sized_extern_c_bounds_for_ctype(generics, &field_types));

    let ctype = gen_ctype_struct_item(repr, None, vis, &name, &variant_generics, &fields);
    let fields_methods = gen_fields_methods(&ctype.ident, &ctype.generics, &ctype.fields, false);
    let ctype_impls = gen_struct_ctype_impls::<true>(&ctype, true, None, None, false);
    quote! { #ctype #fields_methods #ctype_impls }
}

pub(super) fn gen_union_fields(
    name: &syn::Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> (Vec<syn::Generics>, syn::FieldsNamed) {
    let filtered_generics = variants
        .iter()
        .map(|variant| {
            let variant_fields = variant.fields.iter().map(|f| &f.ty).collect::<Vec<_>>();
            filter_generics(generics, &variant_fields)
        })
        .collect::<Vec<_>>();

    let named =
        variants
            .iter()
            .zip(&filtered_generics)
            .map(|(variant, variant_generics)| -> syn::Field {
                let variant_name = &variant.ident;
                let (_, ty_generics, _) = variant_generics.split_for_impl();
                let variant_struct_name = gen_variant_struct_name(name, variant_name);
                parse_quote!(#variant_name: #variant_struct_name #ty_generics)
            });

    let fields = syn::FieldsNamed {
        brace_token: Default::default(),
        named: named.collect(),
    };

    (filtered_generics, fields)
}

pub(super) fn gen_union_extern_c_bounds(
    enum_name: &syn::Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> Vec<syn::WherePredicate> {
    let field_types = variants
        .iter()
        .flat_map(|variant| variant.fields.iter().map(|field| &field.ty))
        .collect::<Vec<_>>();
    let mut predicates = gen_sized_extern_c_bounds_for_ctype(generics, &field_types);

    let (_, union_fields) = gen_union_fields(enum_name, generics, variants);
    predicates.extend(union_fields.named.iter().map(|field| {
        let ty = &field.ty;
        parse_quote!(#ty: Copy)
    }));

    predicates
}

pub(super) fn gen_sized_extern_c_bounds_for_ctype(
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> Vec<syn::WherePredicate> {
    fields
        .iter()
        .flat_map(|&ty| {
            if is_phantom_data(ty) {
                Vec::new()
            } else if is_parametrized(ty, generics) {
                vec![parse_quote!(#ty: co3::ReprC<CType: Sized>)]
            } else {
                gen_hrtb_projection_bounds(ty, generics, quote!(co3::ReprC<CType: Sized>))
            }
        })
        .collect()
}

fn gen_struct_ctype_impls<const GEN_PARTIAL_EQ: bool>(
    ctype: &syn::ItemStruct,
    generate_views_and_spec: bool,
    alignment: Option<&syn::LitInt>,
    owner_name: Option<&syn::Ident>,
    known_unsized_tail: bool,
) -> TokenStream {
    let fields = ctype.fields.iter().map(|f| &f.ty).collect::<Vec<_>>();
    let all_phantom_data = !fields.is_empty() && fields.iter().all(|ty| is_phantom_data(ty));
    let generate_views = generate_views_and_spec && !all_phantom_data;

    let copy_impls = gen_copy_impls(&ctype.ident, &ctype.generics, &fields, known_unsized_tail);
    let partial_eq_impl = GEN_PARTIAL_EQ.then(|| gen_struct_partial_eq(ctype));
    let default_impl = gen_default_impl(&ctype.ident, &ctype.generics, &fields, known_unsized_tail);
    let robust_impls = gen_robust_impls(&ctype.ident, &ctype.generics, &fields, known_unsized_tail);
    let type_spec_impl = generate_views_and_spec
        .then(|| gen_type_spec(&ctype.ident, &ctype.generics, &fields, alignment));
    let const_view = generate_views
        .then(|| gen_ctype_struct_view(ctype.clone(), false, alignment, known_unsized_tail));
    let mut_view = generate_views
        .then(|| gen_ctype_struct_view(ctype.clone(), true, alignment, known_unsized_tail));

    let borrow_cast_impl = if generate_views {
        gen_borrow_cast_impl(&ctype.ident, &ctype.generics, &fields, owner_name, false)
    } else if all_phantom_data {
        gen_identity_borrow_cast_impl(&ctype.ident, &ctype.generics)
    } else {
        quote! {}
    };

    quote! {
        #copy_impls
        #partial_eq_impl
        #default_impl
        #robust_impls
        #type_spec_impl
        #const_view
        #mut_view

        #borrow_cast_impl
    }
}

fn gen_conditional_trait_bounds(
    ty: &syn::Type,
    generics: &syn::Generics,
    bound: TokenStream,
) -> Vec<TokenStream> {
    if is_parametrized(ty, generics) {
        vec![quote!(#ty: #bound)]
    } else {
        let speculative = speculative_marker(true);
        let mut bounds = vec![quote!(#speculative #ty: #bound)];
        bounds.extend(
            gen_hrtb_projection_bounds(ty, generics, bound)
                .into_iter()
                .map(|predicate| quote!(#predicate)),
        );
        bounds
    }
}

fn gen_struct_partial_eq(ctype: &syn::ItemStruct) -> TokenStream {
    let name = &ctype.ident;
    let (impl_generics, ty_generics, where_clause) = ctype.generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|clause| &clause.predicates);
    let bounds = ctype.fields.iter().flat_map(|field| {
        gen_conditional_trait_bounds(&field.ty, &ctype.generics, quote!(core::cmp::PartialEq))
    });
    let comparisons = ctype
        .fields
        .members()
        .map(|member| quote!(self.#member == other.#member));

    quote! {
        impl #impl_generics core::cmp::PartialEq for #name #ty_generics
        where
            #(#bounds,)*
            #predicates
        {
            fn eq(&self, other: &Self) -> bool {
                true #(&& #comparisons)*
            }
        }
    }
}

fn gen_union_ctype_impls(
    ctype: &syn::ItemUnion,
    alignment: Option<&syn::LitInt>,
    owner_name: Option<&syn::Ident>,
) -> TokenStream {
    let fields = ctype.fields.named.iter().map(|f| &f.ty).collect::<Vec<_>>();

    let copy_impls = gen_copy_impls(&ctype.ident, &ctype.generics, &fields, false);
    let default_impl = gen_default_impl(&ctype.ident, &ctype.generics, &fields, false);
    let robust_impls = gen_robust_impls(&ctype.ident, &ctype.generics, &fields, false);
    let type_spec_impl = gen_type_spec(&ctype.ident, &ctype.generics, &fields, alignment);
    let const_view = gen_ctype_union_view(ctype.clone(), false, alignment);
    let mut_view = gen_ctype_union_view(ctype.clone(), true, alignment);

    let borrow_cast_impl =
        gen_borrow_cast_impl(&ctype.ident, &ctype.generics, &fields, owner_name, true);

    quote! {
        #copy_impls
        #default_impl
        #robust_impls
        #type_spec_impl

        #const_view
        #mut_view

        #borrow_cast_impl
    }
}

fn gen_c_fn_impls(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> TokenStream {
    let co3 = co3_path();
    let (last_field_sized_bound, omit) = c_fn_last_field_bounds(fields, generics);
    if omit {
        return quote! {};
    }

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let speculative = speculative_marker(generics.params.is_empty());
    let size_bound = quote! {
        #speculative Self: #co3::rust_spec::RustSpec<
            Size = #co3::rust_spec::size::Sized<#co3::rust_spec::Gt<#co3::rust_spec::Zero>>
        >,
    };

    quote! {
        unsafe impl #impl_generics #co3::CFnArg for #ident #ty_generics where
            #last_field_sized_bound
            #size_bound
            #predicates
        {}
        unsafe impl #impl_generics #co3::CFnReturn for #ident #ty_generics where
            #last_field_sized_bound
            #size_bound
            #predicates
        {}
    }
}

fn gen_robust_impls(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
    known_unsized_tail: bool,
) -> TokenStream {
    let co3 = co3_path();
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let codec_impls = gen_identity_codec_impls(ident, generics, true);
    let c_fn_impls = if known_unsized_tail {
        quote! {}
    } else {
        gen_c_fn_impls(ident, generics, fields)
    };

    quote! {
        unsafe impl #impl_generics #co3::CType for #ident #ty_generics #where_clause {}

        #c_fn_impls

        unsafe impl #impl_generics #co3::transmute::CheckedTransmute for #ident #ty_generics #where_clause {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        #codec_impls
    }
}

// TODO: This is duplicated from rust-spec. There is some complicated logic here.
// Maybe we can use some derive macro attribute in the aformentioned crate to dedup?
fn gen_type_spec(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
    repr_alignment: Option<&syn::LitInt>,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let (size, size_bounds) = gen_field_family(
        generics,
        fields,
        quote! { core::ops::Add },
        quote! { Size },
        quote! { co3::rust_spec::size::Sized<co3::rust_spec::Zero> },
    );
    let (mut alignment, alignment_bounds) = gen_field_family(
        generics,
        fields,
        quote! { co3::rust_spec::Max },
        quote! { Alignment },
        quote! { co3::rust_spec::One },
    );
    if repr_alignment.is_some_and(|alignment| alignment.base10_digits() != "1") {
        alignment = quote!(co3::rust_spec::Gt<co3::rust_spec::One>);
    }
    let rust_spec_bounds = fields
        .iter()
        .filter(|field| !is_parametrized(field, generics))
        .flat_map(|field| {
            gen_hrtb_projection_bounds(field, generics, quote! { co3::rust_spec::RustSpec })
        });

    quote! {
        unsafe impl #impl_generics co3::rust_spec::RustSpec for #ident #ty_generics
        where
            #(#rust_spec_bounds,)*
            #(#size_bounds,)*
            #(#alignment_bounds,)*
            #predicates
        {
            type Layout = co3::rust_spec::Stable;
            type Size = #size;
            type Alignment = #alignment;
            type Trap = co3::rust_spec::layout::Robust;
            type Niche = co3::rust_spec::niche::WithoutNiche;
            type Mutability = co3::rust_spec::mutability::Exclusive;
            type Drop = co3::rust_spec::drop::NoDrop;
            type __IndirectTrap = co3::rust_spec::layout::Robust;
        }
    }
}

fn gen_field_family(
    generics: &syn::Generics,
    fields: &[&syn::Type],
    operator: TokenStream,
    axis: TokenStream,
    identity: TokenStream,
) -> (TokenStream, Vec<TokenStream>) {
    let (concrete_family, parametrized_family, mut bounds) =
        gen_field_family_parts(operator.clone(), axis, generics, fields);

    let family = match (concrete_family, parametrized_family) {
        (Some(concrete), Some(parametrized)) => {
            bounds.push(quote! { #concrete: #operator<#parametrized> });
            quote! { <#concrete as #operator<#parametrized>>::Output }
        }
        (Some(concrete), None) => concrete,
        (None, Some(parametrized)) => parametrized,
        (None, None) => identity,
    };

    (family, bounds)
}

fn gen_field_family_parts(
    operator: TokenStream,
    axis: TokenStream,
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> (Option<TokenStream>, Option<TokenStream>, Vec<TokenStream>) {
    let mut bounds = Vec::new();
    let (concrete_fields, parametrized_fields): (Vec<_>, Vec<_>) = fields
        .iter()
        .copied()
        .partition(|field| !is_parametrized(field, generics));

    let mut concrete_fields = concrete_fields.into_iter();
    let concrete_family = concrete_fields.next().map(|first| {
        let mut family = quote! { <#first as co3::rust_spec::RustSpec>::#axis };

        for field in concrete_fields {
            let field_family = quote! { <#field as co3::rust_spec::RustSpec>::#axis };
            family = quote! { <#family as #operator<#field_family>>::Output };
        }

        family
    });

    let mut parametrized_fields = parametrized_fields.into_iter();
    let parametrized_family = parametrized_fields.next().map(|first| {
        bounds.push(quote! { #first: co3::rust_spec::RustSpec });
        let mut family = quote! { <#first as co3::rust_spec::RustSpec>::#axis };

        for field in parametrized_fields {
            let field_family = quote! { <#field as co3::rust_spec::RustSpec>::#axis };
            bounds.push(quote! { #field: co3::rust_spec::RustSpec });
            bounds.push(quote! { #family: #operator<#field_family> });
            family = quote! { <#family as #operator<#field_family>>::Output };
        }

        family
    });

    (concrete_family, parametrized_family, bounds)
}

fn gen_identity_codec_impls(
    ident: &syn::Ident,
    generics: &syn::Generics,
    needs_sized_bound: bool,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let mut decode_generics = generics.clone();
    decode_generics.params.insert(0, parse_quote!('d));
    let (decode_impl_generics, _, _) = decode_generics.split_for_impl();

    let sized_bound = needs_sized_bound.then(|| {
        let speculative = speculative_marker(generics.params.is_empty());
        quote!(#speculative Self: Sized,)
    });

    quote! {
        impl #impl_generics co3::ReprC for #ident #ty_generics
        where
            #predicates
        {
            type CType = Self;
        }
        unsafe impl #impl_generics co3::stored::EncodeOwned for #ident #ty_generics
        where
            #sized_bound
            #predicates
        {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                use co3::stored::DirectEncodeOwned as _;
                unsafe { self.soft_encode_unchecked(&mut ()) }
            }
        }
        unsafe impl #impl_generics co3::stored::DirectEncodeOwned for #ident #ty_generics
        where
            #sized_bound
            #predicates
        {
            type SoftStore = ();

            unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
                self
            }
        }
        unsafe impl #decode_impl_generics co3::stored::DecodeOwned<'d> for #ident #ty_generics
        where
            #sized_bound
            #predicates
        {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Some(source)
            }
        }

        impl #impl_generics co3::Encode for #ident #ty_generics
        where
            #sized_bound
            #predicates
        {}
        impl #impl_generics co3::DirectEncode for #ident #ty_generics
        where
            #sized_bound
            #predicates
        {}
        impl #impl_generics co3::Decode<'_> for #ident #ty_generics
        where
            #sized_bound
            #predicates
        {}
    }
}

pub(super) fn gen_identity_repr_c_impls(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let codec_impls = gen_identity_codec_impls(ident, generics, true);
    let c_fn_impls = gen_c_fn_impls(ident, generics, fields);

    quote! {
        unsafe impl #impl_generics co3::CType for #ident #ty_generics #where_clause {}

        #c_fn_impls

        unsafe impl #impl_generics co3::transmute::CheckedTransmute for #ident #ty_generics
        #where_clause
        {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        #codec_impls
    }
}

fn gen_borrow_cast_impl(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
    owner_name: Option<&syn::Ident>,
    union_fields: bool,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let const_view_name = gen_const_view_name(ident);
    let mut_view_name = gen_mut_view_name(ident);

    let mut const_bounds = gen_ctype_borrow_cast_bounds(generics, fields, false);
    let mut mut_bounds = gen_ctype_borrow_cast_bounds(generics, fields, true);
    if union_fields {
        const_bounds.extend(
            fields
                .iter()
                .filter(|&&ty| !is_phantom_data(ty))
                .map(|&ty| quote!(#ty: co3::borrow::BorrowCast<AsConst: Copy> + Copy)),
        );
        mut_bounds.extend(
            fields
                .iter()
                .filter(|&&ty| !is_phantom_data(ty))
                .map(|&ty| quote!(#ty: co3::borrow::BorrowCastMut<AsMut: Copy> + Copy)),
        );
    }

    let cast_impls = if let Some(owner_name) = owner_name {
        let owner_type = quote!(#owner_name #ty_generics);
        let speculative = speculative_marker(generics.params.is_empty());
        let no_drop_bound = quote!(#speculative #owner_type: co3::rust_spec::RustSpec<Drop = co3::rust_spec::drop::NoDrop>,);
        let auto_drop_bound = quote!(#speculative #owner_type: co3::rust_spec::RustSpec<Drop = co3::rust_spec::drop::AutoDrop>,);
        quote! {
            const _: () = {
                use co3::borrow::{BorrowCast, BorrowCastMut};

                co3::disjoint_impls! {
                    #[disjoint_impls(remote)]
                    #[allow(clippy::missing_safety_doc)]
                    pub unsafe trait BorrowCast: co3::CType {
                        type AsConst: co3::CType + ?Sized;
                    }

                    unsafe impl #impl_generics BorrowCast for #ident #ty_generics
                    where
                        #no_drop_bound
                        #predicates
                    {
                        type AsConst = Self;
                    }

                    unsafe impl #impl_generics BorrowCast for #ident #ty_generics
                    where
                        #auto_drop_bound
                        #(#const_bounds,)*
                        #predicates
                    {
                        type AsConst = #const_view_name #ty_generics;
                    }
                }

                co3::disjoint_impls! {
                    #[disjoint_impls(remote)]
                    #[allow(clippy::missing_safety_doc)]
                    pub unsafe trait BorrowCastMut: co3::CType {
                        type AsMut: co3::CType + ?Sized;
                    }

                    unsafe impl #impl_generics BorrowCastMut for #ident #ty_generics
                    where
                        #no_drop_bound
                        #predicates
                    {
                        type AsMut = Self;
                    }

                    unsafe impl #impl_generics BorrowCastMut for #ident #ty_generics
                    where
                        #auto_drop_bound
                        #(#mut_bounds,)*
                        #predicates
                    {
                        type AsMut = #mut_view_name #ty_generics;
                    }
                }
            };
        }
    } else {
        quote! {
            unsafe impl #impl_generics co3::borrow::BorrowCast for #ident #ty_generics
            where
                #(#const_bounds,)*
                #predicates
            {
                type AsConst = #const_view_name #ty_generics;
            }

            unsafe impl #impl_generics co3::borrow::BorrowCastMut for #ident #ty_generics
            where
                #(#mut_bounds,)*
                #predicates
            {
                type AsMut = #mut_view_name #ty_generics;
            }
        }
    };

    quote! {
        #cast_impls
    }
}

pub(super) fn gen_identity_borrow_cast_impl(
    ident: &syn::Ident,
    generics: &syn::Generics,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        unsafe impl #impl_generics co3::borrow::BorrowCast for #ident #ty_generics #where_clause {
            type AsConst = Self;
        }

        unsafe impl #impl_generics co3::borrow::BorrowCastMut for #ident #ty_generics #where_clause {
            type AsMut = Self;
        }
    }
}

fn gen_ctype_repr_attr(repr: Option<&ReprKind>, alignment: Option<&syn::LitInt>) -> TokenStream {
    let kind = if repr == Some(&ReprKind::Transparent) {
        quote!(transparent)
    } else {
        quote!(C)
    };
    let alignment = alignment.map(|alignment| quote!(, align(#alignment)));
    quote! { #[repr(#kind #alignment)] }
}

fn gen_ctype_struct_view(
    mut ctype: syn::ItemStruct,
    is_mut: bool,
    alignment: Option<&syn::LitInt>,
    known_unsized_tail: bool,
) -> TokenStream {
    rewrite_ctype_view_struct(&mut ctype, is_mut);
    let fields_methods = gen_fields_methods(
        &ctype.ident,
        &ctype.generics,
        &ctype.fields,
        known_unsized_tail,
    );

    let fields = ctype
        .fields
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();

    let copy_impls = gen_copy_impls(&ctype.ident, &ctype.generics, &fields, known_unsized_tail);
    let default_impl = gen_default_impl(&ctype.ident, &ctype.generics, &fields, known_unsized_tail);
    let robust_impls = gen_robust_impls(&ctype.ident, &ctype.generics, &fields, known_unsized_tail);
    let type_spec_impl = gen_type_spec(&ctype.ident, &ctype.generics, &fields, alignment);

    quote! {
        #ctype
        #fields_methods
        #copy_impls
        #default_impl
        #robust_impls
        #type_spec_impl
    }
}

fn gen_ctype_union_view(
    mut ctype: syn::ItemUnion,
    is_mut: bool,
    alignment: Option<&syn::LitInt>,
) -> TokenStream {
    // The source union's `Copy` predicates describe its original variant
    // structs. A union view has different fields (the const/mut view structs),
    // so carrying those predicates over would unnecessarily require both
    // representations to be `Copy` just to define the view.
    let original_fields = ctype
        .fields
        .named
        .iter()
        .map(|field| field.ty.clone())
        .collect::<Vec<_>>();
    rewrite_ctype_view_union(&mut ctype, is_mut);

    if let Some(where_clause) = &mut ctype.generics.where_clause {
        where_clause.predicates = where_clause
            .predicates
            .iter()
            .filter(|predicate| {
                !matches!(predicate, syn::WherePredicate::Type(predicate)
                    if original_fields.iter().any(|field| field == &predicate.bounded_ty)
                        && predicate.bounds.iter().any(|bound| matches!(bound, syn::TypeParamBound::Trait(bound)
                            if bound.path.is_ident("Copy"))))
            })
            .cloned()
            .collect();
    }

    let view_field_tys = ctype
        .fields
        .named
        .iter()
        .map(|field| field.ty.clone())
        .collect::<Vec<_>>();
    for ty in &view_field_tys {
        ctype
            .generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(#ty: Copy));
    }

    let fields = ctype
        .fields
        .named
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();

    let copy_impls = gen_copy_impls(&ctype.ident, &ctype.generics, &fields, false);
    let default_impl = gen_default_impl(&ctype.ident, &ctype.generics, &fields, false);
    let robust_impls = gen_robust_impls(&ctype.ident, &ctype.generics, &fields, false);
    let type_spec_impl = gen_type_spec(&ctype.ident, &ctype.generics, &fields, alignment);

    quote! {
        #ctype
        #copy_impls
        #default_impl
        #robust_impls
        #type_spec_impl
    }
}

fn rewrite_ctype_view_struct(ctype: &mut syn::ItemStruct, is_mut: bool) {
    rewrite_ctype_view_name(&mut ctype.ident, is_mut);
    rewrite_ctype_view_generics(
        &mut ctype.generics,
        ctype.fields.iter_mut().map(|field| &mut field.ty),
        is_mut,
    );
}

fn rewrite_ctype_view_union(ctype: &mut syn::ItemUnion, is_mut: bool) {
    rewrite_ctype_view_name(&mut ctype.ident, is_mut);
    rewrite_ctype_view_generics(
        &mut ctype.generics,
        ctype.fields.named.iter_mut().map(|field| &mut field.ty),
        is_mut,
    );
}

fn rewrite_ctype_view_name(ident: &mut syn::Ident, is_mut: bool) {
    *ident = if is_mut {
        gen_mut_view_name(ident)
    } else {
        gen_const_view_name(ident)
    };
}

fn rewrite_ctype_view_generics<'a>(
    generics: &mut syn::Generics,
    fields: impl Iterator<Item = &'a mut syn::Type>,
    is_mut: bool,
) {
    let field_tys = rewrite_ctype_view_field_tys(fields, is_mut);
    let field_tys = field_tys.iter().collect::<Vec<_>>();

    for bound in gen_ctype_borrow_cast_bounds(generics, &field_tys, is_mut) {
        generics.make_where_clause().predicates.push(parse_quote! {
            #bound
        });
    }
}

fn rewrite_ctype_view_field_tys<'a>(
    fields: impl Iterator<Item = &'a mut syn::Type>,
    is_mut: bool,
) -> Vec<syn::Type> {
    fields
        .map(|field_ty| {
            let ty = field_ty.clone();

            if is_phantom_data(&ty) {
                return ty;
            }

            let (borrow_cast_trait, view_ty) = if is_mut {
                (quote!(co3::borrow::BorrowCastMut), quote!(AsMut))
            } else {
                (quote!(co3::borrow::BorrowCast), quote!(AsConst))
            };
            *field_ty = parse_quote! { <#ty as #borrow_cast_trait>::#view_ty };
            ty
        })
        .collect()
}

fn gen_copy_impls(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
    known_unsized_tail: bool,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    if known_unsized_tail
        || fields.last().is_some_and(|field| {
            syntactic_sizedness(field, generics) == SyntacticSizedness::Unsized
        })
    {
        return quote! {};
    }

    let copy_bounds = gen_copy_bounds(generics, fields);

    quote! {
        impl #impl_generics Clone for #ident #ty_generics
        where
            #(#copy_bounds,)*
            #predicates
        {
            fn clone(&self) -> Self {
                *self
            }
        }

        impl #impl_generics Copy for #ident #ty_generics
        where
            #(#copy_bounds,)*
            #predicates
        {}
    }
}

fn gen_copy_bounds(generics: &syn::Generics, fields: &[&syn::Type]) -> Vec<TokenStream> {
    let Some((last, fields)) = fields.split_last() else {
        return Vec::new();
    };

    let mut predicates = fields
        .iter()
        .map(|&ty| {
            if is_parametrized(ty, generics) {
                parse_quote! { #ty: Copy }
            } else {
                let speculative = speculative_marker(true);
                parse_quote! { #speculative #ty: Copy }
            }
        })
        .collect::<Vec<_>>();

    if is_parametrized(last, generics) {
        predicates.push(quote!(#last: Copy));
    } else {
        let speculative = speculative_marker(true);
        predicates.push(quote!(#speculative #last: Copy));
    }

    predicates
}
fn gen_default_impl(
    ident: &syn::Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
    known_unsized_tail: bool,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let (sized_bound, omit_impl) = c_fn_last_field_bounds(fields, generics);
    if known_unsized_tail || omit_impl {
        return quote! {};
    }

    quote! {
        impl #impl_generics Default for #ident #ty_generics
        where
            #sized_bound
            #predicates
        {
            #[inline(always)]
            fn default() -> Self {
                unsafe { core::mem::zeroed() }
            }
        }
    }
}

pub(super) fn gen_ctype_name(item_name: &syn::Ident) -> syn::Ident {
    format_ident!("C{item_name}")
}

fn gen_const_view_name(item_name: &syn::Ident) -> syn::Ident {
    format_ident!("{item_name}ConstView")
}

fn gen_mut_view_name(item_name: &syn::Ident) -> syn::Ident {
    format_ident!("{item_name}MutView")
}

pub(super) fn gen_variant_struct_name(
    enum_name: &syn::Ident,
    variant_name: &syn::Ident,
) -> syn::Ident {
    format_ident!("C{enum_name}{variant_name}")
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

pub(super) fn filter_generics(
    generics: &syn::Generics,
    field_types: &[&syn::Type],
) -> syn::Generics {
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

pub(super) fn gen_extern_c_bounds_for_ctype(
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> Vec<syn::WherePredicate> {
    let gen_bound = |ty: &syn::Type, ctype_bound: TokenStream| {
        if is_phantom_data(ty) {
            return vec![];
        }

        if is_parametrized(ty, generics) {
            vec![parse_quote! { #ty: co3::ReprC #ctype_bound }]
        } else {
            gen_hrtb_projection_bounds(ty, generics, quote! { co3::ReprC #ctype_bound })
        }
    };

    let Some((last, fields)) = fields.split_last() else {
        return Vec::new();
    };

    fields
        .iter()
        .flat_map(|&ty| gen_bound(ty, quote!(<CType: Sized>)))
        .chain(gen_bound(last, quote!()))
        .collect::<Vec<_>>()
}

pub(super) fn gen_copy_extern_c_bounds_for_ctype(
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> Vec<syn::WherePredicate> {
    fields
        .iter()
        .flat_map(|&ty| {
            if is_phantom_data(ty) {
                Vec::new()
            } else if is_parametrized(ty, generics) {
                vec![parse_quote!(#ty: co3::ReprC<CType: Copy>)]
            } else {
                gen_hrtb_projection_bounds(ty, generics, quote!(co3::ReprC<CType: Copy>))
            }
        })
        .collect()
}

fn gen_hrtb_projection_bounds(
    ty: &syn::Type,
    generics: &syn::Generics,
    bound: TokenStream,
) -> Vec<syn::WherePredicate> {
    #[derive(Default)]
    struct ProjectionVisitor<'a> {
        projections: Vec<&'a syn::TypePath>,
    }

    impl<'ast> Visit<'ast> for ProjectionVisitor<'ast> {
        fn visit_type_path(&mut self, type_path: &'ast syn::TypePath) {
            if type_path.qself.is_some() {
                self.projections.push(type_path);
                for segment in &type_path.path.segments {
                    self.visit_path_arguments(&segment.arguments);
                }
                return;
            }
            syn::visit::visit_type_path(self, type_path);
        }
    }

    let mut visitor = ProjectionVisitor::default();
    visitor.visit_type(ty);

    let mut bounds = Vec::new();
    for projection in visitor.projections {
        let Some(qself) = projection.qself.as_ref() else {
            continue;
        };
        let trait_path = syn::Path {
            leading_colon: projection.path.leading_colon,
            segments: projection
                .path
                .segments
                .iter()
                .take(qself.position)
                .cloned()
                .collect(),
        };
        for predicate in generics
            .where_clause
            .iter()
            .flat_map(|where_clause| &where_clause.predicates)
        {
            let syn::WherePredicate::Type(predicate) = predicate else {
                continue;
            };
            let Some(lifetimes) = &predicate.lifetimes else {
                continue;
            };
            let bounded_ty = &predicate.bounded_ty;
            let projected_ty = &qself.ty;
            if quote!(#bounded_ty).to_string() != quote!(#projected_ty).to_string() {
                continue;
            }

            for prerequisite in &predicate.bounds {
                let syn::TypeParamBound::Trait(trait_bound) = prerequisite else {
                    continue;
                };
                let predicate_trait_path = &trait_bound.path;
                if quote!(#predicate_trait_path).to_string() != quote!(#trait_path).to_string() {
                    continue;
                }

                let generated: syn::WherePredicate =
                    parse_quote! { #lifetimes #projection: #bound };
                bounds.push(generated);
            }
        }
    }

    bounds
}

fn gen_ctype_borrow_cast_bounds(
    generics: &syn::Generics,
    fields: &[&syn::Type],
    is_mut: bool,
) -> Vec<TokenStream> {
    let (borrow_cast_trait, assoc_type) = if is_mut {
        (quote! { co3::borrow::BorrowCastMut }, quote!(AsMut))
    } else {
        (quote! { co3::borrow::BorrowCast }, quote!(AsConst))
    };

    let Some((last, fields)) = fields.split_last() else {
        return Vec::new();
    };

    let mut predicates = fields
        .iter()
        .filter(|ty| !is_phantom_data(ty) && is_parametrized(ty, generics))
        .map(|&ty| {
            let ctype_bound = quote! { <#assoc_type: Sized> };
            parse_quote! { #ty: #borrow_cast_trait #ctype_bound }
        })
        .collect::<Vec<_>>();

    if !is_phantom_data(last) && is_parametrized(last, generics) {
        predicates.push(parse_quote! { #last: #borrow_cast_trait });
    }

    predicates
}
