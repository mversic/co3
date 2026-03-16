//! Crate containing FFI related macro functionality
use std::collections::BTreeMap;

use manyhow::{emit, manyhow};
use proc_macro2::TokenStream;
use quote::quote;

use crate::{
    emitter::Emitter,
    generate::{emit_decl_exports, expand_extern_import_decls},
    impl_visitor::Arg,
    preprocess::{BlockKind, parse_items_with_kind},
    repr::derive_extern_c,
    validate::{
        is_inner_special_attr, validate_export_decl_attrs, validate_extern_decl_attrs,
        validate_extern_type_drop_requirements, validate_inner_attrs, validate_ownership_attrs,
    },
};

mod attr;
mod emitter;
mod ffi_fn;
mod generate;
mod handle;
mod impl_visitor;
mod preprocess;
mod repr;
mod utils;
mod validate;
mod wrapper;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum OwnershipMode {
    #[default]
    Borrow,
    ByValue,
}

fn ownership_mode_for_type(ty: &syn::Type) -> OwnershipMode {
    if matches!(ty, syn::Type::Reference(_)) {
        OwnershipMode::ByValue
    } else {
        OwnershipMode::Borrow
    }
}

// TODO: Can previous be used instead of this fn?
fn ownership_mode_for_receiver(receiver: &syn::Receiver) -> OwnershipMode {
    if receiver.reference.is_some() {
        OwnershipMode::ByValue
    } else {
        OwnershipMode::Borrow
    }
}

fn is_export_skip_attr(attr: &syn::Attribute) -> bool {
    if attr
        .path()
        .segments
        .last()
        .is_none_or(|seg| seg.ident != "export")
    {
        return false;
    }

    attr.parse_args::<syn::Ident>()
        .is_ok_and(|arg| arg == "skip")
}

#[derive(Default)]
struct ParsedLinkAttr {
    link_crate: Option<syn::LitStr>,
    link_name: Option<syn::LitStr>,
}

fn parse_link_attr(attr: &syn::Attribute) -> Result<Option<ParsedLinkAttr>, syn::Error> {
    if !attr.path().is_ident("link") {
        return Ok(None);
    }

    let syn::Meta::List(list) = &attr.meta else {
        return Err(syn::Error::new_spanned(
            attr,
            "expected `#[link(name = \"...\")]` or `#[link(crate = \"...\")]`",
        ));
    };

    let metas = list.parse_args_with(
        syn::punctuated::Punctuated::<syn::MetaNameValue, syn::Token![,]>::parse_terminated,
    )?;

    let mut out = ParsedLinkAttr::default();
    for nv in metas {
        let Some(ident) = nv.path.get_ident() else {
            return Err(syn::Error::new_spanned(
                nv.path,
                "expected `name = \"...\"` or `crate = \"...\"`",
            ));
        };
        let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(value),
            ..
        }) = nv.value
        else {
            return Err(syn::Error::new_spanned(
                nv,
                "expected string literal in `#[link(...)]`",
            ));
        };

        if ident == "name" {
            if out.link_name.replace(value).is_some() {
                return Err(syn::Error::new_spanned(
                    ident,
                    "`name` can only be provided once in `#[link(...)]`",
                ));
            }
        } else if ident == "crate" {
            if out.link_crate.replace(value).is_some() {
                return Err(syn::Error::new_spanned(
                    ident,
                    "`crate` can only be provided once in `#[link(...)]`",
                ));
            }
        } else {
            return Err(syn::Error::new_spanned(
                ident,
                "expected `name = \"...\"` or `crate = \"...\"`",
            ));
        }
    }

    Ok(Some(out))
}

fn parse_link_name_attr(attr: &syn::Attribute) -> Result<Option<syn::LitStr>, syn::Error> {
    if !attr.path().is_ident("link_name") {
        return Ok(None);
    }

    let syn::Meta::NameValue(nv) = &attr.meta else {
        return Err(syn::Error::new_spanned(
            attr,
            "expected `#[link_name = \"...\"]`",
        ));
    };
    let syn::Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Str(link),
        ..
    }) = &nv.value
    else {
        return Err(syn::Error::new_spanned(
            &nv.value,
            "expected string literal in `#[link_name = \"...\"]`",
        ));
    };

    Ok(Some(link.clone()))
}

