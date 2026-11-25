use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_quote, punctuated::Punctuated, visit::Visit};

use super::inference::RecursiveFields;
use crate::{
    layout::{
        attr::ReprKind,
        borrow::gen_identity_borrow_impls,
        fields::{FieldType, field_types},
        infer_repr, is_phantom_data, is_transparent_enum_repr, primitive_tag_type,
        wide::{
            gen_alloc_methods, gen_data_struct_name, gen_dst_methods, gen_identity_header_impl,
            header_repr_c_predicate, last_field, wide_predicate, wide_tail_type,
            wide_usize_predicate,
        },
    },
    utils::{co3_path, peel_grouped_type, speculative_marker},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TailClass {
    Sized,
    Unsized(UnsizedTail),
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum UnsizedTail {
    SliceLike,
    NulTerminated,
    DynTraitLike,
    Unknown,
}

impl TailClass {
    pub(super) fn is_known_unsized(self) -> bool {
        matches!(
            self,
            Self::Unsized(
                UnsizedTail::SliceLike | UnsizedTail::NulTerminated | UnsizedTail::DynTraitLike
            )
        )
    }
}

fn tail_type_param<'a>(ty: &syn::Type, generics: &'a syn::Generics) -> Option<&'a syn::TypeParam> {
    let syn::Type::Path(path) = peel_grouped_type(ty) else {
        return None;
    };
    let ident = path
        .qself
        .is_none()
        .then(|| path.path.get_ident())
        .flatten()?;
    generics.type_params().find(|param| param.ident == *ident)
}

fn has_maybe_sized_bound(param: &syn::TypeParam, generics: &syn::Generics) -> bool {
    let maybe_sized = |bound: &syn::TypeParamBound| {
        matches!(bound, syn::TypeParamBound::Trait(bound)
            if bound.maybe.is_some() && crate::utils::builtin_path(&bound.path, "Sized", &["core::marker", "std::marker"]))
    };
    param.bounds.iter().any(maybe_sized)
        || generics
            .where_clause
            .iter()
            .flat_map(|clause| &clause.predicates)
            .any(|predicate| {
                matches!(predicate, syn::WherePredicate::Type(predicate)
                    if matches!(&predicate.bounded_ty, syn::Type::Path(path)
                        if path.qself.is_none() && path.path.is_ident(&param.ident))
                    && predicate.bounds.iter().any(maybe_sized))
            })
}

pub(super) fn classify_tail(ty: &syn::Type, generics: &syn::Generics) -> TailClass {
    if let Some(param) = tail_type_param(ty, generics) {
        return if has_maybe_sized_bound(param, generics) {
            TailClass::Unsized(UnsizedTail::Unknown)
        } else {
            TailClass::Sized
        };
    }
    match peel_grouped_type(ty) {
        syn::Type::Slice(_) => TailClass::Unsized(UnsizedTail::SliceLike),
        syn::Type::TraitObject(_) => TailClass::Unsized(UnsizedTail::DynTraitLike),
        syn::Type::Reference(_) | syn::Type::Ptr(_) | syn::Type::Array(_) | syn::Type::Never(_) => {
            TailClass::Sized
        }
        syn::Type::Tuple(tuple) => tuple
            .elems
            .last()
            .map_or(TailClass::Sized, |tail| classify_tail(tail, generics)),
        syn::Type::Path(path) => match crate::known_type::recognize_path(path) {
            Some(known) if known.is_sized() => TailClass::Sized,
            Some(known) if known.is_transparent_wrapper() => {
                classify_tail(known.inner.expect("wrapper element"), generics)
            }
            Some(known) if known.kind == crate::known_type::Kind::Str => {
                TailClass::Unsized(UnsizedTail::SliceLike)
            }
            Some(known) if known.kind == crate::known_type::Kind::CStr => {
                TailClass::Unsized(UnsizedTail::NulTerminated)
            }
            _ => TailClass::Unknown,
        },
        _ => TailClass::Unknown,
    }
}

pub(super) fn c_fn_last_field_bounds(
    fields: &[&syn::Type],
    generics: &syn::Generics,
) -> (Punctuated<syn::WherePredicate, syn::Token![,]>, bool) {
    let mut bounds = Punctuated::new();
    let Some(last) = fields.last() else {
        return (bounds, false);
    };

    match classify_tail(last, generics) {
        TailClass::Sized => (bounds, false),
        TailClass::Unsized(
            UnsizedTail::SliceLike | UnsizedTail::NulTerminated | UnsizedTail::DynTraitLike,
        ) => (bounds, true),
        TailClass::Unsized(UnsizedTail::Unknown) | TailClass::Unknown => {
            bounds.push(FieldType::new(last, generics).bound(quote!(Sized)));
            bounds.push_punct(Default::default());
            (bounds, false)
        }
    }
}

