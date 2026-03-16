use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::Parser;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DispatchSigKind {
    MethodLike,
    FreeFn,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlockKind {
    Export,
    Extern,
}

fn receiver_to_typed_arg_tokens(receiver: syn::Receiver) -> TokenStream {
    let attrs = receiver.attrs;
    let mutability = receiver.mutability;
    let self_ident = syn::Ident::new("self_", proc_macro2::Span::call_site());
    let ty: syn::Type = match receiver.reference {
        Some((and_token, lifetime)) => syn::Type::Reference(syn::TypeReference {
            and_token,
            lifetime,
            mutability: receiver.mutability,
            elem: Box::new(syn::parse_quote!(Self)),
        }),
        None => syn::parse_quote!(Self),
    };
    quote! {
        #(#attrs)*
        #mutability #self_ident: #ty
    }
}

struct PreprocessedDispatchArg {
    tokens: TokenStream,
}

impl PreprocessedDispatchArg {
    fn parse_with(input: syn::parse::ParseStream, sig_kind: DispatchSigKind) -> syn::Result<Self> {
        let attrs = input.call(syn::Attribute::parse_outer)?;
        let mut merged_attrs = attrs;
        if input.peek(syn::Token![move]) {
            input.parse::<syn::Token![move]>()?;
            merged_attrs.push(syn::parse_quote!(#[by_val]));
        }

        let receiver = if sig_kind == DispatchSigKind::MethodLike {
            if input.peek(syn::Token![&]) {
                let ahead = input.fork();
                let _ = ahead.parse::<syn::Token![&]>()?;
                let _ = ahead.parse::<Option<syn::Lifetime>>()?;
                let _ = ahead.parse::<Option<syn::Token![mut]>>()?;
                ahead.peek(syn::Token![self])
            } else if input.peek(syn::Token![self]) {
                let ahead = input.fork();
                let _ = ahead.parse::<syn::Token![self]>()?;
                !ahead.peek(syn::Token![:])
            } else {
                false
            }
        } else {
            false
        };

        if receiver {
            let mut receiver = input.parse::<syn::Receiver>()?;
            merged_attrs.append(&mut receiver.attrs);
            receiver.attrs = merged_attrs;
            return Ok(Self {
                tokens: receiver_to_typed_arg_tokens(receiver),
            });
        }

        let mut arg = input.parse::<syn::PatType>()?;
        if sig_kind == DispatchSigKind::MethodLike
            && let syn::Pat::Ident(pat_ident) = arg.pat.as_ref()
            && pat_ident.ident.to_string().ends_with("_id")
        {
            merged_attrs.push(syn::parse_quote!(#[id]));
        }
        merged_attrs.append(&mut arg.attrs);
        arg.attrs = merged_attrs;
        Ok(Self {
            tokens: quote!(#arg),
        })
    }
}

fn preprocess_dispatch_inputs(
    inputs: TokenStream,
    sig_kind: DispatchSigKind,
) -> syn::Result<TokenStream> {
    let parser = move |input: syn::parse::ParseStream| -> syn::Result<TokenStream> {
        let mut args = Vec::new();
        while !input.is_empty() {
            args.push(PreprocessedDispatchArg::parse_with(input, sig_kind)?.tokens);
            if input.is_empty() {
                break;
            }
            input.parse::<syn::Token![,]>()?;
        }
        Ok(quote!(#(#args),*))
    };
    parser.parse2(inputs)
}

pub(crate) fn preprocess_dispatch_signature_tokens(
    signature_tokens: TokenStream,
    sig_kind: DispatchSigKind,
) -> syn::Result<TokenStream> {
    let mut rewritten = Vec::new();
    let mut saw_inputs = false;
    for tt in signature_tokens {
        if let proc_macro2::TokenTree::Group(group) = &tt
            && group.delimiter() == proc_macro2::Delimiter::Parenthesis
            && !saw_inputs
        {
            let rewritten_args = preprocess_dispatch_inputs(group.stream(), sig_kind)?;
            let mut new_group =
                proc_macro2::Group::new(proc_macro2::Delimiter::Parenthesis, rewritten_args);
            new_group.set_span(group.span());
            rewritten.push(proc_macro2::TokenTree::Group(new_group));
            saw_inputs = true;
            continue;
        }
        rewritten.push(tt);
    }

    Ok(rewritten.into_iter().collect())
}

pub(crate) fn parse_dispatch_signature(
    input: syn::parse::ParseStream,
    sig_kind: DispatchSigKind,
) -> syn::Result<syn::Signature> {
    let mut signature_tokens = TokenStream::new();
    while !input.peek(syn::Token![;]) && !input.peek(syn::token::Brace) {
        let tt: proc_macro2::TokenTree = input.parse()?;
        signature_tokens.extend(std::iter::once(tt));
    }

    let rewritten = preprocess_dispatch_signature_tokens(signature_tokens, sig_kind)?;
    syn::parse2::<syn::Signature>(rewritten)
}

fn rewrite_method_body_tokens(tokens: TokenStream) -> TokenStream {
    fn recurse(tokens: TokenStream) -> TokenStream {
        let items = tokens.into_iter().collect::<Vec<_>>();
        let mut out = TokenStream::new();
        let mut idx = 0usize;
        while idx < items.len() {
            if idx + 4 < items.len()
                && let proc_macro2::TokenTree::Ident(base_ident) = &items[idx]
                && base_ident == "self"
                && matches!(&items[idx + 1], proc_macro2::TokenTree::Punct(p) if p.as_char() == ':')
                && matches!(&items[idx + 2], proc_macro2::TokenTree::Punct(p) if p.as_char() == ':')
                && matches!(&items[idx + 3], proc_macro2::TokenTree::Punct(p) if p.as_char() == '<')
                && matches!(&items[idx + 4], proc_macro2::TokenTree::Ident(_))
            {
                let mut end = idx + 5;
                let mut depth = 1usize;
                while end < items.len() {
                    match &items[end] {
                        proc_macro2::TokenTree::Punct(p) if p.as_char() == '<' => depth += 1,
                        proc_macro2::TokenTree::Punct(p) if p.as_char() == '>' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    end += 1;
                }
                let ident = syn::Ident::new("self_", base_ident.span());
                out.extend(quote!(#ident));
                idx = end + 1;
                continue;
            }

            let tt = match &items[idx] {
                proc_macro2::TokenTree::Group(group) => {
                    let mut new_group =
                        proc_macro2::Group::new(group.delimiter(), recurse(group.stream()));
                    new_group.set_span(group.span());
                    proc_macro2::TokenTree::Group(new_group)
                }
                proc_macro2::TokenTree::Ident(ident) if ident == "self" => {
                    proc_macro2::TokenTree::Ident(syn::Ident::new("self_", ident.span()))
                }
                other => other.clone(),
            };
            out.extend(core::iter::once(tt));
            idx += 1;
        }
        out
    }

    recurse(tokens)
}

fn parse_fn_item(input: syn::parse::ParseStream) -> syn::Result<syn::ItemFn> {
    let attrs = input.call(syn::Attribute::parse_outer)?;
    let vis = input.parse::<syn::Visibility>()?;
    let sig = parse_dispatch_signature(input, DispatchSigKind::FreeFn)?;
    if input.peek(syn::token::Brace) {
        return Err(input.error("function bodies are not allowed here"));
    }
    input.parse::<syn::Token![;]>()?;
    syn::parse2(quote! {
        #(#attrs)* #vis #sig {}
    })
}

fn parse_trait_item(
    input: syn::parse::ParseStream,
    block_kind: BlockKind,
) -> syn::Result<syn::ItemTrait> {
    fn preprocess_trait_items(
        input: syn::parse::ParseStream,
        block_kind: BlockKind,
    ) -> syn::Result<TokenStream> {
        let mut out = TokenStream::new();
        while !input.is_empty() {
            while input.peek(syn::Token![;]) {
                let _: syn::Token![;] = input.parse()?;
            }
            if input.is_empty() {
                break;
            }

            let attrs = input.call(syn::Attribute::parse_outer)?;
            let ahead = input.fork();
            let is_assoc_type = ahead.peek(syn::Token![type]);
            let is_assoc_const = if ahead.peek(syn::Token![const]) {
                let fork = ahead.fork();
                let _: syn::Token![const] = fork.parse()?;
                !fork.peek(syn::Token![fn])
            } else {
                false
            };

            if is_assoc_type {
                if block_kind == BlockKind::Export {
                    return Err(input.error(
                        "trait export entries do not support associated types; only method declarations are allowed",
                    ));
                }
                let item = input.parse::<syn::TraitItemType>()?;
                out.extend(quote!(#(#attrs)* #item));
                continue;
            }
            if is_assoc_const {
                if block_kind == BlockKind::Export {
                    return Err(input.error(
                        "trait export entries do not support associated consts; only method declarations are allowed",
                    ));
                }
                let item = input.parse::<syn::TraitItemConst>()?;
                out.extend(quote!(#(#attrs)* #item));
                continue;
            }

            let sig = parse_dispatch_signature(input, DispatchSigKind::MethodLike)?;
            let body = if input.peek(syn::token::Brace) {
                if block_kind != BlockKind::Export {
                    return Err(input.error("method bodies are not allowed here"));
                }
                let content;
                syn::braced!(content in input);
                let tokens = rewrite_method_body_tokens(content.parse()?);
                quote!({ #tokens })
            } else {
                input.parse::<syn::Token![;]>()?;
                quote!({})
            };
            out.extend(quote!(#(#attrs)* #sig #body));
        }
        Ok(out)
    }

    let attrs = input.call(syn::Attribute::parse_outer)?;
    let vis = input.parse::<syn::Visibility>()?;
    input.parse::<syn::Token![trait]>()?;
    let ident = input.parse::<syn::Ident>()?;
    let mut generics = input.parse::<syn::Generics>()?;
    let supertraits = if input.peek(syn::Token![:]) {
        let colon = input.parse::<syn::Token![:]>()?;
        let bounds = syn::punctuated::Punctuated::<
            syn::TypeParamBound,
            syn::Token![+],
        >::parse_separated_nonempty(input)?;
        Some((colon, bounds))
    } else {
        None
    };
    generics.where_clause = input.parse::<Option<syn::WhereClause>>()?;

    let content;
    syn::braced!(content in input);
    let items = preprocess_trait_items(&content, block_kind)?;

    let trait_tokens = if let Some((colon, bounds)) = supertraits {
        quote! {
            #(#attrs)* #vis trait #ident #generics #colon #bounds {
                #items
            }
        }
    } else {
        quote! {
            #(#attrs)* #vis trait #ident #generics {
                #items
            }
        }
    };
    syn::parse2(trait_tokens)
}

fn parse_impl_item(
    input: syn::parse::ParseStream,
    block_kind: BlockKind,
) -> syn::Result<syn::ItemImpl> {
    fn preprocess_impl_items(
        input: syn::parse::ParseStream,
        block_kind: BlockKind,
    ) -> syn::Result<TokenStream> {
        let mut out = TokenStream::new();
        while !input.is_empty() {
            while input.peek(syn::Token![;]) {
                let _: syn::Token![;] = input.parse()?;
            }
            if input.is_empty() {
                break;
            }

            let attrs = input.call(syn::Attribute::parse_outer)?;
            let vis = input.parse::<syn::Visibility>()?;
            let ahead = input.fork();
            if ahead.peek(syn::Token![type]) {
                if block_kind == BlockKind::Export {
                    return Err(input.error("export impl entries only support method declarations"));
                }
                let item = input.parse::<syn::ImplItemType>()?;
                out.extend(quote!(#(#attrs)* #vis #item));
                continue;
            }
            if ahead.peek(syn::Token![const]) {
                if block_kind == BlockKind::Export {
                    return Err(input.error("export impl entries only support method declarations"));
                }
                let item = input.parse::<syn::ImplItemConst>()?;
                out.extend(quote!(#(#attrs)* #vis #item));
                continue;
            }

            let sig = parse_dispatch_signature(input, DispatchSigKind::MethodLike)?;
            let body = if input.peek(syn::token::Brace) {
                return Err(input.error("method bodies are not allowed here"));
            } else {
                input.parse::<syn::Token![;]>()?;
                quote!({})
            };
            out.extend(quote!(#(#attrs)* #vis #sig #body));
        }
        Ok(out)
    }

    let attrs = input.call(syn::Attribute::parse_outer)?;
    let defaultness = input.parse::<Option<syn::Token![default]>>()?;
    let unsafety = input.parse::<Option<syn::Token![unsafe]>>()?;
    input.parse::<syn::Token![impl]>()?;

    let mut header = TokenStream::new();
    while !input.peek(syn::token::Brace) {
        let tt: proc_macro2::TokenTree = input.parse()?;
        header.extend(std::iter::once(tt));
    }

    let content;
    syn::braced!(content in input);
    let items = preprocess_impl_items(&content, block_kind)?;

    let impl_tokens = quote! {
        #(#attrs)* #defaultness #unsafety impl #header {
            #items
        }
    };
    syn::parse2(impl_tokens)
}

fn parse_type_item(input: syn::parse::ParseStream) -> syn::Result<syn::ItemStruct> {
    let attrs = input.call(syn::Attribute::parse_outer)?;
    let vis = input.parse::<syn::Visibility>()?;
    input.parse::<syn::Token![type]>()?;
    let ident = input.parse::<syn::Ident>()?;
    let generics = input.parse::<syn::Generics>()?;
    input.parse::<syn::Token![;]>()?;
    syn::parse2(quote! {
        #(#attrs)* #vis struct #ident #generics;
    })
}

pub(crate) fn parse_items_with_kind(
    input: syn::parse::ParseStream,
    block_kind: BlockKind,
) -> syn::Result<Vec<syn::Item>> {
    let mut items = Vec::new();
    while !input.is_empty() {
        let ahead = input.fork();
        let _ = ahead.call(syn::Attribute::parse_outer)?;
        let item = if ahead.peek(syn::Token![impl]) {
            syn::Item::Impl(parse_impl_item(input, block_kind)?)
        } else if ahead.peek(syn::Token![trait]) {
            if block_kind == BlockKind::Extern {
                return Err(ahead.error(
                    "trait declarations are not supported in extern blocks; declare extern impls instead",
                ));
            }
            syn::Item::Trait(parse_trait_item(input, block_kind)?)
        } else {
            let _ = ahead.parse::<syn::Visibility>()?;
            if ahead.peek(syn::Token![struct]) {
                let msg = if block_kind == BlockKind::Extern {
                    "struct declarations are not allowed in extern blocks; use `type Foo;`"
                } else {
                    "struct declarations are not supported here"
                };
                return Err(ahead.error(msg));
            }
            if ahead.peek(syn::Token![type]) {
                syn::Item::Struct(parse_type_item(input)?)
            } else if ahead.peek(syn::Token![enum])
                || ahead.peek(syn::Token![union])
                || ahead.peek(syn::Token![mod])
                || ahead.peek(syn::Token![use])
                || ahead.peek(syn::Token![static])
            {
                return Err(ahead.error("item not supported here"));
            } else if ahead.peek(syn::Token![const]) {
                let fork = ahead.fork();
                let _: syn::Token![const] = fork.parse()?;
                if !fork.peek(syn::Token![fn]) {
                    return Err(ahead.error("item not supported here"));
                }
                syn::Item::Fn(parse_fn_item(input)?)
            } else {
                syn::Item::Fn(parse_fn_item(input)?)
            }
        };
        items.push(item);
    }
    Ok(items)
}
