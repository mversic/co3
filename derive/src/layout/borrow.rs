//! Borrowed conversion uses the original Rust type and its existing C carrier.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{DeriveInput, Ident, parse_quote};

use crate::{codec::Conversion, layout::aggregate::Operation, utils::speculative_marker};

pub fn gen_either_name(len: usize) -> Ident {
    format_ident!("Either{len}")
}

pub fn either_variant_name(idx: usize) -> Ident {
    format_ident!("V{idx}")
}

/// Identity carriers delegate borrowed conversion to the corresponding value operation.
pub(crate) fn gen_identity_borrow_impls(name: &Ident, generics: &syn::Generics) -> TokenStream {
    let (_, args, clause) = generics.split_for_impl();
    let predicates = clause.map(|c| &c.predicates);
    let speculative = speculative_marker(generics.params.is_empty());
    let mut output = TokenStream::new();
    for contract in Conversion::for_type(name, generics) {
        for (op, target) in [
            (Operation::BorrowedEncode, Operation::Encode),
            (Operation::BorrowedDecode, Operation::Decode),
        ] {
            let mut g = contract.generics(generics);
            if !op.encoding() {
                g.params.insert(0, parse_quote!('_dšč));
            }
            let (params, _, _) = g.split_for_impl();
            let path = op.path(&contract);
            let delegate = target.path(&contract);
            let state = contract.store();
            let owner = (op.encoding() && !contract.checked()).then(|| {
                quote!(
                    type Owner = ();
                )
            });
            let method = if op.encoding() {
                let method = contract.method("soft_encode");
                let call = quote!(<Self as #delegate>::#method(self,store));
                let (unsafety, body) = if contract.checked() {
                    (None, call)
                } else {
                    (Some(quote!(unsafe)), quote!(unsafe { #call }))
                };
                quote!(#unsafety fn #method<'itm>(self,store: &'itm mut Self::#state,_: &'itm mut <Self as co3::borrow::BorrowedEncodeUnchecked>::Owner) -> Self::CType where Self: 'itm { #body })
            } else {
                let method = contract.method("soft_decode");
                let result = contract.result(quote!(Self));
                quote!(unsafe fn #method<'itm: '_dšč>(source: Self::CType,store: &'itm mut Self::#state) -> #result { unsafe { <Self as #delegate>::#method(source,store) } })
            };
            output.extend(quote!(unsafe impl #params #path for #name #args where #speculative Self: #delegate, #predicates {
                type #state = <Self as #delegate>::#state;
                #owner #method
            }));
        }
    }
    output
}

pub(super) fn fieldless_borrow_impls(input: &DeriveInput) -> TokenStream {
    let mut g = input.generics.clone();
    let speculative = speculative_marker(g.params.is_empty());
    let p = &mut g.make_where_clause().predicates;
    p.push(parse_quote!(Self: co3::rust_spec::RustSpec));
    p.push(parse_quote!(#speculative <Self as co3::rust_spec::RustSpec>::Drop: __Co3NonCustomDrop));
    non_custom_drop_scope(gen_identity_borrow_impls(&input.ident, &g))
}

pub(super) fn non_custom_drop_scope(implementations: TokenStream) -> TokenStream {
    quote! {
        const _: () = {
            trait __Co3NonCustomDrop {}
            impl __Co3NonCustomDrop for co3::rust_spec::drop::NoDrop {}
            impl __Co3NonCustomDrop for co3::rust_spec::drop::AutoDrop {}

            #implementations
        };
    }
}
