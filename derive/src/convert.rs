use core::str::FromStr as _;
use std::fmt::{Display, Formatter};

use darling::{
    FromAttributes, FromDeriveInput, FromField, FromVariant,
    ast::{Fields, Style},
    util::SpannedValue,
};
use manyhow::emit;
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    Attribute, Field, Ident, ext::IdentExt as _, parse::ParseStream, parse_quote,
    spanned::Spanned as _, visit::Visit as _,
};

#[cfg(feature = "getset")]
use crate::attr_parse::getset::{DocAttrs, GetSetFieldAttrs, GetSetStructAttrs};
use crate::{
    attr_parse::{
        derive::DeriveAttrs,
        repr::{Repr, ReprKind, ReprPrimitive},
    },
    emitter::Emitter,
};

#[derive(Debug)]
enum FfiTypeToken {
    Transparent(Option<syn::Expr>, syn::ExprClosure),
    UnsafeNonOwning,
    Opaque,
    Local,
}

impl Display for FfiTypeToken {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            FfiTypeToken::UnsafeNonOwning => write!(f, "#[mineral(unsafe(non_owning))]"),
            FfiTypeToken::Opaque => write!(f, "#[mineral(opaque)]"),
            FfiTypeToken::Local => write!(f, "#[mineral(local)]"),
            FfiTypeToken::Transparent(niche, is_valid) => {
                write!(f, "#[mineral(")?;
                if let Some(niche) = niche {
                    write!(f, "NICHE_VALUE = {}, ", quote!(#niche))?;
                }
                write!(f, "unsafe(is_valid = {}))]", quote!(#is_valid))
            }
        }
    }
}

#[derive(Debug)]
struct SpannedFfiTypeToken {
    span: Span,
    token: FfiTypeToken,
}

impl syn::parse::Parse for SpannedFfiTypeToken {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        fn join_span(span: &mut Option<Span>, new_span: Span) {
            *span = Some(match *span {
                Some(existing) => existing.join(new_span).unwrap_or(existing),
                None => new_span,
            });
        }

        let mut span: Option<Span> = None;
        let mut niche_value = None;
        let mut is_valid = None;
        let mut token = None;

        while !input.is_empty() {
            let ident: Ident = input.call(Ident::parse_any)?;
            join_span(&mut span, ident.span());

            match ident.to_string().as_str() {
                "opaque" => {
                    token = Some(FfiTypeToken::Opaque);
                }
                "local" => {
                    token = Some(FfiTypeToken::Local);
                }
                "unsafe" => {
                    if !input.peek(syn::token::Paren) {
                        return Err(syn::Error::new(
                            ident.span(),
                            "expected `(...)` after `unsafe`",
                        ));
                    }

                    let content;
                    syn::parenthesized!(content in input);
                    join_span(&mut span, content.span());

                    let inner_ident: Ident = content.parse().map_err(|_| {
                        syn::Error::new(content.span(), "expected ffi type kind inside unsafe(...)")
                    })?;
                    let inner_str = inner_ident.to_string();

                    match inner_str.as_str() {
                        "non_owning" => {
                            if !content.is_empty() {
                                return Err(syn::Error::new(
                                    content.span(),
                                    "`unsafe(non_owning) should contain only one identifier",
                                ));
                            }

                            token = Some(FfiTypeToken::UnsafeNonOwning);
                        }
                        "is_valid" => {
                            content.parse::<syn::Token![=]>()?;
                            let closure: syn::ExprClosure = content.parse()?;
                            join_span(&mut span, closure.span());

                            if !content.is_empty() {
                                return Err(syn::Error::new(
                                    content.span(),
                                    "unexpected tokens after `is_valid` closure",
                                ));
                            }

                            is_valid = Some(closure);
                        }
                        other => {
                            return Err(syn::Error::new(
                                inner_ident.span(),
                                format!("unknown unsafe ffi type kind: {other}"),
                            ));
                        }
                    }
                }
                "NICHE_VALUE" => {
                    input.parse::<syn::Token![=]>()?;
                    let value: syn::Expr = input.parse()?;
                    join_span(&mut span, value.span());
                    niche_value = Some(value);
                }
                other => {
                    return Err(syn::Error::new(
                        ident.span(),
                        format!("unknown type kind: {other}"),
                    ));
                }
            }

            if input.is_empty() {
                break;
            }

            if input.peek(syn::Token![,]) {
                let comma: syn::token::Comma = input.parse()?;
                join_span(&mut span, comma.span);
                if input.is_empty() {
                    break;
                }
            } else {
                return Err(input.error("expected `,`"));
            }
        }

        let span = span.unwrap_or_else(Span::call_site);

        if let Some(token) = token {
            if is_valid.is_some() || niche_value.is_some() {
                return Err(syn::Error::new(
                    span,
                    "unexpected tokens after ffi type kind",
                ));
            }

            return Ok(Self { span, token });
        }

        let Some(is_valid) = is_valid else {
            if niche_value.is_some() {
                return Err(syn::Error::new(
                    span,
                    "expected `unsafe(is_valid = ...)` when specifying `NICHE_VALUE`",
                ));
            }

            return Err(syn::Error::new(span, "expected ffi type kind"));
        };

        Ok(Self {
            span,
            token: FfiTypeToken::Transparent(niche_value, is_valid),
        })
    }
}

/// This represents an `#[mineral(...)]` attribute on a type
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum FfiTypeKindAttribute {
    Transparent(Option<syn::Expr>, syn::ExprClosure),
    Opaque,
    Local,
}

impl syn::parse::Parse for FfiTypeKindAttribute {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        input.call(SpannedFfiTypeToken::parse).and_then(|token| {
            Ok(match token.token {
                FfiTypeToken::Transparent(niche_value, is_valid) => {
                    FfiTypeKindAttribute::Transparent(niche_value, is_valid)
                }
                FfiTypeToken::Opaque => FfiTypeKindAttribute::Opaque,
                FfiTypeToken::Local => FfiTypeKindAttribute::Local,
                other => {
                    return Err(syn::Error::new(
                        token.span,
                        format!("`{other}` cannot be used on a type"),
                    ));
                }
            })
        })
    }
}

