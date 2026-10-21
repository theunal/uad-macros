#![allow(dead_code)]

include!("support/app.rs");

use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::{Extension, Router};
use domain::permission::Permission;
use routing::{MiddlewareKind, RouteRegistrar};
use tower::ServiceExt;

#[uad_macros::controller("/users")]
#[uad_macros::middleware(auth)]
mod users {
    use crate::CurrentUser;
    use axum::Extension;
    use uad_macros::*;

    // `list` / `list_disabled` share method + path but are gated by mutually
    // exclusive cfgs (`test` is set for integration tests): not a conflict.
    #[cfg(test)]
    #[get]
    async fn list(Extension(_user): Extension<CurrentUser>) -> &'static str {
        "users:list"
    }

    #[cfg(not(test))]
    #[get]
    async fn list_disabled() -> &'static str {
        "users:list_disabled"
    }

    #[post]
    #[authorize(Permission::UserWrite)]
    async fn create() -> &'static str {
        "users:create"
    }

    #[get("/{id}")]
    async fn show() -> &'static str {
        "users:show"
    }

    // Written above the route macro and with a duplicate permission.
    #[authorize(Permission::UserRead, Permission::UserRead)]
    #[delete("{id}")]
    async fn remove() -> &'static str {
        "users:remove"
    }

    #[public]
    #[get("/health")]
    async fn health() -> &'static str {
        "users:health"
    }

    // Explicit auth on top of the controller-level one must not be duplicated.
    #[get("/me")]
    #[middleware(auth)]
    async fn me() -> &'static str {
        "users:me"
    }
}

#[uad_macros::controller("/admin")]
#[uad_macros::middleware(auth)]
#[uad_macros::authorize(Permission::AdminAccess)]
mod admin {
    use uad_macros::*;

    #[get]
    async fn dashboard() -> &'static str {
        "admin:dashboard"
    }

    // Route-level permissions are added after the controller-level ones.
    #[post("/users")]
    #[authorize(Permission::UserWrite)]
    async fn create_user() -> &'static str {
        "admin:create_user"
    }

    // Repeating the controller permission must not duplicate it.
    #[authorize(Permission::AdminAccess, Permission::UserRead)]
    #[get("/users")]
    async fn list_users() -> &'static str {
        "admin:list_users"
    }

    #[public]
    #[get("/status")]
    async fn status() -> &'static str {
        "admin:status"
    }
}

// Controller-level attributes written ABOVE #[controller] relocate below it.
#[uad_macros::authorize(Permission::UserRead)]
#[uad_macros::middleware(auth)]
#[uad_macros::controller("/reports")]
mod reports {
    use uad_macros::*;

    #[get]
    async fn index() -> &'static str {
        "reports:index"
    }
}

#[uad_macros::controller("/posts/")]
mod posts {
    use uad_macros::*;

    #[get]
    async fn list() -> &'static str {
        "posts:list"
    }

    #[post("")]
    async fn create() -> &'static str {
        "posts:create"
    }

    #[middleware(auth)]
    #[get("/{id}")]
    async fn show() -> &'static str {
        "posts:show"
    }

    #[put("//{id}//edit")]
    async fn edit() -> &'static str {
        "posts:edit"
    }

    #[patch("/{id}")]
    #[public]
    async fn touch() -> &'static str {
        "posts:touch"
    }

    // Plain helper without a route macro is left alone.
    pub fn helper() -> u8 {
        1
    }
}

#[uad_macros::controller]
mod root {
    use uad_macros::*;

    #[get]
    async fn index() -> &'static str {
        "root:index"
    }
}

#[uad_macros::get("/standalone")]
async fn standalone() -> &'static str {
    "standalone"
}

#[uad_macros::options]
async fn bare_outside_controller() -> &'static str {
    "bare_outside"
}

#[derive(Debug)]
struct Resolved {
    method: Method,
    path: &'static str,
    middlewares: &'static [MiddlewareKind],
    permissions: &'static [Permission],
    public: bool,
}

