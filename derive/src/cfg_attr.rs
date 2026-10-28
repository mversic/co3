//! Expand only leading inner attributes of ffi!/raw! into gated invocations.
//! Predicates and all tokens in the declaration body remain opaque.
use proc_macro2::{Delimiter, Group, TokenStream, TokenTree};
use quote::quote;

#[derive(Clone)]
pub(crate) struct Variant {
    // The bool records the branch we constructed, not an evaluated predicate.
    pub(crate) cfgs: Vec<(TokenStream, bool)>,
    pub(crate) tokens: TokenStream,
    active: bool,
}

impl Variant {
    fn plain(tokens: TokenStream) -> Self {
        Self {
            cfgs: Vec::new(),
            tokens,
            active: true,
        }
    }
}

pub(crate) fn expand(input: TokenStream) -> syn::Result<Vec<Variant>> {
    let tokens = input.into_iter().collect::<Vec<_>>();
    let mut variants = vec![Variant::plain(TokenStream::new())];
    let mut index = 0;
    while let Some((group, consumed)) = inner_attribute(&tokens[index..]) {
        variants = append_variants(variants, expand_attribute(group)?);
        index += consumed;
    }
    // Do not descend into declarations, signatures, groups, macro arguments,
    // or outer attributes. Their existing parsing/forwarding is independent.
    for variant in &mut variants {
        variant.tokens.extend(tokens[index..].iter().cloned());
    }
    Ok(variants)
}

