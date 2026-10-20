use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::{Attribute, spanned::Spanned as _, visit::Visit};

use crate::{
    layout::attr::{ReprKind, parse_repr},
    layout::item::{derive_fieldless_enum, derive_item},
    utils::{ParamUseDetector, push_error, speculative_marker},
};

mod attr;
mod borrow;
mod ctype;
mod field;
mod item;
mod niche;
mod nul_terminated;
mod wide;

const FFI_TYPE_ATTR: &str = "repr_c";

#[derive(Default)]
pub(super) struct ReprCAttrs {
    pub(super) niche_value: Option<syn::Expr>,
    pub(super) is_valid: Option<syn::ExprClosure>,
    pub(super) is_identity: bool,
    pub(super) is_transparent: bool,
    pub(super) is_view: bool,
    pub(super) is_wide_data: bool,
    pub(super) allow_zero_tag: bool,
}

#[derive(Default)]
pub(super) struct VariantReprCAttrs {
    pub(super) is_valid: Option<syn::ExprClosure>,
    pub(super) allow_zero_tag: bool,
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
            if meta.path.is_ident("identity") {
                if repr_c.is_identity {
                    return Err(meta.error("Duplicate `identity` within attribute"));
                }
                repr_c.is_identity = true;
                return Ok(());
            }

