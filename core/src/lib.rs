//! This crate contains most of the internal implementation of the macros in the
//! `macro_magic_macros` crate. For the most part, the proc macros in `macro_magic_macros` just
//! call their respective `_internal` variants in this crate.

use convert_case::{Case, Casing};
use derive_syn_parse::Parse;
use proc_macro2::Span;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, ToTokens};
use syn::parse2;
use syn::parse_quote;
use syn::{
    parse::Nothing,
    token::{Brace, Comma},
    Ident, Item, Path, Result, Token,
};

/// Used to parse the args for the [`import_tokens_internal`] function.
#[derive(Parse)]
pub struct ImportTokensArgs {
    _let: Token![let],
    tokens_var_ident: Ident,
    _eq: Token![=],
    source_path: Path,
}

/// Contains the contents of the [`ImportedTokensBrace`] struct.
#[derive(Parse)]
pub struct ImportedTokensBraceContents {
    tokens_var_ident: Ident,
    _comma: Comma,
    item: Item,
}

/// Used to parse the args for the [`import_tokens_inner_internal`] function.
#[derive(Parse)]
pub struct ImportedTokensBrace {
    #[brace]
    _braces: Brace,
    #[inside(_braces)]
    contents: ImportedTokensBraceContents,
}

/// Appends `member` to the end of the `::macro_magic::__private` path and returns the
/// resulting [`Path`]
pub fn private_path(member: &TokenStream2) -> Path {
    parse_quote!(::macro_magic::__private::#member)
}

/// "Flattens" an ident by converting it to snake case. This is used by
/// [`export_tokens_macro_ident`].
pub fn flatten_ident(ident: &Ident) -> Ident {
    Ident::new(
        ident.to_string().to_case(Case::Snake).as_str(),
        ident.span(),
    )
}

/// Produces the full path for the auto-generated callback-based tt macro that allows us to
/// forward tokens across crate boundaries
pub fn export_tokens_macro_ident(ident: &Ident) -> Ident {
    let ident = flatten_ident(&ident);
    let ident_string = format!("__export_tokens_tt_{}", ident.to_token_stream().to_string());
    Ident::new(ident_string.as_str(), Span::call_site())
}

/// The internal code behind the `#[export_tokens]` attribute macro. The `attr` variable
/// contains the tokens for the optional naming [`Ident`] (necessary on [`Item`]s that don't
/// have an inherent [`Ident`]) is the optional `attr` and the `tokens` variable is the tokens
/// for the [`Item`] the attribute macro can be attached to. The `attr` variable can be blank
/// tokens for supported items, which includes every valid [`syn::Item`] except for
/// [`syn::ItemForeignMod`], [`syn::ItemUse`], [`syn::ItemImpl`], and [`Item::Verbatim`], which
/// all require `attr` to be specified.
pub fn export_tokens_internal<T: Into<TokenStream2>, E: Into<TokenStream2>>(
    attr: T,
    tokens: E,
) -> Result<TokenStream2> {
    let attr = attr.into();
    let item: Item = parse2(tokens.into())?;
    let ident = match item.clone() {
        Item::Const(item_const) => Some(item_const.ident),
        Item::Enum(item_enum) => Some(item_enum.ident),
        Item::ExternCrate(item_extern_crate) => Some(item_extern_crate.ident),
        Item::Fn(item_fn) => Some(item_fn.sig.ident),
        Item::Macro(item_macro) => item_macro.ident, // note this one might not have an Ident as well
        Item::Macro2(item_macro2) => Some(item_macro2.ident),
        Item::Mod(item_mod) => Some(item_mod.ident),
        Item::Static(item_static) => Some(item_static.ident),
        Item::Struct(item_struct) => Some(item_struct.ident),
        Item::Trait(item_trait) => Some(item_trait.ident),
        Item::TraitAlias(item_trait_alias) => Some(item_trait_alias.ident),
        Item::Type(item_type) => Some(item_type.ident),
        Item::Union(item_union) => Some(item_union.ident),
        // Item::ForeignMod(item_foreign_mod) => None,
        // Item::Use(item_use) => None,
        // Item::Impl(item_impl) => None,
        _ => None,
    };
    let ident = match ident {
        Some(ident) => {
            if let Ok(_) = parse2::<Nothing>(attr.clone()) {
                ident
            } else {
                parse2::<Ident>(attr)?
            }
        }
        None => parse2::<Ident>(attr)?,
    };
    let ident = flatten_ident(&ident);
    Ok(quote! {
        #[macro_export]
        macro_rules! #ident {
            ($tokens_var:ident, $callback:path) => {
                $callback! {
                    {
                        $tokens_var,
                        #item
                    }
                }
            };
        }
        #[allow(unused)]
        #item
    })
}

