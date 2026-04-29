//! Crate containing FFI related macro functionality
use std::{collections::BTreeMap, marker::PhantomData};

use manyhow::manyhow;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Attribute, ItemFn, ItemImpl, LitStr, Path, Result, Type, parse_quote, parse_quote_spanned,
    punctuated::Punctuated, spanned::Spanned, visit_mut::VisitMut,
};

use crate::{
    dispatch::{find_dispatch_attr, parse_dispatch_attr, parse_handle_id_attr},
    generate::{emit_decl_exports, expand_extern_import_decls},
    parse::ParsedForeignItem,
    repr::derive_extern_c,
    utils::{
        has_non_lifetime_generics, is_drop_impl, is_type_erased, path_symbol_name, push_error,
        type_symbol_name,
    },
    validate::{validate_dispatch_self_id, validate_export_decls, validate_extern_decls},
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
    const IS_EXTERN: bool;
}

impl InputKind for ExportBlock {
    const DROP_ATTR_KIND: DropAttrKind = DropAttrKind::ExportName;
    const MISSING_DROP_ERR: Option<&'static str> = None;
    const IS_EXTERN: bool = false;
}

impl InputKind for ExternBlock {
    const DROP_ATTR_KIND: DropAttrKind = DropAttrKind::LinkName;
    const MISSING_DROP_ERR: Option<&'static str> =
        Some("extern types must provide `#![link(crate = \"...\")]` or explicit `impl Drop`");
    const IS_EXTERN: bool = true;
}

struct Input<T> {
    abi: syn::Abi,
    attrs: Vec<Attribute>,
    items: Vec<ForeignItem>,

    _kind: PhantomData<T>,
}

enum ForeignItem {
    Type(ForeignItemType),
    DynImpl(DynImpl),
    Impl(ItemImpl),
    Fn(ItemFn),
}

struct DynImpl {
    impl_: ItemImpl,
    args: Punctuated<syn::AngleBracketedGenericArguments, syn::Token![,]>,
}

struct ForeignItemType {
    ty: syn::ForeignItemType,
    id: Option<Box<syn::Type>>,
    drop: Option<DropImpl>,

    dyn_self_impls: Vec<DynImpl>,
}

enum DropImpl {
    /// Impl dispatched on `dyn Self`
    DynSelfImpl(DynImpl),
    /// Any other dispatch
    DynImpl(DynImpl),
    /// Concrete impl
    Impl(ItemImpl),
}

// TODO: reprC(`local`) is a workaround for https://github.com/rust-lang/rust/issues/48214.
// It should be removed once derived types no longer need that escape hatch.
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
#[proc_macro_attribute]
#[allow(non_snake_case)]
pub fn derive_ReprC(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    if !attr.is_empty() {
        return Err(syn::Error::new_spanned(
            attr,
            "`#[derive_ReprC]` does not accept arguments",
        ));
    }

    let item = syn::parse2::<syn::ItemTrait>(item)?;
    let trait_name = &item.ident;
    let (impl_generics, ty_generics, where_clause) = &item.generics.split_for_impl();

    Ok(quote! {
        #item

        impl #impl_generics co3::dst::DstFamily for (dyn #trait_name #ty_generics) #where_clause {
            type Kind = co3::dst::ExternTypeLike;
        }

        impl #impl_generics co3::ir::ReprFamily for (dyn #trait_name #ty_generics) #where_clause {
            type Kind = co3::ir::Opaque;
        }

        //impl #impl_generics co3::dst::TraitObjectDst for (dyn #trait_name #ty_generics) #where_clause {
        //}
    })
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

    let Input {
        abi, items: decls, ..
    } = input;
    Ok(emit_decl_exports(abi, decls))
}

