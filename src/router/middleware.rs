use proc_macro::TokenStream;

use super::common::{MiddlewareKind, relocate_below_owner};

/// Only expands when `#[middleware(...)]` is written ABOVE its owner (route
/// macro or `#[controller]`); below the owner it is consumed as an inert
/// attribute. The argument is validated here so typos fail early, then the
/// attribute is moved below the owner.
pub fn middleware_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = proc_macro2::TokenStream::from(attr);
    let path: syn::Path = match syn::parse2(args.clone()) {
        Ok(p) => p,
        Err(_) => {
            return syn::Error::new_spanned(
                &args,
                "middleware expects a single middleware name, e.g. #[middleware(auth)]",
            )
            .to_compile_error()
            .into();
        }
    };
    if let Err(e) = MiddlewareKind::from_path(&path) {
        return e.to_compile_error().into();
    }
    relocate_below_owner("middleware", args, item)
}
