//! Checks for names whose spelling has special meaning during macro expansion.

use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, TypePath, visit::Visit};

use crate::{
    ForeignItem,
    parse::{ParsedAlias, RawFnDecl},
    utils::cfg_attrs,
};

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

    fn add(&mut self, actual: TokenStream, expected: &[syn::Type], message: &str, use_alloc: bool) {
        let cfg = &self.cfg;
        let alloc = use_alloc.then(|| {
            quote!(
                extern crate alloc;
            )
        });
        let check = quote! {
            #(#cfg)*
            {
                #alloc
                #[diagnostic::on_unimplemented(message = #message)]
                trait ExactType {}
                #(impl ExactType for #expected {})*
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
        if let Some(known) = crate::known_type::recognize_path(ty) {
            let name = &ty.path.segments.last().unwrap().ident;
            let mut actual = ty.clone();
            if known.arity > 0 {
                let syn::Type::Path(canonical) = &known.canonical else {
                    unreachable!()
                };
                actual.path.segments.last_mut().unwrap().arguments =
                    canonical.path.segments.last().unwrap().arguments.clone();
            }
            let mut expected = vec![known.canonical];
            if let Some(alternate) = known.alternate {
                expected.push(alternate);
            }
            let message = match known.kind {
                crate::known_type::Kind::Option => {
                    "`Option` must resolve to `core::option::Option` or `co3::option::Option`"
                        .to_owned()
                }
                crate::known_type::Kind::String => {
                    "`String` must resolve to `alloc::string::String` or `co3::string::String`"
                        .to_owned()
                }
                crate::known_type::Kind::Vec => {
                    "`Vec` must resolve to `alloc::vec::Vec` or `co3::vec::Vec`".to_owned()
                }
                crate::known_type::Kind::Result => {
                    "`Result` must resolve to `core::result::Result` or `co3::result::Result`"
                        .to_owned()
                }
                crate::known_type::Kind::NonZero => {
                    "`NonZero` must resolve to `core::num::NonZero` or `co3::marker::NonZero`"
                        .to_owned()
                }
                crate::known_type::Kind::Box if name == "Box" => {
                    "`Box` must resolve to `co3::boxed::Box`".to_owned()
                }
                _ => format!("`{name}` must resolve to `{}`", known.canonical_name),
            };
            self.add(
                quote!(#actual),
                &expected,
                &message,
                matches!(
                    known.kind,
                    crate::known_type::Kind::Vec
                        | crate::known_type::Kind::String
                        | crate::known_type::Kind::CString
                ),
            );
        }
        syn::visit::visit_type_path(self, ty);
    }
}

#[cfg(test)]
mod tests {
    use syn::parse_quote;

    use super::TypeChecks;

    #[test]
    fn collects_nested_special_types_once() {
        let item = parse_quote! {
            struct Fields<T>(Option<u32>, Option<u32>, core::marker::PhantomData<T>, Box<[u32]>, core::mem::MaybeUninit<T>);
        };
        let checks = TypeChecks::for_derive(&item).to_string();
        assert_eq!(checks.matches("trait ExactType").count(), 5);
        assert!(checks.contains("core :: option :: Option < () >"));
        assert!(checks.contains("core :: primitive :: u32"));
        assert!(checks.contains("core :: marker :: PhantomData < () >"));
        assert!(
            checks.contains(
                "`Option` must resolve to `core::option::Option` or `co3::option::Option`"
            )
        );
        assert!(checks.contains("`u32` must resolve to `core::primitive::u32`"));
        assert!(checks.contains("`Box` must resolve to `co3::boxed::Box`"));
        assert!(checks.contains("core :: mem :: MaybeUninit < () >"));
        assert!(checks.contains("`MaybeUninit` must resolve to `core::mem::MaybeUninit`"));
        assert!(!checks.contains("check :: < T >"));
    }

    #[test]
    fn box_identity_excludes_abi_carriers() {
        let item = parse_quote!(
            struct Node {
                next: Box<Node>,
            }
        );
        let checks = TypeChecks::for_derive(&item).to_string();
        assert!(checks.contains("impl ExactType for co3 :: boxed :: Box"));
        assert!(!checks.contains("impl ExactType for co3 :: boxed :: abi :: Box"));
        assert!(
            crate::known_type::recognize(&parse_quote!(Box<u8>))
                .unwrap()
                .alternate
                .is_none()
        );
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

    #[test]
    fn checks_co3_tuple_identity() {
        let item = parse_quote! {
            struct Fields(co3::tuple::Tuple1<u8>, Tuple2<u8, u16>, co3::tuple::Tuple12<u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8>);
        };
        let checks = TypeChecks::for_derive(&item).to_string();
        for arity in [1, 2, 12] {
            assert!(checks.contains(&format!(
                "`Tuple{arity}` must resolve to `co3::tuple::Tuple{arity}`"
            )));
        }
    }
}
