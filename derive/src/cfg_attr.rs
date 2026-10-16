use proc_macro2::{Delimiter, Group, TokenStream, TokenTree};
use quote::quote;
use std::collections::HashSet;

#[derive(Clone)]
pub(crate) struct Variant {
    pub(crate) cfgs: Vec<TokenStream>,
    pub(crate) tokens: TokenStream,
}

pub(crate) fn expand(input: TokenStream) -> syn::Result<Vec<Variant>> {
    expand_stream(input)
}

pub(crate) fn emit_macro_invocations(
    macro_path: TokenStream,
    prefix: TokenStream,
    variants: Vec<Variant>,
) -> TokenStream {
    let invocations = variants.into_iter().map(|variant| {
        let cfgs = variant.cfgs.iter().map(|cfg| quote!(#[cfg(#cfg)]));
        let tokens = variant.tokens;

        quote! {
            #(#cfgs)*
            #macro_path! {
                #prefix
                #tokens
            }
        }
    });

    quote!(#(#invocations)*)
}

fn expand_stream(input: TokenStream) -> syn::Result<Vec<Variant>> {
    let mut variants = vec![Variant {
        cfgs: Vec::new(),
        tokens: TokenStream::new(),
    }];

    let tokens = input.into_iter().collect::<Vec<_>>();
    let mut idx = 0usize;
    while idx < tokens.len() {
        if let Some((attr_variants, consumed)) = expand_attr(&tokens[idx..])? {
            variants = append_variants(variants, attr_variants);
            idx += consumed;
            continue;
        }

        let token = tokens[idx].clone();
        let token_variants = match token {
            TokenTree::Group(group) => expand_group(group)?,
            token => vec![Variant {
                cfgs: Vec::new(),
                tokens: TokenStream::from(token),
            }],
        };

        variants = append_variants(variants, token_variants);
        idx += 1;
    }

    Ok(variants)
}

fn expand_group(group: Group) -> syn::Result<Vec<Variant>> {
    let delimiter = group.delimiter();
    let span = group.span();

    expand_stream(group.stream()).map(|variants| {
        variants
            .into_iter()
            .map(|variant| {
                let mut group = Group::new(delimiter, variant.tokens);
                group.set_span(span);

                Variant {
                    cfgs: variant.cfgs,
                    tokens: TokenStream::from(TokenTree::Group(group)),
                }
            })
            .collect()
    })
}

fn append_variants(lhs: Vec<Variant>, rhs: Vec<Variant>) -> Vec<Variant> {
    let mut out = Vec::with_capacity(lhs.len() * rhs.len());

    for left in lhs {
        for right in &rhs {
            let mut cfgs = Vec::with_capacity(left.cfgs.len() + right.cfgs.len());
            let mut seen_cfgs = HashSet::with_capacity(cfgs.capacity());
            for cfg in left.cfgs.iter().chain(&right.cfgs) {
                if seen_cfgs.insert(cfg.to_string()) {
                    cfgs.push(cfg.clone());
                }
            }

            let mut tokens = left.tokens.clone();
            tokens.extend(right.tokens.clone());

            out.push(Variant { cfgs, tokens });
        }
    }

    out
}

fn expand_attr(tokens: &[TokenTree]) -> syn::Result<Option<(Vec<Variant>, usize)>> {
    let Some(TokenTree::Punct(pound)) = tokens.first() else {
        return Ok(None);
    };
    if pound.as_char() != '#' {
        return Ok(None);
    }

    let (inner, group, consumed) = match tokens.get(1) {
        Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Bracket => {
            (false, group, 2)
        }
        Some(TokenTree::Punct(bang)) if bang.as_char() == '!' => {
            let Some(TokenTree::Group(group)) = tokens.get(2) else {
                return Ok(None);
            };
            if group.delimiter() != Delimiter::Bracket {
                return Ok(None);
            }

            (true, group, 3)
        }
        _ => return Ok(None),
    };

    let Some((predicate, attrs)) = parse_cfg_attr(group.stream())? else {
        let mut attr = TokenStream::from(tokens[0].clone());
        attr.extend(tokens[1..consumed].iter().cloned());

        return Ok(Some((
            vec![Variant {
                cfgs: Vec::new(),
                tokens: attr,
            }],
            consumed,
        )));
    };

    let present_attrs = attrs
        .into_iter()
        .map(|attr| quote_attr(inner, attr))
        .collect::<TokenStream>();

    let mut present = expand_stream(present_attrs)?;
    for variant in &mut present {
        variant.cfgs.insert(0, predicate.clone());
    }

    Ok(Some((
        {
            let mut variants = Vec::with_capacity(present.len() + 1);
            variants.extend(present);
            variants.push(Variant {
                cfgs: vec![quote!(not(#predicate))],
                tokens: TokenStream::new(),
            });
            variants
        },
        consumed,
    )))
}

fn quote_attr(inner: bool, attr: TokenStream) -> TokenStream {
    if inner {
        quote!(#![#attr])
    } else {
        quote!(#[#attr])
    }
}

fn parse_cfg_attr(input: TokenStream) -> syn::Result<Option<(TokenStream, Vec<TokenStream>)>> {
    let tokens = input.into_iter().collect::<Vec<_>>();
    let Some(TokenTree::Ident(path)) = tokens.first() else {
        return Ok(None);
    };
    if path != "cfg_attr" {
        return Ok(None);
    }

    let Some(TokenTree::Group(args)) = tokens.get(1) else {
        return Err(syn::Error::new_spanned(path, "expected cfg_attr arguments"));
    };
    if args.delimiter() != Delimiter::Parenthesis || tokens.len() != 2 {
        return Err(syn::Error::new_spanned(path, "expected cfg_attr arguments"));
    }

    let mut parts = split_top_level_commas(args.stream());
    if parts.is_empty() {
        return Err(syn::Error::new_spanned(path, "expected cfg_attr predicate"));
    }

    let predicate = parts.remove(0);
    if predicate.is_empty() {
        return Err(syn::Error::new_spanned(path, "expected cfg_attr predicate"));
    }
    if parts.is_empty() {
        return Err(syn::Error::new_spanned(path, "expected attribute"));
    }

    for attr in &parts {
        if attr.is_empty() {
            return Err(syn::Error::new_spanned(path, "expected attribute"));
        }
    }

    Ok(Some((predicate, parts)))
}

fn split_top_level_commas(input: TokenStream) -> Vec<TokenStream> {
    let mut parts = Vec::new();
    let mut current = TokenStream::new();

    for token in input {
        if matches!(&token, TokenTree::Punct(punct) if punct.as_char() == ',') {
            parts.push(current);
            current = TokenStream::new();
        } else {
            current.extend(TokenStream::from(token));
        }
    }

    parts.push(current);
    parts
}
