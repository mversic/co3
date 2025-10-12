use core::str::FromStr as _;
use std::fmt::{Display, Formatter};

use darling::{
    FromAttributes, FromDeriveInput, FromField, FromVariant, ast::Style, util::SpannedValue,
};
use manyhow::{emit, error_message};
use proc_macro2::{Delimiter, Span, TokenStream};
use quote::quote;
use syn::{
    Attribute, Field, Ident, parse::ParseStream, parse_quote, spanned::Spanned as _,
    visit::Visit as _,
};

#[cfg(feature = "getset")]
use crate::attr_parse::getset::{DocAttrs, GetSetFieldAttrs, GetSetStructAttrs};
use crate::{
    attr_parse::{
        derive::DeriveAttrs,
        repr::{Repr, ReprKind},
    },
    emitter::Emitter,
};

#[derive(Debug)]
enum FfiTypeToken {
    Opaque,
    UnsafeRobust(bool),
    UnsafeNonOwning,
    Local,
}

impl Display for FfiTypeToken {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            FfiTypeToken::Opaque => write!(f, "#[mineral(opaque)]"),
            FfiTypeToken::UnsafeRobust(has_niche) => {
                write!(f, "#[mineral(unsafe(robust, has_niche = {has_niche}))]",)
            }
            FfiTypeToken::UnsafeNonOwning => write!(f, "#[mineral(unsafe(non_owning))]"),
            FfiTypeToken::Local => write!(f, "#[mineral(local)]"),
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
        let (span, token) = input.step(|cursor| {
            let Some((token, after_ident)) = cursor.ident() else {
                return Err(cursor.error("expected ffi type kind"));
            };

            let mut span = token.span();
            let token = token.to_string();
            match token.as_str() {
                "opaque" => Ok(((span, FfiTypeToken::Opaque), after_ident)),
                "local" => Ok(((span, FfiTypeToken::Local), after_ident)),
                "unsafe" => {
                    let Some((inside, group_span, after_group)) =
                        after_ident.group(Delimiter::Parenthesis)
                    else {
                        return Err(cursor.error("expected `(...)` after `unsafe`"));
                    };
                    span = span.join(group_span.span()).unwrap_or(span);

                    let Some((inner_ident, after_inner_ident)) = inside.ident() else {
                        return Err(cursor.error("expected ffi type kind inside unsafe(...)"));
                    };
                    let inner_str = inner_ident.to_string();

                    match inner_str.as_str() {
                        "robust" => {
                            let mut after = after_inner_ident;
                            let Some((punct, after_punct)) = after.punct() else {
                                return Err(
                                    cursor.error("expected `, has_niche = ...` after `robust`")
                                );
                            };
                            if punct.as_char() != ',' {
                                return Err(
                                    cursor.error("expected `, has_niche = ...` after `robust`")
                                );
                            }
                            after = after_punct;

                            let Some((niche_ident, after_niche_ident)) = after.ident() else {
                                return Err(cursor.error("expected `has_niche =` after comma"));
                            };
                            if niche_ident != "has_niche" {
                                return Err(syn::Error::new(
                                    niche_ident.span(),
                                    "expected `has_niche`",
                                ));
                            }
                            after = after_niche_ident;

                            let Some((eq, after_eq)) = after.punct() else {
                                return Err(cursor.error("expected `=` after `has_niche`"));
                            };
                            if eq.as_char() != '=' {
                                return Err(cursor.error("expected `=` after `has_niche`"));
                            }
                            after = after_eq;

                            let expr = syn::parse2::<syn::LitStr>(after.token_stream())?;
                            let expr_value = expr.value().parse().map_err(|_| {
                                syn::Error::new(
                                    niche_ident.span(),
                                    "expected `has_niche = \"bool\"`",
                                )
                            })?;
                            Ok(((span, FfiTypeToken::UnsafeRobust(expr_value)), after_group))
                        }
                        "non_owning" => {
                            if !after_ident.eof() {
                                return Err(cursor.error(
                                    "`unsafe(non_owning) should contain only one identifier",
                                ));
                            }

                            Ok(((span, FfiTypeToken::UnsafeNonOwning), after_group))
                        }
                        other => Err(syn::Error::new(
                            token.span(),
                            format!("unknown unsafe ffi type kind: {other}"),
                        )),
                    }
                }
                other => Err(syn::Error::new(span, format!("unknown type kind: {other}"))),
            }
        })?;

        Ok(Self { span, token })
    }
}

