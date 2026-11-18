//! Borrowed conversion uses the original Rust type and its existing C carrier.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{DeriveInput, Ident, parse_quote, visit_mut::VisitMut};

use super::ctype::classify_tail;
use crate::utils::speculative_marker;

pub fn gen_either_name(len: usize) -> Ident {
    format_ident!("Either{len}")
}

pub fn either_variant_name(idx: usize) -> Ident {
    format_ident!("V{idx}")
}

/// Identity carriers and no-drop values reuse their ordinary value codecs.
pub(crate) fn gen_identity_borrow_impls(name: &Ident, generics: &syn::Generics) -> TokenStream {
    gen_delegate_impls(name, generics)
}

fn gen_delegate_impls(name: &Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|clause| &clause.predicates);
    let speculative = speculative_marker(generics.params.is_empty());
    let mut decode_generics = generics.clone();
    decode_generics.params.insert(0, parse_quote!('_dšč));
    let (decode_impl_generics, _, _) = decode_generics.split_for_impl();
    let unchecked = quote!(co3::convert::EncodeMoveUnchecked);
    let checked = quote!(co3::convert::EncodeMove);
    let decode = quote!(co3::convert::DecodeMove<'_dšč>);
    let unchecked_store = quote!(<Self as co3::convert::EncodeMoveUnchecked>::SoftStore);
    let checked_store = quote!(<Self as co3::convert::EncodeMove>::Store);
    let unchecked_body =
        quote!(unsafe { co3::convert::EncodeMoveUnchecked::soft_encode_unchecked(self, store) });
    let checked_body = quote!(co3::convert::EncodeMove::soft_encode(self, store));
    quote! {
        unsafe impl #impl_generics co3::borrow::BorrowedEncodeUnchecked for #name #ty_generics
        where #speculative Self: #unchecked, #predicates {
            type Owner = ();
            type SoftStore = #unchecked_store;
            unsafe fn soft_encode_unchecked<'itm>(
                self,
                store: &'itm mut Self::SoftStore,
                _owner: &'itm mut <Self as co3::borrow::BorrowedEncodeUnchecked>::Owner,
            ) -> Self::CType
            where Self: 'itm { #unchecked_body }
        }
        unsafe impl #impl_generics co3::borrow::BorrowedEncode for #name #ty_generics
        where #speculative Self: #checked, #predicates {
            type Store = #checked_store;
            fn soft_encode<'itm>(
                self,
                store: &'itm mut Self::Store,
                _owner: &'itm mut <Self as co3::borrow::BorrowedEncodeUnchecked>::Owner,
            ) -> Self::CType
            where Self: 'itm { #checked_body }
        }
        unsafe impl #decode_impl_generics co3::borrow::BorrowedDecode<'_dšč> for #name #ty_generics
        where #speculative Self: #decode, #predicates {
            type Store = <Self as #decode>::Store;
            unsafe fn soft_decode<'itm: '_dšč>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self> {
                unsafe { <Self as #decode>::soft_decode(source, store) }
            }
            unsafe fn soft_decode_unchecked<'itm: '_dšč>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
                unsafe { <Self as #decode>::soft_decode_unchecked(source, store) }
            }
        }
    }
}

/// Reuse the fieldwise codec bodies, changing only which field conversions they call.
/// No Rust view or carrier cast is needed: both modes use the owner's CType.
struct BorrowedCodecs;
impl VisitMut for BorrowedCodecs {
    // Value codec closures are user-provided validation expressions.
    fn visit_expr_closure_mut(&mut self, _: &mut syn::ExprClosure) {}

    fn visit_path_mut(&mut self, path: &mut syn::Path) {
        syn::visit_mut::visit_path_mut(self, path);
        if path.segments.len() < 3
            || path.segments[0].ident != "co3"
            || path.segments[1].ident != "convert"
        {
            return;
        }
        let name = match path.segments[2].ident.to_string().as_str() {
            "EncodeMoveUnchecked" => "BorrowedEncodeUnchecked",
            "EncodeMove" => "BorrowedEncode",
            "DecodeMove" => "BorrowedDecode",
            _ => return,
        };
        path.segments[1].ident = format_ident!("borrow");
        path.segments[2].ident = format_ident!("{name}");
    }
}