/// This represents an `#[mineral(...)]` attribute on a field
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum FfiTypeKindFieldAttribute {
    UnsafeNonOwning,
}

impl syn::parse::Parse for FfiTypeKindFieldAttribute {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        input.call(SpannedFfiTypeToken::parse).and_then(|token| {
            Ok(match token.token {
                FfiTypeToken::UnsafeNonOwning => FfiTypeKindFieldAttribute::UnsafeNonOwning,
                other => {
                    return Err(syn::Error::new(
                        token.span,
                        format!("`{other}` cannot be used on a field"),
                    ));
                }
            })
        })
    }
}

const FFI_TYPE_ATTR: &str = "mineral";

pub struct FfiTypeAttr {
    pub kind: Option<FfiTypeKindAttribute>,
}

impl FromAttributes for FfiTypeAttr {
    fn from_attributes(attrs: &[Attribute]) -> darling::Result<Self> {
        parse_single_list_attr_opt(FFI_TYPE_ATTR, attrs).map(|kind| Self { kind })
    }
}

pub struct FfiTypeFieldAttr {
    kind: Option<FfiTypeKindFieldAttribute>,
}

impl FromAttributes for FfiTypeFieldAttr {
    fn from_attributes(attrs: &[Attribute]) -> darling::Result<Self> {
        parse_single_list_attr_opt(FFI_TYPE_ATTR, attrs).map(|kind| Self { kind })
    }
}

pub type FfiTypeData = darling::ast::Data<SpannedValue<FfiTypeVariant>, FfiTypeField>;
#[cfg(feature = "getset")]
pub type FfiTypeFields = darling::ast::Fields<FfiTypeField>;

pub struct FfiTypeInput {
    pub vis: syn::Visibility,
    pub ident: syn::Ident,
    pub generics: syn::Generics,
    pub data: FfiTypeData,
    pub derive_attr: DeriveAttrs,
    repr_attr: Repr,
    pub ffi_type_attr: FfiTypeAttr,
    pub span: Span,
    /// The original `DeriveInput` this structure was parsed from
    pub ast: syn::DeriveInput,
    #[cfg(feature = "getset")]
    pub getset_attr: GetSetStructAttrs,
}