pub(crate) fn emit_macro_invocations(
    macro_path: TokenStream,
    prefix: TokenStream,
    variants: Vec<Variant>,
) -> TokenStream {
    let invocations = variants
        .into_iter()
        .filter(|variant| variant.active)
        .map(|variant| {
            let cfgs = variant.cfgs.iter().map(|(predicate, enabled)| {
                if *enabled {
                    quote!(#[cfg(#predicate)])
                } else {
                    quote!(#[cfg(not(#predicate))])
                }
            });
            let tokens = variant.tokens;
            quote! { #(#cfgs)* #macro_path! { #prefix #tokens } }
        });
    quote!(#(#invocations)*)
}

fn append_variants(lhs: Vec<Variant>, rhs: Vec<Variant>) -> Vec<Variant> {
    let mut out = Vec::new();
    for left in lhs {
        for right in &rhs {
            let mut cfgs = left.cfgs.clone();
            let mut compatible = true;
            for (predicate, enabled) in &right.cfgs {
                if let Some((_, previous)) = cfgs
                    .iter()
                    .find(|(p, _)| p.to_string() == predicate.to_string())
                {
                    if previous != enabled {
                        compatible = false;
                        break;
                    }
                } else {
                    cfgs.push((predicate.clone(), *enabled));
                }
            }
            if !compatible {
                continue;
            }
            let mut tokens = left.tokens.clone();
            tokens.extend(right.tokens.clone());
            out.push(Variant {
                cfgs,
                tokens,
                active: left.active && right.active,
            });
        }
    }
    out
}

// Recognize only the attribute envelope. Other attribute contents are opaque,
// including tokens which happen to look like nested conditional attributes.
fn inner_attribute(tokens: &[TokenTree]) -> Option<(&Group, usize)> {
    if !punct(tokens.first(), '#') || !punct(tokens.get(1), '!') {
        return None;
    }
    let TokenTree::Group(group) = tokens.get(2)? else {
        return None;
    };
    (group.delimiter() == Delimiter::Bracket).then_some((group, 3))
}

fn expand_attribute(group: &Group) -> syn::Result<Vec<Variant>> {
    let tokens = group.stream().into_iter().collect::<Vec<_>>();
    let Some(TokenTree::Ident(name)) = tokens.first() else {
        return Ok(vec![Variant::plain(quote_attr(group.stream()))]);
    };
    if name != "cfg" && name != "cfg_attr" {
        return Ok(vec![Variant::plain(quote_attr(group.stream()))]);
    }
    let Some(TokenTree::Group(args)) = tokens.get(1) else {
        return Err(syn::Error::new_spanned(
            group,
            "expected conditional attribute arguments",
        ));
    };
    if args.delimiter() != Delimiter::Parenthesis || tokens.len() != 2 {
        return Err(syn::Error::new_spanned(
            group,
            "expected conditional attribute arguments",
        ));
    }
    if name == "cfg" {
        return Ok(vec![
            Variant {
                cfgs: vec![(args.stream(), true)],
                tokens: TokenStream::new(),
                active: true,
            },
            Variant {
                cfgs: vec![(args.stream(), false)],
                tokens: TokenStream::new(),
                active: false,
            },
        ]);
    }

    // Split at token-level commas: nested groups, including all predicate and
    // attribute argument syntax, are left intact. No syn::Meta predicate parser.
    let mut parts = split_commas(args.stream());
    if parts.last().is_some_and(TokenStream::is_empty) {
        parts.pop();
    }
    if parts.len() < 2 || parts.iter().any(TokenStream::is_empty) {
        return Err(syn::Error::new_spanned(
            group,
            "expected cfg_attr(predicate, attributes...)",
        ));
    }
    let predicate = parts.remove(0);
    let mut present = vec![Variant::plain(TokenStream::new())];
    for payload in parts {
        let mut payload_group = Group::new(Delimiter::Bracket, payload);
        payload_group.set_span(group.span());
        present = append_variants(present, expand_attribute(&payload_group)?);
    }
    present = append_variants(
        vec![Variant {
            cfgs: vec![(predicate.clone(), true)],
            tokens: TokenStream::new(),
            active: true,
        }],
        present,
    );
    present.push(Variant {
        cfgs: vec![(predicate, false)],
        tokens: TokenStream::new(),
        active: true,
    });
    Ok(present)
}

fn quote_attr(tokens: TokenStream) -> TokenStream {
    quote!(#![#tokens])
}

fn split_commas(input: TokenStream) -> Vec<TokenStream> {
    let mut parts = vec![TokenStream::new()];
    for token in input {
        if punct(Some(&token), ',') {
            parts.push(TokenStream::new());
        } else {
            parts.last_mut().unwrap().extend([token]);
        }
    }
    parts
}

fn punct(token: Option<&TokenTree>, ch: char) -> bool {
    matches!(token, Some(TokenTree::Punct(p)) if p.as_char() == ch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inner_predicates_and_payloads_are_opaque() {
        let variants = expand(quote! {
            #![cfg_attr(arbitrary => tokens, custom(a => b), custom(a => b))]
            fn f();
        })
        .unwrap();
        assert_eq!(variants.len(), 2);
        let present = variants.iter().find(|v| v.cfgs[0].1).unwrap();
        assert_eq!(
            present.tokens.to_string(),
            quote! {
                #![custom(a => b)] #![custom(a => b)] fn f();
            }
            .to_string()
        );
        let absent = variants.iter().find(|v| !v.cfgs[0].1).unwrap();
        assert_eq!(
            absent.tokens.to_string(),
            quote!(
                fn f();
            )
            .to_string()
        );
    }

    #[test]
    fn outer_attributes_and_argument_tokens_are_never_inspected() {
        let body = quote! {
            #[cfg(arbitrary => tokens)]
            #[cfg_attr(unparsed)]
            impl<#[cfg(P)] T> Type<T> {
                #![cfg_attr(not_a_macro_level_attribute)]
                #[cfg_attr(opaque => payload)]
                fn f(#[cfg_attr(unparsed)] value: custom!(#![cfg(Q)] anything));
            }
        };
        let variants = expand(body.clone()).unwrap();
        assert_eq!(variants.len(), 1);
        assert!(variants[0].cfgs.is_empty());
        assert_eq!(variants[0].tokens.to_string(), body.to_string());

        let variants = expand(quote!(#![cfg_attr(P, symbol_prefix = "prefix")] #body)).unwrap();
        assert_eq!(variants.len(), 2);
        for variant in variants {
            assert!(variant.tokens.to_string().ends_with(&body.to_string()));
            assert_eq!(variant.cfgs.len(), 1);
        }
    }

    #[test]
    fn nested_inner_conditions_preserve_order_and_duplicates() {
        let variants = expand(quote! {
            #![cfg_attr(P, cfg_attr(P, custom(first), custom(first), custom(second),),)]
            fn f();
        })
        .unwrap();
        assert_eq!(variants.len(), 2);
        assert_eq!(
            variants
                .iter()
                .find(|v| v.cfgs[0].1)
                .unwrap()
                .tokens
                .to_string(),
            quote! {
                #![custom(first)] #![custom(first)] #![custom(second)] fn f();
            }
            .to_string()
        );
    }

    #[test]
    fn inner_cfg_gates_the_whole_invocation() {
        let variants = expand(quote!(#![cfg(P)] #![unsafe(export("C"))] fn f();)).unwrap();
        let emitted = emit_macro_invocations(quote!(co3::ffi), TokenStream::new(), variants);
        let file: syn::File = syn::parse2(emitted).unwrap();
        assert_eq!(file.items.len(), 1);
    }
}
