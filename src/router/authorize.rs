use proc_macro::TokenStream;
use syn::{Path, punctuated::Punctuated};

use super::common::relocate_below_owner;

pub fn parse_permission_paths(tokens: proc_macro2::TokenStream) -> syn::Result<Vec<Path>> {
    let span_source = tokens.clone();
    let parsed =
        syn::parse::Parser::parse2(Punctuated::<Path, syn::Token![,]>::parse_terminated, tokens)?;
    let paths: Vec<Path> = parsed.into_iter().collect();
    if paths.is_empty() {
        return Err(syn::Error::new_spanned(
            span_source,
            "authorize expects one or more Permission variants: \
             #[authorize(Permission::ServerRead)]",
        ));
    }
    for p in &paths {
        let valid = p.segments.len() == 2 && p.segments[0].ident == "Permission";
        if !valid {
            return Err(syn::Error::new_spanned(
                p,
                "authorize expects Permission::Variant paths, e.g. #[authorize(Permission::ServerRead)]",
            ));
        }
    }
    Ok(paths)
}

/// `#[authorize(Permission::UserRead, ...)]` — read by the route macro. Only
/// expands on its own when written ABOVE the route macro; it validates the
/// input and moves itself below the route macro.
pub fn authorize_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = proc_macro2::TokenStream::from(attr);
    if let Err(e) = parse_permission_paths(args.clone()) {
        return e.to_compile_error().into();
    }
    relocate_below_owner("authorize", args, item)
}
