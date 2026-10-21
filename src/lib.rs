mod router;
mod validator;

use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn validator(attr: TokenStream, item: TokenStream) -> TokenStream {
    validator::validator_impl(attr, item)
}

#[proc_macro_attribute]
pub fn get(attr: TokenStream, item: TokenStream) -> TokenStream {
    router::route::generate_route_impl(attr, item, "GET")
}

#[proc_macro_attribute]
pub fn post(attr: TokenStream, item: TokenStream) -> TokenStream {
    router::route::generate_route_impl(attr, item, "POST")
}

#[proc_macro_attribute]
pub fn put(attr: TokenStream, item: TokenStream) -> TokenStream {
    router::route::generate_route_impl(attr, item, "PUT")
}

#[proc_macro_attribute]
pub fn delete(attr: TokenStream, item: TokenStream) -> TokenStream {
    router::route::generate_route_impl(attr, item, "DELETE")
}

#[proc_macro_attribute]
pub fn patch(attr: TokenStream, item: TokenStream) -> TokenStream {
    router::route::generate_route_impl(attr, item, "PATCH")
}

#[proc_macro_attribute]
pub fn head(attr: TokenStream, item: TokenStream) -> TokenStream {
    router::route::generate_route_impl(attr, item, "HEAD")
}

#[proc_macro_attribute]
pub fn options(attr: TokenStream, item: TokenStream) -> TokenStream {
    router::route::generate_route_impl(attr, item, "OPTIONS")
}

#[proc_macro_attribute]
pub fn controller(attr: TokenStream, item: TokenStream) -> TokenStream {
    router::controller::controller_impl(attr, item)
}

#[proc_macro_attribute]
pub fn middleware(attr: TokenStream, item: TokenStream) -> TokenStream {
    router::middleware::middleware_impl(attr, item)
}

#[proc_macro_attribute]
pub fn authorize(attr: TokenStream, item: TokenStream) -> TokenStream {
    router::authorize::authorize_impl(attr, item)
}

/// Marks a route as reachable without the auth middleware (also opts the
/// route out of controller-level `#[middleware(...)]`). Bare marker read by
/// the route macro; when written above the route macro it moves itself below.
#[proc_macro_attribute]
pub fn public(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[public] takes no arguments",
        )
        .to_compile_error()
        .into();
    }
    router::common::relocate_below_owner("public", proc_macro2::TokenStream::new(), item)
}
