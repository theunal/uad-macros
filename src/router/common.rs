use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Attribute, Item, Path};

pub const ROUTE_MACROS: &[&str] = &["get", "post", "put", "delete", "patch", "head", "options"];

/// Last path segment of an attribute, so both `#[get]` and `#[uad_macros::get]` match.
pub fn attr_name(attr: &Attribute) -> Option<String> {
    attr.path().segments.last().map(|s| s.ident.to_string())
}

pub fn is_route_attr(attr: &Attribute) -> bool {
    attr_name(attr).is_some_and(|n| ROUTE_MACROS.contains(&n.as_str()))
}

pub fn has_attr(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|a| attr_name(a).as_deref() == Some(name))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MiddlewareKind {
    Auth,
    Authorize,
}

impl MiddlewareKind {
    pub fn from_path(path: &Path) -> syn::Result<Self> {
        let name = path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect::<Vec<_>>()
            .join("::");
        match name.as_str() {
            "auth_middleware" | "auth" => Ok(Self::Auth),
            other => Err(syn::Error::new_spanned(
                path,
                format!("Unknown middleware: '{other}'. Supported: auth"),
            )),
        }
    }

    pub fn to_tokens(self) -> TokenStream {
        match self {
            Self::Auth => quote! { crate::routing::MiddlewareKind::Auth },
            Self::Authorize => quote! { crate::routing::MiddlewareKind::Authorize },
        }
    }
}

