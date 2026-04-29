use std::fmt::{Display, Formatter};

use darling::{
    FromAttributes, FromDeriveInput, FromField, FromVariant, ast::Style, util::SpannedValue,
};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{
    Attribute, Field, Ident, ext::IdentExt as _, parse::ParseStream, spanned::Spanned as _,
    visit::Visit,
};

use crate::{
    attr::repr::{Repr, ReprKind},
    repr::{
        no_repr::derive_no_repr_fieldless_enum,
        repr_c::{
            derive_data_enum, derive_fieldless_enum, derive_repr_c_data_enum, derive_repr_c_struct,
        },
    },
    utils::push_error,
};
use no_repr::{derive_no_repr_data_enum, derive_no_repr_struct};
use transparent::derive_transparent_item;

mod niche;
mod no_repr;
mod repr_c;
mod transparent;

pub(crate) use repr_c::gen_sized_family;

#[derive(Debug)]
enum FfiTypeToken {
    Transparent(Option<syn::Expr>, Box<syn::ExprClosure>),
    UnsafeNonOwning,
    Local,
}

impl Display for FfiTypeToken {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            FfiTypeToken::UnsafeNonOwning => write!(f, "#[reprC(unsafe(non_owning))]"),
            FfiTypeToken::Local => write!(f, "#[reprC(local)]"),
            FfiTypeToken::Transparent(niche, is_valid) => {
                write!(f, "#[reprC(")?;
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
            token: FfiTypeToken::Transparent(niche_value, Box::new(is_valid)),
        })
    }
}

/// This represents an `#[reprC(...)]` attribute on a type
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum FfiTypeKindAttribute {
    Transparent(Option<syn::Expr>, Box<syn::ExprClosure>),
    Local,
}

