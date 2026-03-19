//! Crate containing FFI related macro functionality
use std::marker::PhantomData;

use manyhow::manyhow;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Item, ItemFn, ItemImpl, LitStr, Path, Result, Type, punctuated::Punctuated};

use crate::{
    dispatch::{find_dispatch_attr, parse_dispatch_attr, parse_handle_id_attr},
    generate::{emit_decl_exports, expand_extern_import_decls},
    repr::derive_extern_c,
    utils::{has_non_lifetime_generics, is_drop_impl, path_symbol_name, type_symbol_name},
    validate::{validate_export_decls, validate_extern_decls},
};

mod attr;
mod dispatch;
mod ffi_fn;
mod generate;
mod parse;
mod repr;
mod utils;
mod validate;
mod wrapper;

enum ExportBlock {}
enum ExternBlock {}

enum DropAttrKind {
    ExportName,
    LinkName,
}

trait InputKind {
    const DROP_ATTR_KIND: DropAttrKind;
    const MISSING_DROP_ERR: Option<&'static str>;
}

impl InputKind for ExportBlock {
    const DROP_ATTR_KIND: DropAttrKind = DropAttrKind::ExportName;
    const MISSING_DROP_ERR: Option<&'static str> = None;
}

impl InputKind for ExternBlock {
    const DROP_ATTR_KIND: DropAttrKind = DropAttrKind::LinkName;
    const MISSING_DROP_ERR: Option<&'static str> =
        Some("extern types must provide `#![link(crate = \"...\")]` or explicit `impl Drop`");
}

struct Input<T> {
    abi: syn::Abi,
    attrs: Vec<syn::Attribute>,
    decls: Vec<DeclItem>,

    _kind: PhantomData<T>,
}

enum DeclItem {
    Item(ForeignItem),
    Dispatch(DispatchItem),
}

enum ForeignItem {
    Fn(ItemFn),
    Impl(ItemImpl),
    Type(ForeignItemType),
}

struct DispatchItem {
    impl_: ItemImpl,
    self_ty_id_repr: Option<syn::Type>,
    args: Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]>,
}

struct ForeignItemType {
    id: Option<Box<syn::Type>>,
    ty: syn::ForeignItemType,
    drop: Option<DropImpl>,
}

enum DropImpl {
    Impl(ItemImpl),
    Dispatch(DispatchItem),
}

// TODO: reprC(`local`) is a workaround for https://github.com/rust-lang/rust/issues/48214
// because some derived types cannot derive `NonLocal` othwerise. Should be removed in future
/// Derive implementations of traits required to convert to and from an FFI-compatible type
///
/// # Attributes
///
/// * `#[reprC(NICHE_VALUE = <expr>, unsafe(is_valid = |target| ...))]`
/// customize [`co3::niche::Niche`] value and validation function for `#[repr(transparent)]` types.
/// `NICHE_VALUE` can be ommitted in which case the implementation delegates to the wrapped type.
///
/// # Safety
///
/// `is_valid` must not return false positives
///
/// Check [`co3::transmute::CheckedTransmute`] or [`co3::reprC`] for more details
///
/// * `#[reprC(local)]`
/// marks the type as local, meaning it contains references to the local frame. If a type
/// contains references to the local frame you won't be able to return it from an FFI function
/// because the frame is destroyed on function return which would invalidate your type's references.
///
/// Only applicable to data-carrying enums.
///
/// NOTE: This attribute is likely to be removed in future versions
///
/// * `#[reprC(unsafe(non_owning))]`
/// when a type contains a raw pointer (e.g. `*const T`/*mut T`) it's not possible to figure out
/// whether it carries ownership of the data pointed to. Place this attribute on the field to
/// indicate pointer doesn't own the data and is robust in the type. If the type
/// is not carrying ownership, but is not robust convert it into an equivalent [`co3::ReprC`]
/// type that is validated when crossing the FFI boundary. It is also ok to mark non-owning,
/// non-robust type as opaque via `export_!`/`export_C!` or `extern_!`/`extern_C!` `type Foo;`
///
/// # Safety
///
/// * wrapping type must allow for all possible values of the pointer including `null` (it's robust)
/// * the wrapping types's field of the pointer type must not carry ownership (it's non owning)
///
/// ```
/// use co3::ReprC as ReprCAlias;
///
/// #[derive(ReprCAlias)]
/// pub struct Hello(u32);
/// ```
///
/// It assumes that the derive is imported and referred to by its original name.
#[manyhow]
#[proc_macro_derive(ReprC, attributes(reprC))]
pub fn repr_c_derive(item: syn::DeriveInput) -> Result<TokenStream> {
    if let Some(export_attr) = item.attrs.iter().find(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|seg| seg.ident == "export")
    }) {
        return Err(syn::Error::new_spanned(
            export_attr,
            "Opaque items can't derive ReprC",
        ));
    }

    derive_extern_c(&item)
}