/// `#[middleware]`, `#[authorize]` and `#[public]` are read by the route
/// macro (or `#[controller]`) as inert attributes. Attribute macros expand
/// top-down, so when one of them is written ABOVE its owner it expands first.
/// Instead of a thread-local side channel (which leaks into whichever route
/// expands next), re-emit the attribute directly below the owner so the owner
/// reads it on its own expansion.
pub fn relocate_below_owner(
    attr_name: &str,
    args: TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let item: Item = match syn::parse(item) {
        Ok(i) => i,
        Err(e) => return e.to_compile_error().into(),
    };
    let ident = syn::Ident::new(attr_name, Span::call_site());
    let marker: Attribute = if args.is_empty() {
        syn::parse_quote!(#[#ident])
    } else {
        syn::parse_quote!(#[#ident(#args)])
    };

    let result = match item {
        Item::Fn(mut func) => match func.attrs.iter().position(is_route_attr) {
            Some(idx) => {
                func.attrs.insert(idx + 1, marker);
                Ok(quote! { #func })
            }
            None => Err(format!(
                "#[{attr_name}] must be used together with a route macro \
                 (#[get], #[post], #[put], #[delete], #[patch], #[head], #[options])"
            )),
        },
        Item::Mod(mut item_mod) if allowed_on_controller(attr_name) => {
            match item_mod
                .attrs
                .iter()
                .position(|a| attr_name_is(a, "controller"))
            {
                Some(idx) => {
                    item_mod.attrs.insert(idx + 1, marker);
                    Ok(quote! { #item_mod })
                }
                None => Err(format!(
                    "#[{attr_name}] on a module requires #[controller(...)]"
                )),
            }
        }
        _ => Err(format!(
            "#[{attr_name}] can only be applied to route handler functions{}",
            if allowed_on_controller(attr_name) {
                " or #[controller] modules"
            } else {
                ""
            }
        )),
    };

    match result {
        Ok(ts) => ts.into(),
        Err(msg) => syn::Error::new(Span::call_site(), msg)
            .to_compile_error()
            .into(),
    }
}

fn allowed_on_controller(attr_name: &str) -> bool {
    matches!(attr_name, "middleware" | "authorize")
}

pub fn attr_name_is(attr: &Attribute, name: &str) -> bool {
    attr_name(attr).as_deref() == Some(name)
}

/// Guarantees a leading `/` and collapses repeated slashes
/// (`users//{id}` -> `/users/{id}`, `""` -> `/`).
pub fn normalize_route_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len() + 1);
    out.push('/');
    for segment in path.trim().split('/').filter(|s| !s.is_empty()) {
        if !out.ends_with('/') {
            out.push('/');
        }
        out.push_str(segment);
    }
    if path.trim().len() > 1 && path.trim().ends_with('/') && !out.ends_with('/') {
        out.push('/');
    }
    out
}

/// `""` and `"/"` both resolve to the prefix itself, so `#[get]`,
/// `#[get("")]` and `#[get("/")]` inside `#[controller("/users")]` are all
/// `/users` (and therefore conflict with each other).
pub fn join_paths(prefix: &str, path: &str) -> String {
    let prefix = prefix.trim().trim_end_matches('/');
    let path = path.trim();
    if path.is_empty() || path == "/" {
        return normalize_route_path(prefix);
    }
    normalize_route_path(&format!("{prefix}/{path}"))
}

/// Route shape used for conflict detection: parameter names are erased, so
/// `/{id}` and `/{user_id}` (or axum 0.7 style `/:id`) are treated as the same
/// route — axum would panic on them at startup anyway.
pub fn route_shape(path: &str) -> String {
    normalize_route_path(path)
        .split('/')
        .map(|seg| {
            if seg.starts_with("{*") || seg.starts_with('*') {
                "{*}"
            } else if (seg.starts_with('{') && seg.ends_with('}') && !seg.starts_with("{{"))
                || seg.starts_with(':')
            {
                "{}"
            } else {
                seg
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_adds_leading_slash_and_collapses_repeats() {
        assert_eq!(normalize_route_path(""), "/");
        assert_eq!(normalize_route_path("/"), "/");
        assert_eq!(normalize_route_path("users"), "/users");
        assert_eq!(normalize_route_path("//users///{id}"), "/users/{id}");
        assert_eq!(normalize_route_path("  /users  "), "/users");
    }

    #[test]
    fn normalize_keeps_a_single_trailing_slash() {
        assert_eq!(normalize_route_path("/users/"), "/users/");
        assert_eq!(normalize_route_path("/users///"), "/users/");
    }

    #[test]
    fn join_empty_or_root_path_is_the_prefix() {
        assert_eq!(join_paths("/users", ""), "/users");
        assert_eq!(join_paths("/users", "/"), "/users");
        assert_eq!(join_paths("/users/", ""), "/users");
        assert_eq!(join_paths("", ""), "/");
        assert_eq!(join_paths("", "/"), "/");
    }

    #[test]
    fn join_avoids_double_slashes() {
        assert_eq!(join_paths("/users", "/{id}"), "/users/{id}");
        assert_eq!(join_paths("/users", "{id}"), "/users/{id}");
        assert_eq!(join_paths("/users/", "/{id}"), "/users/{id}");
        assert_eq!(join_paths("", "/health"), "/health");
        assert_eq!(join_paths("/api", "//v1//users"), "/api/v1/users");
    }

    #[test]
    fn shape_ignores_parameter_names_and_syntax() {
        assert_eq!(route_shape("/users/{id}"), route_shape("/users/{user_id}"));
        assert_eq!(route_shape("/users/{id}"), route_shape("/users/:id"));
        assert_eq!(route_shape("/files/{*rest}"), route_shape("/files/*path"));
    }

    #[test]
    fn shape_distinguishes_different_routes() {
        assert_ne!(route_shape("/users/{id}"), route_shape("/users/me"));
        assert_ne!(route_shape("/users/{id}"), route_shape("/users/{id}/posts"));
        assert_ne!(route_shape("/files/{id}"), route_shape("/files/{*rest}"));
        assert_ne!(route_shape("/users"), route_shape("/users/"));
        // `{{` is an escaped literal brace in axum, not a parameter.
        assert_ne!(route_shape("/{{literal}}"), route_shape("/{id}"));
    }

    #[test]
    fn route_attr_detection_accepts_qualified_paths() {
        let bare: Attribute = syn::parse_quote!(#[get]);
        let qualified: Attribute = syn::parse_quote!(#[uad_macros::post("/x")]);
        let other: Attribute = syn::parse_quote!(#[middleware(auth)]);
        assert!(is_route_attr(&bare));
        assert!(is_route_attr(&qualified));
        assert!(!is_route_attr(&other));
    }

    #[test]
    fn middleware_kind_parsing() {
        let auth: Path = syn::parse_quote!(auth);
        let long: Path = syn::parse_quote!(auth_middleware);
        let unknown: Path = syn::parse_quote!(logging);
        assert_eq!(
            MiddlewareKind::from_path(&auth).unwrap(),
            MiddlewareKind::Auth
        );
        assert_eq!(
            MiddlewareKind::from_path(&long).unwrap(),
            MiddlewareKind::Auth
        );
        let err = MiddlewareKind::from_path(&unknown).unwrap_err().to_string();
        assert!(err.contains("Unknown middleware: 'logging'"), "{err}");
    }
}