/// This represents an `#[mineral(...)]` attribute on a type
#[derive(Debug, PartialEq, Eq, Clone)]
enum FfiTypeKindAttribute {
    Opaque,
    UnsafeRobust(bool),
    Local,
}

impl syn::parse::Parse for FfiTypeKindAttribute {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        input.call(SpannedFfiTypeToken::parse).and_then(|token| {
            Ok(match token.token {
                FfiTypeToken::Opaque => FfiTypeKindAttribute::Opaque,
                FfiTypeToken::UnsafeRobust(niche) => FfiTypeKindAttribute::UnsafeRobust(niche),
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

struct FfiTypeAttr {
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
    ffi_type_attr: FfiTypeAttr,
    #[cfg(feature = "getset")]
    pub getset_attr: GetSetStructAttrs,
    pub span: Span,
    /// The original `DeriveInput` this structure was parsed from
    pub ast: syn::DeriveInput,
}

impl FfiTypeInput {
    pub fn is_opaque(&self) -> bool {
        self.repr_attr.kind.is_none()
            || self.ffi_type_attr.kind == Some(FfiTypeKindAttribute::Opaque)
    }
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
        #[cfg(feature = "getset")]
        let getset_attr = GetSetStructAttrs::from_attributes(&input.attrs)?;
        let span = input.span();

        Ok(FfiTypeInput {
            vis,
            ident,
            generics,
            data,
            derive_attr,
            repr_attr,
            ffi_type_attr,
            #[cfg(feature = "getset")]
            getset_attr,
            span,
            ast: input.clone(),
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
    #[cfg(feature = "getset")]
    pub ident: Option<syn::Ident>,
    pub ty: syn::Type,
    #[cfg(feature = "getset")]
    pub doc_attrs: DocAttrs,
    pub ffi_type_attr: FfiTypeFieldAttr,
    #[cfg(feature = "getset")]
    pub getset_attr: GetSetFieldAttrs,
}

impl FromField for FfiTypeField {
    fn from_field(field: &Field) -> darling::Result<Self> {
        #[cfg(feature = "getset")]
        let ident = field.ident.clone();
        let ty = field.ty.clone();
        #[cfg(feature = "getset")]
        let doc_attrs = DocAttrs::from_attributes(&field.attrs)?;
        let ffi_type_attr = FfiTypeFieldAttr::from_attributes(&field.attrs)?;
        #[cfg(feature = "getset")]
        let getset_attr = GetSetFieldAttrs::from_attributes(&field.attrs)?;
        Ok(Self {
            #[cfg(feature = "getset")]
            ident,
            ty,
            #[cfg(feature = "getset")]
            doc_attrs,
            ffi_type_attr,
            #[cfg(feature = "getset")]
            getset_attr,
        })
    }
}

pub fn derive_ffi_type(emitter: &mut Emitter, input: &syn::DeriveInput) -> TokenStream {
    let Some(mut input) = emitter.handle(FfiTypeInput::from_derive_input(input)) else {
        return quote!();
    };

    let name = &input.ident;
    if let darling::ast::Data::Enum(variants) = &input.data
        && variants.is_empty()
    {
        emit!(emitter, name, "Uninhabited enums are not allowed in FFI");
    }

    if input.is_opaque() {
        return derive_ffi_type_for_opaque_item(name, &input.generics);
    }
    if input.repr_attr.kind.as_deref() == Some(&ReprKind::Transparent) {
        return derive_ffi_type_for_transparent_item(emitter, &input);
    }

    match &input.data {
        darling::ast::Data::Enum(variants) => {
            if variants.iter().all(|v| v.fields.is_empty()) {
                if let Some(variant) = variants.iter().find(|v| v.discriminant.is_some()) {
                    emit!(
                        emitter,
                        &variant.span(),
                        "Fieldless enums with explicit discriminants are prohibited",
                    );
                }

                derive_ffi_type_for_fieldless_enum(&input.repr_attr, &input.ident, variants)
            } else {
                verify_is_non_owning(emitter, &input.data);
                let local = input.ffi_type_attr.kind == Some(FfiTypeKindAttribute::Local);

                derive_ffi_type_for_data_carrying_enum(
                    emitter,
                    &input.ident,
                    input.generics,
                    variants,
                    local,
                )
            }
        }
        darling::ast::Data::Struct(item) => {
            let ffi_type_impl = derive_ffi_type_for_repr_c(emitter, &input);

            let repr_c_impl = {
                let predicates = &mut input.generics.make_where_clause().predicates;
                let add_bound = |ty| predicates.push(parse_quote! {#ty: co3::ReprC});

                if item.style == Style::Unit {
                    emit!(
                        emitter,
                        &input.span,
                        "Unit structs cannot implement `ReprC`"
                    );
                }

                item.fields
                    .iter()
                    .map(|field| &field.ty)
                    .for_each(add_bound);

                derive_unsafe_repr_c(&input.ident, &input.generics)
            };

            quote! {
                #repr_c_impl
                #ffi_type_impl
            }
        }
    }
}

/// Before deriving this trait make sure that all invariants are upheld
fn derive_unsafe_repr_c(name: &Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        // SAFETY: Type is robust with #[repr(C)] attribute attached
        unsafe impl #impl_generics co3::ReprC for #name #ty_generics #where_clause {}
    }
}

fn derive_ffi_type_for_opaque_item(name: &Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    quote! {
        impl #impl_generics co3::ir::Ir for #name #ty_generics #where_clause {
            type Type = co3::ir::Opaque;
        }

        // SAFETY: Opaque types are never dereferenced and therefore &mut T is considered to be transmutable
        unsafe impl #impl_generics co3::transmute::InfallibleTransmute for #name #ty_generics #where_clause {}

        impl #impl_generics co3::option::Niche for #name #ty_generics #where_clause {
            const NICHE_VALUE: *mut Self = core::ptr::null_mut();
        }
    }
}

fn derive_ffi_type_for_transparent_item(
    emitter: &mut Emitter,
    input: &FfiTypeInput,
) -> TokenStream {
    assert_eq!(
        input.repr_attr.kind.as_deref().copied(),
        Some(ReprKind::Transparent)
    );

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let name = &input.ident;

    // TODO: We don't check to find which field is not a ZST.
    // It is just assumed that it is the first field
    let inner = match &input.data {
        darling::ast::Data::Enum(variants) => {
            let first_variant = emitter.handle(variants.iter().next().ok_or_else(|| {
                error_message!("transparent enum must have exactly one variant, but it has none")
            }));

            if let Some(first_variant) = first_variant.and_then(|v| v.fields.fields.first()) {
                &first_variant.ty
            } else {
                // NOTE: one-variant fieldless enums have representation of ()
                return derive_ffi_type_for_opaque_item(name, &input.generics);
            }
        }
        darling::ast::Data::Struct(item) => {
            if let Some(first_field) = item.fields.first() {
                &first_field.ty
            } else {
                // NOTE: Fieldless structs have representation of ()
                return derive_ffi_type_for_opaque_item(name, &input.generics);
            }
        }
    };

    if let Some(FfiTypeKindAttribute::UnsafeRobust(niche)) = input.ffi_type_attr.kind {
        let niche_value = if niche {
            quote!(const NICHE_VALUE = "DELEGATE";)
        } else {
            quote!()
        };

        return quote! {
            co3::mineral! {
                // SAFETY: User must make sure the type is robust
                unsafe impl #impl_generics Transparent for #name #ty_generics #where_clause {
                    type Target = #inner;
                    #niche_value
                }
            }
        };
    }

    quote! {}
}

fn derive_ffi_type_for_fieldless_enum(
    repr: &Repr,
    enum_name: &Ident,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> TokenStream {
    let tag_type = gen_enum_tag_type(repr);

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

        impl co3::WrapperTypeOf<#enum_name> for #tag_type {
            type Type = #enum_name;
        }
    }
}

fn derive_ffi_type_for_data_carrying_enum(
    emitter: &mut Emitter,
    enum_name: &Ident,
    mut generics: syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
    local: bool,
) -> TokenStream {
    let (repr_c_enum_name, repr_c_enum) =
        gen_data_carrying_repr_c_enum(emitter, enum_name, &generics, variants);

    generics.make_where_clause();
    let lifetime = quote! {'__CO3_itm};
    let (impl_generics, ty_generics, where_clause) = split_for_impl(&generics);

    let variant_rust_stores = variants
        .iter()
        .map(|variant| {
            variant_mapper(
                emitter,
                variant,
                || quote! { () },
                |field| {
                    let ty = &field.ty;
                    quote! { <#ty as co3::FfiConvert<#lifetime>>::RustStore }
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
                    quote! { <#ty as co3::FfiConvert<#lifetime>>::FfiStore }
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
                                #variant_name: core::mem::ManuallyDrop::new(
                                    co3::FfiConvert::encode(payload, &mut store.#idx)
                                )
                            };

                            #repr_c_enum_name { tag: #idx, payload }
                        }
                    }
                },
            )
        })
        .collect::<Vec<_>>();

    let variants_decode = variants.iter().enumerate().map(|(i, variant)| {
        let idx = TokenStream::from_str(&format!("{i}")).expect("Valid");
        let variant_name = &variant.ident;

        variant_mapper(
            emitter,
            variant,
            || quote! { #idx => Ok(Self::#variant_name) },
            |_| {
                quote! {
                    #idx => {
                        let payload = core::mem::ManuallyDrop::into_inner(
                            source.payload.#variant_name
                        );

                        co3::FfiConvert::decode(payload, &mut store.#idx).map(Self::#variant_name)
                    }
                }
            },
        )
    }).collect::<Vec<_>>();

    // TODO: Tuples don't support impl of `Default` for arity > 12 currently.
    // Once this limitation is lifted `Option<tuple>` will not be necessary
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
                .push(parse_quote! {#ty: co3::out_ptr::NonLocal});
        }

        quote! {
            unsafe impl<#impl_generics> co3::out_ptr::NonLocal for #enum_name #ty_generics #non_local_where_clause {}

            impl<#impl_generics> co3::FfiWrapperType for #enum_name #ty_generics #non_local_where_clause {
                type InputType = Self;
                type ReturnType = Self;
            }
            impl<#impl_generics> co3::out_ptr::OutPtr for #enum_name #ty_generics #non_local_where_clause {
                type OutPtr = Self::CType;
            }
            impl<#impl_generics> co3::out_ptr::OutPtrWrite for #enum_name #ty_generics #non_local_where_clause {
                unsafe fn write_out(self, out_ptr: *mut Self::OutPtr) {
                    co3::repr_c::write_non_local::<_, Self>(self, out_ptr);
                }
            }
            impl<#impl_generics> co3::out_ptr::OutPtrRead for #enum_name #ty_generics #non_local_where_clause {
                unsafe fn try_read_out(out_ptr: Self::OutPtr) -> co3::Result<Self> {
                    co3::repr_c::read_non_local::<Self, Self>(out_ptr)
                }
            }
        }
    };

    quote! {
        #repr_c_enum

        // NOTE: Data-carrying enum cannot implement `ReprC` unless it is robust `repr(C)`
        impl<#impl_generics> co3::ir::Ir for #enum_name #ty_generics #where_clause {
            type Type = Self;
        }

        impl<#impl_generics> co3::ExternC for #enum_name #ty_generics #where_clause {
            type CType = #repr_c_enum_name #ty_generics;
        }
        impl<#lifetime, #impl_generics> co3::FfiConvert<#lifetime> for #enum_name #ty_generics #where_clause {
            type RustStore = #rust_store;
            type FfiStore = #ffi_store;

