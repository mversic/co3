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
    gen_delegate_impls(name, generics, false)
}

fn gen_delegate_impls(name: &Ident, generics: &syn::Generics, owner: bool) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|clause| &clause.predicates);
    let speculative = speculative_marker(generics.params.is_empty());
    let mut decode_generics = generics.clone();
    decode_generics.params.insert(0, parse_quote!('_dšč));
    let (decode_impl_generics, _, _) = decode_generics.split_for_impl();
    let (unchecked, checked, decode, unchecked_store, checked_store, unchecked_body, checked_body) =
        if owner {
            (
                quote!(co3::borrow::BorrowedOwnerEncode),
                quote!(co3::borrow::BorrowedOwnerEncode),
                quote!(co3::borrow::BorrowedOwnerDecode<'_dšč>),
                quote!(co3::borrow::BorrowedEncodeStore<Self, <Self as co3::borrow::BorrowedOwnerEncode>::ReferenceStore>),
                quote!(co3::borrow::BorrowedEncodeStore<Self, <Self as co3::borrow::BorrowedOwnerEncode>::ReferenceStore>),
                quote!(co3::borrow::BorrowedOwnerEncode::encode_borrowed(
                    self, store
                )),
                quote!(co3::borrow::BorrowedOwnerEncode::encode_borrowed(
                    self, store
                )),
            )
        } else {
            (
                quote!(co3::stored::EncodeMoveUnchecked),
                quote!(co3::stored::EncodeMove),
                quote!(co3::stored::DecodeMove<'_dšč>),
                quote!(<Self as co3::stored::EncodeMoveUnchecked>::SoftStore),
                quote!(<Self as co3::stored::EncodeMove>::Store),
                quote!(unsafe {
                    co3::stored::EncodeMoveUnchecked::soft_encode_unchecked(self, store)
                }),
                quote!(co3::stored::EncodeMove::soft_encode(self, store)),
            )
        };
    quote! {
        unsafe impl #impl_generics co3::borrow::BorrowedEncodeUnchecked for #name #ty_generics
        where #speculative Self: #unchecked, #predicates {
            type SoftStore = #unchecked_store;
            unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType
            where Self: 'itm { #unchecked_body }
        }
        unsafe impl #impl_generics co3::borrow::BorrowedEncode for #name #ty_generics
        where #speculative Self: #checked, #predicates {
            type Store = #checked_store;
            fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType
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
            || path.segments[1].ident != "stored"
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
        let mut generics = input.generics.clone();
        generics.make_where_clause().predicates.push(parse_quote!(
            #speculative Self: co3::rust_spec::RustSpec<Drop = co3::rust_spec::drop::AutoDrop>
        ));
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
        for implementation in &mut implementations {
            implementation.generics.make_where_clause().predicates.push(parse_quote!(
                #speculative Self: co3::rust_spec::RustSpec<Drop = co3::rust_spec::drop::AutoDrop>
            ));
        }
    }
    let mut no_drop_generics = input.generics.clone();
    no_drop_generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(
            #speculative Self: co3::rust_spec::RustSpec<Drop = co3::rust_spec::drop::NoDrop>
        ));
    let no_drop = gen_identity_borrow_impls(&input.ident, &no_drop_generics);
    let mut owner_generics = input.generics.clone();
    let drop_kind = (0..)
        .map(|n| format_ident!("__co3_drop_kind_{n}"))
        .find(|name| !owner_generics.type_params().any(|p| p.ident == *name))
        .unwrap();
    owner_generics
        .params
        .push(parse_quote!(#drop_kind: co3::rust_spec::drop::DropKind));
    owner_generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(
            Self: co3::rust_spec::RustSpec<Drop = co3::rust_spec::drop::CustomDrop<#drop_kind>>
        ));
    // The additional dispatch parameter belongs to the impl, not the owner's type arguments.
    let (_, owner_args, _) = input.generics.split_for_impl();
    let mut owner: syn::File =
        syn::parse2(gen_delegate_impls(&input.ident, &owner_generics, true)).unwrap();
    let name = &input.ident;
    for item in &mut owner.items {
        if let syn::Item::Impl(item) = item {
            item.self_ty = Box::new(parse_quote!(#name #owner_args));
            if input.generics.params.is_empty() {
                let bound = if item
                    .trait_
                    .as_ref()
                    .unwrap()
                    .0
                    .segments
                    .last()
                    .unwrap()
                    .ident
                    == "BorrowedDecode"
                {
                    quote!(co3::borrow::BorrowedOwnerDecode<'_dšč>)
                } else {
                    quote!(co3::borrow::BorrowedOwnerEncode)
                };
                item.generics.make_where_clause().predicates[0] =
                    parse_quote!(#speculative Self: #bound);
            }
        }
    }
    group_borrowed_impls(quote!(#(#implementations)* #no_drop #owner))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrowed_codecs_preserve_custom_validation_expressions() {
        let validator: syn::ExprClosure =
            parse_quote!(|value| { co3::stored::EncodeMove::soft_encode(value, &mut ()) != 0 });
        let mut implementation: syn::ItemImpl = parse_quote! {
            unsafe impl co3::stored::BorrowPlaceholder for Example {
                fn convert(value: u8) -> bool {
                    let validate = #validator;
                    validate(co3::stored::EncodeMove::soft_encode(value, &mut ()))
                }
            }
        };
        BorrowedCodecs.visit_item_impl_mut(&mut implementation);
        let converted = quote!(#implementation).to_string();
        assert!(converted.contains(&quote!(#validator).to_string()));
        assert!(converted.contains("co3 :: borrow :: BorrowedEncode :: soft_encode"));
    }
}

fn group_borrowed_impls(implementations: TokenStream) -> TokenStream {
    let file: syn::File = syn::parse2(implementations).expect("borrowed codecs");
    let mut groups: [Vec<syn::ItemImpl>; 3] = Default::default();
    for item in file.items {
        let syn::Item::Impl(mut item) = item else {
            unreachable!()
        };
        let path = &mut item.trait_.as_mut().unwrap().0;
        let segment = path.segments.last().unwrap().clone();
        let index = match segment.ident.to_string().as_str() {
            "BorrowedEncodeUnchecked" => 0,
            "BorrowedEncode" => 1,
            "BorrowedDecode" => 2,
            _ => unreachable!(),
        };
        *path = parse_quote!(#segment);
        groups[index].push(item);
    }
    let [unchecked, checked, decode] = groups;
    quote! {
        #[allow(clippy::missing_safety_doc)]
        const _: () = {
            use co3::borrow::{BorrowedEncodeUnchecked, BorrowedEncode, BorrowedDecode};
            co3::disjoint_impls! {
                #[disjoint_impls(remote)]
                pub unsafe trait BorrowedEncodeUnchecked: co3::ReprC<CType: Sized> + Sized {
                    type SoftStore: co3::stored::Store + Default;
                    unsafe fn soft_encode_unchecked<'itm>(self, store: &'itm mut Self::SoftStore) -> Self::CType where Self: 'itm;
                }
                #(#unchecked)*
            }
            co3::disjoint_impls! {
                #[disjoint_impls(remote)]
                pub unsafe trait BorrowedEncode: co3::borrow::BorrowedEncodeUnchecked {
                    type Store: co3::stored::Store + Default;
                    fn soft_encode<'itm>(self, store: &'itm mut Self::Store) -> Self::CType where Self: 'itm;
                }
                #(#checked)*
            }
            co3::disjoint_impls! {
                #[disjoint_impls(remote)]
                pub unsafe trait BorrowedDecode<'d>: co3::ReprC<CType: Sized> + Sized {
                    type Store: co3::stored::Store + Default;
                    unsafe fn soft_decode<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Option<Self>;
                    unsafe fn soft_decode_unchecked<'itm: 'd>(source: Self::CType, store: &'itm mut Self::Store) -> Self { unsafe { Self::soft_decode(source, store) }.unwrap() }
                }
                #(#decode)*
            }
        };
    }
}