impl syn::parse::Parse for FfiTypeKindAttribute {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        input.call(SpannedFfiTypeToken::parse).and_then(|token| {
            Ok(match token.token {
                FfiTypeToken::Transparent(niche_value, is_valid) => {
                    FfiTypeKindAttribute::Transparent(niche_value, is_valid)
                }
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

/// This represents an `#[reprC(...)]` attribute on a field
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

const FFI_TYPE_ATTR: &str = "reprC";

pub struct FfiTypeAttr {
    pub kind: Option<FfiTypeKindAttribute>,
}

impl FromAttributes for FfiTypeAttr {
    fn from_attributes(attrs: &[Attribute]) -> darling::Result<Self> {
        let mut accumulator = darling::error::Accumulator::default();
        let kind = accumulator
            .handle(parse_single_list_attr_opt(FFI_TYPE_ATTR, attrs))
            .flatten();
        accumulator.finish_with(Self { kind })
    }
}

pub struct FfiTypeFieldAttr {
    kind: Option<FfiTypeKindFieldAttribute>,
}

impl FromAttributes for FfiTypeFieldAttr {
    fn from_attributes(attrs: &[Attribute]) -> darling::Result<Self> {
        let mut accumulator = darling::error::Accumulator::default();
        let repr_c_kind = accumulator
            .handle(parse_single_list_attr_opt(FFI_TYPE_ATTR, attrs))
            .flatten();
        accumulator.finish_with(Self { kind: repr_c_kind })
    }
}

pub type FfiTypeData = darling::ast::Data<SpannedValue<FfiTypeVariant>, FfiTypeField>;

pub struct FfiTypeInput {
    pub ident: syn::Ident,
    pub generics: syn::Generics,
    pub data: FfiTypeData,
    repr_attr: Repr,
    pub ffi_type_attr: FfiTypeAttr,
    pub span: Span,
}

impl darling::FromDeriveInput for FfiTypeInput {
    fn from_derive_input(input: &syn::DeriveInput) -> darling::Result<Self> {
        let ident = input.ident.clone();
        let generics = input.generics.clone();
        let data = darling::ast::Data::try_from(&input.data)?;
        let repr_attr = Repr::from_attributes(&input.attrs)?;
        let ffi_type_attr = FfiTypeAttr::from_attributes(&input.attrs)?;
        let span = input.span();

        Ok(FfiTypeInput {
            ident,
            generics,
            data,
            repr_attr,
            ffi_type_attr,
            span,
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
    pub ffi_type_attr: FfiTypeFieldAttr,
    pub ident: Option<syn::Ident>,
    pub ty: syn::Type,
}

impl FromField for FfiTypeField {
    fn from_field(field: &Field) -> darling::Result<Self> {
        let ty = field.ty.clone();
        let ffi_type_attr = FfiTypeFieldAttr::from_attributes(&field.attrs)?;
        let ident = field.ident.clone();
        Ok(Self {
            ty,
            ffi_type_attr,
            ident,
        })
    }
}

pub fn derive_extern_c(input: &syn::DeriveInput) -> syn::Result<TokenStream> {
    derive_extern_c_internal::<true>(input)
}

pub(crate) fn derive_extern_c_internal<const NEEDS_DROP: bool>(
    input: &syn::DeriveInput,
) -> syn::Result<TokenStream> {
    let mut errors = None::<syn::Error>;
    let mut input = FfiTypeInput::from_derive_input(input)
        .map_err(|err| syn::Error::new_spanned(input, err.to_string()))?;

    match &input.data {
        // FIXME: allow ZST fields as long as there is at least one non-ZST
        darling::ast::Data::Struct(darling::ast::Fields {
            style: Style::Unit, ..
        }) => {
            push_error(
                &mut errors,
                syn::Error::new(
                    input.span,
                    "Unit struct is a ZST. You can declare it as an opaque type in `export_!` or `extern_!` with `type Foo;`",
                ),
            );
        }
        darling::ast::Data::Enum(variants)
            // FIXME: allow ZST fields as long as there is at least one non-ZST
            if variants.len() == 1 && variants[0].fields.fields.is_empty() =>
        {
            push_error(
                &mut errors,
                syn::Error::new(
                    input.span,
                    "Single-variant fieldless enum is a ZST. You can declare it as an opaque type in `export_!` or `extern_!` with `type Foo;`",
                ),
            );
        }
        darling::ast::Data::Struct(fields) => {
            for field in fields.iter() {
                verify_field_non_owning(&mut errors, field);
            }
        }
        darling::ast::Data::Enum(variants) => {
            verify_variants_non_owning(&mut errors, variants);

            for variant in variants {
                if variant.discriminant.is_some() {
                    push_error(
                        &mut errors,
                        syn::Error::new(variant.span(), "Explicit discriminants are not supported"),
                    );
                }

                match &variant.fields.style {
                    Style::Tuple if variant.fields.fields.len() > 1 => push_error(
                        &mut errors,
                        syn::Error::new(
                            variant.span(),
                            "Tuple variants with arity > 1 are not supported",
                        ),
                    ),
                    Style::Struct => push_error(
                        &mut errors,
                        syn::Error::new(variant.span(), "Structure variants are not supported"),
                    ),
                    _ => {}
                }
            }
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    input.generics.make_where_clause();
    let tokens = match input.repr_attr.kind.as_deref() {
        Some(ReprKind::Transparent) => derive_transparent_item(&input),
        Some(ReprKind::C(None)) if let darling::ast::Data::Struct(fields) = &input.data => {
            derive_repr_c_struct::<NEEDS_DROP>(&input.ident, &input.generics, fields)
        }
        Some(ReprKind::C(None)) => {
            push_error(
                &mut errors,
                syn::Error::new_spanned(
                    &input.ident,
                    "repr(C) on enums requires a primitive type (e.g., repr(C, u8))",
                ),
            );

            quote! {}
        }
        Some(ReprKind::C(Some(repr)))
            if let darling::ast::Data::Enum(variants) = &input.data
                && variants.iter().any(|v| !v.fields.fields.is_empty()) =>
        {
            derive_repr_c_data_enum::<NEEDS_DROP>(*repr, &input.ident, &input.generics, variants)
        }
        Some(ReprKind::C(Some(_))) => quote! {},
        Some(ReprKind::Primitive(repr)) if let darling::ast::Data::Enum(variants) = &input.data => {
            if variants.iter().all(|v| v.fields.fields.is_empty()) {
                derive_fieldless_enum(*repr, &input.ident, &input.generics, variants)
            } else {
                derive_data_enum::<NEEDS_DROP>(*repr, &input.ident, &input.generics, variants)
            }
        }
        Some(ReprKind::Primitive(_)) => quote! {},
        None => {
            let local = input.ffi_type_attr.kind == Some(FfiTypeKindAttribute::Local);

            match &input.data {
                darling::ast::Data::Enum(variants) if variants.is_empty() => {
                    push_error(
                        &mut errors,
                        syn::Error::new_spanned(
                            &input.ident,
                            "Uninhabited enum is a never type. You can declare it as an opaque type in `export_!` or `extern_!` with `type Foo;`",
                        ),
                    );

                    quote! {}
                }
                darling::ast::Data::Enum(variants) => {
                    if variants.iter().all(|v| v.fields.fields.is_empty()) {
                        derive_no_repr_fieldless_enum(&input.ident, &input.generics, variants)
                    } else {
                        derive_no_repr_data_enum::<NEEDS_DROP>(
                            &input.ident,
                            &input.generics,
                            variants,
                            local,
                        )
                    }
                }
                darling::ast::Data::Struct(fields) => derive_no_repr_struct::<NEEDS_DROP>(
                    &input.ident,
                    &input.generics,
                    fields,
                    local,
                ),
            }
        }
    };

    if let Some(errors) = errors {
        Err(errors)
    } else {
        Ok(tokens)
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

/// Visitor to check if a type contains any of the specified type parameters
struct TypeParamVisitor<'a> {
    type_params: &'a [&'a syn::Ident],
    is_generic: bool,
}

impl Visit<'_> for TypeParamVisitor<'_> {
    fn visit_type_path(&mut self, type_path: &syn::TypePath) {
        if let Some(ident) = type_path.path.get_ident()
            && self.type_params.contains(&ident)
        {
            self.is_generic = true;
        }

        syn::visit::visit_type_path(self, type_path);
    }
}

/// Check if a type contains any of the type parameters from generics
pub fn is_type_parameterized(ty: &syn::Type, generics: &syn::Generics) -> bool {
    let type_param_idents: Vec<_> = generics.type_params().map(|tp| &tp.ident).collect();

    let mut visitor = TypeParamVisitor {
        type_params: &type_param_idents,
        is_generic: false,
    };
    visitor.visit_type(ty);
    visitor.is_generic
}

// NOTE: Except for the raw pointers there should be no other type
// that is at the same time Robust and also transfers ownership
/// Verifies each field's pointer types are marked as non-owning
fn verify_field_non_owning(errors: &mut Option<syn::Error>, field: &FfiTypeField) {
    use syn::visit::Visit;

    if field.ffi_type_attr.kind == Some(FfiTypeKindFieldAttribute::UnsafeNonOwning) {
        return;
    }

    struct PtrVisitor<'a> {
        errors: &'a mut Option<syn::Error>,
    }
    impl Visit<'_> for PtrVisitor<'_> {
        fn visit_type_ptr(&mut self, node: &syn::TypePtr) {
            push_error(
                self.errors,
                syn::Error::new_spanned(
                    node,
                    "Raw pointer found. If the pointer doesn't own the data, attach `#[reprC(unsafe(non_owning))` to the field. Otherwise, declare the type as an opaque FFI type in an `export_!` or `extern_!` with `type Foo;`",
                ),
            );
        }
        fn visit_type_path(&mut self, node: &syn::TypePath) {
            if node
                .path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "NonNull")
            {
                push_error(
                    self.errors,
                    syn::Error::new_spanned(
                        node,
                        "NonNull pointer found. If the pointer doesn't own the data, attach `#[reprC(unsafe(non_owning))` to the field. Otherwise, declare the type as an opaque FFI type in an `export_!` or `extern_!` with `type Foo;`",
                    ),
                );
            }

            syn::visit::visit_type_path(self, node);
        }
    }

    let mut ptr_visitor = PtrVisitor { errors };
    ptr_visitor.visit_type(&field.ty);
}

/// Verifies each field in enum variants are non-owning
fn verify_variants_non_owning(
    errors: &mut Option<syn::Error>,
    variants: &[SpannedValue<FfiTypeVariant>],
) {
    for variant in variants {
        for field in variant.fields.iter() {
            verify_field_non_owning(errors, field);
        }
    }
}
