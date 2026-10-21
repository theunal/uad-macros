#![allow(dead_code)]

include!("support/app.rs");

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::{Extension, Router};
use domain::permission::Permission;
use routing::{MiddlewareKind, RouteRegistrar};
use tower::ServiceExt;

#[uad_macros::head("/head")]
async fn head_only() -> &'static str {
    "head"
}

#[uad_macros::head("/head-secured")]
#[uad_macros::middleware(auth)]
#[uad_macros::authorize(Permission::UserRead)]
async fn head_secured() -> &'static str {
    "head-secured"
}

fn app() -> Router {
    let mut app: Router<Arc<state::AppState>> = Router::new();
    for reg in inventory::iter::<RouteRegistrar> {
        app = app.route(reg.path, (reg.method_router_fn)());
    }
    app.layer(Extension(CurrentUser))
        .with_state(Arc::new(state::AppState))
}

// HEAD responses have no body, so the handler cannot be identified by probing
// the response; this suite checks the method and the registration metadata.
#[tokio::test]
async fn head_routes_answer_head_requests() {
    for path in ["/head", "/head-secured"] {
        let req = Request::builder()
            .method(Method::HEAD)
            .uri(path)
            .body(Body::empty())
            .unwrap();
        let res = app().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK, "{path}");
    }
}

#[test]
fn head_registrations_carry_middleware_and_permissions() {
    let secured = inventory::iter::<RouteRegistrar>
        .into_iter()
        .find(|reg| reg.path == "/head-secured")
        .expect("/head-secured is not registered");

    assert_eq!(
        secured.middlewares,
        [MiddlewareKind::Auth, MiddlewareKind::Authorize]
    );
    assert_eq!(secured.required_permissions, [Permission::UserRead]);
    assert!(!secured.public);
}
