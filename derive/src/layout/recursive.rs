//! Nominal state breaks recursive field store projections. Its initially absent
//! tuple also terminates Default before following the next Box or Vec.
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_quote, visit_mut::VisitMut};

use crate::{
    codec::{Conversion, TraitRef},
    known_type::Kind,
    layout::{aggregate::Operation, inference::RecursiveStores},
};

struct NamedState<'a> {
    codec: &'a TraitRef,
    generics: &'a syn::Generics,
    own: &'a syn::Type,
    case: usize,
    ordinary: &'a [&'a syn::Type],
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LayoutCase {
    Any,
    Stable,
    StableRobust,
    StableNonRobust,
    Unstable,
}

pub(super) struct ValueImpl {
    pub generics: syn::Generics,
    pub store: syn::Type,
    pub owner: Option<syn::Type>,
    pub body: TokenStream,
}

pub(super) fn value_impl(
    input: &syn::DeriveInput,
    stores: &RecursiveStores,
    op: Operation,
    contract: &Conversion,
    ValueImpl {
        generics,
        store: store_ty,
        owner: owner_ty,
        body,
    }: ValueImpl,
) -> TokenStream {
    let (_, args, _) = input.generics.split_for_impl();
    let ident = &input.ident;
    let own: syn::Type = parse_quote!(#ident #args);
    let ctype_name = super::ctype::gen_ctype_name(ident);
    let companion = quote!(#ctype_name #args);
    let codec = op.path(contract);
    let TraitRef {
        module,
        name,
        arguments,
    } = &codec;
    let borrowed_decode = op == Operation::BorrowedDecode;
    let checked = op == Operation::Encode && contract.checked();
    let checked_borrow = op == Operation::BorrowedEncode && contract.checked();
    let has_box = stores
        .allocations
        .iter()
        .any(|(kind, _)| *kind == Kind::Box);
    use LayoutCase::*;
    let cases: &[LayoutCase] = if stores.allocations.is_empty() || borrowed_decode {
        &[Any]
    } else if (checked || checked_borrow) && has_box {
        &[StableRobust, StableNonRobust, Unstable]
    } else {
        &[Stable, Unstable]
    };
    let state = contract.store();
    let owner_arg = (op == Operation::BorrowedEncode)
        .then(|| quote!(, owner: &'_išč mut <Self as co3::borrow::BorrowedEncodeUnchecked>::Owner));
    let method = contract.method(if op.encoding() {
        "soft_encode"
    } else {
        "soft_decode"
    });
    let signature = if op.encoding() {
        let unsafety = (!contract.checked()).then(|| quote!(unsafe));
        quote!(#unsafety fn #method<'_išč>(self, store: &'_išč mut Self::#state #owner_arg) -> Self::CType where Self: '_išč)
    } else {
        let ret = contract.result(quote!(Self));
        quote!(unsafe fn #method<'_išč: '_dšč>(source: Self::CType, store: &'_išč mut Self::#state) -> #ret)
    };
    let mut definitions = Vec::new();
    let mut blocks = Vec::new();
    for (i, &case) in cases.iter().enumerate() {
        let mut generics = generics.clone();
        if !stores.allocations.is_empty() {
            let predicates = &mut generics.make_where_clause().predicates;
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
                if (checked && has_box)
                    || checked_borrow
                    || (op == Operation::Decode && contract.checked())
                {
                    let language = contract.language().unwrap();
                    predicates.push(parse_quote!(#own: co3::convert::AbiValidate<#language>));
                } else {
                    predicates.push(parse_quote!(#own: co3::marker::AbiCompatible));
                }
            }
            if borrowed_decode {
                predicates.push(parse_quote!(#companion: Copy));
            }
            for &(kind, alloc) in &stores.allocations {
                let alloc = crate::utils::peel_grouped_type(alloc);
                if kind == Kind::Vec && stable && op == Operation::Decode {
                    predicates.push(parse_quote!(<#own as co3::rust_spec::RustSpec>::Mutability: co3::rust_spec::mutability::PointerMutability));
                }
                if kind == Kind::Box {
                    let niche = quote!(co3::rust_spec::niche::WithNiche<co3::rust_spec::Specified>);
                    predicates.push(parse_quote!(#alloc: co3::rust_spec::RustSpec<#layout_bound Niche = #niche, Drop: co3::rust_spec::drop::DropKind> + co3::ReprC<CType = co3::boxed::abi::Box<#companion>>));
                }
            }

            NormalizeGroups.visit_generics_mut(&mut generics);
        }
        let (mut store, mut owner) = (quote!(#store_ty), owner_ty.as_ref().map(|ty| quote!(#ty)));
        let bindings = stores.has_owned.then(|| {
            let mut concrete = crate::utils::ConcreteSelfType(&own);
            let mut concrete_generics = generics.clone();
            concrete.visit_generics_mut(&mut concrete_generics);
            let names = NamedState {
                codec: &codec,
                generics: &concrete_generics,
                own: &own,
                case: i,
                ordinary: &stores.codec_fields,
            };
            let mut define = |state, fields, is_store| {
                let (ty, def) = names.define(state, fields, is_store);
                definitions.push(def);
                ty
            };
            store = define(&state, &store_ty, true);
            if let Some(ty) = &owner_ty {
                owner = Some(define(&format_ident!("Owner"), ty, false));
            }
            let owner = owner_arg
                .as_ref()
                .map(|_| quote!(let owner = owner.0.get_or_insert_default();));
            quote!(let store = store.0.get_or_insert_default(); #owner)
        });
        let owner = owner.map(|ty| quote!(type Owner = #ty;));
        let (params, _, clause) = generics.split_for_impl();
        let path = if cases.len() > 1 {
            quote!(#name #arguments)
        } else {
            quote!(#codec)
        };
        blocks.push(quote!(unsafe impl #params #path for #own #clause {
            type #state = #store;
            #owner
            #signature { #bindings #body }
        }));
    }
    if cases.len() == 1 {
        return quote!(const _: () = { #(#definitions)* #(#blocks)* };);
    }
    let supers = if checked {
        quote!(co3::convert::EncodeMoveUnchecked)
    } else if checked_borrow {
        quote!(co3::borrow::BorrowedEncodeUnchecked)
    } else {
        quote!(co3::ReprC<CType: Sized> + Sized)
    };
    let owner = owner_ty.map(|_| {
        quote!(
            type Owner: Default;
        )
    });
    let language = contract
        .language()
        .map(|l| quote!(where #l: co3::lang::Language));

    quote!(const _: () = {
        use #module #name;
        #(#definitions)*
        co3::disjoint_impls! {
            #[allow(clippy::missing_safety_doc, reason = "internal conversion trait generated by ReprC")]
            #[disjoint_impls(remote)]
            pub unsafe trait #name #arguments: #supers #language {
                type #state: co3::convert::Store + Default;
                #owner
                #signature;
            }
            #(#blocks)*
        }
    };)
}

impl NamedState<'_> {
    fn define(
        &self,
        state: &syn::Ident,
        fields: &syn::Type,
        is_store: bool,
    ) -> (TokenStream, TokenStream) {
        let Self {
            generics,
            codec,
            case,
            ..
        } = *self;
        let mut concrete = crate::utils::ConcreteSelfType(self.own);
        let (params, args, predicates) = generics.split_for_impl();
        let name = format_ident!("__Co3{state}{case}");
        let mut fields = fields.clone();
        concrete.visit_type_mut(&mut fields);
        let sync = is_store.then(|| {
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
        (
            quote!(#name #args),
            quote! {
                pub struct #name #params (core::option::Option<#fields>) #predicates;
                impl #params core::default::Default for #name #args #predicates {
                    fn default() -> Self { Self(core::option::Option::None) }
                }
                #sync
            },
        )
    }
}

// Equivalent parenthesized predicates must share the same associated-type keys
// when disjoint_impls groups the recursive layout cases.
struct NormalizeGroups;
impl VisitMut for NormalizeGroups {
    fn visit_type_mut(&mut self, ty: &mut syn::Type) {
        *ty = crate::utils::peel_grouped_type(ty).clone();
        syn::visit_mut::visit_type_mut(self, ty);
    }
}