/// The internal implementation for the `import_tokens` macro. You can call this in your own
/// proc macros to make use of the `import_tokens` functionality directly. The arguments should
/// be a [`TokenStream2`] that can parse into an [`ImportTokensArgs`] successfully. That is a
/// valid `let` variable declaration set to equal a path where an `#[export_tokens]` with the
/// specified ident can be found.
///
/// ### Example:
/// ```
/// use macro_magic_core::*;
/// use quote::quote;
///
/// let some_ident = quote!(tokens);
/// let some_path = quote!(other_crate::exported_item);
/// let tokens = import_tokens_internal(quote!(let #some_ident = other_crate::exported_item)).unwrap();
/// assert_eq!(
///     tokens.to_string(),
///     "other_crate :: __export_tokens_tt_exported_item ! (tokens , \
///     :: macro_magic :: __private :: __import_tokens_inner)");
/// ```
pub fn import_tokens_internal(tokens: TokenStream2) -> Result<TokenStream2> {
    let args = parse2::<ImportTokensArgs>(tokens)?;
    let Some(source_ident_seg) = args.source_path.segments.last() else { unreachable!("must have at least one segment") };
    let source_ident_seg = export_tokens_macro_ident(&source_ident_seg.ident);
    let source_path = if args.source_path.segments.len() > 1 {
        let Some(crate_seg) = args.source_path.segments.first() else {
            unreachable!("path has at least two segments, so there is a first segment");
        };
        quote!(#crate_seg::#source_ident_seg)
    } else {
        quote!(#source_ident_seg)
    };
    let inner_macro_path = private_path(&quote!(__import_tokens_inner));
    let tokens_var_ident = args.tokens_var_ident;
    Ok(quote! {
        #source_path!(#tokens_var_ident, #inner_macro_path)
    })
}

/// The internal implementation for the `__import_tokens_inner` macro. You shouldn't need to
/// call this in any circumstances but it is provided just in case.
pub fn import_tokens_inner_internal(tokens: TokenStream2) -> Result<TokenStream2> {
    let parsed = parse2::<ImportedTokensBrace>(tokens)?;
    let tokens_string = parsed.contents.item.to_token_stream().to_string();
    let ident = parsed.contents.tokens_var_ident;
    let token_stream_2 = private_path(&quote!(TokenStream2));
    Ok(quote! {
        let #ident = #tokens_string.parse::<#token_stream_2>().expect("failed to parse quoted tokens");
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_tokens_internal_missing_ident() {
        assert!(export_tokens_internal(quote!(), quote!(impl MyTrait for Something)).is_err());
    }

    #[test]
    fn export_tokens_internal_normal_no_ident() {
        assert!(export_tokens_internal(
            quote!(),
            quote!(
                struct MyStruct {}
            )
        )
        .unwrap()
        .to_string()
        .contains("my_struct"));
    }

    #[test]
    fn export_tokens_internal_normal_ident() {
        assert!(export_tokens_internal(
            quote!(some_name),
            quote!(
                struct Something {}
            ),
        )
        .unwrap()
        .to_string()
        .contains("some_name"));
    }

    #[test]
    fn export_tokens_internal_generics_no_ident() {
        assert!(export_tokens_internal(
            quote!(),
            quote!(
                struct MyStruct<T> {}
            ),
        )
        .unwrap()
        .to_string()
        .contains("my_struct {"));
    }

    #[test]
    fn export_tokens_internal_bad_ident() {
        assert!(export_tokens_internal(
            quote!(Something<T>),
            quote!(
                struct MyStruct {}
            ),
        )
        .is_err());
        assert!(export_tokens_internal(
            quote!(some::path),
            quote!(
                struct MyStruct {}
            ),
        )
        .is_err());
    }

    #[test]
    fn import_tokens_internal_simple_path() {
        assert!(
            import_tokens_internal(quote!(let tokens = my_crate::SomethingCool))
                .unwrap()
                .to_string()
                .contains("__export_tokens_tt_something_cool")
        );
    }

    #[test]
    fn import_tokens_internal_flatten_long_paths() {
        assert!(import_tokens_internal(
            quote!(let tokens = my_crate::some_mod::complex::SomethingElse)
        )
        .unwrap()
        .to_string()
        .contains("__export_tokens_tt_something_else"));
    }

    #[test]
    fn import_tokens_internal_invalid_token_ident() {
        assert!(import_tokens_internal(quote!(let 3 * 2 = my_crate::something)).is_err());
    }

    #[test]
    fn import_tokens_internal_invalid_path() {
        assert!(import_tokens_internal(quote!(let my_tokens = 2 - 2)).is_err());
    }

    #[test]
    fn import_tokens_inner_internal_basic() {
        assert!(import_tokens_inner_internal(quote! {
            {
                my_ident,
                fn my_function() -> u32 {
                    33
                }
            }
        })
        .unwrap()
        .to_string()
        .contains("my_ident"));
    }

    #[test]
    fn import_tokens_inner_internal_impl() {
        assert!(import_tokens_inner_internal(quote! {
            {
                another_ident,
                impl Something for MyThing {
                    fn something() -> CoolStuff {
                        CoolStuff {}
                    }
                }
            }
        })
        .unwrap()
        .to_string()
        .contains("something ()"));
    }

    #[test]
    fn import_tokens_inner_internal_missing_comma() {
        assert!(import_tokens_inner_internal(quote! {
            {
                another_ident
                impl Something for MyThing {
                    fn something() -> CoolStuff {
                        CoolStuff {}
                    }
                }
            }
        })
        .is_err());
    }

    #[test]
    fn import_tokens_inner_internal_non_item() {
        assert!(import_tokens_inner_internal(quote! {
            {
                another_ident,
                2 + 2
            }
        })
        .is_err());
    }
}