/// Field owners have the same shape as unchecked field stores.
struct OwnerStorage;
impl VisitMut for OwnerStorage {
    fn visit_ident_mut(&mut self, ident: &mut Ident) {
        if ident == "store" {
            *ident = format_ident!(
                "__co3_owner_storage",
                span = proc_macro2::Span::mixed_site()
            );
        }
    }

    fn visit_path_mut(&mut self, path: &mut syn::Path) {
        syn::visit_mut::visit_path_mut(self, path);
        if path
            .segments
            .iter()
            .any(|segment| segment.ident == "BorrowedEncodeUnchecked")
            && let Some(last) = path.segments.last_mut()
            && last.ident == "SoftStore"
        {
            last.ident = format_ident!("Owner");
        }
    }
}

struct EncodeOwners;
impl VisitMut for EncodeOwners {
    // Validation expressions belong to the user, not the generated codec.
    fn visit_expr_closure_mut(&mut self, _: &mut syn::ExprClosure) {}

    fn visit_expr_call_mut(&mut self, call: &mut syn::ExprCall) {
        syn::visit_mut::visit_expr_call_mut(self, call);
        let syn::Expr::Path(path) = &*call.func else {
            return;
        };
        let segments = &path.path.segments;
        if segments.len() == 4
            && segments[0].ident == "co3"
            && segments[1].ident == "borrow"
            && matches!(
                segments[2].ident.to_string().as_str(),
                "BorrowedEncode" | "BorrowedEncodeUnchecked"
            )
            && matches!(
                segments[3].ident.to_string().as_str(),
                "soft_encode" | "soft_encode_unchecked"
            )
        {
            let mut owner = call.args.last().expect("field store").clone();
            OwnerStorage.visit_expr_mut(&mut owner);
            call.args.push(owner);
        }
    }

    fn visit_block_mut(&mut self, block: &mut syn::Block) {
        syn::visit_mut::visit_block_mut(self, block);
        block.stmts = core::mem::take(&mut block.stmts).into_iter().flat_map(|stmt| {
            // Enum codecs initialize the active variant before encoding its fields.
            let owner = if let syn::Stmt::Local(local) = &stmt
                && let Some(init) = &local.init
                && let syn::Expr::MethodCall(call) = &*init.expr
                && call.method == "insert"
                && matches!(&*call.receiver, syn::Expr::Path(path) if path.path.is_ident("store"))
            {
                let mut owner = stmt.clone();
                OwnerStorage.visit_stmt_mut(&mut owner);
                Some(owner)
            } else {
                None
            };
            core::iter::once(stmt).chain(owner)
        }).collect();
    }
}

fn split_encode_owner(implementation: &mut syn::ItemImpl) {
    let name = &implementation
        .trait_
        .as_ref()
        .unwrap()
        .0
        .segments
        .last()
        .unwrap()
        .ident;
    if name == "BorrowedDecode" {
        return;
    }
    if name == "BorrowedEncodeUnchecked" {
        let mut owner = implementation
            .items
            .iter()
            .find_map(|item| match item {
                syn::ImplItem::Type(item) if item.ident == "SoftStore" => Some(item.clone()),
                _ => None,
            })
            .expect("unchecked store");
        owner.ident = format_ident!("Owner");
        OwnerStorage.visit_type_mut(&mut owner.ty);
        implementation.items.insert(0, syn::ImplItem::Type(owner));
    }
    for item in &mut implementation.items {
        if let syn::ImplItem::Fn(method) = item {
            let lifetime = method
                .sig
                .generics
                .lifetimes()
                .next()
                .expect("encoding lifetime")
                .lifetime
                .clone();
            let owner = format_ident!(
                "__co3_owner_storage",
                span = proc_macro2::Span::mixed_site()
            );
            method.sig.inputs.push(parse_quote!(
                #owner: &#lifetime mut <Self as co3::borrow::BorrowedEncodeUnchecked>::Owner
            ));
            EncodeOwners.visit_block_mut(&mut method.block);
        }
    }
}