impl darling::FromDeriveInput for FfiTypeInput {
    fn from_derive_input(input: &syn::DeriveInput) -> darling::Result<Self> {
        let vis = input.vis.clone();
        let ident = input.ident.clone();
        let generics = input.generics.clone();
        let data = darling::ast::Data::try_from(&input.data)?;
        let derive_attr = DeriveAttrs::from_attributes(&input.attrs)?;
        let repr_attr = Repr::from_attributes(&input.attrs)?;
        let ffi_type_attr = FfiTypeAttr::from_attributes(&input.attrs)?;
        let span = input.span();
        #[cfg(feature = "getset")]
        let getset_attr = GetSetStructAttrs::from_attributes(&input.attrs)?;

        Ok(FfiTypeInput {
            vis,
            ident,
            generics,
            data,
            derive_attr,
            repr_attr,
            ffi_type_attr,
            span,
            ast: input.clone(),
            #[cfg(feature = "getset")]
            getset_attr,
        })
    }
}

#[derive(FromVariant)]
pub struct FfiTypeVariant {
    pub ident: syn::Ident,
    pub discriminant: Option<syn::Expr>,
    pub fields: darling::ast::Fields<FfiTypeField>,
}

pub struct FfiTypeField {
    pub ty: syn::Type,
    pub ffi_type_attr: FfiTypeFieldAttr,
    pub ident: Option<syn::Ident>,
    #[cfg(feature = "getset")]
    pub doc_attrs: DocAttrs,
    #[cfg(feature = "getset")]
    pub getset_attr: GetSetFieldAttrs,
}

impl FromField for FfiTypeField {
    fn from_field(field: &Field) -> darling::Result<Self> {
        let ty = field.ty.clone();
        let ffi_type_attr = FfiTypeFieldAttr::from_attributes(&field.attrs)?;
        let ident = field.ident.clone();
        #[cfg(feature = "getset")]
        let doc_attrs = DocAttrs::from_attributes(&field.attrs)?;
        #[cfg(feature = "getset")]
        let getset_attr = GetSetFieldAttrs::from_attributes(&field.attrs)?;
        Ok(Self {
            ty,
            ffi_type_attr,
            ident,
            #[cfg(feature = "getset")]
            doc_attrs,
            #[cfg(feature = "getset")]
            getset_attr,
        })
    }
}

pub fn derive_ffi_type(emitter: &mut Emitter, input: &syn::DeriveInput) -> TokenStream {
    let Some(input) = emitter.handle(FfiTypeInput::from_derive_input(input)) else {
        return quote!();
    };

    if input.ffi_type_attr.kind == Some(FfiTypeKindAttribute::Opaque) {
        return derive_ffi_type_for_opaque_item(&input.ident, &input.generics);
    }

    if let darling::ast::Data::Struct(darling::ast::Fields {
        style: Style::Unit, ..
    }) = &input.data
    {
        emit!(
            emitter,
            &input.span,
            "Unit structs are not allowed in FFI. Annotate with #[co3::mineral(opaque)]?",
        );

        return quote! {};
    }
    match input.repr_attr.kind.as_deref() {
        Some(ReprKind::Transparent) => return derive_ffi_type_for_transparent_item(&input),
        Some(ReprKind::C) => return derive_ffi_type_for_repr_c_item(emitter, &input),
        Some(ReprKind::Primitive(repr)) => {
            if let darling::ast::Data::Enum(variants) = &input.data {
                return derive_ffi_type_for_fieldless_enum(*repr, &input.ident, variants);
            }
        }
        None => {}
    }

    let local = input.ffi_type_attr.kind == Some(FfiTypeKindAttribute::Local);
    verify_is_non_owning(emitter, &input.data);
    match &input.data {
        darling::ast::Data::Enum(variants) if variants.is_empty() => {
            emit!(
                emitter,
                input.ident,
                "Uninhabited enums are not allowed in FFI. Annotate with #[co3::mineral(opaque)]?"
            );

            quote! {}
        }
        darling::ast::Data::Enum(variants) => derive_ffi_type_for_no_repr_data_carrying_enum(
            emitter,
            &input.ident,
            input.generics,
            variants,
            local,
        ),
        darling::ast::Data::Struct(fields) => {
            derive_ffi_type_for_no_repr_struct(&input.ident, input.generics, fields, local)
        }
    }
}

