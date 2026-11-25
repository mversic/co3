//! The conversion contract selected before emitting an impl or boundary call.

use proc_macro2::TokenStream;
use quote::{ToTokens, format_ident, quote};
use syn::{Generics, Ident, Path, parse_quote};

use crate::{parse::FailureMode, utils::co3_path};

#[derive(Clone, Copy)]
pub(crate) struct Boundary<'a> {
    pub(crate) contract: &'a Conversion,
    pub(crate) failure_mode: FailureMode,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Conversion {
    Checked(Path),
    Unchecked,
}

impl Conversion {
    pub(crate) fn c() -> Self {
        let co3 = co3_path();
        Self::Checked(parse_quote!(#co3::lang::C))
    }

    pub(crate) fn for_type(name: &Ident, generics: &Generics) -> [Self; 2] {
        let clause = &generics.where_clause;
        Self::derived(quote!(#name #generics #clause))
    }

    /// Reserve the language parameter against identifiers in the source, including
    /// unqualified field types and names inside user-provided validator macros.
    pub(crate) fn derived(source: TokenStream) -> [Self; 2] {
        let names = crate::utils::source_idents(source);
        let language = std::iter::once("__Co3Lang".to_owned())
            .chain((1..).map(|n| format!("__Co3Lang{n}")))
            .find(|name| !names.contains(name))
            .unwrap();
        let ident = format_ident!("{language}");
        [Self::Checked(parse_quote!(#ident)), Self::Unchecked]
    }

    pub(crate) fn generics(&self, original: &Generics) -> Generics {
        let mut generics = original.clone();
        if let Self::Checked(language) = self {
            let index = generics.lifetimes().count();
            generics
                .params
                .insert(index, parse_quote!(#language: co3::lang::Language));
        }
        generics
    }

    pub(crate) fn language(&self) -> Option<&Path> {
        match self {
            Self::Checked(lang) => Some(lang),
            Self::Unchecked => None,
        }
    }

    pub(crate) fn checked(&self) -> bool {
        self.language().is_some()
    }

    pub(crate) fn trait_name(&self, base: &str) -> Ident {
        if self.checked() {
            format_ident!("{base}")
        } else {
            format_ident!("{base}Unchecked")
        }
    }

    pub(crate) fn method(&self, base: &str) -> Ident {
        if self.checked() {
            format_ident!("{base}")
        } else {
            format_ident!("{base}_unchecked")
        }
    }

    pub(crate) fn store(&self) -> Ident {
        if self.checked() {
            format_ident!("Store")
        } else {
            format_ident!("SoftStore")
        }
    }

    pub(crate) fn result(&self, value: TokenStream) -> TokenStream {
        if self.checked() {
            quote!(Option<#value>)
        } else {
            value
        }
    }

    pub(crate) fn success(&self, value: TokenStream) -> TokenStream {
        if self.checked() {
            quote!(Some(#value))
        } else {
            value
        }
    }

    pub(crate) fn compatibility(&self) -> TokenStream {
        match self {
            Self::Checked(l) => quote!(co3::convert::AbiValidate<#l>),
            Self::Unchecked => quote!(co3::marker::AbiCompatible),
        }
    }

    pub(crate) fn args(&self, lifetime: Option<syn::Lifetime>) -> TokenStream {
        let lang = self.language();
        let args = lifetime
            .into_iter()
            .map(|lt| quote!(#lt))
            .chain(lang.map(|l| quote!(#l)));
        quote!(<#(#args),*>)
    }
}

/// An implemented trait and its arguments also identify its disjoint impl group.
pub(crate) struct TraitRef {
    pub(crate) module: TokenStream,
    pub(crate) name: Ident,
    pub(crate) arguments: TokenStream,
}

impl ToTokens for TraitRef {
    fn to_tokens(&self, output: &mut TokenStream) {
        let Self {
            module,
            name,
            arguments,
        } = self;
        output.extend(quote!(#module #name #arguments));
    }
}

/// Identity carriers, owned opaque handles and fieldless enums all convert
/// without storage. Only the value expressions and marker eligibility vary.
pub(crate) fn stateless(
    name: &Ident,
    generics: &Generics,
    marker_generics: &Generics,
    encode: TokenStream,
    checked_decode: TokenStream,
    unchecked_decode: TokenStream,
) -> TokenStream {
    let (_, args, clause) = generics.split_for_impl();
    let (_, _, markers) = marker_generics.split_for_impl();
    let implementations = Conversion::for_type(name, generics).into_iter().map(|contract| {
        let encode_params = contract.generics(generics);
        let mut decode_params = encode_params.clone();
        decode_params.params.insert(0, parse_quote!('_dšč));
        let (encode_generics, _, _) = encode_params.split_for_impl();
        let (decode_generics, _, _) = decode_params.split_for_impl();
        let encode_trait = contract.bound(Carrier::Move, None, quote!());
        let decode_trait = contract.bound(Carrier::Move, Some(&parse_quote!('_dšč)), quote!());
        let encode_marker = contract.bound(Carrier::Value, None, quote!());
        let decode_marker = contract.bound(Carrier::Value, Some(&parse_quote!('_)), quote!());
        let encode_method = contract.method("soft_encode");
        let decode_method = contract.method("soft_decode");
        let state = contract.store();
        let unsafety = (!contract.checked()).then(|| quote!(unsafe));
        let output = contract.result(quote!(Self));
        let decode = if contract.checked() { &checked_decode } else { &unchecked_decode };
        quote! {
            unsafe impl #encode_generics #encode_trait for #name #args #clause {
                type #state = ();
                #unsafety fn #encode_method<'itm>(self, (): &'itm mut ()) -> Self::CType
                where Self: 'itm { #encode }
            }
            unsafe impl #decode_generics #decode_trait for #name #args #clause {
                type #state = ();
                unsafe fn #decode_method<'itm: '_dšč>(source: Self::CType, (): &'itm mut ()) -> #output {
                    #decode
                }
            }
            impl #encode_generics #encode_marker for #name #args #markers {}
            impl #encode_generics #decode_marker for #name #args #markers {}
        }
    });
    quote!(#(#implementations)*)
}

pub(crate) fn validator(name: &Ident, generics: &Generics, body: TokenStream) -> TokenStream {
    let co3 = co3_path();
    let (_, args, clause) = generics.split_for_impl();
    let [contract, _] = Conversion::for_type(name, generics);
    let g = contract.generics(generics);
    let (params, _, _) = g.split_for_impl();
    let lang = contract.language();
    quote! {
        unsafe impl #params #co3::convert::AbiValidate<#lang> for #name #args #clause {
            #[inline(always)]
            unsafe fn is_valid(_source: &Self::CType) -> bool { #body }
        }
    }
}

/// The value API admits marker traits, borrowed carriers retain an owner,
/// and explicit Drop glue invokes the move trait without the marker bound.
#[derive(Clone, Copy)]
pub(crate) enum Carrier {
    Value,
    Borrowed,
    Move,
}

impl Carrier {
    pub(crate) fn for_borrow(borrowed: bool) -> Self {
        if borrowed {
            Self::Borrowed
        } else {
            Self::Value
        }
    }
}

impl Conversion {
    pub(crate) fn bound(
        &self,
        carrier: Carrier,
        lifetime: Option<&syn::Lifetime>,
        bindings: TokenStream,
    ) -> TraitRef {
        let co3 = co3_path();
        let (module, base) = match (carrier, lifetime.is_some()) {
            (Carrier::Value, false) => (quote!(#co3::convert), "Encode"),
            (Carrier::Value, true) => (quote!(#co3::convert), "Decode"),
            (Carrier::Borrowed, false) => (quote!(#co3::borrow), "BorrowedEncode"),
            (Carrier::Borrowed, true) => (quote!(#co3::borrow), "BorrowedDecode"),
            (Carrier::Move, false) => (quote!(#co3::convert), "EncodeMove"),
            (Carrier::Move, true) => (quote!(#co3::convert), "DecodeMove"),
        };
        let name = self.trait_name(base);
        let language = self.language();
        let lifetime = lifetime.map(|lt| quote!(#lt,));
        let language = language.map(|l| quote!(#l,));
        let params = quote!(#lifetime #language #bindings);
        let arguments = if params.is_empty() {
            params
        } else {
            quote!(<#params>)
        };
        TraitRef {
            module: quote!(#module::),
            name,
            arguments,
        }
    }

    pub(crate) fn empty_store(&self) -> TokenStream {
        let co3 = co3_path();
        let state = self.store();
        quote!(#state: #co3::convert::EmptyStore)
    }

    pub(crate) fn encode(
        &self,
        carrier: Carrier,
        ty: TokenStream,
        value: TokenStream,
        store: Option<TokenStream>,
        owner: Option<TokenStream>,
    ) -> TokenStream {
        self.call(carrier, BoundaryCall::Encode, ty, value, store, owner)
    }

    pub(crate) fn decode(
        &self,
        carrier: Carrier,
        ty: TokenStream,
        lifetime: Option<&syn::Lifetime>,
        value: TokenStream,
        store: Option<TokenStream>,
    ) -> TokenStream {
        let call = self.call(
            carrier,
            BoundaryCall::Decode(lifetime),
            ty,
            value,
            store,
            None,
        );
        // Boundary accessors and argument cleanup share an Option-valued interface.
        if self.checked() {
            call
        } else {
            quote!(Some(#call))
        }
    }

    fn call(
        &self,
        carrier: Carrier,
        operation: BoundaryCall<'_>,
        ty: TokenStream,
        value: TokenStream,
        store: Option<TokenStream>,
        owner: Option<TokenStream>,
    ) -> TokenStream {
        let (verb, lifetime) = match operation {
            BoundaryCall::Encode => ("encode", None),
            BoundaryCall::Decode(lifetime) => ("decode", lifetime),
        };
        let base = if store.is_some() {
            format!("soft_{verb}")
        } else if matches!(carrier, Carrier::Move) {
            format!("{verb}_move")
        } else {
            verb.to_owned()
        };
        let method = self.method(&base);
        let stored_method = store.is_some() && !matches!(carrier, Carrier::Value);
        let target = if stored_method {
            let inferred = parse_quote!('_);
            let lifetime = match operation {
                BoundaryCall::Encode => None,
                BoundaryCall::Decode(lt) => Some(lt.unwrap_or(&inferred)),
            };
            let path = self.bound(carrier, lifetime, quote!());
            quote!(<#ty as #path>::#method)
        } else {
            let co3 = co3_path();
            let module = match carrier {
                Carrier::Value => co3,
                Carrier::Move => quote!(#co3::convert),
                Carrier::Borrowed => quote!(#co3::borrow),
            };
            let lifetime = lifetime.map(|lt| quote!(#lt,));
            let language = self.language().map(|l| quote!(, #l));
            quote!(#module::#method::<#lifetime #ty #language>)
        };
        let store = store.map(|s| quote!(, #s));
        let owner = owner.map(|o| quote!(, #o));
        let call = quote!(#target(#value #store #owner));
        if stored_method && matches!(operation, BoundaryCall::Encode) && self.checked() {
            call
        } else {
            quote!(unsafe { #call })
        }
    }
}

#[derive(Clone, Copy)]
enum BoundaryCall<'a> {
    Encode,
    Decode(Option<&'a syn::Lifetime>),
}