fn collect_codecs(items: Vec<syn::Item>, output: &mut Vec<syn::ItemImpl>) {
    for item in items {
        match item {
            syn::Item::Impl(mut implementation)
                if implementation.trait_.as_ref().is_some_and(|(path, _)| {
                    matches!(
                        path.segments.last().unwrap().ident.to_string().as_str(),
                        "EncodeMove" | "EncodeMoveUnchecked" | "DecodeMove"
                    )
                }) =>
            {
                BorrowedCodecs.visit_item_impl_mut(&mut implementation);
                split_encode_owner(&mut implementation);
                if implementation.trait_.as_ref().unwrap().0.segments.last().unwrap().ident == "BorrowedDecode"
                    && !implementation.items.iter().any(|item| matches!(item, syn::ImplItem::Fn(f) if f.sig.ident == "soft_decode_unchecked"))
                {
                    implementation.items.push(parse_quote! {
                        unsafe fn soft_decode_unchecked<'itm: '_dšč>(source: Self::CType, store: &'itm mut Self::Store) -> Self {
                            unsafe { <Self as co3::borrow::BorrowedDecode<'_dšč>>::soft_decode(source, store) }.unwrap()
                        }
                    });
                }
                output.push(implementation);
            }
            syn::Item::Const(item) => {
                if let syn::Expr::Block(block) = *item.expr {
                    collect_codecs(
                        block
                            .block
                            .stmts
                            .into_iter()
                            .filter_map(|stmt| match stmt {
                                syn::Stmt::Item(item) => Some(item),
                                _ => None,
                            })
                            .collect(),
                        output,
                    );
                }
            }
            _ => {}
        }
    }
}

pub(super) fn gen_item_borrow_impls(input: &DeriveInput, codecs: TokenStream) -> TokenStream {
    if matches!(&input.data, syn::Data::Struct(data) if data.fields.iter().any(|field| classify_tail(&field.ty, &input.generics).is_known_unsized()))
    {
        return quote! {};
    }
    let speculative = speculative_marker(input.generics.params.is_empty());
    let mut implementations = Vec::new();
    if codecs.is_empty() {
        let generics = input.generics.clone();
        let file: syn::File =
            syn::parse2(gen_identity_borrow_impls(&input.ident, &generics)).unwrap();
        implementations.extend(file.items.into_iter().map(|item| match item {
            syn::Item::Impl(item) => item,
            _ => unreachable!(),
        }));
    } else {
        collect_codecs(
            syn::parse2::<syn::File>(codecs)
                .expect("value codecs")
                .items,
            &mut implementations,
        );
    }
    for implementation in &mut implementations {
        implementation
            .generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(
                #speculative Self: co3::rust_spec::RustSpec
            ));
        implementation
            .generics
            .make_where_clause()
            .predicates
            .push(parse_quote!(
                #speculative <Self as co3::rust_spec::RustSpec>::Drop: __Co3NonCustomDrop
            ));
    }
    group_borrowed_impls(quote!(#(#implementations)*))
}

fn group_borrowed_impls(implementations: TokenStream) -> TokenStream {
    quote! {
        const _: () = {
            trait __Co3NonCustomDrop {}
            impl __Co3NonCustomDrop for co3::rust_spec::drop::NoDrop {}
            impl __Co3NonCustomDrop for co3::rust_spec::drop::AutoDrop {}

            #implementations
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrowed_codecs_preserve_custom_validation_expressions() {
        let validator: syn::ExprClosure =
            parse_quote!(|value| { co3::convert::EncodeMove::soft_encode(value, &mut ()) != 0 });
        let mut implementation: syn::ItemImpl = parse_quote! {
            unsafe impl co3::convert::BorrowPlaceholder for Example {
                fn convert(value: u8) -> bool {
                    let validate = #validator;
                    validate(co3::convert::EncodeMove::soft_encode(value, &mut ()))
                }
            }
        };
        BorrowedCodecs.visit_item_impl_mut(&mut implementation);
        let converted = quote!(#implementation).to_string();
        assert!(converted.contains(&quote!(#validator).to_string()));
        assert!(converted.contains("co3 :: borrow :: BorrowedEncode :: soft_encode"));
    }
}
