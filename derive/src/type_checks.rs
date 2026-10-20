//! Checks for names whose spelling has special meaning during macro expansion.

use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, TypePath, visit::Visit};

use crate::{ForeignItem, parse::ParsedAlias, parse::RawFnDecl, utils::cfg_attrs};

#[derive(Default)]
pub(crate) struct TypeChecks {
    checks: Vec<TokenStream>,
    seen: HashSet<String>,
    cfg: Vec<Attribute>,
}

impl TypeChecks {
    pub(crate) fn for_derive(input: &syn::DeriveInput) -> TokenStream {
        let mut checks = Self::default();
        checks.visit_derive_input(input);
        checks.finish()
    }

    pub(crate) fn for_ffi(items: &[ForeignItem], aliases: &[ParsedAlias]) -> TokenStream {
        let mut checks = Self::default();
        for alias in aliases {
            checks.with_cfg(&alias.item.attrs, |checks| {
                checks.visit_item_type(&alias.item)
            });
        }
        for item in items {
            match item {
                ForeignItem::Type(item) => {
                    checks.with_cfg(&item.ty.attrs, |checks| {
                        checks.visit_foreign_item_type(&item.ty);
                        if let Some(id) = &item.id {
                            checks.visit_type(id);
                        }
                    });
                }
                ForeignItem::Static(item) => {
                    checks.with_cfg(&item.attrs, |checks| checks.visit_type(&item.ty));
                }
                ForeignItem::Impl(item) => {
                    checks.with_cfg(&item.item.attrs, |checks| {
                        checks.visit_item_impl(&item.item)
                    });
                }
                ForeignItem::Fn(item) => {
                    checks.with_cfg(&item.item.attrs, |checks| checks.visit_item_fn(&item.item));
                }
            }
        }
        checks.finish()
    }

    pub(crate) fn for_raw(items: &[RawFnDecl]) -> TokenStream {
        let mut checks = Self::default();
        for item in items {
            checks.with_cfg(&item.attrs, |checks| {
                checks.visit_signature(&item.sig);
                if let Some(owner) = &item.owner {
                    checks.with_cfg(&owner.attrs, |checks| {
                        checks.visit_type(&owner.self_ty);
                        checks.visit_generics(&owner.generics);
                    });
                }
            });
        }
        checks.finish()
    }

    fn with_cfg(&mut self, attrs: &[Attribute], f: impl FnOnce(&mut Self)) {
        let len = self.cfg.len();
        self.cfg.extend(cfg_attrs(attrs).cloned());
        f(self);
        self.cfg.truncate(len);
    }

    fn add(&mut self, actual: TokenStream, expected: TokenStream, message: &str) {
        let cfg = &self.cfg;
        let check = quote! {
            #(#cfg)*
            {
                #[diagnostic::on_unimplemented(message = #message)]
                trait ExactType {}
                impl ExactType for #expected {}
                fn check<T: ExactType + ?Sized>() {}
                let _ = check::<#actual>;
            }
        };
        if self.seen.insert(check.to_string()) {
            self.checks.push(check);
        }
    }

    fn finish(self) -> TokenStream {
        if self.checks.is_empty() {
            return TokenStream::new();
        }
        let checks = self.checks;
        quote! {
            const _: () = {
                #(#checks)*
            };
        }
    }
}

impl<'ast> Visit<'ast> for TypeChecks {
    fn visit_field(&mut self, field: &'ast syn::Field) {
        self.with_cfg(&field.attrs, |checks| {
            syn::visit::visit_field(checks, field)
        });
    }

    fn visit_variant(&mut self, variant: &'ast syn::Variant) {
        self.with_cfg(&variant.attrs, |checks| {
            syn::visit::visit_variant(checks, variant)
        });
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        self.with_cfg(&item.attrs, |checks| {
            syn::visit::visit_impl_item_fn(checks, item)
        });
    }

    fn visit_fn_arg(&mut self, arg: &'ast syn::FnArg) {
        match arg {
            syn::FnArg::Typed(arg) => {
                self.with_cfg(&arg.attrs, |checks| syn::visit::visit_pat_type(checks, arg));
            }
            syn::FnArg::Receiver(receiver) => {
                self.with_cfg(&receiver.attrs, |checks| {
                    checks.visit_type(&crate::utils::receiver_ty(receiver))
                });
            }
        }
    }

    fn visit_type_path(&mut self, ty: &'ast TypePath) {
        if ty.qself.is_none() {
            if let Some(identity) = crate::ffi_fn::copy_primitive_type(ty) {
                let name = ty.path.segments.last().unwrap().ident.to_string();
                let canonical = if name.starts_with("c_") {
                    format!("co3::ffi::{name}")
                } else {
                    format!("core::primitive::{name}")
                };
                self.add(
                    quote!(#ty),
                    quote!(#identity),
                    &format!("`{name}` must resolve to `{canonical}`"),
                );
            } else if let Some(last) = ty.path.segments.last() {
                let name = last.ident.to_string();
                match name.as_str() {
                    "str" if last.arguments.is_empty() => {
                        self.add(
                            quote!(#ty),
                            quote!(core::primitive::str),
                            "`str` must resolve to `core::primitive::str`",
                        );
                    }
                    "CStr" if last.arguments.is_empty() => {
                        self.add(
                            quote!(#ty),
                            quote!(core::ffi::CStr),
                            "`CStr` must resolve to `core::ffi::CStr`",
                        );
                    }
                    "Option" | "PhantomData" | "Box" => {
                        if let syn::PathArguments::AngleBracketed(args) = &last.arguments
                            && args.args.len() == 1
                            && matches!(args.args.first(), Some(syn::GenericArgument::Type(_)))
                        {
                            let mut actual = ty.clone();
                            actual.path.segments.last_mut().unwrap().arguments =
                                syn::PathArguments::AngleBracketed(syn::parse_quote!(<()>));
                            let (expected, canonical) = match name.as_str() {
                                "Option" => {
                                    (quote!(core::option::Option<()>), "core::option::Option")
                                }
                                "PhantomData" => (
                                    quote!(core::marker::PhantomData<()>),
                                    "core::marker::PhantomData",
                                ),
                                "Box" => (quote!(co3::boxed::Box<()>), "co3::boxed::Box"),
                                _ => unreachable!(),
                            };
                            self.add(
                                quote!(#actual),
                                expected,
                                &format!("`{name}` must resolve to `{canonical}`"),
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
        syn::visit::visit_type_path(self, ty);
    }
}

#[cfg(test)]
mod tests {
    use super::TypeChecks;
    use syn::parse_quote;

    #[test]
    fn collects_nested_special_types_once() {
        let item = parse_quote! {
            struct Fields<T>(Option<u32>, Option<u32>, core::marker::PhantomData<T>);
        };
        let checks = TypeChecks::for_derive(&item).to_string();
        assert_eq!(checks.matches("trait ExactType").count(), 3);
        assert!(checks.contains("core :: option :: Option < () >"));
        assert!(checks.contains("core :: primitive :: u32"));
        assert!(checks.contains("core :: marker :: PhantomData < () >"));
        assert!(checks.contains("`Option` must resolve to `core::option::Option`"));
        assert!(checks.contains("`u32` must resolve to `core::primitive::u32`"));
        assert!(!checks.contains("check :: < T >"));
    }

    #[test]
    fn preserves_field_cfg_on_checks() {
        let item = parse_quote! {
            struct Fields {
                #[cfg(feature = "maybe")]
                value: CStr,
            }
        };
        let checks = TypeChecks::for_derive(&item).to_string();
        assert!(checks.contains("cfg (feature = \"maybe\")"));
    }
}