#[manyhow]
#[proc_macro]
pub fn export_(input: TokenStream) -> Result<TokenStream> {
    export__(input)
}

#[manyhow]
#[proc_macro]
pub fn extern_(input: TokenStream) -> Result<TokenStream> {
    extern__(input)
}

/// [`export_`] with abi set to `"C"`
#[manyhow]
#[proc_macro]
#[allow(non_snake_case)]
pub fn export_C(input: TokenStream) -> Result<TokenStream> {
    export__(quote! {
        #![abi = "C"]
        #input
    })
}

/// [`extern_`] with abi set to `"C"`
#[manyhow]
#[proc_macro]
#[allow(non_snake_case)]
pub fn extern_C(input: TokenStream) -> Result<TokenStream> {
    extern__(quote! {
        #![abi = "C"]
        #input
    })
}

fn export__(input: TokenStream) -> Result<TokenStream> {
    let input = syn::parse2::<Input<ExportBlock>>(input)?;

    let Input { abi, decls, .. } = input;
    Ok(emit_decl_exports(abi, decls))
}

fn extern__(input: TokenStream) -> Result<TokenStream> {
    let input = syn::parse2::<Input<ExternBlock>>(input)?;

    let Input {
        abi, attrs, decls, ..
    } = input;

    Ok(expand_extern_import_decls(abi, &attrs, decls))
}