fn parse_unsafe_export_name(attr: &syn::Attribute) -> Option<syn::LitStr> {
    if attr.path().is_ident("export") {
        let syn::Meta::List(meta_list) = &attr.meta else {
            return None;
        };

        let metas = meta_list
            .parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
            )
            .ok()?;

        for meta in metas {
            let syn::Meta::NameValue(nv) = meta else {
                continue;
            };
            if !nv.path.is_ident("name") {
                continue;
            }
            let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(name),
                ..
            }) = nv.value
            else {
                continue;
            };
            return Some(name);
        }

        return None;
    }

    if !attr.path().is_ident("unsafe") {
        return None;
    }

    let syn::Meta::List(meta_list) = &attr.meta else {
        return None;
    };

    let metas = meta_list
        .parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
        .ok()?;

    for meta in metas {
        let syn::Meta::NameValue(nv) = meta else {
            continue;
        };
        if !nv.path.is_ident("export_name") {
            continue;
        }
        let syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(name),
            ..
        }) = nv.value
        else {
            continue;
        };
        return Some(name);
    }

    None
}

fn is_unsafe_no_mangle(attr: &syn::Attribute) -> bool {
    if !attr.path().is_ident("unsafe") {
        return false;
    }

    let syn::Meta::List(meta_list) = &attr.meta else {
        return false;
    };

    let Ok(metas) = meta_list.parse_args_with(
        syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
    ) else {
        return false;
    };

    metas.into_iter().any(|meta| match meta {
        syn::Meta::Path(path) => path.is_ident("no_mangle"),
        _ => false,
    })
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
pub fn repr_c_derive(input: TokenStream) -> TokenStream {
    let mut emitter = Emitter::new();

    let Some(item) = emitter.handle(syn::parse2::<syn::DeriveInput>(input)) else {
        return emitter.finish_token_stream();
    };

    if let Some(export_attr) = item.attrs.iter().find(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|seg| seg.ident == "export")
    }) {
        emit!(emitter, export_attr, "Opaque items can't derive ReprC");
        return emitter.finish_token_stream();
    };

    let result = derive_extern_c(&mut emitter, &item);
    emitter.finish_token_stream_with(result)
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
/// #[export("C")]
/// impl Foo {
///     pub fn new(id: u8) -> Self {
///         Self(id)
///     }
///
///     #[export(skip)]
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
pub fn export(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut emitter = Emitter::new();

    let mut item = match syn::parse2::<syn::Item>(item) {
        Err(err) => return err.to_compile_error(),
        Ok(item) => item,
    };

    let abi = match syn::parse2::<syn::Abi>(quote!(extern #attr)) {
        Err(err) => return err.to_compile_error(),
        Ok(abi) => abi,
    };

    let exported = match &mut item {
        syn::Item::Impl(item) => {
            let mut exported_methods = Vec::new();

            if abi.name.is_none() {
                return syn::Error::new_spanned(
                    &item,
                    "#[export] on an impl block requires a named ABI",
                )
                .to_compile_error();
            }

            for impl_item in &mut item.items {
                if let syn::ImplItem::Fn(method) = impl_item {
                    let mut export_attrs = Vec::new();
                    let mut other_attrs = Vec::new();

                    let mut should_skip = false;
                    for attr in core::mem::take(&mut method.attrs) {
                        if parse_unsafe_export_name(&attr).is_some() || is_unsafe_no_mangle(&attr) {
                            export_attrs.push(attr);
                        } else if is_export_skip_attr(&attr) {
                            should_skip = true;
                        } else {
                            other_attrs.push(attr);
                        }
                    }

                    method.attrs = other_attrs;
                    if !should_skip {
                        let vis = &method.vis;
                        let sig = &method.sig;

                        exported_methods.push(quote! {
                            #(#export_attrs)*
                            #vis #sig;
                        });
                    }
                }
            }

            quote! { #(#exported_methods)* }
        }
        syn::Item::Fn(item) => {
            let mut export_attrs = Vec::new();
            let mut other_attrs = Vec::new();

            if abi.name.is_none() {
                return syn::Error::new_spanned(
                    &item,
                    "#[export] on an impl block requires a named ABI",
                )
                .to_compile_error();
            }

            for attr in core::mem::take(&mut item.attrs) {
                if parse_unsafe_export_name(&attr).is_some() || is_unsafe_no_mangle(&attr) {
                    export_attrs.push(attr);
                } else {
                    other_attrs.push(attr);
                }
            }

            item.attrs = other_attrs;
            let vis = &item.vis;
            let sig = &item.sig;

            quote! {
                #(#export_attrs)*
                #vis #sig;
            }
        }
        syn::Item::Struct(item) => {
            let vis = &item.vis;
            let ident = &item.ident;
            let generics = &item.generics;
            let (impl_generics, _, _) = generics.split_for_impl();

            if abi.name.is_some() {
                return syn::Error::new_spanned(&item, "#[export] on a struct can't have ABI")
                    .to_compile_error();
            }

            quote! { #vis type #ident #impl_generics; }
        }
        syn::Item::Enum(item) => {
            let vis = &item.vis;
            let ident = &item.ident;
            let generics = &item.generics;
            let (impl_generics, _, _) = generics.split_for_impl();

            if abi.name.is_some() {
                return syn::Error::new_spanned(&item, "#[export] on an enum can't have ABI")
                    .to_compile_error();
            }

            quote! { #vis type #ident #impl_generics; }
        }
        syn::Item::Union(item) => {
            let vis = &item.vis;
            let ident = &item.ident;
            let generics = &item.generics;
            let (impl_generics, _, _) = generics.split_for_impl();

            if abi.name.is_some() {
                return syn::Error::new_spanned(&item, "#[export] on a union can't have ABI")
                    .to_compile_error();
            }

            quote! { #vis type #ident #impl_generics; }
        }
        item => {
            emit!(emitter, item, "Item not supported");
            quote! { #item }
        }
    };

    let result = quote! {
        #item

        co3::export_! {
            #![abi = #abi]

            #exported
        }
    };

    emitter.finish_token_stream_with(result)
}

#[manyhow]
#[proc_macro]
pub fn export_(input: TokenStream) -> TokenStream {
    struct ExportInput {
        attrs: Vec<syn::Attribute>,
        decls: Vec<syn::Item>,
    }

    impl syn::parse::Parse for ExportInput {
        fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
            let attrs = input.call(syn::Attribute::parse_inner)?;
            let decls = parse_item_list(input, BlockKind::Export)?;
            Ok(Self { attrs, decls })
        }
    }

    let input = match syn::parse2::<ExportInput>(input) {
        Ok(input) => input,
        Err(err) => return err.to_compile_error(),
    };

    let mut emitter = Emitter::new();
    let Some(abi) = parse_inner_abi(&mut emitter, &input.attrs) else {
        return emitter.finish_token_stream();
    };

    if let Err(err) = validate_export_decl_attrs(&input.decls) {
        return err.to_compile_error();
    }

    emit_decl_exports(abi, input.decls)
}

#[manyhow]
#[proc_macro]
pub fn extern_(input: TokenStream) -> TokenStream {
    struct ExternInput {
        attrs: Vec<syn::Attribute>,
        decls: Vec<syn::Item>,
    }

    impl syn::parse::Parse for ExternInput {
        fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
            let attrs = input.call(syn::Attribute::parse_inner)?;
            let decls = parse_item_list(input, BlockKind::Extern)?;
            Ok(Self { attrs, decls })
        }
    }

    let input = match syn::parse2::<ExternInput>(input) {
        Err(err) => return err.to_compile_error(),
        Ok(input) => input,
    };
    let mut emitter = Emitter::new();
    let Some(abi) = parse_inner_abi(&mut emitter, &input.attrs) else {
        return emitter.finish_token_stream();
    };
    if let Err(err) = validate_inner_attrs(&input.attrs) {
        return err.to_compile_error();
    };
    if let Err(err) = validate_extern_decl_attrs(&input.decls) {
        return err.to_compile_error();
    }
    if let Err(err) = validate_extern_type_drop_requirements(&input.decls) {
        return err.to_compile_error();
    }
    let link_crate = match parse_inner_link_crate(&input.attrs) {
        Err(err) => return err.to_compile_error(),
        Ok(link_prefix) => link_prefix,
    };
    let module_attrs: Vec<_> = input
        .attrs
        .iter()
        .filter(|attr| !is_inner_special_attr(attr))
        .map(inner_attr_to_outer)
        .collect();

    expand_extern_import_decls(&abi, input.decls, &module_attrs, link_crate.as_ref())
}

#[manyhow]
#[proc_macro]
#[allow(non_snake_case)]
pub fn export_C(input: TokenStream) -> TokenStream {
    let delegated = quote! {
        #![abi = "C"]
        #input
    };

    export_(delegated.into()).into()
}

/// See `[extern_]`
#[manyhow]
#[proc_macro]
#[allow(non_snake_case)]
pub fn extern_C(input: TokenStream) -> TokenStream {
    let delegated = quote! {
        #![abi = "C"]
        #input
    };

    extern_(delegated.into()).into()
}

fn parse_abi_attr(attr: &syn::Attribute) -> Result<Option<syn::Abi>, syn::Error> {
    let syn::Meta::NameValue(nv) = &attr.meta else {
        return Ok(None);
    };

    if !nv.path.is_ident("abi") {
        return Ok(None);
    }

    let syn::Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Str(abi_lit),
        ..
    }) = &nv.value
    else {
        return Err(syn::Error::new_spanned(
            &nv.value,
            "expected string literal in `[abi = \"...\"]`",
        ));
    };

    syn::parse2::<syn::Abi>(quote!(extern #abi_lit)).map(Some)
}

fn parse_inner_abi(emitter: &mut Emitter, attrs: &[syn::Attribute]) -> Option<syn::Abi> {
    for attr in attrs {
        match parse_abi_attr(attr) {
            Ok(Some(ok)) => {
                if matches!(attr.style, syn::AttrStyle::Outer) {
                    emit!(emitter, attr, "outer attribute where inner was expected");
                    return None;
                }

                return Some(ok);
            }
            Ok(None) => {}
            Err(err) => {
                emit!(emitter, attr, "{}", err);
                return None;
            }
        };
    }

    None
}

fn parse_inner_link_crate(attrs: &[syn::Attribute]) -> Result<Option<syn::LitStr>, syn::Error> {
    let mut link_prefix = None;

    for attr in attrs {
        let link_crate_lit = if let Some(link_meta) = parse_link_attr(attr)? {
            let Some(link_crate_lit) = link_meta.link_crate else {
                continue;
            };

            link_crate_lit
        } else {
            continue;
        };

        if link_prefix.is_some() {
            return Err(syn::Error::new_spanned(
                attr,
                "`link(crate = \"...\")` can only be provided once",
            ));
        }

        link_prefix = Some(link_crate_lit);
    }

    Ok(link_prefix)
}

fn inner_attr_to_outer(attr: &syn::Attribute) -> syn::Attribute {
    let meta = &attr.meta;
    syn::parse_quote!(#[#meta])
}

fn parse_item_list(
    input: syn::parse::ParseStream,
    block_kind: BlockKind,
) -> syn::Result<Vec<syn::Item>> {
    parse_items_with_kind(input, block_kind)
}

fn has_marker_attr(attrs: &[syn::Attribute], attr_name: &str) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident(attr_name))
}

fn receiver_ownership(sig: &syn::Signature) -> Option<OwnershipMode> {
    sig.receiver().map(|receiver| {
        if has_marker_attr(&receiver.attrs, "by_val") && receiver.reference.is_none() {
            OwnershipMode::ByValue
        } else {
            OwnershipMode::Borrow
        }
    })
}

fn input_ownerships(sig: &syn::Signature) -> Vec<OwnershipMode> {
    sig.inputs
        .iter()
        .filter_map(|input| {
            let syn::FnArg::Typed(arg) = input else {
                return None;
            };
            Some(
                if has_marker_attr(&arg.attrs, "by_val")
                    && !matches!(arg.ty.as_ref(), syn::Type::Reference(_))
                {
                    OwnershipMode::ByValue
                } else {
                    OwnershipMode::Borrow
                },
            )
        })
        .collect()
}

fn handle_id_positions(sig: &syn::Signature) -> BTreeMap<String, usize> {
    let mut positions = BTreeMap::new();
    for (idx, input) in sig.inputs.iter().enumerate() {
        let syn::FnArg::Typed(arg) = input else {
            continue;
        };
        if has_marker_attr(&arg.attrs, "id")
            && let syn::Pat::Ident(pat_ident) = arg.pat.as_ref()
        {
            positions.insert(pat_ident.ident.to_string(), idx);
        }
    }
    positions
}

fn strip_internal_sig_attrs(sig: &mut syn::Signature) {
    fn retain_user_attrs(attrs: &mut Vec<syn::Attribute>) {
        attrs.retain(|attr| !attr.path().is_ident("by_val") && !attr.path().is_ident("id"));
    }

    for input in &mut sig.inputs {
        match input {
            syn::FnArg::Receiver(receiver) => retain_user_attrs(&mut receiver.attrs),
            syn::FnArg::Typed(arg) => retain_user_attrs(&mut arg.attrs),
        }
    }
}
