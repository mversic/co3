use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, parenthesized, spanned::Spanned as _, visit::Visit};

use crate::{
    layout::attr::{ReprKind, parse_repr},
    layout::item::{derive_fieldless_enum, derive_item},
    utils::push_error,
};

mod attr;
mod borrow;
mod ctype;
mod custom;
mod item;
mod niche;
mod nul_terminated;
mod wide;

const FFI_TYPE_ATTR: &str = "repr_c";

#[derive(Default)]
pub(super) struct ReprCAttrs {
    pub(super) as_type: Option<syn::Type>,
    pub(super) niche_value: Option<syn::Expr>,
    pub(super) is_valid: Option<syn::ExprClosure>,
    pub(super) is_identity: bool,
    pub(super) is_view: bool,
    pub(super) is_wide_data: bool,
}

#[derive(Default)]
pub(super) struct VariantReprCAttrs {
    pub(super) is_valid: Option<syn::ExprClosure>,
}

fn parse_repr_c_attrs(attrs: &[Attribute]) -> syn::Result<ReprCAttrs> {
    let mut repr_c = ReprCAttrs::default();
    let mut found_attr = false;

    for attr in attrs
        .iter()
        .filter(|attr| attr.path().is_ident(FFI_TYPE_ATTR))
    {
        found_attr = true;

        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("as") {
                let content;
                parenthesized!(content in meta.input);
                let value: syn::Type = content.parse()?;
                if !content.is_empty() {
                    return Err(content.error("expected one intermediate type"));
                }
                if matches!(&value, syn::Type::Path(path) if path.qself.is_none() && path.path.is_ident("Self")) {
                    return Err(syn::Error::new_spanned(value, "use `repr_c(identity)` for `Self`"));
                }
                if repr_c.as_type.replace(value).is_some() {
                    return Err(meta.error("Duplicate `as` within attribute"));
                }
                return Ok(());
            }

            if meta.path.is_ident("identity") {
                if repr_c.is_identity {
                    return Err(meta.error("Duplicate `identity` within attribute"));
                }
                repr_c.is_identity = true;
                return Ok(());
            }

            if meta.path.is_ident("view") {
                if repr_c.is_view {
                    return Err(meta.error("Duplicate `view` within attribute"));
                }
                repr_c.is_view = true;
                return Ok(());
            }

            if meta.path.is_ident("__wide_data") {
                if repr_c.is_wide_data {
                    return Err(meta.error("Duplicate `__wide_data` within attribute"));
                }
                repr_c.is_wide_data = true;
                return Ok(());
            }

            if meta.path.is_ident("is_valid") {
                let value: syn::ExprClosure = meta.value()?.parse()?;
                if repr_c.is_valid.replace(value).is_some() {
                    return Err(meta.error("Duplicate `is_valid` within attribute"));
                }
                return Ok(());
            }

            if meta.path.is_ident("NICHE") {
                let value: syn::Expr = meta.value()?.parse()?;
                if repr_c.niche_value.replace(value).is_some() {
                    return Err(meta.error("Duplicate `NICHE` within attribute"));
                }
                return Ok(());
            }

            Err(meta.error("unknown type kind"))
        })?;
    }

    if !found_attr {
        return Ok(ReprCAttrs::default());
    }

    if repr_c.niche_value.is_none()
        && repr_c.as_type.is_none()
        && repr_c.is_valid.is_none()
        && !repr_c.is_identity
        && !repr_c.is_view
        && !repr_c.is_wide_data
    {
        return Err(syn::Error::new_spanned(
            attrs
                .iter()
                .find(|attr| attr.path().is_ident(FFI_TYPE_ATTR))
                .expect("repr_c attr was found"),
            "expected ffi type kind",
        ));
    }

    Ok(repr_c)
}

