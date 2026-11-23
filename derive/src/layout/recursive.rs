//! Nominal storage for recursive value conversions.
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_quote, visit_mut::VisitMut};

use super::inference::RecursiveStores;

/// Name the recursive state *after* borrowed conversion has selected its
/// field stores and owners. The tuple for Box<Node> can contain Box<Node::Store>.
/// A named enclosing type breaks that projection cycle. An initially absent
/// tuple lets Default terminate before it follows the next box.
pub(super) fn name_stores(
    input: &syn::DeriveInput,
    stores: &RecursiveStores,
    codecs: TokenStream,
) -> TokenStream {
    if !stores.has_owned {
        return codecs;
    }
    let mut file = syn::parse2::<syn::File>(codecs).expect("generated value codecs");
    NameStores {
        ordinary: &stores.codec_fields,
        dispatch: &stores.allocations,
        input,
    }
    .visit_file_mut(&mut file);
    quote!(#file)
}

struct NameStores<'a> {
    ordinary: &'a [&'a syn::Type],
    dispatch: &'a [(crate::known_type::Kind, &'a syn::Type)],
    input: &'a syn::DeriveInput,
}

impl VisitMut for NameStores<'_> {
    fn visit_item_mut(&mut self, item: &mut syn::Item) {
        let syn::Item::Impl(implementation) = item else {
            if let syn::Item::Const(_) = item {
                syn::visit_mut::visit_item_mut(self, item);
            }
            return;
        };
        let Some((codec, _)) = &implementation.trait_ else {
            return;
        };
        // The value codecs have Store or SoftStore. Marker and ReprC impls
        // share this scope and have no conversion state to wrap.
        if !implementation.items.iter().any(|item| {
            matches!(item,
                syn::ImplItem::Type(assoc) if assoc.ident == "Store" || assoc.ident == "SoftStore"
            )
        }) {
            return;
        }
        if self.dispatch.is_empty() {
            let defs = self.state_items(implementation, &codec.clone(), "");
            *item = parse_quote!(const _: () = { #(#defs)* #implementation };);
        } else {
            *item = self.dispatch_codec(implementation);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LayoutCase {
    Any,
    Stable,
    StableRobust,
    StableNonRobust,
    Unstable,
}

impl NameStores<'_> {
    fn state_items(
        &self,
        implementation: &mut syn::ItemImpl,
        codec: &syn::Path,
        suffix: &str,
    ) -> Vec<TokenStream> {
        let mut concrete = crate::utils::ConcreteSelfType(&implementation.self_ty);
        let mut generics = implementation.generics.clone();
        concrete.visit_generics_mut(&mut generics);
        let mut storage = Vec::new();
        for member in &mut implementation.items {
            match member {
                syn::ImplItem::Type(assoc) => {
                    storage.push(self.define_state(assoc, codec, &generics, &mut concrete, suffix));
                }
                syn::ImplItem::Fn(method) => bind_field_stores(method),
                _ => {}
            }
        }
        storage
    }

    /// Make each library conversion type-check with the node's actual layout
    /// in scope. Requiring the finished field codec on this impl would send
    /// the trait solver back to the node. Method bodies still call that codec.
    fn dispatch_codec(&self, original: &syn::ItemImpl) -> syn::Item {
        use crate::known_type::Kind;
        let codec = &original.trait_.as_ref().unwrap().0;
        let segment = codec.segments.last().unwrap();
        let name = &segment.ident;
        let operation = name.to_string();
        let borrowed_decode = operation == "BorrowedDecode";
        let checked = operation == "EncodeMove";
        let checked_borrow = operation == "BorrowedEncode";
        let own = &*original.self_ty;
        let (_, args, _) = self.input.generics.split_for_impl();
        let ctype_name = super::ctype::gen_ctype_name(&self.input.ident);
        let companion: syn::Type = parse_quote!(#ctype_name #args);
        let has_box = self.dispatch.iter().any(|(kind, _)| *kind == Kind::Box);
        let mut blocks = Vec::new();
        let mut definitions = Vec::new();
        use LayoutCase::*;
        let variants: &[LayoutCase] = if borrowed_decode {
            &[Any]
        } else if checked && has_box {
            &[StableRobust, StableNonRobust, Unstable]
        } else {
            &[Stable, Unstable]
        };
        for (index, &case) in variants.iter().enumerate() {
            let mut implementation = original.clone();
            let predicates = &mut implementation.generics.make_where_clause().predicates;
            let stable = matches!(case, Stable | StableRobust | StableNonRobust);
            let layout_bound = match case {
                Any => None,
                Unstable => Some(quote!(Layout = __Co3Unstable,)),
                _ => Some(quote!(Layout = co3::rust_spec::Specified,)),
            };
            let trap_bound = match case {
                StableRobust => Some(quote!(Trap = co3::rust_spec::layout::Robust,)),
                StableNonRobust => Some(quote!(Trap = co3::rust_spec::layout::NonRobust,)),
                _ => None,
            };
            predicates.push(parse_quote!(#own: co3::rust_spec::RustSpec<#layout_bound #trap_bound Size = co3::rust_spec::size::Sized<co3::rust_spec::Gt<co3::rust_spec::Zero>>>));
            predicates.push(parse_quote!(#own: co3::ReprC<CType = #companion>));
            if stable {
                if (checked && has_box) || operation == "DecodeMove" {
                    predicates.push(parse_quote!(#own: co3::convert::AbiCompatibleValidate));
                } else {
                    predicates.push(parse_quote!(#own: co3::marker::AbiCompatible));
                }
            }
            if borrowed_decode {
                predicates.push(parse_quote!(#companion: Copy));
            }
            for &(kind, alloc) in self.dispatch {
                if kind == Kind::Vec && stable && operation == "DecodeMove" {
                    predicates.push(parse_quote!(<#own as co3::rust_spec::RustSpec>::Mutability: co3::rust_spec::mutability::PointerMutability));
                }
                if kind == Kind::Box {
                    let niche = quote!(co3::rust_spec::niche::WithNiche<co3::rust_spec::Specified>);
                    predicates.push(parse_quote!(#alloc: co3::rust_spec::RustSpec<#layout_bound Niche = #niche> + co3::ReprC<CType = co3::boxed::abi::Box<#companion>>));
                }
            }
            let suffix = index.to_string();
            definitions.extend(self.state_items(&mut implementation, codec, &suffix));
            if variants.len() > 1 {
                implementation.trait_.as_mut().unwrap().0 = parse_quote!(#segment);
            }
            blocks.push(implementation);
        }
        if variants.len() == 1 {
            return parse_quote!(const _: () = { #(#definitions)* #(#blocks)* };);
        }
        let supertraits = if checked {
            quote!(co3::convert::EncodeMoveUnchecked)
        } else if checked_borrow {
            quote!(co3::borrow::BorrowedEncodeUnchecked)
        } else {
            quote!(co3::ReprC<CType:Sized> + Sized)
        };
        let members = original.items.iter().map(|m| match m {
            syn::ImplItem::Type(t) => {
                let ident = &t.ident;
                let store = (ident != "Owner").then(|| quote!(co3::convert::Store+));
                quote!(type #ident: #store Default;)
            }
            syn::ImplItem::Fn(f) => {
                let sig = &f.sig;
                quote!(#sig;)
            }
            _ => unreachable!(),
        });
        let arguments = &segment.arguments;
        let mut codec_import = codec.clone();
        codec_import.segments.last_mut().unwrap().arguments = syn::PathArguments::None;
        parse_quote!(const _: () = {
            use #codec_import;
            #(#definitions)*
            co3::disjoint_impls! {
                #[allow(clippy::missing_safety_doc, reason = "internal conversion trait generated by ReprC")]
                #[disjoint_impls(remote)] pub unsafe trait #name #arguments: #supertraits { #(#members)* }
                #(#blocks)*
            }
        };)
    }

    fn define_state(
        &self,
        assoc: &mut syn::ImplItemType,
        codec: &syn::Path,
        generics: &syn::Generics,
        concrete: &mut crate::utils::ConcreteSelfType<'_>,
        suffix: &str,
    ) -> TokenStream {
        let (params, args, predicates) = generics.split_for_impl();
        let state = &assoc.ident;
        let name = format_ident!("__Co3{state}{suffix}");
        let mut fields = assoc.ty.clone();
        concrete.visit_type_mut(&mut fields);
        let sync = (state != "Owner").then(|| {
            let mut empty = generics.clone();
            for ty in self.ordinary {
                let mut ty = (*ty).clone();
                concrete.visit_type_mut(&mut ty);
                let field = super::fields::FieldType::new(&ty, generics);
                let speculative = crate::utils::speculative_marker(field.concrete);
                empty.make_where_clause().predicates.push(parse_quote!(
                    #speculative <#ty as #codec>::#state: co3::convert::EmptyStore
                ));
            }
            let (empty_params, _, empty_where) = empty.split_for_impl();
            quote! {
                impl #params co3::convert::Store for #name #args #predicates {
                    fn sync(self) -> core::option::Option<()> {
                        co3::convert::Store::sync(self.0)
                    }
                }
                // Each child may assume the marker while proving the real field
                // tuple empty. Every nonrecursive leaf must satisfy it independently.
                unsafe impl #empty_params co3::convert::EmptyStore for #name #args #empty_where {}
                impl #empty_params #name #args #empty_where {
                    #[expect(dead_code)]
                    fn check_empty() {
                        fn check<S: co3::convert::EmptyStore>() {}
                        check::<#fields>();
                    }
                }
            }
        });
        assoc.ty = parse_quote!(#name #args);
        quote! {
            pub struct #name #params (core::option::Option<#fields>) #predicates;
            impl #params core::default::Default for #name #args #predicates {
                fn default() -> Self { Self(core::option::Option::None) }
            }
            #sync
        }
    }
}

fn bind_field_stores(method: &mut syn::ImplItemFn) {
    // The first parameter is the value or carrier. The others are its store
    // and, for borrowed encoding, its owner. Preserve their hygienic names.
    for arg in method.sig.inputs.iter().skip(1) {
        let syn::FnArg::Typed(arg) = arg else {
            unreachable!("codec store argument")
        };
        let syn::Pat::Ident(pattern) = &*arg.pat else {
            unreachable!("codec store binding")
        };
        let var = &pattern.ident;
        method
            .block
            .stmts
            .insert(0, parse_quote!(let #var = #var.0.get_or_insert_default();));
    }
}
