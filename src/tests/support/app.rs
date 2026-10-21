// Minimal stand-in for the API server crate. The macros emit paths like
// `crate::routing::RouteRegistrar`, so these items must live at the crate
// root of every test binary: pull this file in with `include!`, not `mod`.

pub mod state {
    pub struct AppState;
}

pub mod domain {
    pub mod permission {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Permission {
            UserRead,
            UserWrite,
            AdminAccess,
        }
    }
}

pub mod routing {
    use std::sync::Arc;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MiddlewareKind {
        Auth,
        Authorize,
    }

    pub struct RouteRegistrar {
        pub path: &'static str,
        pub method_router_fn: fn() -> axum::routing::MethodRouter<Arc<crate::state::AppState>>,
        pub middlewares: &'static [MiddlewareKind],
        pub required_permissions: &'static [crate::domain::permission::Permission],
        pub public: bool,
    }

    inventory::collect!(RouteRegistrar);
}

pub mod validation_rules {
    #[derive(Debug, PartialEq)]
    pub struct ValidationError(pub String);

    pub trait Validator<T> {
        fn check(&self, value: &T) -> Result<(), String>;
    }

    pub fn validate_entity<V: Validator<T>, T>(validator: &V, value: &T) -> Result<(), ValidationError> {
        validator.check(value).map_err(ValidationError)
    }
}

#[derive(Clone, Debug)]
pub struct CurrentUser;