/// Generate FFI functions
///
/// Works on `impl` blocks and free `fn` items.
/// Type items are not supported by this attribute; use `export_!`/`export_C!` for selecting individual exports.
///
/// # Example:
/// ```rust
/// use co3::{ReprC, export, export_C};
///
/// trait MyTrait {
///     fn foo();
/// }
///
/// #[derive(ReprC, Clone)]
/// #[repr(transparent)]
/// pub struct Foo(u8);
///
/// #[export("C")]
/// impl MyTrait for Foo {
///     fn foo() {}
/// }
///
/// #[export("C", crate = "this_crate")]
/// impl Foo {
///     pub fn new(id: u8) -> Self {
///         Self(id)
///     }
///
///     pub fn new2(id: u8) -> Self {
///         Self(id)
///     }
/// }
///
/// #[export("C")]
/// fn selected_only() -> Foo {
///     Foo::new(7)
/// }
///
/// fn selected_only2() -> Foo {
///     Foo::new(7)
/// }
///
/// export_C! {
///     impl Foo {
///         pub fn new2(id: u8) -> Self;
///     }
///
///     fn selected_only2() -> Foo;
/// }
/// ```
#[manyhow]
#[proc_macro_attribute]
pub fn export(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let generics_err = "generic types are not supported by `#[export]`; use `export_!`/`export_C!`";

    let ExportAttrArgs { abi, crate_name } = parse_export_attr(attr)?;

    let mut item = syn::parse2::<Item>(item)?;
    let result = match &mut item {
        Item::Impl(item) => {
            let attrs = take_forwarded_export_attrs(&mut item.attrs);
            let (impl_generics, _, where_clause) = item.generics.split_for_impl();

            let defaultness = &item.defaultness;
            let unsafety = &item.unsafety;
            let self_ty = &item.self_ty;
            let trait_ = item.trait_.as_ref().map(|(_, path, _)| quote!(#path for));

            let items = item.items.iter_mut().filter_map(|item| {
                let syn::ImplItem::Fn(method) = item else {
                    return None;
                };

                let attrs = take_forwarded_export_attrs(&mut method.attrs);
                let defaultness = &method.defaultness;
                let mut sig = method.sig.clone();

                ensure_export_arg_names(&mut sig);
                Some(quote! { #(#attrs)* #defaultness #sig; })
            });

            quote! {
                #(#attrs)*
                #defaultness #unsafety impl #impl_generics #trait_ #self_ty #where_clause {
                    #(#items)*
                }
            }
        }
        Item::Fn(item) => {
            let attrs = take_forwarded_export_attrs(&mut item.attrs);
            let mut sig = item.sig.clone();

            let vis = &item.vis;
            ensure_export_arg_names(&mut sig);
            quote! { #(#attrs)* #vis #sig; }
        }
        Item::Struct(item) => {
            let item_id_ty = parse_handle_id_attr(&mut item.attrs)?.map(|ty| quote!(#[id(#ty)]));

            if !item.generics.params.is_empty() {
                return Err(syn::Error::new_spanned(&item.generics, generics_err));
            }

            let vis = &item.vis;
            let ident = &item.ident;

            quote! { #item_id_ty #vis type #ident; }
        }
        Item::Enum(item) => {
            let item_id_ty = parse_handle_id_attr(&mut item.attrs)?.map(|ty| quote!(#[id(#ty)]));

            if !item.generics.params.is_empty() {
                return Err(syn::Error::new_spanned(&item.generics, generics_err));
            }

            let vis = &item.vis;
            let ident = &item.ident;

            quote! { #item_id_ty #vis type #ident; }
        }
        Item::Union(item) => {
            let item_id = parse_handle_id_attr(&mut item.attrs)?.map(|ty| quote!(#[id(#ty)]));

            if !item.generics.params.is_empty() {
                return Err(syn::Error::new_spanned(&item.generics, generics_err));
            }

            let vis = &item.vis;
            let ident = &item.ident;

            quote! { #item_id #vis type #ident; }
        }
        item => return Err(syn::Error::new_spanned(&*item, "Item not supported")),
    };

    let export_crate_attr = crate_name
        .as_ref()
        .map(|crate_name| quote!(#![export(crate = #crate_name)]));

    let exports = export__(quote! {
        #![abi = #abi]
        #export_crate_attr
        #result
    })?;

    Ok(quote! {
        #item
        #exports
    })
}

impl<T: InputKind> Input<T> {
    fn new(
        mut attrs: Vec<syn::Attribute>,
        crate_name: Option<LitStr>,
        decls: Vec<ForeignItem>,
    ) -> Result<Self> {
        let abi = parse_abi_attr(&mut attrs)?;
        let mut decls = pack_type_drop_impls::<T>(crate_name, decls)?;

        for item in &mut decls {
            match item {
                ForeignItem::Type(ForeignItemType { id, ty, drop }) => {
                    ensure_single_dispatch_attr(&ty.attrs)?;

                    if let Some(drop) = drop {
                        let attrs = match drop {
                            DropImpl::Impl(impl_) => &impl_.attrs,
                            DropImpl::Dispatch(dispatch) => &dispatch.impl_.attrs,
                        };

                        ensure_single_dispatch_attr(attrs)?;
                    }

                    if !ty.generics.params.is_empty() && id.is_none() {
                        let err_msg = "Generic types must declare handle #[id(...)]";
                        return Err(syn::Error::new_spanned(ty, err_msg));
                    }
                }
                ForeignItem::Fn(ItemFn { attrs, .. })
                | ForeignItem::Impl(ItemImpl { attrs, .. }) => {
                    ensure_single_dispatch_attr(attrs)?;
                }
            }
        }

        let decls = decls
            .into_iter()
            .map(|item| {
                Ok(match item {
                    ForeignItem::Impl(mut impl_) if find_dispatch_attr(&impl_.attrs).is_some() => {
                        let args = parse_dispatch_attr(&impl_)?;
                        let self_ty_id_repr = parse_self_ty_id_repr(&impl_.self_ty);
                        impl_.attrs.retain(|attr| !attr.path().is_ident("dispatch"));
                        DeclItem::Dispatch(DispatchItem {
                            impl_,
                            self_ty_id_repr,
                            args,
                        })
                    }
                    item => DeclItem::Item(item),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            abi,
            attrs,
            decls,

            _kind: PhantomData,
        })
    }
}

impl syn::parse::Parse for Input<ExportBlock> {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let mut attrs = input.call(syn::Attribute::parse_inner)?;
        let export_crate = parse_export_crate_attr(&mut attrs)?;

        for attr in &attrs {
            if !attr.path().is_ident("abi") {
                let err_msg = "Attribute not supported in this position";
                return Err(syn::Error::new_spanned(attr, err_msg));
            }
        }

        let mut decls = ExportBlock::parse_items(input)?;

        for item in &mut decls {
            match item {
                ForeignItem::Fn(ItemFn { attrs, sig, .. }) => {
                    ensure_export_name_on_fn(attrs, &export_crate, &sig.ident);
                }
                ForeignItem::Impl(impl_) => {
                    let trait_ = impl_.trait_.as_ref().map(|(_, path, _)| path);
                    let self_ty = &impl_.self_ty;

                    for item in &mut impl_.items {
                        let syn::ImplItem::Fn(syn::ImplItemFn { attrs, sig, .. }) = item else {
                            continue;
                        };

                        ensure_export_name_on_impl_fn(
                            attrs,
                            &export_crate,
                            trait_,
                            self_ty,
                            &impl_.generics,
                            &sig.ident,
                        );
                    }
                }
                _ => {}
            }
        }

        validate_export_decls(&decls)?;
        Self::new(attrs, Some(export_crate), decls)
    }
}

impl syn::parse::Parse for Input<ExternBlock> {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let mut attrs = input.call(syn::Attribute::parse_inner)?;

        let link_crate = parse_link_crate_attr(&mut attrs)?;
        let mut decls = ExternBlock::parse_items(input)?;

        let mut errors = None::<syn::Error>;
        for decl in &mut decls {
            match decl {
                ForeignItem::Fn(ItemFn { attrs, sig, .. }) => {
                    let fn_name = &sig.ident;

                    if !attrs.iter().any(is_link_name_attr)
                        && let Some(link_crate) = &link_crate
                    {
                        let link_name = LitStr::new(
                            &format!("{}__{fn_name}", link_crate.value()),
                            fn_name.span(),
                        );

                        attrs.push(syn::parse_quote!(#[link_name = #link_name]));
                    }
                }
                ForeignItem::Impl(impl_) => {
                    for item in &mut impl_.items {
                        let trait_ = impl_.trait_.as_ref().map(|(_, path, _)| path);
                        let self_ty = type_symbol_name(&impl_.self_ty, &impl_.generics);

                        let syn::ImplItem::Fn(syn::ImplItemFn { attrs, sig, .. }) = item else {
                            continue;
                        };

                        let fn_name = &sig.ident;
                        if !attrs.iter().any(is_link_name_attr)
                            && let Some(link_crate) = &link_crate
                        {
                            let link_name = if let Some(trait_) = trait_ {
                                let trait_ = path_symbol_name(trait_, &impl_.generics);
                                LitStr::new(
                                    &format!(
                                        "{}__{trait_}__{self_ty}__{fn_name}",
                                        link_crate.value()
                                    ),
                                    fn_name.span(),
                                )
                            } else {
                                LitStr::new(
                                    &format!("{}__{self_ty}__{fn_name}", link_crate.value()),
                                    fn_name.span(),
                                )
                            };

                            attrs.push(syn::parse_quote!(#[link_name = #link_name]));
                        }

                        // FIXME: Consider that extern fn item imports are never mangled by the compiler!
                        // Therefore, we could link directly to the method name (aka sig.ident)
                        if !attrs.iter().any(is_link_name_attr) {
                            let err = syn::Error::new_spanned(
                                &sig.ident,
                                "Undefined link name. Use `#![link(crate = \"...\")]` or `#[link_name = \"...\"]`",
                            );

                            if let Some(errors) = &mut errors {
                                errors.combine(err);
                            } else {
                                errors = Some(err);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if let Some(errors) = errors {
            return Err(errors);
        }

        validate_extern_decls(&decls)?;
        Self::new(attrs, link_crate, decls)
    }
}

fn parse_abi_attr(attrs: &mut Vec<syn::Attribute>) -> Result<syn::Abi> {
    let mut kept = Vec::with_capacity(attrs.len());

    let mut abi = None;
    for attr in attrs.drain(..) {
        if !attr.path().is_ident("abi") {
            kept.push(attr);
            continue;
        }

        let err_msg = "Expected `#![abi = \"...\"]`";
        let syn::Meta::NameValue(nv) = &attr.meta else {
            return Err(syn::Error::new_spanned(&attr, err_msg));
        };

        let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(abi_lit),
            ..
        }) = &nv.value
        else {
            return Err(syn::Error::new_spanned(&nv.value, err_msg));
        };

        if abi.replace(syn::parse2(quote!(extern #abi_lit))?).is_some() {
            let msg = "Duplicate `#![abi = \"...\"]`";
            return Err(syn::Error::new_spanned(attr, msg));
        }
    }

    *attrs = kept;
    abi.ok_or(syn::Error::new(
        proc_macro2::Span::call_site(),
        "missing `#![abi = \"...\"]`",
    ))
}

fn parse_link_crate_attr(attrs: &mut Vec<syn::Attribute>) -> Result<Option<LitStr>> {
    parse_named_crate_attr(attrs, "link")
}

fn parse_export_crate_attr(attrs: &mut Vec<syn::Attribute>) -> Result<LitStr> {
    Ok(parse_named_crate_attr(attrs, "export")?.unwrap_or_else(|| {
        LitStr::new(
            &std::env::var("CARGO_CRATE_NAME").unwrap_or_else(|_| "co3".to_owned()),
            proc_macro2::Span::call_site(),
        )
    }))
}

fn parse_named_crate_attr(
    attrs: &mut Vec<syn::Attribute>,
    attr_name: &str,
) -> Result<Option<LitStr>> {
    let mut kept = Vec::with_capacity(attrs.len());

    let mut crate_name = None;
    for attr in attrs.drain(..) {
        if !attr.path().is_ident(attr_name) {
            kept.push(attr);
            continue;
        }

        let syn::Meta::List(list) = &attr.meta else {
            kept.push(attr);
            continue;
        };

        let Ok(metas) =
            list.parse_args_with(Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
        else {
            kept.push(attr);
            continue;
        };

        let mut kept_meta = Punctuated::<syn::Meta, syn::Token![,]>::new();
        for meta in metas {
            let syn::Meta::NameValue(nv) = &meta else {
                kept_meta.push(meta);
                continue;
            };
            let Some(ident) = nv.path.get_ident() else {
                kept_meta.push(meta);
                continue;
            };

            let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(value),
                ..
            }) = &nv.value
            else {
                kept_meta.push(meta);
                continue;
            };

            if ident == "crate" {
                if crate_name.replace(value.clone()).is_some() {
                    let msg = format!("Duplicate `#![{attr_name}(crate = \"...\")]`");
                    return Err(syn::Error::new_spanned(attr, msg));
                }
            } else {
                kept_meta.push(meta);
            }
        }

        if kept_meta.is_empty() {
            continue;
        }

        let attr_ident = format_ident!("{attr_name}");
        kept.push(syn::parse_quote!(#![#attr_ident(#kept_meta)]));
    }

    *attrs = kept;
    Ok(crate_name)
}

fn is_link_name_attr(attr: &syn::Attribute) -> bool {
    attr.path().is_ident("link_name")
}

struct ExportAttrArgs {
    abi: LitStr,
    crate_name: Option<LitStr>,
}

impl syn::parse::Parse for ExportAttrArgs {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let abi = input.parse::<LitStr>()?;
        let mut crate_name = None;

        while !input.is_empty() {
            input.parse::<syn::Token![,]>()?;
            if input.peek(syn::Token![crate]) {
                let crate_token = input.parse::<syn::Token![crate]>()?;
                input.parse::<syn::Token![=]>()?;
                let value = input.parse::<LitStr>()?;
                if crate_name.replace(value).is_some() {
                    return Err(syn::Error::new_spanned(
                        crate_token,
                        "duplicate `crate = \"...\"`",
                    ));
                }
                continue;
            }

            let unexpected: syn::Ident = input.parse()?;
            return Err(syn::Error::new_spanned(
                unexpected,
                "Unexpected characters`",
            ));
        }

        Ok(Self { abi, crate_name })
    }
}

fn parse_export_attr(attr: TokenStream) -> Result<ExportAttrArgs> {
    if attr.is_empty() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "expected ABI string literal, e.g. `#[export(\"C\")]`",
        ));
    }

    syn::parse2::<ExportAttrArgs>(attr)
}

fn has_unsafe_export_name(attr: &syn::Attribute) -> bool {
    if !attr.path().is_ident("unsafe") {
        return false;
    }

    let syn::Meta::List(meta_list) = &attr.meta else {
        return false;
    };

    let metas = meta_list
        .parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
        .ok();
    let Some(metas) = metas else {
        return false;
    };

    for meta in metas {
        let syn::Meta::NameValue(nv) = meta else {
            continue;
        };
        if !nv.path.is_ident("export_name") {
            continue;
        }
        return true;
    }

    false
}

fn take_forwarded_export_attrs(attrs: &mut Vec<syn::Attribute>) -> Vec<syn::Attribute> {
    let mut forwarded = Vec::new();

    attrs.retain(|attr| {
        if attr.path().is_ident("dispatch")
            || generate::is_unsafe_no_mangle(attr)
            || has_unsafe_export_name(attr)
        {
            forwarded.push(attr.clone());
            false
        } else {
            true
        }
    });

    forwarded
}

fn pack_type_drop_impls<T: InputKind>(
    crate_name: Option<LitStr>,
    decls: Vec<ForeignItem>,
) -> Result<Vec<ForeignItem>> {
    let mut explicit_drops = std::collections::BTreeMap::<syn::Ident, DropImpl>::new();
    let mut declared_drop_types = std::collections::BTreeSet::<syn::Ident>::new();
    let mut kept_decls = Vec::with_capacity(decls.len());
    let mut errors = None::<syn::Error>;

    for decl in decls {
        match decl {
            ForeignItem::Impl(impl_) if is_drop_impl(&impl_) => {
                let Type::Path(syn::TypePath { path, .. }) = &*impl_.self_ty else {
                    continue;
                };
                let Some(ident) = path.segments.last().map(|seg| seg.ident.clone()) else {
                    continue;
                };

                declared_drop_types.insert(ident.clone());
                let drop_impl = match parse_drop_impl(impl_) {
                    Ok(drop_impl) => drop_impl,
                    Err(err) => {
                        if let Some(errors) = &mut errors {
                            errors.combine(err);
                        } else {
                            errors = Some(err);
                        }
                        continue;
                    }
                };

                if let Some(prev) = explicit_drops.insert(ident, drop_impl) {
                    let err_msg = "duplicate explicit `impl Drop` for type";
                    let self_ty = match prev {
                        DropImpl::Impl(impl_) => impl_.self_ty,
                        DropImpl::Dispatch(dispatch) => dispatch.impl_.self_ty,
                    };
                    let err = syn::Error::new_spanned(self_ty, err_msg);

                    if let Some(errors) = &mut errors {
                        errors.combine(err);
                    } else {
                        errors = Some(err);
                    }
                }
            }
            other => kept_decls.push(other),
        }
    }

    for decl in &mut kept_decls {
        if let ForeignItem::Type(item) = decl {
            item.drop = explicit_drops.remove(&item.ty.ident);

            if item.drop.is_none() {
                if has_non_lifetime_generics(&item.ty.generics)
                    && !declared_drop_types.contains(&item.ty.ident)
                {
                    let err_msg = "generic types must provide explicit `impl Drop` declaration";
                    let err = syn::Error::new_spanned(&item.ty, err_msg);

                    if let Some(errors) = &mut errors {
                        errors.combine(err);
                    } else {
                        errors = Some(err);
                    }
                } else if let Some(crate_name) = crate_name.as_ref() {
                    item.drop = Some(DropImpl::Impl(synthesize_default_drop_impl(
                        T::DROP_ATTR_KIND,
                        crate_name,
                        &item.ty,
                    )));
                } else if let Some(err_msg) = T::MISSING_DROP_ERR {
                    let err = syn::Error::new_spanned(&item.ty, err_msg);

                    if let Some(errors) = &mut errors {
                        errors.combine(err);
                    } else {
                        errors = Some(err);
                    }
                }
            }
        }
    }

    for drop_impl in explicit_drops.into_values() {
        let err_msg = "explicit `impl Drop` is only allowed for declared types";
        let self_ty = match drop_impl {
            DropImpl::Impl(impl_) => impl_.self_ty,
            DropImpl::Dispatch(dispatch) => dispatch.impl_.self_ty,
        };
        let err = syn::Error::new_spanned(self_ty, err_msg);

        if let Some(errors) = &mut errors {
            errors.combine(err);
        } else {
            errors = Some(err);
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(kept_decls)
}

fn parse_drop_impl(mut impl_: ItemImpl) -> Result<DropImpl> {
    if find_dispatch_attr(&impl_.attrs).is_none() {
        return Ok(DropImpl::Impl(impl_));
    }

    let args = parse_dispatch_attr(&impl_)?;
    let self_ty_id_repr = parse_self_ty_id_repr(&impl_.self_ty);
    impl_.attrs.retain(|attr| !attr.path().is_ident("dispatch"));
    Ok(DropImpl::Dispatch(DispatchItem {
        impl_,
        self_ty_id_repr,
        args,
    }))
}

fn parse_self_ty_id_repr(self_ty: &syn::Type) -> Option<syn::Type> {
    let syn::Type::Paren(paren) = self_ty else {
        return None;
    };
    let syn::Type::Path(type_path) = paren.elem.as_ref() else {
        return None;
    };
    if type_path.qself.is_some() {
        return None;
    }

    let first = type_path.path.segments.first()?;
    if first.ident != "dyn" {
        return None;
    }

    let syn::PathArguments::Parenthesized(args) = &first.arguments else {
        return None;
    };

    let repr = args.inputs.first()?.clone();
    (args.inputs.len() == 1).then_some(repr)
}

fn synthesize_default_drop_impl(
    attr_kind: DropAttrKind,
    crate_name: &LitStr,
    ty: &syn::ForeignItemType,
) -> ItemImpl {
    let (impl_generics, ty_generics, where_clause) = ty.generics.split_for_impl();

    let ident = &ty.ident;
    let symbol_name = LitStr::new(
        &format!(
            "{}__{}__{}__drop",
            crate_name.value(),
            path_symbol_name(&syn::parse_quote!(Drop), &Default::default()),
            type_symbol_name(&syn::parse_quote!(#ident #ty_generics), &ty.generics),
        ),
        ident.span(),
    );

    let name_attr: syn::Attribute = match attr_kind {
        DropAttrKind::ExportName => syn::parse_quote!(#[unsafe(export_name = #symbol_name)]),
        DropAttrKind::LinkName => syn::parse_quote!(#[link_name = #symbol_name]),
    };

    syn::parse_quote! {
        impl #impl_generics Drop for #ident #ty_generics #where_clause {
            #name_attr
            fn drop(&mut self) {}
        }
    }
}


fn ensure_single_dispatch_attr(attrs: &[syn::Attribute]) -> Result<()> {
    let mut dispatch_attrs = attrs.iter().filter(|attr| attr.path().is_ident("dispatch"));

    let mut errors = None::<syn::Error>;
    if dispatch_attrs.next().is_none() {
        return Ok(());
    };

    for attr in dispatch_attrs {
        let err = syn::Error::new_spanned(attr, "duplicate `#[dispatch]` attribute");

        if let Some(errors) = &mut errors {
            errors.combine(err);
        } else {
            errors = Some(err);
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(())
}

fn ensure_export_name_on_fn(
    attrs: &mut Vec<syn::Attribute>,
    crate_name: &LitStr,
    fn_name: &syn::Ident,
) {
    let has_export_name = attrs
        .iter()
        .any(|attr| generate::is_unsafe_no_mangle(attr) || has_unsafe_export_name(attr));

    if !has_export_name {
        let crate_prefix = crate_name;
        let export_name = LitStr::new(&fn_name.to_string(), fn_name.span());

        attrs.push(syn::parse_quote! {
            #[unsafe(export_name = concat!(#crate_prefix, "__", #export_name))]
        });
    }
}

fn ensure_export_name_on_impl_fn(
    attrs: &mut Vec<syn::Attribute>,
    crate_name: &LitStr,
    trait_: Option<&Path>,
    self_ty: &Type,
    generics: &syn::Generics,
    fn_name: &syn::Ident,
) {
    let mut saw_export = false;
    let mut saw_no_mangle = false;

    attrs.retain(|attr| {
        if generate::is_unsafe_no_mangle(attr) {
            saw_no_mangle = true;
            return false;
        }
        if has_unsafe_export_name(attr) {
            saw_export = true;
        }

        true
    });

    if !saw_export {
        let crate_prefix = crate_name;
        let export_name = if saw_no_mangle {
            let literal = LitStr::new(&fn_name.to_string(), fn_name.span());
            quote!(#literal)
        } else {
            let default_export_name = trait_.map_or_else(
                || format!("{}__{}", type_symbol_name(self_ty, generics), fn_name),
                |trait_| {
                    format!(
                        "{}__{}__{}",
                        path_symbol_name(trait_, generics),
                        type_symbol_name(self_ty, generics),
                        fn_name
                    )
                },
            );
            let default_export_name =
                LitStr::new(&default_export_name, proc_macro2::Span::call_site());
            quote!(concat!(#crate_prefix, "_", #default_export_name))
        };

        attrs.push(syn::parse_quote! {
            #[unsafe(export_name = #export_name)]
        });
    }
}

fn ensure_export_arg_names(sig: &mut syn::Signature) {
    let mut arg_idx = 1usize;

    for input in sig.inputs.iter_mut() {
        let syn::FnArg::Typed(syn::PatType { pat, .. }) = input else {
            continue;
        };

        let syn::Pat::Ident(ident) = &mut **pat else {
            let ident = format_ident!("arg{arg_idx}");
            *pat = syn::parse_quote!(#ident);
            arg_idx += 1;
            continue;
        };

        ident.by_ref = None;
        ident.mutability = None;
        ident.subpat = None;
    }
}