const PROBE_METHODS: [Method; 6] = [
    Method::GET,
    Method::POST,
    Method::PUT,
    Method::DELETE,
    Method::PATCH,
    Method::OPTIONS,
];

fn single_route_app(reg: &RouteRegistrar) -> Router {
    Router::new()
        .route(reg.path, (reg.method_router_fn)())
        .layer(Extension(CurrentUser))
        .with_state(Arc::new(state::AppState))
}

/// RouteRegistrar does not store the HTTP method, so each route is probed
/// with real requests; the response body identifies the handler.
async fn resolve(reg: &RouteRegistrar) -> (String, Method) {
    let uri = reg.path.replace("{id}", "1");
    for method in PROBE_METHODS {
        let req = Request::builder()
            .method(method.clone())
            .uri(&uri)
            .body(Body::empty())
            .unwrap();
        let res = single_route_app(reg).oneshot(req).await.unwrap();
        if res.status() == StatusCode::OK {
            let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
            return (String::from_utf8(bytes.to_vec()).unwrap(), method);
        }
    }
    panic!("route {} did not answer any probed method", reg.path);
}

async fn routes() -> HashMap<String, Resolved> {
    let mut out = HashMap::new();
    for reg in inventory::iter::<RouteRegistrar> {
        let (handler, method) = resolve(reg).await;
        let previous = out.insert(
            handler.clone(),
            Resolved {
                method,
                path: reg.path,
                middlewares: reg.middlewares,
                permissions: reg.required_permissions,
                public: reg.public,
            },
        );
        assert!(previous.is_none(), "handler {handler} registered twice");
    }
    out
}

async fn route(handler: &str) -> Resolved {
    routes()
        .await
        .remove(handler)
        .unwrap_or_else(|| panic!("handler {handler} is not registered"))
}

#[tokio::test]
async fn registers_every_route_exactly_once() {
    let mut handlers: Vec<_> = routes().await.into_keys().collect();
    handlers.sort();
    assert_eq!(
        handlers,
        [
            "admin:create_user",
            "admin:dashboard",
            "admin:list_users",
            "admin:status",
            "bare_outside",
            "posts:create",
            "posts:edit",
            "posts:list",
            "posts:show",
            "posts:touch",
            "reports:index",
            "root:index",
            "standalone",
            "users:create",
            "users:health",
            "users:list",
            "users:me",
            "users:remove",
            "users:show",
        ]
    );
}

#[tokio::test]
async fn bare_route_maps_to_controller_prefix() {
    let r = route("users:list").await;
    assert_eq!((r.method, r.path), (Method::GET, "/users"));

    let r = route("users:create").await;
    assert_eq!((r.method, r.path), (Method::POST, "/users"));
}

#[tokio::test]
async fn empty_string_path_maps_to_controller_prefix() {
    let r = route("posts:create").await;
    assert_eq!((r.method, r.path), (Method::POST, "/posts"));
}

#[tokio::test]
async fn controller_prefix_and_paths_are_normalized() {
    assert_eq!(route("users:show").await.path, "/users/{id}");
    assert_eq!(route("users:remove").await.path, "/users/{id}");
    assert_eq!(route("posts:list").await.path, "/posts");

    let r = route("posts:edit").await;
    assert_eq!((r.method, r.path), (Method::PUT, "/posts/{id}/edit"));

    let r = route("posts:touch").await;
    assert_eq!((r.method, r.path), (Method::PATCH, "/posts/{id}"));
}

#[tokio::test]
async fn controller_without_prefix_and_bare_route_outside_controller_map_to_root() {
    let r = route("root:index").await;
    assert_eq!((r.method, r.path), (Method::GET, "/"));

    let r = route("bare_outside").await;
    assert_eq!((r.method, r.path), (Method::OPTIONS, "/"));

    let r = route("standalone").await;
    assert_eq!((r.method, r.path), (Method::GET, "/standalone"));
}