fn type_is_valid_closure(
    fields: &syn::Fields,
    is_valid: &Option<syn::ExprClosure>,
) -> syn::Result<()> {
    let Some(is_valid) = is_valid else {
        return Ok(());
    };

    if fields.len() != is_valid.inputs.len() {
        return Err(syn::Error::new_spanned(
            &is_valid.inputs,
            "`is_valid` closure must have exactly one argument per field",
        ));
    }

    Ok(())
}

pub(super) fn gen_is_valid_call(
    is_valid: &syn::ExprClosure,
    field_types: &[&syn::Type],
    args: &[TokenStream],
) -> TokenStream {
    quote! {{
        let __co3_is_valid: fn(#(&#field_types),*) -> bool = #is_valid;
        __co3_is_valid(#(#args),*)
    }}
}

pub(crate) fn derive_repr_c(input: &syn::DeriveInput) -> syn::Result<TokenStream> {
    let mut errors = None::<syn::Error>;

    let repr = parse_repr(&input.attrs)?;
    let repr_attr = repr.kind.as_ref();
    let repr_alignment = repr.alignment.as_ref();
    let repr_c_attrs = parse_repr_c_attrs(&input.attrs)?;
    let mut variant_attrs = Vec::new();

    let is_custom = repr_c_attrs.as_type.is_some();
    if is_custom && (repr_c_attrs.is_identity || repr_c_attrs.is_view || repr_c_attrs.is_wide_data)
    {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "`as(T)` cannot be combined with other representation modes",
        ));
    }

    if repr_c_attrs.is_identity {
        match &input.data {
            syn::Data::Struct(_)
                if !matches!(repr_attr, Some(ReprKind::C(None) | ReprKind::Transparent)) =>
            {
                push_error(
                    &mut errors,
                    syn::Error::new_spanned(
                        &input.ident,
                        "`repr_c(identity)` requires `#[repr(C)]` or `#[repr(transparent)]`",
                    ),
                );
            }
            syn::Data::Struct(_) => {}
            _ => push_error(
                &mut errors,
                syn::Error::new_spanned(
                    &input.ident,
                    "`repr_c(identity)` is only supported on structs",
                ),
            ),
        }
        if repr_c_attrs.is_valid.is_some() {
            push_error(
                &mut errors,
                syn::Error::new_spanned(
                    &input.ident,
                    "`repr_c(identity)` cannot be combined with `is_valid`",
                ),
            );
        }
        if repr_c_attrs.niche_value.is_some() {
            push_error(
                &mut errors,
                syn::Error::new_spanned(
                    &input.ident,
                    "`repr_c(identity)` cannot be combined with `NICHE`",
                ),
            );
        }
        if repr_c_attrs.is_view {
            push_error(
                &mut errors,
                syn::Error::new_spanned(
                    &input.ident,
                    "`repr_c(identity)` cannot be combined with `view`",
                ),
            );
        }
        if repr_c_attrs.is_wide_data {
            push_error(
                &mut errors,
                syn::Error::new_spanned(
                    &input.ident,
                    "`repr_c(identity)` cannot be combined with `__wide_data`",
                ),
            );
        }
    }

    match &input.data {
        syn::Data::Struct(data) => {
            validate_fields_no_ffi_type_attr(&data.fields, &mut errors);
            if let Err(err) = type_is_valid_closure(&data.fields, &repr_c_attrs.is_valid) {
                push_error(&mut errors, err);
            }
        }
        syn::Data::Enum(data) => {
            if repr_c_attrs.is_valid.is_some() {
                let err_msg = "`is_valid` is only supported on structs or enum variants";
                push_error(&mut errors, syn::Error::new_spanned(&input.ident, err_msg));
            }

            if repr_c_attrs.niche_value.is_some() {
                let err_msg = "`NICHE` is only supported on structs";
                push_error(&mut errors, syn::Error::new_spanned(&input.ident, err_msg));
            }

            if !is_custom && matches!(repr_attr, Some(ReprKind::C(None))) {
                let err_msg = "#[repr(C)]` not supported; use `#[repr(int)]`/`#[repr(C, int)]`";
                push_error(&mut errors, syn::Error::new_spanned(&input.ident, err_msg));
            }

            let has_data_variant = data
                .variants
                .iter()
                .any(|variant| !matches!(variant.fields, syn::Fields::Unit));

            for variant in &data.variants {
                validate_fields_no_ffi_type_attr(&variant.fields, &mut errors);

                if !is_custom && has_data_variant && variant.discriminant.is_some() {
                    let err_msg = "Explicit discriminants are not supported in data-carrying enums";
                    push_error(&mut errors, syn::Error::new(variant.span(), err_msg));
                }

                let variant_repr_c_attrs = parse_repr_c_attrs(&variant.attrs)?;
                if let Err(err) =
                    type_is_valid_closure(&variant.fields, &variant_repr_c_attrs.is_valid)
                {
                    push_error(&mut errors, err);
                }
                if is_custom {
                    if variant_repr_c_attrs.as_type.is_some()
                        || variant_repr_c_attrs.is_identity
                        || variant_repr_c_attrs.is_view
                        || variant_repr_c_attrs.is_wide_data
                        || variant_repr_c_attrs.niche_value.is_some()
                    {
                        push_error(
                            &mut errors,
                            syn::Error::new_spanned(
                                &variant.ident,
                                "only `is_valid` is supported on variants with `as(T)`",
                            ),
                        );
                    }
                } else {
                    if variant_repr_c_attrs.niche_value.is_some() {
                        let err_msg = "`NICHE` is only supported on types";
                        push_error(&mut errors, syn::Error::new(variant.span(), err_msg));
                    }
                    if variant_repr_c_attrs.is_view {
                        let err_msg = "`view` is only supported on types";
                        push_error(&mut errors, syn::Error::new(variant.span(), err_msg));
                    }
                    if variant_repr_c_attrs.is_identity {
                        let err_msg = "`repr_c(identity)` is only supported on types";
                        push_error(&mut errors, syn::Error::new(variant.span(), err_msg));
                    }
                    if variant_repr_c_attrs.as_type.is_some() {
                        let err_msg = "`as(T)` is only supported on types";
                        push_error(&mut errors, syn::Error::new(variant.span(), err_msg));
                    }
                }
                variant_attrs.push(VariantReprCAttrs {
                    is_valid: variant_repr_c_attrs.is_valid,
                });
            }
        }
        syn::Data::Union(_) => {
            return Err(syn::Error::new_spanned(input, "Unions are not supported"));
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    if is_custom {
        let tokens = custom::derive_custom_repr_c(input, &repr_c_attrs, &variant_attrs);
        return Ok(tokens);
    }

    let tokens = match &input.data {
        syn::Data::Struct(_) => {
            let item = derive_item(repr_attr, repr_alignment, input, &repr_c_attrs, &[]);
            let wide = (!repr_c_attrs.is_view && !repr_c_attrs.is_wide_data)
                .then(|| wide::expand(input, repr_attr))
                .transpose()?;
            let nul_terminated = (!repr_c_attrs.is_view && !repr_c_attrs.is_wide_data)
                .then(|| nul_terminated::expand(input, repr_attr))
                .transpose()?;
            quote! { #item #wide #nul_terminated }
        }
        syn::Data::Enum(data) if data.variants.is_empty() => {
            // TODO: Support uninhabited enums. yes, it is possible
            let err_msg = "Uninhabited enum is a never type. You can declare it as an opaque type in `ffi!` with `type Foo;`";
            push_error(&mut errors, syn::Error::new_spanned(&input.ident, err_msg));

            quote! {}
        }
        syn::Data::Enum(data) => {
            if data
                .variants
                .iter()
                .all(|v| matches!(v.fields, syn::Fields::Unit))
            {
                derive_fieldless_enum(repr_attr, repr_alignment, input)
            } else {
                derive_item(
                    repr_attr,
                    repr_alignment,
                    input,
                    &repr_c_attrs,
                    &variant_attrs,
                )
            }
        }
        syn::Data::Union(_) => unreachable!(),
    };

    if let Some(errors) = errors {
        Err(errors)
    } else {
        let body = quote! {
            #tokens
        };

        if repr_c_attrs.is_wide_data {
            Ok(body)
        } else {
            Ok(quote! {
                #body
            })
        }
    }
}

/// Finds an optional single attribute with specified name.
///
/// Returns `None` if no attributes with specified name are found.
///
/// Emits an error into accumulator if multiple attributes with specified name are found.
pub fn find_single_attr_opt<'a>(
    attr_name: &str,
    attrs: &'a [syn::Attribute],
) -> syn::Result<Option<&'a syn::Attribute>> {
    fn join_spans(spans: impl IntoIterator<Item = proc_macro2::Span>) -> Option<proc_macro2::Span> {
        let mut iter = spans.into_iter();
        let first = iter.next()?;
        Some(iter.try_fold(first, |a, b| a.join(b)).unwrap_or(first))
    }

    let matching_attrs = attrs
        .iter()
        .filter(|a| a.path().is_ident(attr_name))
        .collect::<Vec<_>>();
    let attr = match *matching_attrs.as_slice() {
        [] => return Ok(None),
        [attr] => attr,
        [attr, ref tail @ ..] => {
            return Err(syn::Error::new(
                join_spans(tail.iter().map(syn::spanned::Spanned::span))
                    .unwrap_or_else(|| attr.span()),
                format!("Only one #[{}] attribute is allowed!", attr_name),
            ));
        }
    };

    Ok(Some(attr))
}

fn validate_fields_no_ffi_type_attr(fields: &syn::Fields, errors: &mut Option<syn::Error>) {
    for field in fields {
        match find_single_attr_opt(FFI_TYPE_ATTR, &field.attrs) {
            Ok(Some(attr)) => {
                let err_msg = "`is_valid` is only supported on structs or enum variants";
                push_error(errors, syn::Error::new_spanned(attr, err_msg));
            }
            Ok(None) => {}
            Err(err) => push_error(errors, err),
        }
    }
}

/// Returns whether `ty` is a directly spelled `PhantomData` type.
///
/// Proc macros cannot resolve type aliases, so aliases deliberately remain subject to normal
/// lowering. Direct `PhantomData` fields are always zero-sized and alignment-1.
pub(super) fn is_phantom_data(ty: &syn::Type) -> bool {
    let syn::Type::Path(type_path) = ty else {
        return false;
    };
    if type_path.qself.is_some() {
        return false;
    }

    let segments = &type_path.path.segments;
    match segments.len() {
        1 => segments[0].ident == "PhantomData",
        3 => {
            (segments[0].ident == "core" || segments[0].ident == "std")
                && segments[1].ident == "marker"
                && segments[2].ident == "PhantomData"
        }
        _ => false,
    }
}

/// Check if a type contains any of the type parameters from generics
pub(super) fn is_type_parametrized(ty: &syn::Type, generics: &syn::Generics) -> bool {
    /// Visitor to check if a type contains any of the specified type parameters
    struct TypeParamVisitor<'a> {
        type_params: &'a [&'a syn::Ident],
        is_generic: bool,
    }

    impl Visit<'_> for TypeParamVisitor<'_> {
        fn visit_type_path(&mut self, type_path: &syn::TypePath) {
            if type_path.qself.is_none()
                && let Some(first_segment) = type_path.path.segments.first()
                && self.type_params.contains(&&first_segment.ident)
            {
                self.is_generic = true;
            }

            syn::visit::visit_type_path(self, type_path);
        }
    }

    let type_param_idents = generics
        .type_params()
        .map(|param| &param.ident)
        .collect::<Vec<_>>();

    let mut visitor = TypeParamVisitor {
        type_params: &type_param_idents,
        is_generic: false,
    };

    visitor.visit_type(ty);
    visitor.is_generic
}

