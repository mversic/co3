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
    let unchecked = quote!(co3::stored::EncodeMoveUnchecked);
    let checked = quote!(co3::stored::EncodeMove);
    let decode = quote!(co3::stored::DecodeMove<'_dšč>);
    let unchecked_store = quote!(<Self as co3::stored::EncodeMoveUnchecked>::SoftStore);
    let checked_store = quote!(<Self as co3::stored::EncodeMove>::Store);
    let unchecked_body =
        quote!(unsafe { co3::stored::EncodeMoveUnchecked::soft_encode_unchecked(self, store) });
    let checked_body = quote!(co3::stored::EncodeMove::soft_encode(self, store));
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

/// Owners stay whole while the borrowed carrier is live.
fn gen_owner_impls(input: &DeriveInput, generics: &syn::Generics) -> TokenStream {
    let name = &input.ident;
    let (_, ty_generics, _) = input.generics.split_for_impl();
    let predicates = &input
        .generics
        .where_clause
        .as_ref()
        .map(|clause| &clause.predicates);
    let drop_kind = &generics.type_params().last().unwrap().ident;
    let speculative = speculative_marker(input.generics.params.is_empty());
    let store_param = (0..)
        .map(|n| format_ident!("__co3_reference_store_{n}"))
        .find(|name| !generics.type_params().any(|p| p.ident == *name))
        .unwrap();
    let size_param = (0..)
        .map(|n| format_ident!("__co3_size_kind_{n}"))
        .find(|name| !generics.type_params().any(|p| p.ident == *name))
        .unwrap();
    let mut encode_generics = generics.clone();
    encode_generics
        .params
        .push(parse_quote!(#store_param: co3::stored::Store + Default));
    encode_generics
        .params
        .push(parse_quote!(#size_param: co3::rust_spec::size::SizedKind));
    let (encode_impl_generics, _, _) = encode_generics.split_for_impl();
    let mut implementations = Vec::new();
    for unchecked in [true, false] {
        let (trait_name, move_trait, store_name, method) = if unchecked {
            (
                quote!(co3::borrow::BorrowedEncodeUnchecked),
                quote!(co3::stored::EncodeMoveUnchecked),
                quote!(SoftStore),
                quote!(soft_encode_unchecked),
            )
        } else {
            (
                quote!(co3::borrow::BorrowedEncode),
                quote!(co3::stored::EncodeMove),
                quote!(Store),
                quote!(soft_encode),
            )
        };
        let unsafe_fn = unchecked.then(|| quote!(unsafe));
        let owner_type = unchecked.then(|| {
            quote!(
                type Owner = Option<Self>;
            )
        });
        let owner_bound = (!unchecked).then(|| {
            quote!(
                #speculative Self: co3::borrow::BorrowedEncodeUnchecked<Owner = Option<Self>>,
            )
        });
        let call = quote!(#move_trait::#method(&*value, store));
        let call = if unchecked {
            quote!(unsafe { #call })
        } else {
            call
        };
        implementations.push(quote! {
            unsafe impl #encode_impl_generics #trait_name for #name #ty_generics
            where
                #speculative Self: co3::ReprC<CType: co3::CType + Copy> + Sized
                    + co3::rust_spec::RustSpec<
                        Size = co3::rust_spec::size::Sized<#size_param>,
                        Drop = co3::rust_spec::drop::CustomDrop<#drop_kind>,
                    >,
                for<'_co3_ref> &'_co3_ref Self:
                    #move_trait<#store_name = #store_param>,
                for<'_co3_ref> <&'_co3_ref Self as co3::ReprC>::CType:
                    __Co3BorrowedCarrier<Self>,
                #owner_bound
                #predicates
            {
                #owner_type
                type #store_name = #store_param;

                #unsafe_fn fn #method<'itm>(
                    self,
                    store: &'itm mut Self::#store_name,
                    owner: &'itm mut <Self as co3::borrow::BorrowedEncodeUnchecked>::Owner,
                ) -> Self::CType
                where Self: 'itm {
                    *store = Default::default();
                    let value = owner.insert(self);
                    let carrier = #call;
                    // Only the Copy C payload is duplicated; the owner remains retained.
                    unsafe { __Co3BorrowedCarrier::as_ptr(&carrier).read() }
                }
            }
        });
    }
    quote!(#(#implementations)*)
}

/// A stable borrowed owner is cloned through its whole-value Clone implementation.
fn gen_owner_decode_impl(input: &DeriveInput, generics: &syn::Generics) -> TokenStream {
    let name = &input.ident;
    let (_, ty_generics, _) = input.generics.split_for_impl();
    let mut generics = generics.clone();
    generics.params.insert(0, parse_quote!('_dšč));
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|clause| &clause.predicates);
    let speculative = speculative_marker(input.generics.params.is_empty());
    quote! {
        unsafe impl #impl_generics co3::borrow::BorrowedDecode<'_dšč> for #name #ty_generics
        where
            #speculative Self: co3::ReprC<CType: co3::CType + Copy> + Clone
                + co3::transmute::AbiCompatibleValidate,
            for<'_co3_ref> &'_co3_ref Self: co3::stored::DecodeMove<'_co3_ref, Store = ()>
                + co3::ReprC<CType = co3::reference::CRef<'_co3_ref, Self>>,
            #predicates
        {
            type Store = ();

            unsafe fn soft_decode<'itm: '_dšč>(
                source: Self::CType,
                store: &'itm mut Self::Store,
            ) -> Option<Self> {
                let carrier = co3::reference::CRef::<Self>::from_ref(&source);
                // The reference only borrows this local carrier until Clone finishes.
                let value = unsafe { <&Self as co3::stored::DecodeMove<'_>>::soft_decode(carrier, store)? };
                Some(Self::clone(value))
            }

            unsafe fn soft_decode_unchecked<'itm: '_dšč>(
                source: Self::CType,
                store: &'itm mut Self::Store,
            ) -> Self {
                let carrier = co3::reference::CRef::<Self>::from_ref(&source);
                let value = unsafe { <&Self as co3::stored::DecodeMove<'_>>::soft_decode_unchecked(carrier, store) };
                Self::clone(value)
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
                #speculative Self: co3::rust_spec::RustSpec<Drop = co3::rust_spec::drop::AutoDrop>
            ));
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
    let owner = gen_owner_impls(input, &owner_generics);
    let decode_owner = gen_owner_decode_impl(input, &owner_generics);
    group_borrowed_impls(quote!(#(#implementations)* #no_drop #owner #decode_owner))
}

#[cfg(test)]
mod tests {
    use super::*;

    extern crate alloc;
    use alloc::{vec, vec::Vec};
    use co3::{ReprC, borrow::BorrowedDecode, rust_spec::RustSpec};
    use core::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn stable_custom_drop_encoding_uses_reference_dispatch() {
        static DROPS: AtomicUsize = AtomicUsize::new(0);

        #[derive(RustSpec, ReprC)]
        #[rust_spec(custom_drop)]
        #[repr(transparent)]
        struct Owner(u32);

        impl Drop for Owner {
            fn drop(&mut self) {
                DROPS.fetch_add(1, Ordering::Relaxed);
            }
        }

        let mut owner = None;
        let encoded = co3::borrow::BorrowedEncode::encode(Owner(17), &mut owner);
        assert_eq!(encoded.0, 17);
        assert_eq!(DROPS.load(Ordering::Relaxed), 0);
        drop(owner);
        assert_eq!(DROPS.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn unstable_custom_drop_encoding_uses_reference_dispatch() {
        static DROPS: AtomicUsize = AtomicUsize::new(0);

        #[derive(RustSpec, ReprC)]
        #[rust_spec(custom_drop)]
        struct Owner(Vec<(u8,)>);

        impl Drop for Owner {
            fn drop(&mut self) {
                DROPS.fetch_add(1, Ordering::SeqCst);
            }
        }

        let mut owner = None;
        let mut store = Default::default();
        let encoded = co3::borrow::BorrowedEncode::soft_encode(
            Owner(vec![(3,), (5,)]),
            &mut store,
            &mut owner,
        );
        assert_eq!(encoded.0.len(), 2);
        assert_eq!(DROPS.load(Ordering::SeqCst), 0);
        co3::stored::Store::sync(store).unwrap();
        drop(owner);
        assert_eq!(DROPS.load(Ordering::SeqCst), 1);

        let mut owner = None;
        let mut store = Default::default();
        let encoded = unsafe {
            co3::borrow::BorrowedEncodeUnchecked::soft_encode_unchecked(
                Owner(vec![(7,)]),
                &mut store,
                &mut owner,
            )
        };
        assert_eq!(encoded.0.len(), 1);
        assert_eq!(DROPS.load(Ordering::SeqCst), 1);
        co3::stored::Store::sync(store).unwrap();
        drop(owner);
        assert_eq!(DROPS.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn stable_custom_drop_borrowed_decode_clones_the_whole_value() {
        static CLONES: AtomicUsize = AtomicUsize::new(0);
        static DROPS: AtomicUsize = AtomicUsize::new(0);

        #[derive(RustSpec, ReprC)]
        #[rust_spec(custom_drop)]
        #[repr(transparent)]
        struct Owner(u32);

        impl Clone for Owner {
            fn clone(&self) -> Self {
                CLONES.fetch_add(1, Ordering::SeqCst);
                Self(self.0)
            }
        }

        impl Drop for Owner {
            fn drop(&mut self) {
                DROPS.fetch_add(1, Ordering::SeqCst);
            }
        }

        let mut owner = None;
        let source = co3::borrow::BorrowedEncode::encode(Owner(17), &mut owner);
        let decoded = unsafe { Owner::decode(source) }.unwrap();
        assert_eq!(decoded.0, 17);
        assert_eq!(CLONES.load(Ordering::SeqCst), 1);
        assert_eq!(DROPS.load(Ordering::SeqCst), 0);
        drop(decoded);

        let decoded = unsafe { Owner::soft_decode_unchecked(source, &mut ()) };
        assert_eq!(decoded.0, 17);
        assert_eq!(CLONES.load(Ordering::SeqCst), 2);
        assert_eq!(DROPS.load(Ordering::SeqCst), 1);
        drop(decoded);
        drop(owner);
        assert_eq!(DROPS.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn custom_drop_validation_failure_does_not_clone_or_drop() {
        static CLONES: AtomicUsize = AtomicUsize::new(0);
        static DROPS: AtomicUsize = AtomicUsize::new(0);

        #[derive(RustSpec, ReprC)]
        #[rust_spec(custom_drop)]
        #[rust_spec(non_robust)]
        #[repr(transparent)]
        #[repr_c(is_valid = |value: &u8| *value != 0)]
        struct Owner(u8);

        impl Clone for Owner {
            fn clone(&self) -> Self {
                CLONES.fetch_add(1, Ordering::SeqCst);
                Self(self.0)
            }
        }

        impl Drop for Owner {
            fn drop(&mut self) {
                DROPS.fetch_add(1, Ordering::SeqCst);
            }
        }

        let source = unsafe { <Owner as ReprC>::CType::from_fields(0) };
        assert!(unsafe { Owner::decode(source) }.is_none());
        assert_eq!(CLONES.load(Ordering::SeqCst), 0);
        assert_eq!(DROPS.load(Ordering::SeqCst), 0);
    }

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
    // Bare const arguments parse as types. disjoint_impls can mistake them for
    // type parameters when generalizing impls with different dispatch bounds.
    // Braces preserve their const identity across the macro's parse boundary.
    struct ConstArguments(Vec<Ident>);
    impl VisitMut for ConstArguments {
        fn visit_generic_argument_mut(&mut self, argument: &mut syn::GenericArgument) {
            if let syn::GenericArgument::Type(syn::Type::Path(path)) = argument
                && path.qself.is_none()
                && let Some(ident) = path.path.get_ident()
                && self.0.contains(ident)
            {
                *argument = parse_quote!({ #ident });
            } else {
                syn::visit_mut::visit_generic_argument_mut(self, argument);
            }
        }
    }

    let file: syn::File = syn::parse2(implementations).expect("borrowed codecs");
    let mut groups: [Vec<syn::ItemImpl>; 3] = Default::default();
    for item in file.items {
        let syn::Item::Impl(mut item) = item else {
            unreachable!()
        };
        let mut const_arguments = ConstArguments(
            item.generics
                .const_params()
                .map(|p| p.ident.clone())
                .collect(),
        );
        const_arguments.visit_generics_mut(&mut item.generics);
        const_arguments.visit_type_mut(&mut item.self_ty);
        let path = &mut item.trait_.as_mut().unwrap().0;
        const_arguments.visit_path_mut(path);
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

            trait __Co3BorrowedCarrier<T: co3::ReprC<CType: Sized>> {
                fn as_ptr(&self) -> *const T::CType;
            }

            impl<'a, T: co3::ReprC<CType: Sized>> __Co3BorrowedCarrier<T>
                for co3::reference::CRef<'a, T>
            {
                fn as_ptr(&self) -> *const T::CType {
                    co3::reference::CRef::as_ptr(*self)
                }
            }

            impl<T: co3::ReprC<CType: Sized>> __Co3BorrowedCarrier<T>
                for co3::reference::CSoftRef<T>
            {
                fn as_ptr(&self) -> *const T::CType {
                    co3::reference::CSoftRef::as_ptr(*self)
                }
            }
            co3::disjoint_impls! {
                #[disjoint_impls(remote)]
                pub unsafe trait BorrowedEncodeUnchecked: co3::ReprC<CType: Sized> + Sized {
                    type Owner: Default;
                    type SoftStore: co3::stored::Store + Default;
                    unsafe fn soft_encode_unchecked<'itm>(
                        self,
                        store: &'itm mut Self::SoftStore,
                        owner: &'itm mut <Self as co3::borrow::BorrowedEncodeUnchecked>::Owner,
                    ) -> Self::CType where Self: 'itm;
                }
                #(#unchecked)*
            }
            co3::disjoint_impls! {
                #[disjoint_impls(remote)]
                pub unsafe trait BorrowedEncode: co3::borrow::BorrowedEncodeUnchecked {
                    type Store: co3::stored::Store + Default;
                    fn soft_encode<'itm>(
                        self,
                        store: &'itm mut Self::Store,
                        owner: &'itm mut <Self as co3::borrow::BorrowedEncodeUnchecked>::Owner,
                    ) -> Self::CType where Self: 'itm;
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