            fn encode(self, store: &mut Self::RustStore) -> Self::CType {
                #ffi_store_conversion

                match self {
                    #(#variants_into_ffi,)*
                }
            }

            unsafe fn decode(source: Self::CType, store: &mut Self::FfiStore) -> co3::Result<Self> {
                #rust_store_conversion

                match source.tag {
                    #(#variants_decode,)*
                    _ => Err(co3::FfiReturn::TrapRepresentation)
                }
            }
        }

        // TODO: Enum can be transmutable if all variants are transmutable and the enum is `repr(C)`
        impl<#impl_generics> co3::repr_c::Cloned for #enum_name #ty_generics #where_clause where Self: Clone {}

        #non_locality
    }
}

fn derive_ffi_type_for_repr_c(emitter: &mut Emitter, input: &FfiTypeInput) -> TokenStream {
    verify_is_non_owning(emitter, &input.data);
    if input.repr_attr.kind.as_deref().copied() != Some(ReprKind::C) {
        let span = input
            .repr_attr
            .kind
            .map_or_else(Span::call_site, |kind| kind.span());
        // TODO: this error message may be unclear. Consider adding a note about the `#[mineral]` attribute
        emit!(
            emitter,
            span,
            "To make an FFI type robust you must mark it with `#[repr(C)]`. Alternatively, try using `#[mineral(opaque)]` to make it opaque"
        );
    }

    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let name = &input.ident;

    quote! {
        co3::mineral! {
            impl #impl_generics Robust for #name #ty_generics #where_clause {}
        }
    }
}