fn repr_type_name(repr: &syn::Type) -> Option<&str> {
    let syn::Type::Path(type_path) = repr else {
        return None;
    };

    type_path
        .path
        .get_ident()
        .map(syn::Ident::to_string)
        .map(|s| match s.as_str() {
            "u8" => "u8",
            "i8" => "i8",
            "u16" => "u16",
            "i16" => "i16",
            "u32" => "u32",
            "i32" => "i32",
            "u64" => "u64",
            "i64" => "i64",
            "usize" => "usize",
            "isize" => "isize",
            _ => "",
        })
        .filter(|s| !s.is_empty())
}

pub(super) fn infer_repr(num_variants: usize) -> syn::Type {
    const U8_CAPACITY: usize = u8::MAX as usize + 1;
    const U16_CAPACITY: usize = u16::MAX as usize + 1;
    const U32_CAPACITY: usize = u32::MAX as usize + 1;

    #[expect(clippy::match_overlapping_arm)]
    match num_variants {
        0..=U8_CAPACITY => syn::parse_quote!(u8),
        0..=U16_CAPACITY => syn::parse_quote!(u16),
        0..=U32_CAPACITY => syn::parse_quote!(u32),
        // TODO: is this correct
        _ => syn::parse_quote!(u64),
    }
}

pub(super) fn primitive_tag_type(repr: &ReprKind) -> &syn::Type {
    match repr {
        ReprKind::C(Some(repr)) | ReprKind::Primitive(repr) => repr,
        ReprKind::Transparent => unreachable!("transparent enums have no tag"),
        ReprKind::C(None) => unreachable!("plain repr(C) enums are unsupported"),
    }
}

