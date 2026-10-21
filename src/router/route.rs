use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, LitStr};

use super::authorize::parse_permission_paths;
use super::common::{MiddlewareKind, attr_name, is_route_attr, normalize_route_path};

/// `#[get("/path")]` or bare `#[get]`. Inside a `#[controller]` the bare form
/// is rewritten to the controller prefix before this runs; outside a
/// controller it maps to `/`.
fn parse_route_path(attr: TokenStream) -> syn::Result<String> {
    if attr.is_empty() {
        return Ok("/".to_string());
    }
    let lit: LitStr = syn::parse(attr).map_err(|e| {
        syn::Error::new(
            e.span(),
            "route macro expects a string literal path, e.g. #[get(\"/users\")], or no argument: #[get]",
        )
    })?;
    Ok(normalize_route_path(&lit.value()))
}

pub fn generate_route_impl(attr: TokenStream, item: TokenStream, http_method: &str) -> TokenStream {
    match expand(attr, item, http_method) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand(
    attr: TokenStream,
    item: TokenStream,
    http_method: &str,
) -> syn::Result<proc_macro2::TokenStream> {
    let path = parse_route_path(attr)?;
    let mut func: ItemFn = syn::parse(item)?;

    let fn_name = func.sig.ident.clone();
    let method_router_fn_name =
        syn::Ident::new(&format!("__route_method_router_{fn_name}"), fn_name.span());

    // Attribute macros expand top-down, so the `#[middleware]` / `#[authorize]`
    // / `#[public]` attributes are still on the item here (attributes written
    // above the route macro relocate themselves below it). Consume them.
    let mut middleware_kinds: Vec<MiddlewareKind> = Vec::new();
    let mut authorize_paths: Vec<syn::Path> = Vec::new();
    let mut kept_attrs: Vec<syn::Attribute> = Vec::new();
    let mut public_attr: Option<syn::Attribute> = None;
    let mut explicit_auth: Option<syn::Attribute> = None;

    for a in func.attrs.drain(..) {
        if is_route_attr(&a) {
            return Err(syn::Error::new_spanned(
                a.path(),
                format!(
                    "`{fn_name}` already has a route macro (#[{}]); a handler can only have one",
                    http_method.to_lowercase()
                ),
            ));
        }
        match attr_name(&a).as_deref() {
            Some("public") => {
                if !matches!(a.meta, syn::Meta::Path(_)) {
                    return Err(syn::Error::new_spanned(&a, "#[public] takes no arguments"));
                }
                public_attr = Some(a);
            }
            Some("middleware") => {
                let p: syn::Path = a.parse_args()?;
                let kind = MiddlewareKind::from_path(&p)?;
                if kind == MiddlewareKind::Auth && explicit_auth.is_none() {
                    explicit_auth = Some(a.clone());
                }
                middleware_kinds.push(kind);
            }
            Some("authorize") => {
                let tokens = match &a.meta {
                    syn::Meta::List(ml) => ml.tokens.clone(),
                    _ => proc_macro2::TokenStream::new(),
                };
                let has_tokens = !tokens.is_empty();
                // Empty input has no span of its own; point at the attribute.
                let paths = parse_permission_paths(tokens).map_err(|e| {
                    if has_tokens {
                        e
                    } else {
                        syn::Error::new_spanned(&a, e.to_string())
                    }
                })?;
                authorize_paths.extend(paths);
            }
            // e.g. `#[validator(...)]` must stay so it can still expand.
            _ => kept_attrs.push(a),
        }
    }
    func.attrs = kept_attrs;

    let mut seen = std::collections::HashSet::new();
    middleware_kinds.retain(|k| seen.insert(*k));

    let is_public = public_attr.is_some();
    if let (Some(_), Some(auth)) = (&public_attr, &explicit_auth) {
        return Err(syn::Error::new_spanned(
            auth,
            "#[public] routes cannot use #[middleware(auth)]: remove one of them",
        ));
    }

    // auth_middleware is the only component that inserts the CurrentUser
    // extension; handlers that need it without auth would fail every request.
    let has_auth = middleware_kinds.contains(&MiddlewareKind::Auth);

    if !authorize_paths.is_empty() && !has_auth {
        return Err(syn::Error::new(
            fn_name.span(),
            "#[authorize(...)] requires #[middleware(auth)] on the handler or its #[controller]: \
             the permission check reads the CurrentUser extension that only auth_middleware \
             inserts.",
        ));
    }

    let extracts_current_user = func.sig.inputs.iter().any(|arg| match arg {
        syn::FnArg::Typed(pat) => {
            let ty = &pat.ty;
            type_mentions(ty, "CurrentUser")
        }
        syn::FnArg::Receiver(_) => false,
    });
    if extracts_current_user && !has_auth {
        return Err(syn::Error::new(
            fn_name.span(),
            "handler extracts CurrentUser but has no #[middleware(auth)]: the extension \
             is only inserted by auth_middleware, so every request would fail with a \
             missing-extension error. Add #[middleware(auth)] to the handler or its #[controller].",
        ));
    }

    let mut perm_variants: Vec<syn::Ident> = Vec::new();
    for p in &authorize_paths {
        let ident = p.segments[1].ident.clone();
        if !perm_variants.contains(&ident) {
            perm_variants.push(ident);
        }
    }
    if !perm_variants.is_empty() {
        middleware_kinds.push(MiddlewareKind::Authorize);
    }

    let middleware_tokens: Vec<_> = middleware_kinds.iter().map(|k| k.to_tokens()).collect();
    let perm_literals = perm_variants
        .iter()
        .map(|ident| quote! { crate::domain::permission::Permission::#ident });

    let method_ident = syn::Ident::new(&http_method.to_lowercase(), fn_name.span());
    let state_path: syn::Path = syn::parse_quote!(std::sync::Arc<crate::state::AppState>);

    // IDE navigation anchor: re-emits the original permission paths (same
    // spans) in a compile-time-only const so Ctrl+Click reaches the enum.
    let anchor = if authorize_paths.is_empty() {
        quote! {}
    } else {
        quote! {
            #[doc(hidden)]
            #[allow(unused_imports)]
            const _: () = {
                use crate::domain::permission::Permission;
                let _ = ( #(#authorize_paths,)* );
            };
        }
    };

    Ok(quote! {
        #anchor
        #func

        #[doc(hidden)]
        fn #method_router_fn_name() -> axum::routing::MethodRouter<#state_path> {
            axum::routing::#method_ident(#fn_name)
        }

        inventory::submit! {
            crate::routing::RouteRegistrar {
                path: #path,
                method_router_fn: #method_router_fn_name,
                middlewares: &[#(#middleware_tokens),*],
                required_permissions: &[#(#perm_literals),*],
                public: #is_public,
            }
        }
    })
}

/// True if any path segment in the type is exactly `name`
/// (`Extension<CurrentUser>`, `Option<Extension<auth::CurrentUser>>`, ...),
/// without matching unrelated names like `CurrentUserDto`.
fn type_mentions(ty: &syn::Type, name: &str) -> bool {
    fn walk(tokens: proc_macro2::TokenStream, name: &str) -> bool {
        tokens.into_iter().any(|tt| match tt {
            proc_macro2::TokenTree::Ident(i) => i == name,
            proc_macro2::TokenTree::Group(g) => walk(g.stream(), name),
            _ => false,
        })
    }
    walk(quote!(#ty), name)
}