fn gen_data_carrying_repr_c_enum(
    emitter: &mut Emitter,
    enum_name: &Ident,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> (Ident, TokenStream) {
    let (payload_name, payload) =
        gen_data_carrying_enum_payload(emitter, enum_name, generics, variants);
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let doc = format!(" [`ReprC`] equivalent of [`{enum_name}`]");
    let repr_c_enum_name = gen_repr_c_enum_name(enum_name);

    // FIXME: This is a hack before https://github.com/mversic/co3/issues/10
    let repr_c_attr: Attribute = parse_quote!(#[repr(C)]);
    let repr = &Repr::from_attributes(&[repr_c_attr]).unwrap();
    let tag_type = gen_enum_tag_type(repr);

    let repr_c_enum = quote! {
        #payload

        #[repr(C)]
        #[doc = #doc]
        #[derive(Clone)]
        pub struct #repr_c_enum_name #impl_generics #where_clause {
            tag: #tag_type, payload: #payload_name #ty_generics,
        }

        impl #impl_generics Copy for #repr_c_enum_name #ty_generics where #payload_name #ty_generics: Copy {}
        unsafe impl #impl_generics co3::ReprC for #repr_c_enum_name #ty_generics #where_clause {}
    };

    (repr_c_enum_name, repr_c_enum)
}

fn gen_data_carrying_enum_payload(
    emitter: &mut Emitter,
    enum_name: &Ident,
    generics: &syn::Generics,
    variants: &[SpannedValue<FfiTypeVariant>],
) -> (Ident, TokenStream) {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let field_names = variants.iter().map(|variant| &variant.ident);
    let payload_name = gen_repr_c_enum_payload_name(enum_name);
    let doc = format!(" [`ReprC`] equivalent of [`{enum_name}`]");

    let field_tys = variants
        .iter()
        .map(|variant| {
            variant_mapper(
                emitter,
                variant,
                || quote! {()},
                |field| {
                    let field_ty = &field.ty;
                    quote! {core::mem::ManuallyDrop<<#field_ty as co3::ExternC>::CType>}
                },
            )
        })
        .collect::<Vec<_>>();

    let payload = quote! {
        #[repr(C)]
        #[doc = #doc]
        #[derive(Clone)]
        #[expect(non_snake_case)]
        pub union #payload_name #impl_generics #where_clause {
            #(#field_names: #field_tys),*
        }

        impl #impl_generics Copy for #payload_name #ty_generics where #( #field_tys: Copy ),* {}
        unsafe impl #impl_generics co3::ReprC for #payload_name #ty_generics #where_clause {}
    };

    (payload_name, payload)
}

fn gen_discriminants(
    enum_name: &Ident,
    variants: &[SpannedValue<FfiTypeVariant>],
    tag_type: &syn::Type,
) -> (Vec<Ident>, Vec<TokenStream>) {
    let variant_names: Vec<_> = variants.iter().map(|v| &v.ident).collect();
    let discriminant_values = variant_discriminants(variants);

    variant_names.iter().zip(discriminant_values.iter()).fold(
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

fn gen_repr_c_enum_name(enum_name: &Ident) -> Ident {
    Ident::new(&format!("__co3__ReprC{enum_name}"), Span::call_site())
}

fn gen_repr_c_enum_payload_name(enum_name: &Ident) -> Ident {
    Ident::new(&format!("__co3__{enum_name}Payload"), Span::call_site())
}

// NOTE: Except for the raw pointers there should be no other type
// that is at the same time Robust and also transfers ownership
/// Verifies for each pointer type found inside the `FfiTypeData` that it is marked as non-owning
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

fn gen_enum_tag_type(repr: &Repr) -> syn::Type {
    let Some(kind) = repr.kind else {
        unreachable!()
    };

    match &*kind {
        ReprKind::Primitive(primitive) => parse_quote!(#primitive),
        ReprKind::C => parse_quote! {core::ffi::c_int},
        ReprKind::Transparent => unreachable!(),
    }
}

fn split_for_impl(
    generics: &syn::Generics,
) -> (
    syn::punctuated::Punctuated<syn::GenericParam, syn::Token![,]>,
    syn::TypeGenerics<'_>,
    Option<&syn::WhereClause>,
) {
    let impl_generics = generics.params.clone();
    let (_, ty_generics, where_clause) = generics.split_for_impl();
    (impl_generics, ty_generics, where_clause)
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