#[tokio::test]
async fn controller_middleware_is_applied_to_every_route() {
    for handler in ["users:list", "users:show", "users:me"] {
        assert_eq!(route(handler).await.middlewares, [MiddlewareKind::Auth], "{handler}");
    }
}

#[tokio::test]
async fn explicit_and_controller_middleware_are_deduplicated() {
    assert_eq!(route("users:me").await.middlewares, [MiddlewareKind::Auth]);
}

#[tokio::test]
async fn authorize_adds_permissions_and_authorize_middleware() {
    let r = route("users:create").await;
    assert_eq!(r.middlewares, [MiddlewareKind::Auth, MiddlewareKind::Authorize]);
    assert_eq!(r.permissions, [Permission::UserWrite]);
}

#[tokio::test]
async fn authorize_above_route_macro_works_and_dedupes_permissions() {
    let r = route("users:remove").await;
    assert_eq!(r.method, Method::DELETE);
    assert_eq!(r.middlewares, [MiddlewareKind::Auth, MiddlewareKind::Authorize]);
    assert_eq!(r.permissions, [Permission::UserRead]);
}

#[tokio::test]
async fn public_routes_opt_out_of_controller_middleware() {
    let r = route("users:health").await;
    assert!(r.public);
    assert!(r.middlewares.is_empty());
    assert!(r.permissions.is_empty());
}

#[tokio::test]
async fn public_below_route_macro_is_recognized() {
    assert!(route("posts:touch").await.public);
}

#[tokio::test]
async fn route_level_middleware_without_controller_middleware() {
    assert_eq!(route("posts:show").await.middlewares, [MiddlewareKind::Auth]);
    assert!(route("posts:list").await.middlewares.is_empty());
    assert!(route("standalone").await.middlewares.is_empty());
}

#[tokio::test]
async fn non_public_routes_default_to_not_public() {
    for handler in ["users:list", "posts:list", "standalone"] {
        assert!(!route(handler).await.public, "{handler}");
    }
}

#[tokio::test]
async fn controller_authorize_is_applied_to_every_route() {
    let r = route("admin:dashboard").await;
    assert_eq!((r.method, r.path), (Method::GET, "/admin"));
    assert_eq!(r.middlewares, [MiddlewareKind::Auth, MiddlewareKind::Authorize]);
    assert_eq!(r.permissions, [Permission::AdminAccess]);
}

#[tokio::test]
async fn controller_and_route_permissions_are_merged_in_order() {
    let r = route("admin:create_user").await;
    assert_eq!((r.method, r.path), (Method::POST, "/admin/users"));
    assert_eq!(r.permissions, [Permission::AdminAccess, Permission::UserWrite]);
}

#[tokio::test]
async fn repeated_controller_permission_is_deduplicated() {
    let r = route("admin:list_users").await;
    assert_eq!(r.permissions, [Permission::AdminAccess, Permission::UserRead]);
    assert_eq!(r.middlewares, [MiddlewareKind::Auth, MiddlewareKind::Authorize]);
}

#[tokio::test]
async fn public_routes_opt_out_of_controller_authorize() {
    let r = route("admin:status").await;
    assert!(r.public);
    assert!(r.middlewares.is_empty());
    assert!(r.permissions.is_empty());
}

#[tokio::test]
async fn controller_attributes_above_controller_macro_are_applied() {
    let r = route("reports:index").await;
    assert_eq!((r.method, r.path), (Method::GET, "/reports"));
    assert_eq!(r.middlewares, [MiddlewareKind::Auth, MiddlewareKind::Authorize]);
    assert_eq!(r.permissions, [Permission::UserRead]);
}

#[test]
fn all_routes_merge_into_one_router_without_conflicts() {
    let mut app: Router<Arc<state::AppState>> = Router::new();
    for reg in inventory::iter::<RouteRegistrar> {
        app = app.route(reg.path, (reg.method_router_fn)());
    }
    let _: Router = app.with_state(Arc::new(state::AppState));
}

#[test]
fn handlers_stay_callable_and_helpers_are_untouched() {
    assert_eq!(posts::helper(), 1);
}