pub(super) fn enum_tag_type(repr: Option<&ReprKind>, variants_len: usize) -> syn::Type {
    match repr {
        None => infer_repr(variants_len),
        Some(repr @ (ReprKind::C(Some(_)) | ReprKind::Primitive(_))) => {
            primitive_tag_type(repr).clone()
        }
        Some(ReprKind::Transparent) | Some(ReprKind::C(None)) => unreachable!(),
    }
}

pub(super) fn is_transparent_enum_repr(
    repr: Option<&ReprKind>,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> bool {
    matches!(repr, Some(ReprKind::Transparent)) || repr.is_none() && variants.len() == 1
}

/// Checks if an enum exhausts all possible values of its repr type
pub(super) fn is_exhaustive_enum(num_variants: usize, repr: &syn::Type) -> bool {
    fn repr_type_bit_width(repr: &syn::Type) -> Option<u32> {
        match repr_type_name(repr)? {
            "u8" | "i8" => Some(8),
            "u16" | "i16" => Some(16),
            "u32" | "i32" => Some(32),
            "u64" | "i64" => Some(64),
            _ => None,
        }
    }

    let max_values = match repr_type_bit_width(repr) {
        Some(8) => 1u64 << 8,
        Some(16) => 1u64 << 16,
        Some(32) => 1u64 << 32,
        // TODO: Can we have a 128-bit platform?
        Some(64) | None | Some(_) => return false,
    };

    num_variants as u64 == max_values
}

fn generic_param_idents<'a>(
    generics: impl IntoIterator<Item = &'a syn::GenericParam>,
) -> impl Iterator<Item = TokenStream> {
    generics.into_iter().map(|param| match param {
        syn::GenericParam::Lifetime(syn::LifetimeParam { lifetime, .. }) => quote! { #lifetime },
        syn::GenericParam::Type(syn::TypeParam { ident, .. }) => quote! { #ident },
        syn::GenericParam::Const(syn::ConstParam { ident, .. }) => quote! { #ident },
    })
}
