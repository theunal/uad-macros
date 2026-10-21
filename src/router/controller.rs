use std::collections::HashMap;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{Attribute, ItemMod, LitStr};

use super::authorize::parse_permission_paths;
use super::common::{
    MiddlewareKind, attr_name, attr_name_is, has_attr, is_route_attr, join_paths,
    normalize_route_path, route_shape,
};

/// `#[controller("/users")]` on an inline module:
/// - prefixes every route path in the module (`#[get("/{id}")]` -> `/users/{id}`),
/// - maps bare route macros (`#[get]`) to the prefix itself,
/// - rejects duplicate method + path pairs (including two bare `#[get]`s),
/// - applies controller-level `#[middleware(...)]` and `#[authorize(...)]` to
///   every route that isn't `#[public]`.
pub fn controller_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    match expand(attr, item) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

struct SeenRoute {
    fn_name: String,
    bare: bool,
}

fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<proc_macro2::TokenStream> {
    let prefix = if attr.is_empty() {
        String::new()
    } else {
        let lit: LitStr = syn::parse(attr).map_err(|e| {
            syn::Error::new(
                e.span(),
                "controller expects a string literal prefix, e.g. #[controller(\"/users\")]",
            )
        })?;
        normalize_route_path(&lit.value())
            .trim_end_matches('/')
            .to_string()
    };

    let mut item_mod: ItemMod = syn::parse(item)?;

    let mut controller_middlewares: Vec<(MiddlewareKind, syn::Path)> = Vec::new();
    let mut controller_permissions: Vec<syn::Path> = Vec::new();
    let mut first_authorize_attr: Option<Attribute> = None;
    let mut kept_mod_attrs: Vec<Attribute> = Vec::new();
    for a in item_mod.attrs.drain(..) {
        if attr_name_is(&a, "middleware") {
            let p: syn::Path = a.parse_args()?;
            let kind = MiddlewareKind::from_path(&p)?;
            if !controller_middlewares.iter().any(|(k, _)| *k == kind) {
                controller_middlewares.push((kind, p));
            }
        } else if attr_name_is(&a, "authorize") {
            let tokens = match &a.meta {
                syn::Meta::List(ml) => ml.tokens.clone(),
                _ => proc_macro2::TokenStream::new(),
            };
            let has_tokens = !tokens.is_empty();
            let paths = parse_permission_paths(tokens).map_err(|e| {
                if has_tokens {
                    e
                } else {
                    syn::Error::new_spanned(&a, e.to_string())
                }
            })?;
            for p in paths {
                let variant = &p.segments[1].ident;
                if !controller_permissions
                    .iter()
                    .any(|q| q.segments[1].ident == *variant)
                {
                    controller_permissions.push(p);
                }
            }
            first_authorize_attr.get_or_insert(a);
        } else {
            kept_mod_attrs.push(a);
        }
    }
    item_mod.attrs = kept_mod_attrs;

    // Checked here rather than per route so the error points at the
    // controller attribute once instead of at every handler.
    if let Some(authorize_attr) = &first_authorize_attr
        && !controller_middlewares
            .iter()
            .any(|(k, _)| *k == MiddlewareKind::Auth)
    {
        return Err(syn::Error::new_spanned(
            authorize_attr,
            "controller-level #[authorize(...)] requires controller-level \
             #[middleware(auth)]: the permission check reads the CurrentUser \
             extension that only auth_middleware inserts",
        ));
    }

    let Some((_, content)) = &mut item_mod.content else {
        return Err(syn::Error::new_spanned(
            &item_mod.ident,
            "#[controller] requires an inline module: `mod name { ... }`",
        ));
    };

    // (method, route shape) -> every definition seen so far with its cfg set.
    let mut seen: HashMap<(String, String), Vec<(String, SeenRoute)>> = HashMap::new();
    let mut errors: Option<syn::Error> = None;
    let mut push_error = |e: syn::Error| match &mut errors {
        Some(existing) => existing.combine(e),
        None => errors = Some(e),
    };

    for item in content.iter_mut() {
        let syn::Item::Fn(func) = item else { continue };
        let fn_name = func.sig.ident.to_string();
        let is_public = has_attr(&func.attrs, "public");
        // `#[cfg]` is not evaluated before an outer attribute macro runs, so
        // cfg-gated handlers are visible here. Two definitions conflict when
        // either one is unconditional (it exists in every build) or both have
        // the same cfg set; differently gated variants such as
        // `#[cfg(a)]` / `#[cfg(not(a))]` are allowed.
        let cfg_key = func
            .attrs
            .iter()
            .filter(|a| a.path().is_ident("cfg"))
            .map(|a| quote!(#a).to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let mut route_idx: Option<usize> = None;

        for (idx, a) in func.attrs.iter_mut().enumerate() {
            if !is_route_attr(a) {
                continue;
            }
            route_idx.get_or_insert(idx);
            let method = attr_name(a).unwrap_or_default();

            let (raw_path, bare, lit_span) = match &a.meta {
                syn::Meta::Path(_) => (String::new(), true, Span::call_site()),
                syn::Meta::List(ml) if ml.tokens.is_empty() => {
                    (String::new(), true, Span::call_site())
                }
                syn::Meta::List(ml) => match syn::parse2::<LitStr>(ml.tokens.clone()) {
                    Ok(lit) => (lit.value(), false, lit.span()),
                    // Leave it untouched; the route macro reports the error.
                    Err(_) => continue,
                },
                syn::Meta::NameValue(_) => continue,
            };

            let full_path = join_paths(&prefix, &raw_path);
            let key = (method.clone(), route_shape(&full_path));
            let entries = seen.entry(key).or_default();
            let conflict = entries
                .iter()
                .find(|(other_cfg, _)| {
                    other_cfg.is_empty() || cfg_key.is_empty() || *other_cfg == cfg_key
                })
                .map(|(_, prev)| prev);

            if let Some(prev) = conflict {
                let msg = if bare && prev.bare {
                    format!(
                        "only one bare #[{method}] is allowed per controller: `{}` already uses it",
                        prev.fn_name
                    )
                } else {
                    format!(
                        "duplicate route {} {full_path}: already defined by `{}`",
                        method.to_uppercase(),
                        prev.fn_name
                    )
                };
                push_error(syn::Error::new_spanned(&*a, msg));
            } else {
                entries.push((
                    cfg_key.clone(),
                    SeenRoute {
                        fn_name: fn_name.clone(),
                        bare,
                    },
                ));
            }

            let new_lit = LitStr::new(&full_path, lit_span);
            let path = a.path().clone();
            *a = syn::parse_quote!(#[#path(#new_lit)]);
        }

        // Controller-level middleware goes directly below the route macro so
        // the route macro consumes it. `#[public]` routes opt out.
        let Some(route_idx) = route_idx else { continue };
        if is_public {
            continue;
        }
        let mut insert_at = route_idx + 1;
        for (kind, path) in &controller_middlewares {
            let already = func.attrs.iter().any(|a| {
                attr_name_is(a, "middleware")
                    && a.parse_args::<syn::Path>()
                        .ok()
                        .and_then(|p| MiddlewareKind::from_path(&p).ok())
                        == Some(*kind)
            });
            if already {
                continue;
            }
            let path = path.clone();
            func.attrs
                .insert(insert_at, syn::parse_quote!(#[middleware(#path)]));
            insert_at += 1;
        }
        // The route macro merges these with route-level permissions and
        // removes duplicates. Original spans are kept so an unknown variant
        // is reported on the controller attribute.
        if !controller_permissions.is_empty() {
            let perms = &controller_permissions;
            func.attrs
                .insert(insert_at, syn::parse_quote!(#[authorize(#(#perms),*)]));
        }
    }

    if let Some(e) = errors {
        return Err(e);
    }

    Ok(quote! { #item_mod })
}