            if meta.path.is_ident("transparent") {
                if repr_c.is_transparent {
                    return Err(meta.error("Duplicate `transparent` within attribute"));
                }
                repr_c.is_transparent = true;
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

            if meta.path.is_ident("allow_zero_tag") {
                if repr_c.allow_zero_tag {
                    return Err(meta.error("Duplicate `allow_zero_tag` within attribute"));
                }
                repr_c.allow_zero_tag = true;
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
        return Ok(repr_c);
    }

    if repr_c.niche_value.is_none()
        && repr_c.is_valid.is_none()
        && !repr_c.is_identity
        && !repr_c.is_transparent
        && !repr_c.is_view
        && !repr_c.is_wide_data
        && !repr_c.allow_zero_tag
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
    let mut zero_discriminant_check = TokenStream::new();

    if repr_c_attrs.is_transparent {
        if repr_c_attrs.niche_value.is_some() {
            push_error(
                &mut errors,
                syn::Error::new_spanned(
                    &input.ident,
                    "`repr_c(transparent)` cannot be combined with `NICHE`",
                ),
            );
        }
        if repr_c_attrs.is_valid.is_some() {
            push_error(
                &mut errors,
                syn::Error::new_spanned(
                    &input.ident,
                    "`repr_c(transparent)` cannot be combined with `is_valid`",
                ),
            );
        }
        if repr_c_attrs.is_identity || repr_c_attrs.is_view || repr_c_attrs.is_wide_data {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "`repr_c(transparent)` cannot be combined with other representation options",
            ));
        }
        let fieldless_enum = matches!(&input.data, syn::Data::Enum(data)
            if !data.variants.is_empty()
                && data.variants.iter().all(|variant| matches!(variant.fields, syn::Fields::Unit)));
        let fields = match &input.data {
            syn::Data::Struct(data) => Some(&data.fields),
            syn::Data::Enum(data) if data.variants.len() == 1 || fieldless_enum => {
                Some(&data.variants[0].fields)
            }
            _ => None,
        };
        let Some(fields) = fields else {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "`repr_c(transparent)` requires a struct, a fieldless enum, or a single-variant enum",
            ));
        };
        if fields.len() != 1 && !(matches!(input.data, syn::Data::Enum(_)) && fields.is_empty()) {
            return Err(syn::Error::new_spanned(
                &input.ident,
                "`repr_c(transparent)` requires exactly one field (or a fieldless enum variant)",
            ));
        }
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
            syn::Data::Enum(data)
                if matches!(
                    repr_attr,
                    Some(ReprKind::Primitive(_) | ReprKind::C(Some(_)))
                ) && is_exhaustive_enum(
                    data.variants.len(),
                    &enum_tag_type(repr_attr, data.variants.len()),
                ) => {}
            _ => push_error(
                &mut errors,
                syn::Error::new_spanned(
                    &input.ident,
                    "`repr_c(identity)` requires a struct or an exhaustive enum",
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
            if repr_c_attrs.allow_zero_tag {
                push_error(
                    &mut errors,
                    syn::Error::new_spanned(
                        &input.ident,
                        "`allow_zero_tag` is only supported on enum variants",
                    ),
                );
            }
            validate_fields_no_ffi_type_attr(&data.fields, &mut errors);
            if let Err(err) = type_is_valid_closure(&data.fields, &repr_c_attrs.is_valid) {
                push_error(&mut errors, err);
            }
        }
        syn::Data::Enum(data) => {
            if repr_c_attrs.allow_zero_tag {
                push_error(
                    &mut errors,
                    syn::Error::new_spanned(
                        &input.ident,
                        "`allow_zero_tag` belongs on the zero-tag variant",
                    ),
                );
            }
            if repr_c_attrs.is_valid.is_some() {
                let err_msg = "`is_valid` is only supported on structs or enum variants";
                push_error(&mut errors, syn::Error::new_spanned(&input.ident, err_msg));
            }

            if repr_c_attrs.niche_value.is_some() {
                let err_msg = "`NICHE` is only supported on structs";
                push_error(&mut errors, syn::Error::new_spanned(&input.ident, err_msg));
            }

            if matches!(repr_attr, Some(ReprKind::C(None))) {
                let err_msg = "#[repr(C)]` not supported; use `#[repr(int)]`/`#[repr(C, int)]`";
                push_error(&mut errors, syn::Error::new_spanned(&input.ident, err_msg));
            }

            let is_fieldless = data
                .variants
                .iter()
                .all(|variant| matches!(variant.fields, syn::Fields::Unit));
            let has_integer_repr = matches!(
                repr_attr,
                Some(ReprKind::Primitive(_) | ReprKind::C(Some(_)))
            );

            for variant in &data.variants {
                validate_fields_no_ffi_type_attr(&variant.fields, &mut errors);

                let variant_repr_c_attrs = parse_repr_c_attrs(&variant.attrs)?;
                if repr_c_attrs.is_identity && variant_repr_c_attrs.is_valid.is_some() {
                    push_error(
                        &mut errors,
                        syn::Error::new_spanned(
                            &variant.ident,
                            "`repr_c(identity)` cannot be combined with variant `is_valid`",
                        ),
                    );
                }
                if let Err(err) =
                    type_is_valid_closure(&variant.fields, &variant_repr_c_attrs.is_valid)
                {
                    push_error(&mut errors, err);
                }
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
                variant_attrs.push(VariantReprCAttrs {
                    is_valid: variant_repr_c_attrs.is_valid,
                    allow_zero_tag: variant_repr_c_attrs.allow_zero_tag,
                });
            }

            if (is_fieldless || has_integer_repr)
                && !data.variants.is_empty()
                && !(is_fieldless && data.variants.len() == 1)
                && !matches!(repr_attr, Some(ReprKind::C(None)))
                && !is_transparent_enum_repr(repr_attr, &data.variants)
                && !is_exhaustive_enum(
                    data.variants.len(),
                    &enum_tag_type(repr_attr, data.variants.len()),
                )
            {
                let name = &input.ident;
                let tag_ty = enum_tag_type(repr_attr, data.variants.len());
                let tag_values = enum_variant_tag_exprs(&data.variants, &tag_ty);
                let variant_checks = data
                    .variants
                    .iter()
                    .zip(&variant_attrs)
                    .zip(tag_values)
                    .map(|((variant, attrs), value)| {
                        let variant_name = &variant.ident;
                        let value = if is_fieldless {
                            let tag = format_ident!("__CO3_TAG_{}", variant_name);
                            quote! { #name::#tag }
                        } else {
                            value
                        };
                        if attrs.allow_zero_tag {
                            quote_spanned! {variant_name.span()=>
                                const _: () = assert!(
                                    #value == 0,
                                    concat!("`allow_zero_tag` on ", stringify!(#variant_name), " requires tag zero")
                                );
                            }
                        } else {
                            quote_spanned! {variant_name.span()=>
                                const _: () = assert!(
                                    #value != 0,
                                    concat!(
                                        "variant ", stringify!(#variant_name),
                                        " has tag zero; zero can serve as a niche to optimize some representations; add #[repr_c(allow_zero_tag)] to use zero here and select another niche"
                                    )
                                );
                            }
                        }
                    });
                zero_discriminant_check = quote! { #(#variant_checks)* };
            }

            for (variant, attrs) in data.variants.iter().zip(&variant_attrs) {
                if !attrs.allow_zero_tag {
                    continue;
                }
                let message = if matches!(repr_attr, Some(ReprKind::Transparent)) {
                    Some("`allow_zero_tag` cannot be combined with `#[repr(transparent)]`")
                } else if is_fieldless && data.variants.len() == 1 {
                    Some("`allow_zero_tag` requires more than one fieldless variant")
                } else if !is_fieldless && !has_integer_repr {
                    Some("`allow_zero_tag` on data-carrying enums requires `#[repr(int)]`")
                } else if !matches!(repr_attr, Some(ReprKind::C(None)))
                    && is_exhaustive_enum(
                        data.variants.len(),
                        &enum_tag_type(repr_attr, data.variants.len()),
                    )
                {
                    Some("`allow_zero_tag` cannot be used on an exhaustive enum")
                } else {
                    None
                };
                if let Some(message) = message {
                    push_error(
                        &mut errors,
                        syn::Error::new_spanned(&variant.ident, message),
                    );
                }
            }
        }
        syn::Data::Union(_) => {
            return Err(syn::Error::new_spanned(input, "Unions are not supported"));
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    let primitive_type_checks = gen_primitive_field_type_checks(input);

    if repr_c_attrs.is_transparent
        && !matches!(&input.data, syn::Data::Enum(data) if data.variants.iter().all(|variant| variant.fields.is_empty()))
    {
        let item = field::derive_field_repr_c(input, repr_attr, &repr_c_attrs);
        let wide = wide::expand(input, repr_attr)?;
        let ctype_impl = gen_source_ctype_impl(input, &repr_c_attrs);
        return Ok(quote! { #primitive_type_checks #item #wide #ctype_impl });
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
            if repr_c_attrs.is_identity {
                derive_item(
                    repr_attr,
                    repr_alignment,
                    input,
                    &repr_c_attrs,
                    &variant_attrs,
                )
            } else if data
                .variants
                .iter()
                .all(|v| matches!(v.fields, syn::Fields::Unit))
            {
                derive_fieldless_enum(
                    repr_attr,
                    repr_alignment,
                    input,
                    repr_c_attrs.is_transparent,
                )
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
        let ctype_impl = gen_source_ctype_impl(input, &repr_c_attrs);
        let body = quote! {
            #primitive_type_checks
            #zero_discriminant_check

            #tokens
            #ctype_impl
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

fn gen_source_ctype_impl(input: &syn::DeriveInput, attrs: &ReprCAttrs) -> TokenStream {
    if attrs.is_identity || attrs.is_view || attrs.is_wide_data {
        return quote! {};
    }

    let field_types = match &input.data {
        syn::Data::Struct(data) => data
            .fields
            .iter()
            .map(|field| &field.ty)
            .collect::<Vec<_>>(),
        syn::Data::Enum(data) => data
            .variants
            .iter()
            .flat_map(|variant| variant.fields.iter().map(|field| &field.ty))
            .collect::<Vec<_>>(),
        syn::Data::Union(_) => return quote! {},
    };
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|clause| &clause.predicates);
    let speculative = speculative_marker(generics.params.is_empty());
    let (last_field_sized_bound, omit_c_fn_impls) =
        ctype::c_fn_last_field_bounds(&field_types, generics);
    let field_bounds = field_types.iter().map(|field_type| {
        let speculative =
            (!is_parametrized(field_type, generics)).then_some(quote!(for<'_co3_field>));
        quote!(#speculative #field_type: co3::CType,)
    });

    let c_fn_impls = (!omit_c_fn_impls).then(|| {
        quote! {
            unsafe impl #impl_generics co3::CFnArg for #name #ty_generics
            where
                #last_field_sized_bound
                #speculative Self: co3::CType + co3::rust_spec::RustSpec<
                    Size = co3::rust_spec::size::Sized<co3::rust_spec::Gt<co3::rust_spec::Zero>>,
                >,
                #predicates
            {}
            unsafe impl #impl_generics co3::CFnReturn for #name #ty_generics
            where
                #last_field_sized_bound
                #speculative Self: co3::CType + co3::rust_spec::RustSpec<
                    Size = co3::rust_spec::size::Sized<co3::rust_spec::Gt<co3::rust_spec::Zero>>,
                >,
                #predicates
            {}
        }
    });

    quote! {
        unsafe impl #impl_generics co3::CType for #name #ty_generics
        where
            #speculative Self: co3::rust_spec::RustSpec<
                Layout = co3::rust_spec::Stable,
                Trap = co3::rust_spec::layout::Robust,
                Drop = co3::rust_spec::drop::NoDrop,
            >,
            #(#field_bounds)*
            #predicates
        {}

        #c_fn_impls

        unsafe impl #impl_generics co3::borrow::BorrowCast for #name #ty_generics
        where
            #speculative Self: co3::CType,
            #predicates
        {
            type AsConst = Self;
        }
        unsafe impl #impl_generics co3::borrow::BorrowCastMut for #name #ty_generics
        where
            #speculative Self: co3::CType,
            #predicates
        {
            type AsMut = Self;
        }
    }
}

fn gen_primitive_field_type_checks(input: &syn::DeriveInput) -> TokenStream {
    fn field_check(ty: &syn::Type) -> TokenStream {
        let primitive = crate::ffi_fn::gen_implicit_primitive_checks(ty);
        let mut ungrouped = ty;
        loop {
            ungrouped = match ungrouped {
                syn::Type::Group(group) => &group.elem,
                syn::Type::Paren(paren) => &paren.elem,
                _ => break,
            };
        }
        let option = if crate::ffi_fn::option_inner_type(ungrouped).is_some() {
            quote! {
                {
                    trait ExactOption {}
                    impl<T> ExactOption for core::option::Option<T> {}
                    fn check<T: ExactOption>() {}
                    let _ = check::<#ty>;
                }
            }
        } else {
            TokenStream::new()
        };
        quote! { #primitive #option }
    }

    let checks = match &input.data {
        syn::Data::Struct(data) => data
            .fields
            .iter()
            .filter_map(|field| {
                let check = field_check(&field.ty);
                if check.is_empty() {
                    return None;
                }
                let cfg = crate::utils::cfg_attrs(&field.attrs);
                Some(quote! { #(#cfg)* #check })
            })
            .collect::<Vec<_>>(),
        syn::Data::Enum(data) => data
            .variants
            .iter()
            .flat_map(|variant| {
                let variant_cfg = crate::utils::cfg_attrs(&variant.attrs).collect::<Vec<_>>();
                variant.fields.iter().filter_map(move |field| {
                    let check = field_check(&field.ty);
                    if check.is_empty() {
                        return None;
                    }
                    let field_cfg = crate::utils::cfg_attrs(&field.attrs);
                    Some(quote! { #(#variant_cfg)* #(#field_cfg)* #check })
                })
            })
            .collect::<Vec<_>>(),
        syn::Data::Union(_) => return TokenStream::new(),
    };
    if checks.is_empty() {
        return TokenStream::new();
    }

    let (impl_generics, _, where_clause) = input.generics.split_for_impl();
    quote! {
        const _: () = {
            #[allow(dead_code)]
            fn check #impl_generics () #where_clause {
                #(#checks)*
            }
        };
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

/// Check whether a type mentions any type, lifetime, or const parameter.
pub(super) fn is_parametrized(ty: &syn::Type, generics: &syn::Generics) -> bool {
    struct LifetimeUse<'a> {
        generics: &'a syn::Generics,
        found: bool,
    }

    impl Visit<'_> for LifetimeUse<'_> {
        fn visit_lifetime(&mut self, lifetime: &syn::Lifetime) {
            if self
                .generics
                .lifetimes()
                .any(|param| param.lifetime.ident == lifetime.ident)
            {
                self.found = true;
            }

            syn::visit::visit_lifetime(self, lifetime);
        }
    }

    // syn can parse Foo<N> as a type argument even when N is a const parameter.
    let type_or_const = generics.params.iter().filter_map(|param| match param {
        syn::GenericParam::Type(param) => Some(&param.ident),
        syn::GenericParam::Const(param) => Some(&param.ident),
        syn::GenericParam::Lifetime(_) => None,
    });
    if ParamUseDetector::new(type_or_const).type_mentions_param(ty) {
        return true;
    }

    let mut visitor = LifetimeUse {
        generics,
        found: false,
    };

    visitor.visit_type(ty);
    visitor.found
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

pub(super) fn enum_variant_tag_exprs(
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
    tag_ty: &syn::Type,
) -> Vec<TokenStream> {
    let mut last_explicit = None::<&syn::Expr>;
    let mut offset = 0usize;
    variants
        .iter()
        .map(|variant| {
            if let Some((_, value)) = &variant.discriminant {
                last_explicit = Some(value);
                offset = 0;
                quote! { (#value) as #tag_ty }
            } else if let Some(value) = last_explicit {
                offset += 1;
                let offset = proc_macro2::Literal::usize_unsuffixed(offset);
                quote! { ((#value) as #tag_ty).wrapping_add(#offset as #tag_ty) }
            } else {
                let value = proc_macro2::Literal::usize_unsuffixed(offset);
                offset += 1;
                quote! { #value as #tag_ty }
            }
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::{derive_repr_c, is_parametrized};

    #[test]
    fn zero_discriminant_requires_marked_variant() {
        let input = syn::parse_quote! {
            enum Error { A, B, C }
        };
        let expansion = derive_repr_c(&input).unwrap().to_string();
        assert!(expansion.contains("has tag zero"));

        let input = syn::parse_quote! {
            enum Error { #[repr_c(allow_zero_tag)] A, B, C }
        };
        let expansion = derive_repr_c(&input).unwrap().to_string();
        assert!(!expansion.contains("variant A has tag zero"));
        assert!(expansion.contains("requires tag zero"));
    }

    #[test]
    fn plain_repr_c_enum_reports_its_existing_diagnostic() {
        let fieldless = syn::parse_quote! {
            #[repr(C)]
            enum Fieldless { First, Second }
        };
        let data = syn::parse_quote! {
            #[repr(C)]
            enum WithData { First(u8), Second }
        };
        for input in [fieldless, data] {
            let error = derive_repr_c(&input).unwrap_err();
            assert!(error.to_string().contains("#[repr(C)]` not supported"));
        }
    }

    #[test]
    fn nonzero_discriminants_do_not_require_opt_in() {
        let input = syn::parse_quote! {
            #[repr(u8)]
            enum Error { A = 1, B, C }
        };
        let expansion = derive_repr_c(&input).unwrap().to_string();
        assert!(expansion.contains("has tag zero"));
    }

    #[test]
    fn explicit_discriminant_without_repr_is_left_to_rustc() {
        let input = syn::parse_quote! {
            enum Error { A = 1, B(u8), C }
        };
        assert!(derive_repr_c(&input).is_ok());
    }

    #[test]
    fn full_u8_range_does_not_require_a_niche_opt_in() {
        use quote::{format_ident, quote};

        let variants = (0..=u8::MAX)
            .map(|value| format_ident!("V{value}"))
            .collect::<Vec<_>>();
        for repr in [quote! {}, quote! { #[repr(u8)] }] {
            let input = syn::parse2(quote! {
                #repr
                enum FullRange { #(#variants,)* }
            })
            .unwrap();
            let expansion = derive_repr_c(&input).unwrap().to_string();
            assert!(!expansion.contains("has tag zero"));
        }
    }

    #[test]
    fn exhaustive_enums_reject_allow_zero_tag() {
        use quote::{format_ident, quote};

        let variants = (0..=u8::MAX)
            .map(|value| format_ident!("V{value}"))
            .collect::<Vec<_>>();
        let first = &variants[0];
        let rest = &variants[1..];
        for input in [
            quote! {
                enum InferredFullRange { #[repr_c(allow_zero_tag)] #first, #(#rest,)* }
            },
            quote! {
                #[repr(u8)]
                enum FullRange { #[repr_c(allow_zero_tag)] #first, #(#rest,)* }
            },
            quote! {
                #[repr(u8)]
                enum FullRangeWithData { #[repr_c(allow_zero_tag)] #first(u8), #(#rest,)* }
            },
        ] {
            let input = syn::parse2(input).unwrap();
            let error = derive_repr_c(&input).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("cannot be used on an exhaustive enum")
            );
        }
    }

    #[test]
    fn detects_type_lifetime_and_const_arguments() {
        let generics = syn::parse_quote!(<'a, T, const N: usize>);

        assert!(is_parametrized(&syn::parse_quote!(T), &generics));
        assert!(is_parametrized(&syn::parse_quote!(&'a u8), &generics));
        assert!(is_parametrized(&syn::parse_quote!(Wrapper<N>), &generics));
        assert!(is_parametrized(&syn::parse_quote!([u8; N]), &generics));
        assert!(!is_parametrized(&syn::parse_quote!(Wrapper<1>), &generics));
    }
}