fn lowered_field_ty(field_ty: &syn::Type) -> syn::Type {
    if let Some(known) = crate::known_type::recognize(field_ty)
        .filter(|known| known.kind == crate::known_type::Kind::PhantomData)
    {
        let inner = known.inner.unwrap();
        return parse_quote!(core::marker::PhantomData<<#inner as co3::ReprC>::CType>);
    }
    parse_quote!(<#field_ty as co3::ReprC>::CType)
}

/// Generates the wrapper carrier for a fieldless enum. Exhaustive enums
/// without custom Drop use the tag itself as their CType instead.
pub(super) fn gen_fieldless_enum_ctype(
    tag_type: &syn::Type,
    alignment: Option<&syn::LitInt>,
    vis: &syn::Visibility,
    name: &syn::Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
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
    // fieldless enum is represented by `EnumCType(())` and must not become a CFnArg.
    let impls = gen_struct_ctype_impls::<false>(&ctype, false, alignment, false, &[]);
    let is_valid_tag = if matches!(tag_type, syn::Type::Tuple(_)) {
        quote!(true)
    } else {
        let tags = variants.iter().map(|variant| {
            let ident = format_ident!("__CO3_TAG_{}", variant.ident);
            quote!(self.0 == <#name #ty_generics>::#ident)
        });
        quote!(false #(|| #tags)*)
    };
    let fields = ctype
        .fields
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();
    let type_spec_impl = gen_type_spec(&ctype.ident, &ctype.generics, &fields, alignment, &[]);
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

    let niche_check = (variants.len() != 1 && !super::is_exhaustive_enum(variants.len(), tag_type))
        .then(|| {
            let niche_tag = super::niche::gen_enum_niche_tag(name, variants, tag_type, true);
            quote! {
                const fn niche() -> Self {
                    Self(#niche_tag)
                }

                #[allow(dead_code, reason = "generated helper usage depends on the derived type")]
                pub(crate) const fn is_niche(&self) -> bool {
                    self.tag() == Self::niche().tag()
                }
            }
        });

    quote! {
        #ctype
        #impls
        #type_spec_impl

        impl #impl_generics core::cmp::PartialEq for #ctype_name #ty_generics #where_clause {
            fn eq(&self, other: &Self) -> bool {
                self.0 == other.0 && (#is_valid_tag)
            }
        }

        impl #impl_generics #ctype_name #ty_generics #where_clause {
            #primitive_constructor
            #niche_check

            /// Return the raw discriminant.
            pub const fn tag(&self) -> #tag_type { self.0 }
        }
    }
}

pub(super) fn gen_item_ctype(
    repr: Option<&ReprKind>,
    alignment: Option<&syn::LitInt>,
    input: &syn::DeriveInput,
    generate_spec: bool,
    generate_wide: bool,
    recursion: &RecursiveFields,
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
            generate_spec,
            generate_wide,
            recursion,
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
                generate_spec,
                false,
                recursion,
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
        bounds.push(FieldType::new(last, generics).bound(quote!(co3::ReprC<CType: Sized>)));
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
            pub const fn #variant_name(#(#arg_names: #arg_types),*) -> Self {
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
    let union_impls = gen_union_ctype_impls(&union_def, alignment);
    let partial_eq_impl = gen_tagged_union_partial_eq(&union_def, &tag_type, variants);
    let inspection_impls = gen_tagged_union_inspection(&union_def, &tag_type, variants);
    let constructors = gen_enum_variant_constructors(
        &union_def.ident,
        &union_def.generics,
        name,
        variants,
        Some(&tag_type),
        None,
    );
    quote! { #(#variant_structs)* #union_def #union_impls #partial_eq_impl #inspection_impls #constructors }
}

fn gen_tagged_union_inspection(
    union: &syn::ItemUnion,
    tag_type: &syn::Type,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> TokenStream {
    let name = &union.ident;
    let (impl_generics, ty_generics, where_clause) = union.generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|clause| &clause.predicates);
    let field_types = variants
        .iter()
        .flat_map(|v| v.fields.iter().map(|f| lowered_field_ty(&f.ty)))
        .collect::<Vec<_>>();
    let ord_bounds = field_types
        .iter()
        .map(|ty| quote!(for<'_dummy> #ty: core::cmp::PartialOrd));
    let debug_bounds = field_types
        .iter()
        .map(|ty| quote!(for<'_dummy> #ty: core::fmt::Debug));
    let tags = super::enum_variant_tag_exprs(variants, tag_type);
    let arms = variants.iter().zip(&tags).map(|(variant, tag)| {
        let v = &variant.ident;
        let members = variant.fields.members().map(|member| match member {
            syn::Member::Unnamed(mut index) => { index.index += 1; syn::Member::Unnamed(index) },
            other => other,
        }).collect::<Vec<_>>();
        let comparisons = members.iter().map(|member| quote! {
            match unsafe { self.#v.#member.assume_init_ref().partial_cmp(other.#v.#member.assume_init_ref()) } {
                Some(core::cmp::Ordering::Equal) => {},
                result => return result,
            }
        });
        quote! { value if value == #tag => { #(#comparisons)* Some(core::cmp::Ordering::Equal) } }
    });
    let valid_tags = tags
        .iter()
        .map(|tag| quote!(tag == #tag))
        .collect::<Vec<_>>();
    let debug_arms = variants.iter().zip(&tags).map(|(variant, tag)| {
        let v = &variant.ident;
        let label = format!("{}::{}", name, v);
        let members = variant.fields.members().map(|member| match member {
            syn::Member::Unnamed(mut index) => { index.index += 1; syn::Member::Unnamed(index) },
            other => other,
        }).collect::<Vec<_>>();
        let body = match &variant.fields {
            syn::Fields::Named(fields) => {
                let names = fields.named.iter().map(|field| field.ident.as_ref().unwrap().to_string()).collect::<Vec<_>>();
                quote! { let mut debug = f.debug_struct(#label); #(debug.field(#names, unsafe { self.#v.#members.assume_init_ref() });)* debug.finish() }
            },
            syn::Fields::Unnamed(_) => quote! { let mut debug = f.debug_tuple(#label); #(debug.field(unsafe { self.#v.#members.assume_init_ref() });)* debug.finish() },
            syn::Fields::Unit => quote!(f.write_str(#label)),
        };
        quote!(value if value == #tag => { #body })
    });
    quote! {
        impl #impl_generics core::cmp::PartialOrd for #name #ty_generics
        where #(#ord_bounds,)* #predicates
        {
            fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                let left = self.tag();
                let right = other.tag();
                let is_valid = |tag: #tag_type| false #(|| #valid_tags)*;
                if left != right {
                    return if is_valid(left) && is_valid(right) { left.partial_cmp(&right) } else { None };
                }
                match left { #(#arms,)* _ => None }
            }
        }
        impl #impl_generics core::fmt::Debug for #name #ty_generics
        where #(#debug_bounds,)* #predicates
        {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                match self.tag() {
                    #(#debug_arms,)*
                    tag => f.debug_struct(concat!(stringify!(#name), "::<invalid>")).field("tag", &tag).finish(),
                }
            }
        }
    }
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
            pub const fn tag(&self) -> #tag_type {
                unsafe { *core::ptr::from_ref(self).cast::<#tag_type>() }
            }
        }
    };
    let raw_constructor = if let Some(payload_name) = payload_name {
        quote! {
            /// Construct a carrier from a raw tag and payload union.
            ///
            /// # Safety
            /// When the tag selects a variant, its payload must be initialized.
            pub const unsafe fn from_raw_parts(tag: #tag_type, payload: core::mem::MaybeUninit<#payload_name #ty_generics>) -> Self {
                Self { tag, payload }
            }
        }
    } else {
        quote! {
            /// Construct a carrier from a tag and raw union storage.
            ///
            /// # Safety
            /// When the tag selects a variant, its payload fields must be initialized.
            pub unsafe fn from_raw_parts(tag: #tag_type, mut payload: core::mem::MaybeUninit<Self>) -> Self {
                unsafe {
                    payload.as_mut_ptr().cast::<#tag_type>().write(tag);
                    payload.assume_init()
                }
            }
        }
    };
    let niche_check = (!super::is_exhaustive_enum(variants.len(), tag_type)).then(|| {
        let niche_tag = super::niche::gen_enum_niche_tag(enum_name, variants, tag_type, false);

        let niche_value = if payload_name.is_some() {
            quote! { Self { tag: #niche_tag, payload: core::mem::MaybeUninit::zeroed() } }
        } else {
            let variant = &variants[0];
            let variant_name = &variant.ident;
            let variant_struct = gen_variant_struct_name(enum_name, variant_name);
            let field_values = variant
                .fields
                .iter()
                .map(|_| quote!(core::mem::MaybeUninit::zeroed()));

            let body = match &variant.fields {
                syn::Fields::Named(fields) => {
                    let names = fields
                        .named
                        .iter()
                        .map(|field| field.ident.as_ref().unwrap());
                    quote!(#variant_struct { tag: #niche_tag, #(#names: #field_values),* })
                }
                syn::Fields::Unnamed(_) => quote!(#variant_struct(#niche_tag, #(#field_values),*)),
                syn::Fields::Unit => quote!(#variant_struct { tag: #niche_tag }),
            };

            quote!(Self { #variant_name: #body })
        };
        quote! {
            #[allow(dead_code, reason = "generated helper usage depends on the derived type")]
            const fn niche() -> Self {
                #niche_value
            }
            #[allow(dead_code, reason = "generated helper usage depends on the derived type")]
            pub(crate) const fn is_niche(&self) -> bool {
                self.tag() == Self::niche().tag()
            }
        }
    });
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
        let arg_values = args
            .iter()
            .map(|(name, _)| {
                if tag_value.is_some() {
                    quote!(core::mem::MaybeUninit::new(#name))
                } else {
                    quote!(#name)
                }
            })
            .collect::<Vec<_>>();
        let body = match (&variant.fields, tag_value.as_ref()) {
            (syn::Fields::Named(_), Some(tag)) => {
                quote!(#variant_struct_name { tag: #tag, #(#arg_names: #arg_values),* })
            }
            (syn::Fields::Unnamed(_), Some(tag)) => {
                quote!(#variant_struct_name(#tag, #(#arg_values),*))
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
                payload: core::mem::MaybeUninit::new(#payload_name { #variant_name: #body }),
            })
        } else {
            quote!(Self { #variant_name: #body })
        };

        quote! {
            #[expect(non_snake_case)]
            pub const fn #variant_name(#(#arg_names: #arg_types),*) -> Self {
                #result
            }
        }
    });

    quote! {
        impl #impl_generics #ctype_name #ty_generics #where_clause {
            #tag_accessor
            #raw_constructor
            #niche_check
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
    let field_types = variants
        .iter()
        .flat_map(|variant| {
            variant
                .fields
                .iter()
                .map(|field| lowered_field_ty(&field.ty))
        })
        .collect::<Vec<_>>();
    let bounds = field_types
        .iter()
        .map(|ty| quote!(for<'_dummy> #ty: core::cmp::PartialEq));
    let tags = super::enum_variant_tag_exprs(variants, tag_type);
    let arms = variants.iter().zip(tags).map(|(variant, tag)| {
        let variant_member = &variant.ident;
        let payload_members = variant
            .fields
            .members()
            .map(|member| match member {
                syn::Member::Unnamed(mut index) => {
                    index.index += 1;
                    syn::Member::Unnamed(index)
                }
                other => other,
            })
            .collect::<Vec<_>>();
        let comparisons = payload_members.iter().map(|member| {
            quote! {
                unsafe {
                    // Generated constructors initialize fields for active tags.
                    self.#variant_member.#member.assume_init_ref()
                        == other.#variant_member.#member.assume_init_ref()
                }
            }
        });
        quote!(value if value == #tag => true #(&& #comparisons)*)
    });

    quote! {
        impl #impl_generics core::cmp::PartialEq for #name #ty_generics
        where #(#bounds,)* #predicates
        {
            fn eq(&self, other: &Self) -> bool {
                let self_tag = self.tag();
                let other_tag = other.tag();
                self_tag == other_tag && match self_tag {
                    #(#arms,)*
                    _ => false,
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
    generate_spec: bool,
    generate_wide: bool,
    recursion: &RecursiveFields,
) -> TokenStream {
    let carriers = &recursion.carriers;
    // The fixed header shares the already verified prefix of the enclosing companion.
    let stored_carriers = recursion.header_prefix.unwrap_or(carriers);
    let ctype_def = gen_ctype_struct_item(
        repr,
        alignment,
        vis,
        name,
        generics,
        fields,
        stored_carriers,
    );
    let recursive_checks =
        gen_recursive_carrier_checks(name, &ctype_def.generics, fields, recursion);
    let known_unsized_tail = fields
        .iter()
        .next_back()
        .is_some_and(|field| classify_tail(&field.ty, generics).is_known_unsized());
    let fields_methods = gen_fields_methods(
        &ctype_def.ident,
        &ctype_def.generics,
        &ctype_def.fields,
        known_unsized_tail,
        false,
    );
    let ctype_impls = gen_struct_ctype_impls::<true>(
        &ctype_def,
        generate_spec,
        alignment,
        known_unsized_tail,
        carriers,
    );
    let wide_impl = generate_wide.then(|| {
        gen_ctype_wide_impl(
            repr,
            name,
            &ctype_def,
            fields,
            generics,
            recursion.wide_header,
        )
    });
    let header_impl =
        (!generate_spec).then(|| gen_identity_header_impl(&ctype_def.ident, &ctype_def.generics));

    quote! {
        #ctype_def
        #recursive_checks
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
    wrap_payload: bool,
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
    let arg_types = args.iter().map(|(_, ty)| *ty).collect::<Vec<_>>();
    let (last_field_sized_bound, omit_methods) = c_fn_last_field_bounds(&arg_types, generics);
    if known_unsized_tail || omit_methods {
        return quote! {};
    }
    let members = fields.members().collect::<Vec<_>>();
    let values = arg_names.iter().enumerate().map(|(index, name)| {
        if wrap_payload && index > 0 {
            let label = matches!(fields, syn::Fields::Named(_)).then(|| quote!(#name:));
            quote!(#label core::mem::MaybeUninit::new(#name))
        } else {
            quote!(#name)
        }
    });
    let body = match fields {
        syn::Fields::Named(_) => quote!(Self { #(#values),* }),
        syn::Fields::Unnamed(_) => quote!(Self(#(#values),*)),
        syn::Fields::Unit => quote!(Self),
    };
    let read_fields = members
        .iter()
        .enumerate()
        .map(|(index, member)| {
            if wrap_payload && index > 0 {
                quote!(unsafe { core::ptr::read(&this.#member).assume_init() })
            } else {
                quote!(unsafe { core::ptr::read(&this.#member) })
            }
        })
        .collect::<Vec<_>>();
    let into_body = if fields.is_empty() {
        quote! { let _this = core::mem::ManuallyDrop::new(self); }
    } else {
        quote! {
            let this = core::mem::ManuallyDrop::new(self);
            ( #(#read_fields,)* )
        }
    };
    let into_fields_safety = wrap_payload.then(|| quote!(unsafe));
    let into_fields_safety_doc = wrap_payload.then(|| {
        quote! {
            ///
            /// # Safety
            /// All payload fields must be initialized before this method is called.
        }
    });

    quote! {
        impl #impl_generics #name #ty_generics #where_clause {
            /// Construct this generated struct from its fields.
            ///
            /// # Safety
            ///
            /// The caller must uphold the validity, ownership, and aliasing
            /// requirements of this generated struct.
            #[allow(clippy::too_many_arguments, reason = "constructor arity follows the source fields")]
            pub unsafe fn from_fields(#(#arg_names: #arg_types),*) -> Self
            where
                #last_field_sized_bound
            {
                #body
            }

            /// Consume this generated struct and return its fields in declaration order.
            #into_fields_safety_doc
            pub #into_fields_safety fn into_fields(self) -> (#(#arg_types,)*)
            where
                #last_field_sized_bound
            {
                #into_body
            }
        }
    }
}

fn gen_recursive_carrier_checks(
    name: &syn::Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
    recursion: &RecursiveFields,
) -> TokenStream {
    // Type-check each shortcut against ReprC after defining the companion.
    // This also checks imports that resolve to a different supported wrapper.
    let (impl_generics, args, where_clause) = generics.split_for_impl();
    let requirements = &recursion.source_bounds;
    let predicates = where_clause.map(|w| &w.predicates);
    let checks = fields
        .iter()
        .zip(&recursion.carriers)
        .filter_map(|(source, inferred)| {
            let ty = &source.ty;
            let carrier = inferred.as_ref()?;
            Some(quote!(check::<#ty, #carrier>();))
        })
        .collect::<Vec<_>>();
    let vector_selector = recursion.selectors.vec.then(|| {
        quote! {
            extern crate alloc as __co3_recursive_alloc;
            pub trait __Co3SelectVec {
                type Niche;
                type Carrier<C: co3::CType>;
                type AbiRequirement;
            }
            impl<T> __Co3SelectVec for __co3_recursive_alloc::vec::Vec<T> {
                type Niche = co3::rust_spec::niche::WithNiche<__Co3Unstable>;
                type Carrier<C: co3::CType> = __Co3Native<co3::vec::abi::Vec<C>>;
                type AbiRequirement = Self;
            }
            impl<T> __Co3SelectVec for co3::vec::Vec<T> {
                type Niche = co3::rust_spec::niche::WithNiche<__Co3Unstable>;
                type Carrier<C: co3::CType> = co3::vec::abi::Vec<C>;
                type AbiRequirement = ();
            }
        }
    });
    let option_selector = recursion.selectors.option.then(|| quote! {
        pub trait __Co3SelectOption<Niche> {
            type Carrier<Tagged, NativeTagged, NativeSome>;
        }
        impl __Co3SelectOption<co3::rust_spec::niche::WithoutNiche> for core::option::Option<()> {
            type Carrier<T, N, S> = N;
        }
        impl<L: co3::rust_spec::layout::LayoutKind> __Co3SelectOption<co3::rust_spec::niche::WithNiche<L>> for core::option::Option<()> {
            type Carrier<T, N, S> = S;
        }
        impl<Niche> __Co3SelectOption<Niche> for co3::option::Option<()> {
            type Carrier<T, N, S> = T;
        }
    });
    let reference_selector = recursion.selectors.reference.then(|| quote! {
        pub trait __Co3SelectRef<Direct, Soft> { type Carrier; }
        impl<D, S> __Co3SelectRef<D, S> for co3::rust_spec::Specified { type Carrier = D; }
        impl<D, S> __Co3SelectRef<D, S> for __Co3Unstable { type Carrier = S; }
        impl<D, S> __Co3SelectRef<D, S> for (co3::rust_spec::Specified, co3::rust_spec::layout::Robust) { type Carrier = D; }
        impl<D, S> __Co3SelectRef<D, S> for (co3::rust_spec::Specified, co3::rust_spec::layout::NonRobust) { type Carrier = S; }
        impl<D, S, Trap> __Co3SelectRef<D, S> for (__Co3Unstable, Trap) { type Carrier = S; }
    });
    let check_carriers = (!checks.is_empty()).then(|| {
        quote! {
            const _: () = {
                #[allow(dead_code, reason = "generated helper usage depends on the derived type")]
                trait CheckRecursiveCarriers { fn check(); }
                impl #impl_generics CheckRecursiveCarriers for #name #args where #(#requirements,)* #predicates {
                    fn check() {
                        fn check<R: co3::ReprC<CType = C> + ?Sized, C: co3::CType + ?Sized>() {}
                        #(#checks)*
                    }
                }
            };
        }
    });
    quote!(#option_selector #vector_selector #reference_selector #check_carriers)
}

fn gen_ctype_wide_impl(
    repr: Option<&ReprKind>,
    source_name: &syn::Ident,
    ctype: &syn::ItemStruct,
    source_fields: &syn::Fields,
    source_generics: &syn::Generics,
    recursive: bool,
) -> TokenStream {
    let Some(source_field_ty) = wide_tail_type(source_fields, repr, source_generics) else {
        return quote! {};
    };
    let is_transparent = matches!(repr, Some(ReprKind::Transparent));

    let Some((field, ..)) = last_field(&ctype.fields) else {
        return quote! {};
    };

    let field_ty = &field.ty;
    let name = &ctype.ident;

    let (impl_generics, ty_generics, where_clause) = ctype.generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let data_name = gen_data_struct_name(source_name);

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
    let source_header_predicate = header_repr_c_predicate(source_field_ty, source_generics);
    let for_dummy = speculative_marker(ctype.generics.params.is_empty());

    let data_ty = if is_transparent {
        quote!(<<#source_field_ty as co3::wide::Wide>::Header as co3::ReprC>::CType)
    } else if recursive {
        let header = gen_ctype_name(&data_name);
        quote!(#header #ty_generics)
    } else {
        quote!(<#data_name #ty_generics as co3::ReprC>::CType)
    };
    let transparent_header_predicate =
        is_transparent.then(|| quote!(#for_dummy #data_ty: co3::wide::WideHeader,));

    quote! {
        unsafe impl #impl_generics co3::wide::Wide for #name #ty_generics
        where
            #source_header_predicate,
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

    let payload_impls = gen_union_ctype_impls(&payload_def, None);
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
            payload: core::mem::MaybeUninit<#payload_name #ty_generics>,
        }
    };

    let ctype_impls = gen_struct_ctype_impls::<false>(&ctype_def, true, alignment, false, &[]);
    let fields_methods = gen_fields_methods(
        &ctype_def.ident,
        &ctype_def.generics,
        &ctype_def.fields,
        false,
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
    let fields = payload
        .fields
        .named
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();
    let bounds = gen_partial_eq_bounds(&fields);
    let tag_values = super::enum_variant_tag_exprs(variants, tag_type);
    let arms = variants.iter().enumerate().map(|(index, variant)| {
        let tag = &tag_values[index];
        let member = &variant.ident;
        quote!(value if value == #tag => unsafe { self.payload.assume_init_ref().#member == other.payload.assume_init_ref().#member })
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
                    _ => false,
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
    carriers: &[Option<syn::Type>],
) -> syn::ItemStruct {
    let types = fields.iter().map(|f| &f.ty).collect::<Vec<_>>();
    let bounds = projected_field_bounds(generics, &types, carriers);
    let mut fields = match fields {
        syn::Fields::Unit => syn::Fields::Named(syn::FieldsNamed {
            brace_token: Default::default(),
            named: Default::default(),
        }),
        fields => fields.clone(),
    };
    for (i, field) in fields.iter_mut().enumerate() {
        field.vis = syn::Visibility::Inherited;
        field.attrs.clear();
        field.modifiers = Default::default();
        field.ty = carriers
            .get(i)
            .and_then(Clone::clone)
            .unwrap_or_else(|| lowered_field_ty(&field.ty));
    }
    syn::ItemStruct {
        attrs: vec![
            parse_quote!(#[doc(hidden)]),
            gen_ctype_repr_attr(repr, alignment),
        ],
        vis: vis.clone(),
        struct_token: Default::default(),
        ident: gen_ctype_name(name),
        generics: carrier_generics(generics, bounds),
        semi_token: matches!(fields, syn::Fields::Unnamed(_)).then(Default::default),
        fields,
    }
}

/// Carrier declarations use the impl parameters, which omit source defaults.
fn carrier_generics(source: &syn::Generics, bounds: Vec<syn::WherePredicate>) -> syn::Generics {
    let mut generics = source.clone();
    for param in generics.type_params_mut() {
        param.default = None;
    }
    for param in generics.const_params_mut() {
        param.default = None;
    }
    let predicates = &mut generics.make_where_clause().predicates;
    *predicates = bounds
        .into_iter()
        .chain(std::mem::take(predicates))
        .collect();
    generics
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

    let bounds = gen_union_extern_c_bounds(enum_name, generics, variants);
    let union_def = syn::ItemUnion {
        attrs: vec![
            parse_quote!(#[expect(non_snake_case)]),
            parse_quote!(#[doc(hidden)]),
            gen_ctype_repr_attr(Some(&ReprKind::C(None)), alignment),
        ],
        vis: vis.clone(),
        union_token: Default::default(),
        ident: union_name.clone(),
        generics: carrier_generics(generics, bounds),
        fields: union_fields,
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

    let mut ctype = gen_ctype_struct_item(repr, None, vis, &name, &variant_generics, &fields, &[]);
    let unwrapped = ctype.clone();
    if tag_type.is_some() {
        for field in ctype.fields.iter_mut().skip(1) {
            let ty = &field.ty;
            field.ty = parse_quote!(core::mem::MaybeUninit<#ty>);
        }
    }
    let fields_methods = gen_fields_methods(
        &unwrapped.ident,
        &unwrapped.generics,
        &unwrapped.fields,
        false,
        tag_type.is_some(),
    );
    let ctype_impls = if tag_type.is_some() {
        gen_struct_ctype_impls::<false>(&ctype, true, None, false, &[])
    } else {
        gen_struct_ctype_impls::<true>(&ctype, true, None, false, &[])
    };
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
    field_types(fields, generics)
        .flat_map(|field| gen_field_ctype_bounds(field, generics, quote!(co3::ReprC<CType: Sized>)))
        .collect()
}

fn gen_struct_ctype_impls<const GEN_PARTIAL_EQ: bool>(
    ctype: &syn::ItemStruct,
    generate_spec: bool,
    alignment: Option<&syn::LitInt>,
    known_unsized_tail: bool,
    carriers: &[Option<syn::Type>],
) -> TokenStream {
    let fields = ctype.fields.iter().map(|f| &f.ty).collect::<Vec<_>>();
    let copy_impls = gen_copy_impls(&ctype.ident, &ctype.generics, &fields, known_unsized_tail);
    let partial_eq_impl = GEN_PARTIAL_EQ.then(|| gen_struct_partial_eq(ctype));
    let robust_impls = gen_robust_impls(&ctype.ident, &ctype.generics, &fields, known_unsized_tail);
    let type_spec_impl = generate_spec
        .then(|| gen_type_spec(&ctype.ident, &ctype.generics, &fields, alignment, carriers));
    quote! {
        #copy_impls
        #partial_eq_impl
        #robust_impls
        #type_spec_impl

    }
}

fn gen_partial_eq_bounds(fields: &[&syn::Type]) -> Vec<TokenStream> {
    fields
        .iter()
        .map(|ty| quote!(for<'_dummy> #ty: core::cmp::PartialEq))
        .collect()
}

fn gen_struct_partial_eq(ctype: &syn::ItemStruct) -> TokenStream {
    let name = &ctype.ident;
    let (impl_generics, ty_generics, where_clause) = ctype.generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|clause| &clause.predicates);
    let fields = ctype
        .fields
        .iter()
        .map(|field| &field.ty)
        .collect::<Vec<_>>();
    let bounds = gen_partial_eq_bounds(&fields);
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

fn gen_union_ctype_impls(ctype: &syn::ItemUnion, alignment: Option<&syn::LitInt>) -> TokenStream {
    let fields = ctype.fields.named.iter().map(|f| &f.ty).collect::<Vec<_>>();

    let copy_impls = gen_copy_impls(&ctype.ident, &ctype.generics, &fields, false);
    let robust_impls = gen_robust_impls(&ctype.ident, &ctype.generics, &fields, false);
    let type_spec_impl = gen_type_spec(&ctype.ident, &ctype.generics, &fields, alignment, &[]);
    quote! {
        #copy_impls
        #robust_impls
        #type_spec_impl

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
            #speculative Self: __Co3CFnReturnLayout,
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
    let borrow_impls = (!known_unsized_tail).then(|| gen_identity_borrow_impls(ident, generics));
    let codec_impls = gen_identity_codec_impls(ident, generics, true);
    let c_fn_impls = if known_unsized_tail {
        quote! {}
    } else {
        gen_c_fn_impls(ident, generics, fields)
    };

    let validator = crate::codec::validator(ident, generics, quote!(true));
    quote! {
        unsafe impl #impl_generics #co3::CType for #ident #ty_generics #where_clause {}

        #c_fn_impls

        unsafe impl #impl_generics #co3::marker::AbiCompatible for #ident #ty_generics #where_clause {}
        #validator

        #borrow_impls
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
    carriers: &[Option<syn::Type>],
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    // Only explicit recursive lowerings have a known pointer-bearing representation.
    // Tagged Option preserves its payload alignment, and wide carriers here use usize metadata.
    let inferred = |i: usize| carriers.get(i).is_some_and(Option::is_some);
    let field_family = |operator, axis, identity, recursive_axis| {
        gen_field_family(generics, fields, operator, axis, identity, |i| {
            inferred(i).then_some(&recursive_axis)
        })
    };
    let (size, size_bounds) = field_family(
        quote!(core::ops::Add),
        quote!(Size),
        quote!(co3::rust_spec::size::Sized<co3::rust_spec::Zero>),
        quote!(co3::rust_spec::size::Sized<co3::rust_spec::Gt<co3::rust_spec::Zero>>),
    );
    let (mut alignment, alignment_bounds) = field_family(
        quote!(co3::rust_spec::Max),
        quote!(__Alignment),
        quote!(co3::rust_spec::One),
        quote!(<core::primitive::usize as co3::rust_spec::RustSpec>::__Alignment),
    );
    if repr_alignment.is_some_and(|alignment| alignment.base10_digits() != "1") {
        alignment = quote!(co3::rust_spec::Gt<co3::rust_spec::One>);
    }
    // Following an edge back to this record reaches the same ordinary fields.
    // The recognized pointer carriers add no interior-mutable storage of their own.
    let (mutability, mutability_bounds) = field_family(
        quote!(core::ops::Add),
        quote!(Mutability),
        quote!(co3::rust_spec::mutability::Exclusive<co3::rust_spec::mutability::WithoutInterior>),
        quote!(co3::rust_spec::mutability::Exclusive<co3::rust_spec::mutability::WithoutInterior>),
    );
    let rust_spec_bounds = field_types(fields, generics)
        .enumerate()
        .filter(|(i, field)| !inferred(*i) && field.concrete)
        .flat_map(|(_, field)| {
            gen_hrtb_projection_bounds(field.ty, generics, quote! { co3::rust_spec::RustSpec })
        });

    quote! {
        unsafe impl #impl_generics co3::rust_spec::RustSpec for #ident #ty_generics
        where
            #(#rust_spec_bounds,)*
            #(#size_bounds,)*
            #(#alignment_bounds,)*
            #(#mutability_bounds,)*
            #predicates
        {
            type Layout = co3::rust_spec::Specified;
            type Size = #size;
            type __Alignment = #alignment;
            type Trap = co3::rust_spec::layout::Robust;
            type Niche = co3::rust_spec::niche::WithoutNiche;
            type Mutability = #mutability;
            type Drop = co3::rust_spec::drop::NoDrop;
            type Opacity = co3::rust_spec::NonOpaque;
            type __IndirectTrap = co3::rust_spec::layout::Robust;
        }
    }
}

fn gen_field_family<'a>(
    generics: &syn::Generics,
    fields: &[&syn::Type],
    operator: TokenStream,
    axis: TokenStream,
    identity: TokenStream,
    inferred: impl Fn(usize) -> Option<&'a TokenStream>,
) -> (TokenStream, Vec<TokenStream>) {
    let mut bounds = Vec::new();
    let mut concrete = Vec::new();
    let mut generic = Vec::new();
    for (index, field) in field_types(fields, generics).enumerate() {
        let ty = field.ty;
        let projection = quote!(<#ty as co3::rust_spec::RustSpec>::#axis);
        if let Some(known) = inferred(index) {
            concrete.push(known.clone());
        } else if field.concrete || field.phantom {
            // Marker layout does not depend on its type argument. Keeping this
            // known avoids shadowing its fixed RustSpec axes with generic bounds.
            concrete.push(projection);
        } else {
            bounds.push(quote!(#ty: co3::rust_spec::RustSpec));
            generic.push(projection);
        }
    }
    let mut combine = |left: TokenStream, right: TokenStream, needs_bound: bool| {
        if needs_bound {
            bounds.push(quote!(#left: #operator<#right>));
        }
        quote!(<#left as #operator<#right>>::Output)
    };
    let concrete = concrete.into_iter().reduce(|a, b| combine(a, b, false));
    let generic = generic.into_iter().reduce(|a, b| combine(a, b, true));
    let family = match (concrete, generic) {
        (Some(a), Some(b)) => combine(a, b, true),
        (Some(a), None) | (None, Some(a)) => a,
        (None, None) => identity,
    };
    (family, bounds)
}

fn gen_identity_codec_impls(
    ident: &syn::Ident,
    generics: &syn::Generics,
    needs_sized_bound: bool,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let mut g = generics.clone();
    if needs_sized_bound {
        let speculative = speculative_marker(g.params.is_empty());
        g.make_where_clause()
            .predicates
            .push(parse_quote!(#speculative Self: Sized));
    }
    let values = crate::codec::stateless(
        ident,
        &g,
        &g,
        quote!(self),
        quote!(Some(source)),
        quote!(source),
    );
    quote! {
        impl #impl_generics co3::ReprC for #ident #ty_generics #where_clause { type CType = Self; }
        #values
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

    let validator = crate::codec::validator(ident, generics, quote!(true));
    quote! {
        unsafe impl #impl_generics co3::CType for #ident #ty_generics #where_clause {}

        #c_fn_impls

        unsafe impl #impl_generics co3::marker::AbiCompatible for #ident #ty_generics
        #where_clause
        {}
        #validator

        #codec_impls
    }
}

fn gen_ctype_repr_attr(repr: Option<&ReprKind>, alignment: Option<&syn::LitInt>) -> syn::Attribute {
    let kind = if repr == Some(&ReprKind::Transparent) {
        quote!(transparent)
    } else {
        quote!(C)
    };
    let alignment = alignment.map(|alignment| quote!(, align(#alignment)));
    parse_quote! { #[repr(#kind #alignment)] }
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
        || fields
            .last()
            .is_some_and(|field| classify_tail(field, generics).is_known_unsized())
    {
        return quote! {};
    }

    let copy_bounds = gen_copy_bounds(fields);

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

fn gen_copy_bounds(fields: &[&syn::Type]) -> Vec<TokenStream> {
    fields
        .iter()
        .map(|ty| quote!(for<'_dummy> #ty: Copy))
        .collect()
}

pub(super) fn gen_ctype_name(item_name: &syn::Ident) -> syn::Ident {
    format_ident!("{item_name}CType")
}

pub(super) fn gen_variant_struct_name(
    enum_name: &syn::Ident,
    variant_name: &syn::Ident,
) -> syn::Ident {
    format_ident!("{enum_name}{variant_name}CType")
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
    projected_field_bounds(generics, fields, &[])
}

pub(super) fn projected_field_bounds(
    generics: &syn::Generics,
    fields: &[&syn::Type],
    carriers: &[Option<syn::Type>],
) -> Vec<syn::WherePredicate> {
    field_types(fields, generics)
        .enumerate()
        .filter(|(i, _)| carriers.get(*i).is_none_or(Option::is_none))
        .flat_map(|(i, field)| {
            let bound = if i + 1 == fields.len() {
                quote!(co3::ReprC)
            } else {
                quote!(co3::ReprC<CType: Sized>)
            };
            gen_field_ctype_bounds(field, generics, bound)
        })
        .collect()
}

pub(super) fn gen_copy_extern_c_bounds_for_ctype(
    generics: &syn::Generics,
    fields: &[&syn::Type],
) -> Vec<syn::WherePredicate> {
    field_types(fields, generics)
        .flat_map(|field| gen_field_ctype_bounds(field, generics, quote!(co3::ReprC<CType: Copy>)))
        .collect()
}

fn gen_field_ctype_bounds(
    field: FieldType<'_>,
    generics: &syn::Generics,
    bound: TokenStream,
) -> Vec<syn::WherePredicate> {
    if field.phantom {
        let known = crate::known_type::recognize(field.ty).unwrap();
        super::fields::phantom_mapping_bound(known.inner.unwrap(), generics)
            .into_iter()
            .collect()
    } else if field.concrete {
        gen_hrtb_projection_bounds(field.ty, generics, bound)
    } else {
        vec![field.bound(bound)]
    }
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
            if bounded_ty != projected_ty.as_ref() {
                continue;
            }

            for prerequisite in &predicate.bounds {
                let syn::TypeParamBound::Trait(trait_bound) = prerequisite else {
                    continue;
                };
                let predicate_trait_path = &trait_bound.path;
                if predicate_trait_path != &trait_path {
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

#[cfg(test)]
mod tests {
    #[test]
    fn recursive_carrier_properties_do_not_require_the_companion() {
        let generics = syn::parse_quote!(<T>);
        let ordinary = syn::parse_quote!(T);
        let recursive_fields: Vec<syn::Type> = vec![
            syn::parse_quote!(*const NodeCType<T>),
            syn::parse_quote!(*mut NodeCType<T>),
            syn::parse_quote!(co3::boxed::abi::Box<NodeCType<T>>),
            syn::parse_quote!(co3::wide::abi::WideBox<NodeCType<T>>),
            syn::parse_quote!(co3::marker::NonZero<*mut NodeCType<T>>),
            syn::parse_quote!(co3::vec::abi::Vec<NodeCType<T>>),
            syn::parse_quote!(
                __Co3Native<co3::option::abi::OptionSome<co3::boxed::abi::Box<NodeCType<T>>>>
            ),
        ];
        for recursive in recursive_fields {
            let implementation: syn::ItemImpl = syn::parse2(super::gen_type_spec(
                &syn::parse_quote!(NodeCType),
                &generics,
                &[&ordinary, &recursive],
                None,
                &[None, Some(recursive.clone())],
            ))
            .unwrap();
            let predicates = implementation.generics.where_clause.unwrap().predicates;
            assert!(!quote::quote!(#predicates).to_string().contains("NodeCType"));
        }
    }

    #[test]
    fn recursive_mutability_is_added_to_other_fields() {
        let ident = syn::parse_quote!(NodeCType);
        let generics = syn::Generics::default();
        let recursive = syn::parse_quote!(co3::boxed::abi::Box<NodeCType>);
        let cell = syn::parse_quote!(<core::cell::UnsafeCell<u32> as co3::ReprC>::CType);
        let implementation: syn::ItemImpl = syn::parse2(super::gen_type_spec(
            &ident,
            &generics,
            &[&recursive, &cell],
            None,
            &[Some(recursive.clone()), None],
        ))
        .unwrap();
        let mutability = implementation
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Type(item) if item.ident == "Mutability" => Some(&item.ty),
                _ => None,
            })
            .unwrap();
        let expected: syn::Type = syn::parse_quote!(
            <co3::rust_spec::mutability::Exclusive<co3::rust_spec::mutability::WithoutInterior>
                as core::ops::Add<
                    <<core::cell::UnsafeCell<u32> as co3::ReprC>::CType
                        as co3::rust_spec::RustSpec>::Mutability
                >>::Output
        );
        assert_eq!(mutability, &expected);
    }

    #[test]
    fn recursive_references_select_by_layout_and_mutable_references_also_by_trap() {
        let own = syn::parse_quote!(Node<'a>);
        let companion = syn::parse_quote!(NodeCType<'a>);
        let generics = syn::parse_quote!(<'a>);
        let lower = |ty, tail| {
            crate::layout::inference::recursive_reference_carrier(
                &ty, &own, &companion, tail, &generics,
            )
        };
        let shared = syn::parse_quote!(&'a Node<'a>);
        let exclusive = syn::parse_quote!(&'a mut Node<'a>);
        let layout = quote::quote!(<Node<'a> as co3::rust_spec::RustSpec>::Layout);
        let trap = quote::quote!(<Node<'a> as co3::rust_spec::RustSpec>::Trap);
        assert_eq!(
            lower(shared, TailClass::Sized),
            Some(syn::parse_quote!(
                <#layout as __Co3SelectRef<co3::reference::abi::Ref<'a, NodeCType<'a>>,
                    co3::reference::abi::SoftRef<NodeCType<'a>>>>::Carrier
            ))
        );
        assert_eq!(
            lower(exclusive, TailClass::Unsized(UnsizedTail::SliceLike)),
            Some(syn::parse_quote!(
                <(#layout, #trap) as __Co3SelectRef<co3::wide::abi::WideRefMut<'a, NodeCType<'a>>,
                    co3::wide::abi::SoftWideRefMut<NodeCType<'a>>>>::Carrier
            ))
        );
        assert_eq!(
            lower(syn::parse_quote!(&'a mut Self), TailClass::Unknown),
            None
        );
    }

    #[test]
    fn recursive_box_carrier_uses_the_record_tail() {
        let ty = syn::parse_quote!(Box<Node>);
        let lower = |tail| {
            crate::layout::inference::recursive_carrier(
                &ty,
                &syn::parse_quote!(Node),
                &syn::parse_quote!(NodeCType),
                tail,
            )
        };
        assert_eq!(
            lower(TailClass::Sized),
            Some(syn::parse_quote!(co3::boxed::abi::Box<NodeCType>))
        );
        assert_eq!(
            lower(TailClass::Unsized(UnsizedTail::SliceLike)),
            Some(syn::parse_quote!(co3::wide::abi::WideBox<NodeCType>))
        );
        assert_eq!(lower(TailClass::Unknown), None);
    }

    #[test]
    fn recursive_lowering_preserves_pointer_kind_and_falls_back_on_ambiguity() {
        let own = syn::parse_quote!(Node<T>);
        let companion = syn::parse_quote!(NodeCType<T>);
        let lower = |ty| {
            crate::layout::inference::recursive_carrier(&ty, &own, &companion, TailClass::Sized)
        };
        assert_eq!(
            lower(syn::parse_quote!(*const Node<T>)),
            Some(syn::parse_quote!(*const NodeCType<T>))
        );
        assert_eq!(
            lower(syn::parse_quote!(core::ptr::NonNull<Node<T>>)),
            Some(syn::parse_quote!(co3::marker::NonZero<*mut NodeCType<T>>))
        );
        let payload = quote::quote!(co3::boxed::abi::Box<NodeCType<T>>);
        let tagged = quote::quote!(co3::option::abi::Option<#payload>);
        assert_eq!(
            lower(syn::parse_quote!(std::option::Option<Box<Node<T>>>)),
            Some(syn::parse_quote!(
                <std::option::Option<()> as __Co3SelectOption<co3::rust_spec::niche::WithNiche<co3::rust_spec::Specified>>>::Carrier<
                    #tagged, __Co3Native<#tagged>, __Co3Native<co3::option::abi::OptionSome<#payload>>
                >
            ))
        );
        assert_eq!(lower(syn::parse_quote!(Box<Node<u8>>)), None);
        assert_eq!(lower(syn::parse_quote!(&Node<T>)), None);
        assert_eq!(
            lower(syn::parse_quote!(Vec<Node<T>>)),
            Some(syn::parse_quote!(
                <Vec<()> as __Co3SelectVec>::Carrier<NodeCType<T>>
            ))
        );
    }

    use syn::parse_quote;

    use super::{TailClass, UnsizedTail, classify_tail};

    #[test]
    fn copy_and_partial_eq_bounds_are_speculative() {
        for types in [
            vec![],
            vec![parse_quote!(Opaque)],
            vec![parse_quote!(Opaque), parse_quote!(u8)],
            vec![parse_quote!(u8), parse_quote!(T)],
            vec![parse_quote!(u8), parse_quote!(&'a u8)],
            vec![parse_quote!(u8), parse_quote!([u8; N])],
        ] {
            let refs = types.iter().collect::<Vec<_>>();
            let copy = super::gen_copy_bounds(&refs);
            let eq = super::gen_partial_eq_bounds(&refs);
            for bounds in [copy, eq] {
                let actual = bounds
                    .into_iter()
                    .map(|bound| {
                        let syn::WherePredicate::Type(predicate) = syn::parse2(bound).unwrap()
                        else {
                            unreachable!()
                        };
                        predicate.lifetimes.is_some()
                    })
                    .collect::<Vec<_>>();
                assert_eq!(actual, vec![true; refs.len()]);
            }
        }
    }

    #[test]
    fn tuple_classification_follows_the_final_element() {
        let generics = parse_quote!(<T: ?Sized, S>);
        for ty in [
            parse_quote!(()),
            parse_quote!((S,)),
            parse_quote!((u8, *const T)),
        ] {
            assert!(matches!(classify_tail(&ty, &generics), TailClass::Sized));
        }
        for ty in [parse_quote!((T,)), parse_quote!((u8, (T,)))] {
            assert!(matches!(
                classify_tail(&ty, &generics),
                TailClass::Unsized(UnsizedTail::Unknown)
            ));
        }
        assert!(matches!(
            classify_tail(&parse_quote!((u8, [S])), &generics),
            TailClass::Unsized(UnsizedTail::SliceLike)
        ));
        assert!(matches!(
            classify_tail(&parse_quote!((u8, dyn Send)), &generics),
            TailClass::Unsized(UnsizedTail::DynTraitLike)
        ));
        assert!(matches!(
            classify_tail(&parse_quote!((u8, core::ffi::CStr)), &generics),
            TailClass::Unsized(UnsizedTail::NulTerminated)
        ));
        assert!(matches!(
            classify_tail(&parse_quote!((u8, SomeAlias)), &generics),
            TailClass::Unknown
        ));
    }

    #[test]
    fn classifies_known_and_speculative_tails() {
        let generics = parse_quote!(<T: ?Sized>);

        assert!(matches!(
            classify_tail(&parse_quote!(u32), &generics),
            TailClass::Sized
        ));
        assert!(matches!(
            classify_tail(&parse_quote!(core::marker::PhantomData<T>), &generics),
            TailClass::Sized
        ));
        assert!(matches!(
            classify_tail(&parse_quote!([u8]), &generics),
            TailClass::Unsized(UnsizedTail::SliceLike)
        ));
        assert!(matches!(
            classify_tail(&parse_quote!(str), &generics),
            TailClass::Unsized(UnsizedTail::SliceLike)
        ));
        assert!(matches!(
            classify_tail(&parse_quote!(core::ffi::CStr), &generics),
            TailClass::Unsized(UnsizedTail::NulTerminated)
        ));
        assert!(matches!(
            classify_tail(&parse_quote!(dyn Send), &generics),
            TailClass::Unsized(UnsizedTail::DynTraitLike)
        ));
        assert!(matches!(
            classify_tail(&parse_quote!(T), &generics),
            TailClass::Unsized(UnsizedTail::Unknown)
        ));

        let sized_generics = parse_quote!(<S>);
        assert!(matches!(
            classify_tail(&parse_quote!(S), &sized_generics),
            TailClass::Sized
        ));
        let shadowed = parse_quote!(<CStr>);
        assert!(matches!(
            classify_tail(&parse_quote!(CStr), &shadowed),
            TailClass::Sized
        ));
        assert!(matches!(
            classify_tail(&parse_quote!(SomeUserType), &sized_generics),
            TailClass::Unknown
        ));

        let explicitly_sized = parse_quote!(<S: Sized>);
        assert!(matches!(
            classify_tail(&parse_quote!(S), &explicitly_sized),
            TailClass::Sized
        ));

        let where_unsized: syn::DeriveInput = parse_quote! {
            struct WhereUnsized<U> where U: ?Sized { tail: U }
        };
        assert!(matches!(
            classify_tail(&parse_quote!(U), &where_unsized.generics),
            TailClass::Unsized(UnsizedTail::Unknown)
        ));
        assert!(matches!(
            classify_tail(&parse_quote!((U)), &where_unsized.generics),
            TailClass::Unsized(UnsizedTail::Unknown)
        ));
    }
}