fn extern__(input: TokenStream) -> Result<TokenStream> {
    let input = syn::parse2::<Input<ExternBlock>>(input)?;

    let Input {
        abi,
        attrs,
        items: decls,
        ..
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
    fn take_forwarded_export_fn_attrs(attrs: &mut Vec<Attribute>) -> Vec<Attribute> {
        let mut forwarded = Vec::new();

        attrs.retain(|attr| {
            if generate::is_unsafe_no_mangle(attr) || has_unsafe_export_name(attr) {
                forwarded.push(attr.clone());
                false
            } else {
                true
            }
        });

        forwarded
    }

    fn strip_fn_arg_attrs(signature: &mut syn::Signature) {
        for input in &mut signature.inputs {
            let attrs = match input {
                syn::FnArg::Receiver(node) => &mut node.attrs,
                syn::FnArg::Typed(node) => &mut node.attrs,
            };

            attrs.retain(|a| !a.path().is_ident("by_val") && !a.path().is_ident("unstable_refs"));
        }
    }

    let generics_err = "generic types are not supported by `#[export]`; use `export_!`/`export_C!`";
    let ExportAttrArgs { abi, crate_name } = parse_export_attr(attr)?;

    let mut item = syn::parse2::<syn::Item>(item)?;
    let result = match &mut item {
        syn::Item::Struct(item) => {
            let item_id_ty = parse_handle_id_attr(&mut item.attrs)?.map(|ty| quote!(#[id(#ty)]));

            if !item.generics.params.is_empty() {
                return Err(syn::Error::new_spanned(&item.generics, generics_err));
            }

            let vis = &item.vis;
            let ident = &item.ident;

            quote! { #item_id_ty #vis type #ident; }
        }
        syn::Item::Enum(item) => {
            let item_id_ty = parse_handle_id_attr(&mut item.attrs)?.map(|ty| quote!(#[id(#ty)]));

            if !item.generics.params.is_empty() {
                return Err(syn::Error::new_spanned(&item.generics, generics_err));
            }

            let vis = &item.vis;
            let ident = &item.ident;

            quote! { #item_id_ty #vis type #ident; }
        }
        syn::Item::Union(item) => {
            let item_id = parse_handle_id_attr(&mut item.attrs)?.map(|ty| quote!(#[id(#ty)]));

            if !item.generics.params.is_empty() {
                return Err(syn::Error::new_spanned(&item.generics, generics_err));
            }

            let vis = &item.vis;
            let ident = &item.ident;

            quote! { #item_id #vis type #ident; }
        }
        syn::Item::Fn(item) => {
            let attrs = take_forwarded_export_fn_attrs(&mut item.attrs);

            let vis = &item.vis;

            let mut sig = item.sig.clone();
            ensure_export_arg_names(&mut sig);
            strip_fn_arg_attrs(&mut item.sig);

            quote! { #(#attrs)* #vis #sig; }
        }
        syn::Item::Impl(impl_) => {
            let (impl_generics, _, where_clause) = impl_.generics.split_for_impl();

            let mut attrs = Vec::new();
            impl_.attrs.retain(|attr| {
                if attr.path().is_ident("dispatch") {
                    attrs.push(attr.clone());
                    false
                } else {
                    true
                }
            });

            let defaultness = &impl_.defaultness;
            let unsafety = &impl_.unsafety;
            let trait_ = impl_.trait_.as_ref().map(|(_, path, _)| quote!(#path for));
            let self_ty = &impl_.self_ty;

            let items = impl_.items.iter_mut().filter_map(|item| {
                let syn::ImplItem::Fn(method) = item else {
                    return None;
                };

                let attrs = take_forwarded_export_fn_attrs(&mut method.attrs);
                let (defaultness, vis) = (&method.defaultness, &method.vis);

                let mut sig = method.sig.clone();
                ensure_export_arg_names(&mut sig);
                strip_fn_arg_attrs(&mut method.sig);

                Some(quote! { #(#attrs)* #vis #defaultness #sig; })
            });

            let item_impl = quote! {
                #(#attrs)*
                #defaultness #unsafety impl #impl_generics #trait_ #self_ty #where_clause {
                    #(#items)*
                }
            };

            for param in &mut impl_.generics.params {
                let syn::GenericParam::Type(param) = param else {
                    continue;
                };

                param.attrs.retain(|attr| !is_type_erased(attr));
            }

            item_impl
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
        mut attrs: Vec<Attribute>,
        crate_name: Option<LitStr>,
        decls: Vec<ParsedForeignItem>,
    ) -> Result<Self> {
        let abi = parse_abi_attr(&mut attrs)?;

        for item in &decls {
            match item {
                ParsedForeignItem::Type(ForeignItemType {
                    ty,
                    id,
                    dyn_self_impls,
                    drop,
                }) => {
                    ensure_single_dispatch_attr(&ty.attrs)?;

                    for dispatch in dyn_self_impls {
                        ensure_single_dispatch_attr(&dispatch.impl_.attrs)?;
                    }

                    if let Some(drop) = drop {
                        let attrs = match drop {
                            DropImpl::DynSelfImpl(dispatch) => &dispatch.impl_.attrs,
                            DropImpl::DynImpl(dispatch) => &dispatch.impl_.attrs,
                            DropImpl::Impl(impl_) => &impl_.attrs,
                        };

                        ensure_single_dispatch_attr(attrs)?;
                    }

                    if !ty.generics.params.is_empty() && id.is_none() {
                        let err_msg = "Generic types must declare handle #[id(...)]";
                        return Err(syn::Error::new_spanned(ty, err_msg));
                    }
                }
                ParsedForeignItem::Fn(ItemFn { attrs, .. })
                | ParsedForeignItem::Impl(ItemImpl { attrs, .. }) => {
                    ensure_single_dispatch_attr(attrs)?;
                }
            }
        }

        let decls = decls
            .into_iter()
            .map(|item| {
                Ok(match item {
                    ParsedForeignItem::Impl(mut impl_)
                        if find_dispatch_attr(&impl_.attrs).is_some() =>
                    {
                        let args = if T::IS_EXTERN && is_drop_impl(&impl_) {
                            Punctuated::<_, _>::default()
                        } else {
                            parse_dispatch_attr(&impl_)?
                        };

                        impl_.attrs.retain(|a| !a.path().is_ident("dispatch"));
                        ForeignItem::DynImpl(DynImpl { impl_, args })
                    }
                    ParsedForeignItem::Type(item) => ForeignItem::Type(item),
                    ParsedForeignItem::Impl(impl_) => ForeignItem::Impl(impl_),
                    ParsedForeignItem::Fn(item) => ForeignItem::Fn(item),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let decls = pack_type_dispatch_impls::<T>(decls)?;
        let mut items = pack_type_drop_impls(decls)?;
        default_init::<T>(crate_name, &mut items)?;

        Ok(Self {
            abi,
            attrs,
            items,

            _kind: PhantomData,
        })
    }
}

fn default_init<T: InputKind>(
    crate_name: Option<syn::LitStr>,
    items: &mut [ForeignItem],
) -> Result<()> {
    let mut errors = None;

    for item in items {
        match item {
            ForeignItem::DynImpl(_) => {}
            ForeignItem::Type(item) => {
                if let Some(drop) = &item.drop {
                    match drop {
                        DropImpl::DynSelfImpl(_) => {}
                        DropImpl::DynImpl(_) => {}
                        DropImpl::Impl(_) => {}
                    }

                    continue;
                }

                if let Some(crate_name) = &crate_name {
                    let impl_ =
                        synthesize_default_drop_impl(T::DROP_ATTR_KIND, crate_name, &item.ty);
                    item.drop = Some(DropImpl::Impl(impl_));
                } else if let Some(err_msg) = T::MISSING_DROP_ERR {
                    push_error(&mut errors, syn::Error::new_spanned(&item.ty, err_msg));
                }
            }
            _ => {}
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(())
}

impl syn::parse::Parse for Input<ExportBlock> {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let mut attrs = input.call(Attribute::parse_inner)?;
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
                ParsedForeignItem::Fn(ItemFn { attrs, sig, .. }) => {
                    ensure_export_name_on_fn(attrs, &export_crate, &sig.ident);
                }
                ParsedForeignItem::Impl(impl_) => {
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
        let mut attrs = input.call(Attribute::parse_inner)?;

        let link_crate = parse_link_crate_attr(&mut attrs)?;
        let mut decls = ExternBlock::parse_items(input)?;

        let mut errors = None::<syn::Error>;
        for decl in &mut decls {
            match decl {
                ParsedForeignItem::Fn(ItemFn { attrs, sig, .. }) => {
                    let fn_name = &sig.ident;

                    if !attrs.iter().any(is_link_name_attr)
                        && let Some(link_crate) = &link_crate
                    {
                        let link_name = LitStr::new(
                            &format!("{}__{fn_name}", link_crate.value()),
                            fn_name.span(),
                        );

                        attrs.push(parse_quote!(#[link_name = #link_name]));
                    }
                }
                ParsedForeignItem::Impl(impl_) => {
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

                            attrs.push(parse_quote!(#[link_name = #link_name]));
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

fn parse_abi_attr(attrs: &mut Vec<Attribute>) -> Result<syn::Abi> {
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

fn parse_link_crate_attr(attrs: &mut Vec<Attribute>) -> Result<Option<LitStr>> {
    parse_named_crate_attr(attrs, "link")
}

fn parse_export_crate_attr(attrs: &mut Vec<Attribute>) -> Result<LitStr> {
    Ok(parse_named_crate_attr(attrs, "export")?.unwrap_or_else(|| {
        LitStr::new(
            &std::env::var("CARGO_CRATE_NAME").unwrap_or_else(|_| "co3".to_owned()),
            proc_macro2::Span::call_site(),
        )
    }))
}

fn parse_named_crate_attr(attrs: &mut Vec<Attribute>, attr_name: &str) -> Result<Option<LitStr>> {
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
        kept.push(parse_quote!(#![#attr_ident(#kept_meta)]));
    }

    *attrs = kept;
    Ok(crate_name)
}

fn is_link_name_attr(attr: &Attribute) -> bool {
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

fn has_unsafe_export_name(attr: &Attribute) -> bool {
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

fn pack_type_drop_impls(decls: Vec<ForeignItem>) -> Result<Vec<ForeignItem>> {
    fn insert_drop(
        explicit_drops: &mut BTreeMap<syn::Ident, DropImpl>,
        errors: &mut Option<syn::Error>,
        self_ty: syn::Ident,
        drop_impl: DropImpl,
    ) {
        if let Some(prev) = explicit_drops.insert(self_ty, drop_impl) {
            let impl_ = match prev {
                DropImpl::DynSelfImpl(d) | DropImpl::DynImpl(d) => d.impl_,
                DropImpl::Impl(impl_) => impl_,
            };

            let err_msg = "duplicate explicit `impl Drop` declaration";
            push_error(errors, syn::Error::new_spanned(impl_.self_ty, err_msg));
        }
    }

    fn self_ty_ident(impl_: &syn::ItemImpl) -> Option<syn::Ident> {
        let Type::Path(syn::TypePath { qself: None, path }) = &*impl_.self_ty else {
            return None;
        };

        if path.segments.len() > 1 {
            return None;
        }

        path.segments.last().map(|seg| seg.ident.clone())
    }

    const UNKNOWN_DROP: &str = "explicit `impl Drop` is only allowed for declared types";

    let mut kept_decls = Vec::with_capacity(decls.len());
    let mut explicit_drops = BTreeMap::new();
    let mut errors = None::<syn::Error>;

    for decl in decls {
        match decl {
            ForeignItem::Type(mut item) => {
                let mut kept_dyn_self_impls = Vec::with_capacity(item.dyn_self_impls.len());

                let self_ty = &item.ty.ident;
                for dyn_impl in item.dyn_self_impls {
                    if is_drop_impl(&dyn_impl.impl_) {
                        insert_drop(
                            &mut explicit_drops,
                            &mut errors,
                            self_ty.clone(),
                            DropImpl::DynSelfImpl(dyn_impl),
                        );
                    } else {
                        kept_dyn_self_impls.push(dyn_impl);
                    }
                }

                item.dyn_self_impls = kept_dyn_self_impls;
                kept_decls.push(ForeignItem::Type(item));
            }
            ForeignItem::Impl(impl_) => {
                if is_drop_impl(&impl_) {
                    if let Some(self_ty) = self_ty_ident(&impl_) {
                        insert_drop(
                            &mut explicit_drops,
                            &mut errors,
                            self_ty,
                            DropImpl::Impl(impl_),
                        );
                    } else {
                        let err = syn::Error::new_spanned(&impl_.self_ty, UNKNOWN_DROP);
                        push_error(&mut errors, err);
                    }
                } else {
                    kept_decls.push(ForeignItem::Impl(impl_));
                }
            }
            ForeignItem::DynImpl(dispatch) => {
                if is_drop_impl(&dispatch.impl_) {
                    if let Some(self_ty) = self_ty_ident(&dispatch.impl_) {
                        insert_drop(
                            &mut explicit_drops,
                            &mut errors,
                            self_ty,
                            DropImpl::DynImpl(dispatch),
                        );
                    } else {
                        let err = syn::Error::new_spanned(&dispatch.impl_.self_ty, UNKNOWN_DROP);
                        push_error(&mut errors, err);
                    }
                } else {
                    kept_decls.push(ForeignItem::DynImpl(dispatch));
                }
            }
            ForeignItem::Fn(item) => kept_decls.push(ForeignItem::Fn(item)),
        }
    }

    for decl in &mut kept_decls {
        let ForeignItem::Type(item) = decl else {
            continue;
        };

        item.drop = explicit_drops.remove(&item.ty.ident);
        if item.drop.is_none() && has_non_lifetime_generics(&item.ty.generics) {
            let err_msg = "generic types must provide explicit `impl Drop` declaration";
            push_error(&mut errors, syn::Error::new_spanned(&item.ty, err_msg));
        }
    }

    for drop_impl in explicit_drops.into_values() {
        let self_ty = match drop_impl {
            DropImpl::DynSelfImpl(dispatch) => dispatch.impl_.self_ty,
            DropImpl::DynImpl(dispatch) => dispatch.impl_.self_ty,
            DropImpl::Impl(impl_) => impl_.self_ty,
        };

        push_error(&mut errors, syn::Error::new_spanned(self_ty, UNKNOWN_DROP));
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(kept_decls)
}

pub(crate) fn trait_object_single_trait_bound(self_ty: &syn::Type) -> Option<&syn::TraitBound> {
    use syn::TraitBoundModifier;

    let syn::Type::TraitObject(trait_object) = self_ty else {
        return None;
    };

    let bound = trait_object.bounds.first()?;
    let syn::TypeParamBound::Trait(trait_bound) = bound else {
        return None;
    };
    if trait_bound.modifier != TraitBoundModifier::None || trait_bound.lifetimes.is_some() {
        return None;
    }

    Some(trait_bound)
}

fn pack_type_dispatch_impls<T: InputKind>(decls: Vec<ForeignItem>) -> Result<Vec<ForeignItem>> {
    fn is_self_only_dyn_dispatch(impl_: &ItemImpl) -> bool {
        trait_object_single_trait_bound(&impl_.self_ty).is_some()
            && !impl_
                .generics
                .type_params()
                .any(|param| param.attrs.iter().any(is_type_erased))
    }

    let mut type_dispatch = decls
        .iter()
        .filter_map(|decl| {
            if let ForeignItem::Type(item) = decl {
                Some((item.ty.ident.clone(), Vec::new()))
            } else {
                None
            }
        })
        .collect::<BTreeMap<_, _>>();

    let mut kept_decls = Vec::with_capacity(decls.len());
    let mut errors = None::<syn::Error>;

    for decl in decls {
        let ForeignItem::DynImpl(mut dispatch) = decl else {
            kept_decls.push(decl);
            continue;
        };

        let Some(syn::TraitBound { path, .. }) =
            trait_object_single_trait_bound(&dispatch.impl_.self_ty)
        else {
            kept_decls.push(ForeignItem::DynImpl(dispatch));
            continue;
        };
        if path.segments.len() > 1 {
            kept_decls.push(ForeignItem::DynImpl(dispatch));
            continue;
        }
        let Some(ident) = path.segments.first().map(|seg| seg.ident.clone()) else {
            kept_decls.push(ForeignItem::DynImpl(dispatch));
            continue;
        };
        if !type_dispatch.contains_key(&ident) {
            kept_decls.push(ForeignItem::DynImpl(dispatch));
            continue;
        }

        *dispatch.impl_.self_ty = parse_quote!(#path);
        normalize_self_handle_ids(&mut dispatch.impl_);

        if T::IS_EXTERN
            && let Err(err) = validate_dispatch_self_id(&dispatch.impl_)
        {
            push_error(&mut errors, err);
        }

        type_dispatch.entry(ident).or_default().push(dispatch);
    }

    for decl in &mut kept_decls {
        let ForeignItem::Type(item) = decl else {
            continue;
        };

        item.dyn_self_impls = type_dispatch.remove(&item.ty.ident).unwrap_or_default();
    }

    for decl in &kept_decls {
        let ForeignItem::DynImpl(dispatch) = decl else {
            continue;
        };

        if is_self_only_dyn_dispatch(&dispatch.impl_) {
            let err_msg = "`dyn Self` is only supported for declared types";
            let err = syn::Error::new_spanned(&dispatch.impl_.self_ty, err_msg);

            push_error(&mut errors, err);
        }
    }

    if let Some(errors) = errors {
        return Err(errors);
    }

    Ok(kept_decls)
}

fn normalize_self_handle_ids(impl_: &mut syn::ItemImpl) {
    struct SelfHandleIdNormalizer {
        self_ty: syn::Type,
    }

    impl VisitMut for SelfHandleIdNormalizer {
        fn visit_type_mut(&mut self, node: &mut syn::Type) {
            syn::visit_mut::visit_type_mut(self, node);

            let self_ty = &self.self_ty;
            if *node == parse_quote!(<dyn #self_ty>::ID) {
                *node = parse_quote_spanned!(node.span()=> <dyn Self>::ID);
            }
        }
    }

    let mut normalizer = SelfHandleIdNormalizer {
        self_ty: (*impl_.self_ty).clone(),
    };

    normalizer.visit_item_impl_mut(impl_);
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
            path_symbol_name(&parse_quote!(Drop), &Default::default()),
            type_symbol_name(&parse_quote!(#ident #ty_generics), &ty.generics),
        ),
        ident.span(),
    );

    let name_attr: Attribute = match attr_kind {
        DropAttrKind::ExportName => parse_quote!(#[unsafe(export_name = #symbol_name)]),
        DropAttrKind::LinkName => parse_quote!(#[link_name = #symbol_name]),
    };

    parse_quote! {
        impl #impl_generics Drop for #ident #ty_generics #where_clause {
            #name_attr
            fn drop(&mut self) {}
        }
    }
}

fn ensure_single_dispatch_attr(attrs: &[Attribute]) -> Result<()> {
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

fn ensure_export_name_on_fn(attrs: &mut Vec<Attribute>, crate_name: &LitStr, fn_name: &syn::Ident) {
    let has_export_name = attrs
        .iter()
        .any(|attr| generate::is_unsafe_no_mangle(attr) || has_unsafe_export_name(attr));

    if !has_export_name {
        let crate_prefix = crate_name;
        let export_name = LitStr::new(&fn_name.to_string(), fn_name.span());

        attrs.push(parse_quote! {
            #[unsafe(export_name = concat!(#crate_prefix, "__", #export_name))]
        });
    }
}

fn ensure_export_name_on_impl_fn(
    attrs: &mut Vec<Attribute>,
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
            quote!(concat!(#crate_prefix, "__", #default_export_name))
        };

        attrs.push(parse_quote! {
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
            *pat = parse_quote!(#ident);
            arg_idx += 1;
            continue;
        };

        ident.by_ref = None;
        ident.mutability = None;
        ident.subpat = None;
    }
}