fn derive_ffi_type_for_opaque_item(name: &Ident, generics: &syn::Generics) -> TokenStream {
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

/// Possible transparent items:
///
/// * fieldless structs
/// * one-variant fieldless enums
fn derive_ffi_type_for_transparent_item(input: &FfiTypeInput) -> TokenStream {
    debug_assert_eq!(
        input.repr_attr.kind.as_deref().copied(),
        Some(ReprKind::Transparent)
    );

    let (_, ty_generics, _) = input.generics.split_for_impl();
    let params = &input.generics.params;
    let predicates = input
        .generics
        .where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let name = &input.ident;
    let target = match &input.data {
        darling::ast::Data::Enum(variants) => variants.iter().next().and_then(|v| {
            v.fields
                .fields
                .first()
                .map(|first_variant| &first_variant.ty)
        }),
        // TODO: We don't check to find which struct field is not a ZST. It is just assumed that it is the first field.
        // I think something can be done inside `co3::mineral!` through the use of disjoint_impls! or via macro attribute
        darling::ast::Data::Struct(item) => item.fields.first().map(|first_field| &first_field.ty),
    };

    if target.is_none() {
        return quote! {};
    };

    let custom_validation = if let Some(FfiTypeKindAttribute::Transparent(niche_value, is_valid)) =
        &input.ffi_type_attr.kind
    {
        let niche_value = niche_value
            .as_ref()
            .map(|value| quote!(const NICHE_VALUE: <Self as co3::ExternC>::CType = #value;));

        quote! {
            #niche_value
            fn is_valid(target: &Self::Target) -> bool {
                (#is_valid)(target)
            }
        }
    } else {
        quote!()
    };

    quote! {
        co3::mineral! {
            // SAFETY: `Self` and `Self::Target` are guaranteed to be transmutable, but the user
            // must make sure the provided validation function does not return false positives
            unsafe impl(#params) Transparent for #name #ty_generics where (#predicates) {
                type Target = #target;

                #custom_validation
            }
        }
    }
}

fn derive_ffi_type_for_fieldless_enum(
    repr: ReprPrimitive,
    enum_name: &Ident,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> TokenStream {
    let tag_type = parse_quote!(#repr);

    // FIXME: I think this doesn't actually require variant names, just using a range would suffice
    // (note that we don't support custom discriminants)
    let (discriminants, discriminant_decls) = gen_discriminants(enum_name, variants, &tag_type);

    let len = variants.len();
    let match_ = if discriminants.is_empty() {
        quote! {false}
    } else {
        quote! {
            match *target {
                #( | #discriminants )* => true,
                _ => false,
            }
        }
    };

    quote! {
        co3::mineral! {
            unsafe impl Transparent for #enum_name {
                type Target = #tag_type;

                const NICHE_VALUE: <Self as co3::ExternC>::CType = #len as <Self as co3::ExternC>::CType;
                fn is_valid(target: &Self::Target) -> bool {
                    #(#discriminant_decls)*

                    #match_
                }
            }
        }

        // TODO: Only applicable if number of variants fills out entire discriminant domain space
        //unsafe impl co3::ReprC for #enum_name {}
    }
}

fn derive_ffi_type_for_no_repr_struct(
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
    let (rust_store, ffi_store, rust_store_conversion, ffi_store_conversion) = if num_fields > 12 {
        (
            quote! { Option<(#( #field_rust_stores, )*)> },
            quote! { Option<(#( #field_ffi_stores, )*)> },
            quote! { let store = store.insert(Default::default()); },
            quote! { let store = store.insert(Default::default()); },
        )
    } else {
        (
            quote! { (#( #field_rust_stores, )*) },
            quote! { (#( #field_ffi_stores, )*) },
            quote! {},
            quote! {},
        )
    };

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

    let non_locality = if local {
        quote! {}
    } else {
        let mut non_local_where_clause = where_clause.unwrap().clone();

        for field in fields.iter() {
            let ty = &field.ty;
            non_local_where_clause
                .predicates
                .push(parse_quote! {for<'_dummy> #ty: co3::out_ptr::NonLocal});
        }

        quote! {
            unsafe impl #impl_generics co3::out_ptr::NonLocal for #name #ty_generics #non_local_where_clause {}

            impl #impl_generics co3::out_ptr::OutPtr for #name #ty_generics #non_local_where_clause {
                type OutPtr = Self::CType;
            }
            impl #impl_generics co3::out_ptr::OutPtrWrite for #name #ty_generics #non_local_where_clause {
                unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
                    let mut store = Default::default();
                    let encoded = co3::Encode::encode(self, &mut store);
                    unsafe { out_ptr.write(encoded); }
                }
            }
            impl #impl_generics co3::out_ptr::OutPtrRead for #name #ty_generics #non_local_where_clause {
                unsafe fn try_read_out(out_ptr: Self::OutPtr) -> co3::Result<Self> {
                    let mut store = Default::default();

                    unsafe {
                        let store_ref = &mut *(&mut store as *mut _);
                        co3::Decode::decode(out_ptr, store_ref)
                    }
                }
            }
        }
    };

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

    quote! {
        #repr_c_struct

        impl #impl_generics co3::ir::Cloned for #name #ty_generics #where_clause {}

        impl #impl_generics co3::ir::Ir for #name #ty_generics #where_clause {
            type Type = Self;
        }

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

fn derive_ffi_type_for_no_repr_data_carrying_enum(
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
        if variants.len() > 12 {
            (
                quote! { Option<(#( #variant_rust_stores, )*)> },
                quote! { Option<(#( #variant_ffi_stores, )*)> },
                quote! { let store = store.insert(Default::default()); },
                quote! { let store = store.insert(Default:default()); },
            )
        } else {
            (
                quote! { (#( #variant_rust_stores, )*) },
                quote! { (#( #variant_ffi_stores, )*) },
                quote! {},
                quote! {},
            )
        };

    let non_locality = if local {
        quote! {}
    } else {
        let mut non_local_where_clause = where_clause.unwrap().clone();

        for variant in variants {
            let Some(ty) =
                variant_mapper(emitter, variant, || None, |field| Some(field.ty.clone()))
            else {
                continue;
            };

            non_local_where_clause
                .predicates
                .push(parse_quote! {for<'_dummy> #ty: co3::out_ptr::NonLocal});
        }

        quote! {
            unsafe impl #impl_generics co3::out_ptr::NonLocal for #enum_name #ty_generics #non_local_where_clause {}

            impl #impl_generics co3::out_ptr::OutPtr for #enum_name #ty_generics #non_local_where_clause {
                type OutPtr = Self::CType;
            }
            impl #impl_generics co3::out_ptr::OutPtrWrite for #enum_name #ty_generics #non_local_where_clause {
                unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
                    let mut store = Default::default();
                    let encoded = co3::Encode::encode(self, &mut store);
                    unsafe { out_ptr.write(encoded); }
                }
            }
            impl #impl_generics co3::out_ptr::OutPtrRead for #enum_name #ty_generics #non_local_where_clause {
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
    };

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

    quote! {
        #repr_c_enum

        impl #impl_generics co3::ir::Cloned for #enum_name #ty_generics #where_clause {}

        impl #impl_generics co3::ir::Ir for #enum_name #ty_generics #where_clause {
            type Type = Self;
        }

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

fn derive_ffi_type_for_repr_c_item(emitter: &mut Emitter, input: &FfiTypeInput) -> TokenStream {
    verify_is_non_owning(emitter, &input.data);

    let item_name = &input.ident;
    let (_, ty_generics, _) = input.generics.split_for_impl();
    let params = &input.generics.params;
    let predicates = input
        .generics
        .where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    match &input.data {
        darling::ast::Data::Enum(variants) => {
            let len = TokenStream::from_str(&format!("{}", variants.len())).expect("Valid");

            let (_, repr_c_enum) =
                gen_data_carrying_repr_c_enum(emitter, item_name, &input.generics, variants);

            quote! {
                #repr_c_enum

                co3::mineral! {
                    impl(#params) Transparent for #item_name #ty_generics where (#predicates) {
                        type Target = #repr_c_enum;

                        fn is_valid(target: &Self::Target) -> bool {
                            // TODO: Can it be less than 0?
                            // Depends on the c type used
                            target.tag <= #len
                        }
                    }
                }
            }
        }
        darling::ast::Data::Struct(_) => {
            quote! {
                co3::mineral! {
                    // FIXME: I think this should be Transparent
                    impl(#params) Robust for #item_name #ty_generics where (#predicates) {}
                }

                // TODO: Shouldn't I implement ReprC for the struct as well?
            }
        }
    }
}

fn gen_repr_c_struct(
    name: &Ident,
    generics: &syn::Generics,
    fields: &Fields<FfiTypeField>,
) -> (Ident, TokenStream) {
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

fn gen_data_carrying_repr_c_enum(
    emitter: &mut Emitter,
    enum_name: &Ident,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> (Ident, TokenStream) {
    let (payload_name, payload) =
        gen_data_carrying_enum_payload(emitter, enum_name, generics, variants);

    let doc = format!(" [`ReprC`] equivalent of [`{enum_name}`]");
    let repr_c_enum_name = gen_repr_c_item_name(enum_name);
    // FIXME: What is the correct repr here?
    let tag_type = quote! { core::ffi::c_uint };

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let params = &generics.params;
    let predicates = where_clause
        .as_ref()
        .map(|where_clause| &where_clause.predicates);

    let repr_c_enum = quote! {
        #payload

        #[repr(C)]
        #[doc = #doc]
        #[derive(Clone)]
        struct #repr_c_enum_name #impl_generics #where_clause {
            tag: #tag_type, payload: #payload_name #ty_generics,
        }

        impl #impl_generics Copy for #repr_c_enum_name #ty_generics #where_clause {}
        unsafe impl #impl_generics co3::ReprC for #repr_c_enum_name #ty_generics #where_clause {}

        co3::mineral! {
            impl(#params) Robust for #repr_c_enum_name where (#predicates) {}
        }
    };

    (repr_c_enum_name, repr_c_enum)
}

fn gen_data_carrying_enum_payload(
    emitter: &mut Emitter,
    enum_name: &Ident,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> (Ident, TokenStream) {
    let payload_name = gen_repr_c_enum_payload_name(enum_name);
    let repr_c_enum_name = gen_repr_c_item_name(enum_name);

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let field_names = variants.iter().map(|variant| &variant.ident);
    let doc = format!(" Payload of [`{repr_c_enum_name}`]");

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

    let payload = quote! {
        #[repr(C)]
        #[doc = #doc]
        #[derive(Clone)]
        #[expect(non_snake_case)]
        union #payload_name #impl_generics #where_clause {
            #(#field_names: #field_tys),*
        }

        impl #impl_generics Copy for #payload_name #ty_generics #where_clause {}
        unsafe impl #impl_generics co3::ReprC for #payload_name #ty_generics #where_clause {}
    };

    (payload_name, payload)
}

fn gen_discriminants(
    enum_name: &Ident,
    variants: &[SpannedValue<FfiTypeVariant>],
    tag_type: &syn::Type,
) -> (Vec<Ident>, Vec<TokenStream>) {
    let variant_names = variants.iter().map(|v| &v.ident);
    let discriminant_values = variant_discriminants(variants);

    variant_names.zip(discriminant_values.iter()).fold(
        Default::default(),
        |mut acc, (variant_name, discriminant_value)| {
            let discriminant_name = Ident::new(
                &format!("{enum_name}__{variant_name}").to_uppercase(),
                Span::call_site(),
            );

            acc.1.push(quote! {
                const #discriminant_name: #tag_type = #discriminant_value;
            });
            acc.0.push(discriminant_name);

            acc
        },
    )
}

fn variant_discriminants(variants: &[SpannedValue<FfiTypeVariant>]) -> Vec<proc_macro2::Literal> {
    variants
        .iter()
        .enumerate()
        .map(|(i, _)| proc_macro2::Literal::usize_unsuffixed(i))
        .collect()
}

fn variant_mapper<T: Sized, F0: FnOnce() -> T, F1: FnOnce(&FfiTypeField) -> T>(
    emitter: &mut Emitter,
    variant: &SpannedValue<FfiTypeVariant>,
    unit_mapper: F0,
    field_mapper: F1,
) -> T {
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

fn gen_repr_c_item_name(enum_name: &Ident) -> Ident {
    Ident::new(&format!("__co3__ReprC{enum_name}"), Span::call_site())
}

fn gen_repr_c_enum_payload_name(enum_name: &Ident) -> Ident {
    Ident::new(&format!("__co3__{enum_name}Payload"), Span::call_site())
}

// NOTE: Except for the raw pointers there should be no other type
// that is at the same time Robust and also transfers ownership
/// Verifies for each pointer type found inside the `FfiTypeData` is marked as non-owning
fn verify_is_non_owning(emitter: &mut Emitter, data: &FfiTypeData) {
    struct PtrVisitor<'a> {
        emitter: &'a mut Emitter,
    }
    impl syn::visit::Visit<'_> for PtrVisitor<'_> {
        fn visit_type_ptr(&mut self, node: &syn::TypePtr) {
            emit!(
                self.emitter,
                node,
                "Raw pointer found. If the pointer doesn't own the data, attach `#[mineral(unsafe(non_owning))` to the field. Otherwise, mark the entire type as opaque with `#[mineral(opaque)]`"
            );
        }
    }

    fn visit_field(ptr_visitor: &mut PtrVisitor, field: &FfiTypeField) {
        if field.ffi_type_attr.kind == Some(FfiTypeKindFieldAttribute::UnsafeNonOwning) {
            return;
        }
        ptr_visitor.visit_type(&field.ty);
    }

    let mut ptr_visitor = PtrVisitor { emitter };
    match data {
        FfiTypeData::Enum(variants) => {
            for variant in variants {
                for field in variant.fields.iter() {
                    visit_field(&mut ptr_visitor, field);
                }
            }
        }
        FfiTypeData::Struct(fields) => {
            for field in fields.iter() {
                visit_field(&mut ptr_visitor, field);
            }
        }
    }
}

/// Parses a single attribute of the form `#[attr_name(...)]` for darling using a `syn::parse::Parse` implementation.
///
/// If no attribute with specified name is found, returns `Ok(None)`.
///
/// # Errors
///
/// - If multiple attributes with specified name are found
/// - If attribute is not a list
pub fn parse_single_list_attr_opt<Body: syn::parse::Parse>(
    attr_name: &str,
    attrs: &[syn::Attribute],
) -> darling::Result<Option<Body>> {
    let mut accumulator = Default::default();

    let Some(attr) = find_single_attr_opt(&mut accumulator, attr_name, attrs) else {
        return accumulator.finish_with(None);
    };

    let mut kind = None;

    match &attr.meta {
        syn::Meta::Path(_) | syn::Meta::NameValue(_) => accumulator.push(darling::Error::custom(
            format!("Expected #[{}(...)] attribute to be a list", attr_name),
        )),
        syn::Meta::List(list) => {
            kind = accumulator.handle(syn::parse2(list.tokens.clone()).map_err(Into::into));
        }
    }

    accumulator.finish_with(kind)
}

/// Finds an optional single attribute with specified name.
///
/// Returns `None` if no attributes with specified name are found.
///
/// Emits an error into accumulator if multiple attributes with specified name are found.
#[must_use]
pub fn find_single_attr_opt<'a>(
    accumulator: &mut darling::error::Accumulator,
    attr_name: &str,
    attrs: &'a [syn::Attribute],
) -> Option<&'a syn::Attribute> {
    let matching_attrs = attrs
        .iter()
        .filter(|a| a.path().is_ident(attr_name))
        .collect::<Vec<_>>();
    let attr = match *matching_attrs.as_slice() {
        [] => {
            return None;
        }
        [attr] => attr,
        [attr, ref tail @ ..] => {
            // allow parsing to proceed further to collect more errors
            accumulator.push(
                darling::Error::custom(format!("Only one #[{}] attribute is allowed!", attr_name))
                    .with_spans(tail.iter().map(syn::spanned::Spanned::span)),
            );
            attr
        }
    };

    Some(attr)
}

/// Extension trait for [`darling::Error`].
///
/// Currently exists to add `with_spans` method.
pub trait DarlingErrorExt: Sized {
    /// Attaches a combination of multiple spans to the error.
    ///
    /// Note that it only attaches the first span on stable rustc, as the `Span::join` method is not yet stabilized (<https://github.com/rust-lang/rust/issues/54725#issuecomment-649078500>).
    #[must_use]
    fn with_spans(self, spans: impl IntoIterator<Item = impl Into<proc_macro2::Span>>) -> Self;
}

impl DarlingErrorExt for darling::Error {
    fn with_spans(self, spans: impl IntoIterator<Item = impl Into<proc_macro2::Span>>) -> Self {
        // Unfortunately, the story for combining multiple spans in rustc proc macro is not yet complete.
        // (see https://github.com/rust-lang/rust/issues/54725#issuecomment-649078500, https://github.com/rust-lang/rust/issues/54725#issuecomment-1547795742)
        // syn does some hacks to get error reporting that is a bit better: https://docs.rs/syn/2.0.37/src/syn/error.rs.html#282
        // we can't to that because darling's error type does not let us do that.

        // on nightly, we are fine, as `.join` method works. On stable, we fall back to returning the first span.

        let mut iter = spans.into_iter();
        let Some(first) = iter.next() else {
            return self;
        };
        let first: proc_macro2::Span = first.into();
        let r = iter
            .try_fold(first, |a, b| a.join(b.into()))
            .unwrap_or(first);

        self.with_span(&r)
    }
}
