# uad-macros

Procedural macros for the `uad-server` crate. They replace hand-written route
registration: a handler annotated with a route macro emits an `inventory`
registration that `server/src/routing.rs` turns into the Axum router at startup.
The same crate also provides the `#[validator]` attribute used by request
handlers.

The crate contains **no runtime code** of its own — it only expands to code that
the consuming crate compiles. It depends on `syn`, `quote` and `proc-macro2`.

```toml
[lib]
proc-macro = true

[dependencies]
syn = { version = "3.0.5", features = ["full"] }
quote = "1.0.47"
proc-macro2 = "1.0.107"
```

## Why it exists

- **No manual route list.** Each `#[get]`/`#[post]`/… submits a
  `crate::routing::RouteRegistrar` through `inventory`; routing merges the
  entries by full path.
- **Permissions and middleware travel with the route.** The macro records the
  middleware kinds and `Permission` variants in the registration, and the router
  applies them, so a handler cannot forget its `#[authorize]`.
- **Compile-time guards.** Duplicate method + path pairs, `#[authorize]` without
  auth, extracting `CurrentUser` without auth, and `#[public]` combined with
  auth are rejected while compiling, not at runtime.
- **Controllers without a boilerplate prefix.** `#[controller("/api/users")]`
  prefixes every route in the module and maps bare `#[get]` to the prefix
  itself.

## Expansion flow

Attribute macros expand outside-in, so `#[controller]` runs before the route
macros inside it:

```text
handlers/users.rs
        │
        │  #[controller("/api/users")]          router/controller.rs
        ├─ prefix route paths, detect duplicate method+path pairs
        ├─ copy controller-level #[middleware]/#[authorize] onto every
        │  non-#[public] route
        ▼
        │  #[get] / #[post] ...                    router/route.rs
        ├─ consume #[middleware], #[authorize], #[public]
        ├─ validate auth/authorize/CurrentUser combinations
        ├─ emit the original fn
        ├─ emit __route_method_router_<fn>() -> MethodRouter<Arc<AppState>>
        └─ inventory::submit! { RouteRegistrar { .. } }
        ▼
server/src/routing.rs (startup)
        ├─ inventory::iter::<RouteRegistrar>()
        ├─ merge MethodRouters that share a path
        └─ apply #[middleware(auth)] then #[authorize] unless public
        ▼
Axum Router<Arc<AppState>>
```

`#[middleware]`, `#[authorize]` and `#[public]` are inert markers: they are read
by the route macro (or by `#[controller]`). When written **above** their owner,
they expand first, so `router/common.rs::relocate_below_owner` re-emits them
directly below the owner instead of using a thread-local side channel.

## Crate layout

| Path | Responsibility |
|---|---|
| `src/lib.rs` | Proc-macro entry points (`get`, `post`, …, `controller`, `middleware`, `authorize`, `public`, `validator`). |
| `src/router/route.rs` | Route macro expansion: parse path, consume markers, validate, emit method router + `inventory` entry. |
| `src/router/controller.rs` | Module-level prefixing, bare-route mapping, duplicate detection, controller middleware/authorize propagation. |
| `src/router/common.rs` | Attribute helpers, `MiddlewareKind`, path normalization/joining/shape, marker relocation. |
| `src/router/middleware.rs` | Validates `#[middleware(auth)]` and relocates it below its owner. |
| `src/router/authorize.rs` | Parses `Permission::Variant` lists and relocates `#[authorize]` below its owner. |
| `src/validator/mod.rs` | `#[validator(Validator, param)]` expansion and parameter lookup. |
| `src/tests/` | Route, validator and trybuild UI test sources (see [Tests](#tests)). |

## Macros

### Route macros

`get`, `post`, `put`, `delete`, `patch`, `head`, `options`.

```rust
// Outside a controller the path is absolute; bare means "/".
#[get("/health")]
pub async fn health_check(State(_state): State<Arc<AppState>>) -> &'static str {
    "ok"
}
```

```rust
#[controller("/api/users")]
#[middleware(auth)]
pub mod users_controller {
    use super::*;

    // Bare #[get] is the controller prefix: GET /api/users
    #[get]
    #[authorize(Permission::UserRead)]
    pub async fn search_users(
        State(state): State<Arc<AppState>>,
        Query(q): Query<AdQuery>,
    ) -> AppResult<Json<Vec<AdUser>>> {
        users::search_users(&state, q).await
    }

    // GET /api/users/page
    #[get("page")]
    #[authorize(Permission::UserRead)]
    pub async fn search_users_page(/* … */) -> /* … */ {
        // …
    }

    // POST /api/users
    #[post]
    #[authorize(Permission::UserCreate)]
    #[validator(CreateUserBodyValidator, body)]
    pub async fn create_user(
        State(state): State<Arc<AppState>>,
        Extension(current_user): Extension<CurrentUser>,
        Json(body): Json<CreateUserBody>,
    ) -> AppResult<Json<String>> {
        users::create_user(&state, &current_user, body).await
    }
}
```

Auth is declared once at controller level and inherited by every route. A
controller that mixes public and session-only routes keeps the controller-level
declaration and marks the exception `#[public]` (see the `oidc_identity_controller`
example below); a route-level `#[middleware(auth)]` is only for a single
exception, not a second copy of the controller's rule.

Rules:

- The attribute takes a string literal path or nothing (bare). Bare inside a
  controller means the prefix; bare outside means `/`.
- A handler may have **exactly one** route macro.
- A route macro may coexist with `#[middleware]`, `#[authorize]`, `#[public]`
  and `#[validator]`; those are consumed or kept as appropriate.
- `#[authorize]` requires auth on the route or its controller, and a handler
  that extracts `CurrentUser` must also have auth.
- `#[public]` and explicit `#[middleware(auth)]` on the same route are a
  compile error.

What the macro emits (simplified):

```rust
#[doc(hidden)]
fn __route_method_router_search_users() -> axum::routing::MethodRouter<Arc<AppState>> {
    axum::routing::get(search_users)
}

inventory::submit! {
    crate::routing::RouteRegistrar {
        path: "/api/users",
        method_router_fn: __route_method_router_search_users,
        middlewares: &[
            crate::routing::MiddlewareKind::Auth,
            crate::routing::MiddlewareKind::Authorize,
        ],
        required_permissions: &[crate::domain::permission::Permission::UserRead],
        public: false,
    }
}
```

When permissions are present, a hidden `const _: () = { use Permission; … };`
is also emitted so IDE "go to definition" on `Permission::UserRead` inside the
attribute still lands on the enum.

### `#[controller("/prefix")]`

A module-level macro for an **inline** module (`mod name { … }`).

```rust
#[controller("/api/auth/oidc/identity")]
#[middleware(auth)]
pub mod oidc_identity_controller {
    use super::*;

    #[get("link/start")]
    pub async fn link_start(/* … */) { /* … */ }

    // Entra reaches this one with a browser redirect, before any session:
    // #[public] opts out of the controller-level middleware.
    #[get("link/callback")]
    #[public]
    pub async fn link_callback(/* … */) { /* … */ }
}
```

What it does:

1. **Prefixes paths.** `#[get("/{id}")]` inside `#[controller("/users")]` becomes
   `GET /users/{id}`. A bare route, `#[get("")]` and `#[get("/")]` all become
   `GET /users`. Controller prefixes are normalized and lose a trailing slash
   (`#[controller("/posts/")]` → `/posts`).
2. **Detects duplicate method + path pairs** among the module's functions. Path
   parameters are erased for comparison, so `/{id}` and `/{user_id}` conflict
   (Axum would panic on them anyway). `#[cfg]`-gated definitions are compared by
   their cfg sets: mutually exclusive variants such as `#[cfg(test)]` /
   `#[cfg(not(test))]` coexist; an unconditional duplicate or two identical
   cfgs fail.
3. **Propagates controller-level `#[middleware]` and `#[authorize]`** onto every
   route macro that is not `#[public]`. Explicit route middleware is
   de-duplicated; route permissions are appended after controller permissions
   and de-duplicated by variant.
4. **Requires auth for controller-level `#[authorize]`** — rejected once at the
   controller attribute even if the module has many routes.

Functions without a route macro are left untouched, so helper functions can
live next to the handlers.

### `#[middleware(auth)]`

Declares the auth layer. Accepted names are `auth` and `auth_middleware`; any
other name is a compile error listing the supported value.

- On a route: applied to that route.
- On a `#[controller]` module: applies to every route in the module, except
  routes marked `#[public]`.
- It can be written above or below the route/controller macro; above, it
  relocates itself below the owner.
- Declaring it twice (controller + route) is harmless; middleware kinds are
  de-duplicated.

`auth` is currently the only supported middleware kind. `#[authorize]` is not
written as a middleware by hand — the macro adds `MiddlewareKind::Authorize`
when permissions are present.

### `#[authorize(Permission::UserRead, Permission::UserWrite)]`

One or more `Permission::Variant` paths, comma-separated. Invalid paths (not
exactly two segments, first segment not `Permission`) are compile errors.

- Route-level permissions are added after controller-level ones.
- Duplicate variants are removed; order of first appearance is kept.
- The macro adds `MiddlewareKind::Authorize` automatically; the router runs auth
  first (it inserts `CurrentUser`) and authorize second.
- `#[authorize]` with no per-route auth is only valid if the controller supplies
  `#[middleware(auth)]`. This is checked for the route and for the controller.
- `#[public]` routes opt out of controller-level permissions as well.

### `#[public]`

A bare marker (no arguments) for a route that must be reachable without a
session: login, refresh, health checks, identity-provider callbacks.

- Sets `RouteRegistrar.public = true`; `routing.rs` then skips every middleware
  layer for that route.
- On a controller it opts the route out of controller-level middleware **and**
  controller-level permissions.
- It cannot be combined with explicit `#[middleware(auth)]` on the same route.
- It is not valid on a module; only on a route handler.

### `#[validator(ValidatorPath, param_name)]`

Injects a validation call at the top of the function body:

```rust
#[post]
#[middleware(auth)]
#[authorize(Permission::UserCreate)]
#[validator(CreateUserBodyValidator, body)]
pub async fn create_user(
    State(state): State<Arc<AppState>>,
    Extension(current_user): Extension<CurrentUser>,
    Json(body): Json<CreateUserBody>,
) -> AppResult<Json<String>> {
    users::create_user(&state, &current_user, body).await
}
```

expands to the equivalent of:

```rust
pub async fn create_user(/* … */) -> AppResult<Json<String>> {
    if let Err(e) = crate::validation_rules::validate_entity(
        &CreateUserBodyValidator,
        &body,
    ) {
        return ::core::convert::Into::into(e);
    }
    users::create_user(&state, &current_user, body).await
}
```

- The parameter must be a named binding. Destructured extractors such as
  `Json(body): Json<CreateUserBody>` are supported (the pattern is walked
  recursively); a typo is reported with the function's available parameter
  names.
- A trailing comma after the parameter name is accepted.
- Works on `async` and `sync` functions.
- The handler's return type must implement `From<ValidationError>` (in the
  server this is `AppError`).

## Path normalization and conflict rules

`router/common.rs`:

| Input | Result |
|---|---|
| `""` | `/` |
| `"users"` | `/users` |
| `"//users///{id}"` | `/users/{id}` |
| `"/users/"` | stays `/users/` (a trailing slash is a distinct Axum route) |
| controller prefix `"/posts/"` | `/posts` |
| `#[controller("/posts")]` + `#[get("//{id}//edit")]` | `/posts/{id}/edit` |
| `#[controller("/users")]` + `#[get]` / `#[get("")]` / `#[get("/")]` | `/users` |

Conflict detection uses `route_shape`, which erases parameter names and syntax:
`/{id}` ≡ `/{user_id}` ≡ `/:id`; `/{*rest}` ≡ `/*path`; `{{literal}}` is a
literal brace segment, not a parameter. `/users` and `/users/` are different
routes.

## Contract with the server crate

Because the macros emit paths rooted at `crate`, the consuming crate must
provide:

| Item | Used for |
|---|---|
| `crate::routing::RouteRegistrar` | The `inventory` entry (`path`, `method_router_fn`, `middlewares`, `required_permissions`, `public`). |
| `inventory::collect!(RouteRegistrar)` | Collection point in `server/src/routing.rs`. |
| `crate::routing::MiddlewareKind::{Auth, Authorize}` | Middleware list in the registration. |
| `crate::state::AppState` | `MethodRouter<Arc<AppState>>` return type. |
| `crate::domain::permission::Permission` | `#[authorize(Permission::…)]` variants. |
| `crate::validation_rules::{validate_entity, Validator}` and `ValidationError` | `#[validator]` injection. |

`server/src/routing.rs` iterates the registrations, merges entries that share a
path, and applies layers in reverse declaration order so `Auth` runs before
`Authorize` (auth inserts the `CurrentUser` extension the permission check
reads). Public registrations bypass that block entirely.

### Compile-time diagnostics

The macros reject, among others:

- two route macros on one handler;
- duplicate method + path pairs in a controller (including two bare routes);
- `#[controller]` on a non-inline module;
- `#[middleware]` with an unknown name, or on something that is not a route or
  controller;
- `#[authorize]` with no permissions, or with paths that are not
  `Permission::Variant`;
- `#[authorize]` without auth (on the route or its controller);
- handlers that extract `CurrentUser` without auth;
- `#[public]` with arguments, on a module, or together with
  `#[middleware(auth)]`;
- `#[validator]` naming a parameter that does not exist.

## Tests

`cargo test -p uad-macros` runs the 8 unit tests in
`src/router/common.rs`. They pin path normalization/joining, route-shape
conflict rules, attribute detection and middleware-kind parsing.

`src/tests/` holds three larger suites:

| File | What it covers |
|---|---|
| `src/tests/routes.rs` | Full expansion against a stand-in `AppState`/`RouteRegistrar`: controller prefixes, middleware/authorize propagation, public opt-out, deduplication, cfg-gated duplicates. |
| `src/tests/validator.rs` | `#[validator]` injection on sync/async functions and destructured parameters. |
| `src/tests/ui.rs` + `src/tests/ui/*.rs` / `*.stderr` | Trybuild compile-fail snapshots for the diagnostics above. |

These suites are **not currently Cargo test targets**: `Cargo.toml` declares no
`[[test]]` entries and the crate has no `[dev-dependencies]` (the suites use
`axum`, `tokio`, `tower`, `inventory` and `trybuild`). To run them, add the
dev-dependencies and explicit targets, for example:

```toml
[dev-dependencies]
axum = "0.8.9"
tokio = { version = "1.53.1", features = ["full"] }
tower = "0.5.3"
inventory = "0.3.24"
trybuild = "1"

[[test]]
name = "routes"
path = "src/tests/routes.rs"

[[test]]
name = "validator"
path = "src/tests/validator.rs"

[[test]]
name = "ui"
path = "src/tests/ui.rs"
```

The `include!("support/app.rs")` resolves relative to the test source file, so
keep `src/tests/support/app.rs` next to `routes.rs`/`validator.rs`; the included
items become crate-root items in the test binary, which is what the generated
`crate::routing::…` paths expect.

One further caveat for the UI suite: `src/tests/ui.rs` calls
`compile_fail("tests/ui/*.rs")`, a glob resolved from the crate root, while the
fixtures and `.stderr` snapshots currently sit in `src/tests/ui/`. Before wiring
that target, move the directory to `tests/ui/` (or change the glob) and
regenerate snapshots with `TRYBUILD=overwrite cargo test --test ui`.

## Releasing

Publishing runs in `.github/workflows/release.yml`. Pushing a `v<version>` tag
checks formatting, clippy and the unit tests, verifies that the tag matches the
version in `Cargo.toml`, builds the package, and then publishes it to crates.io.

1. Bump `version` in `Cargo.toml` and commit the change.
2. Tag the commit and push the tag:

   ```sh
   git tag v0.2.0
   git push origin v0.2.0
   ```

One-time setup: create a token at <https://crates.io/settings/tokens> and store
it as the `CARGO_REGISTRY_TOKEN` repository secret (Settings → Secrets and
variables → Actions). The publish job runs in the `crates-io` environment, so
required reviewers or a deployment branch rule can be added there.

If the tag and the `Cargo.toml` version differ, or any check fails, the crate is
not published. `ci.yml` runs the same checks on pushes to `main` and on pull
requests, plus an MSRV check against Rust 1.99.1.

## Extending the crate

**A new HTTP method** (e.g. `trace`):

1. Add `#[proc_macro_attribute] pub fn trace(...)` in `src/lib.rs`, calling
   `router::route::generate_route_impl(attr, item, "TRACE")`.
2. Add `"trace"` to `ROUTE_MACROS` in `src/router/common.rs` so it is detected,
   consumed and included in duplicate checks.
3. Nothing else: the route macro emits `axum::routing::trace(...)`, and the
   server's `routing.rs` already consumes the registration.

**A new middleware kind** (e.g. rate limiting):

1. Add a variant to `MiddlewareKind` in `src/router/common.rs`, plus its
   `from_path` names and `to_tokens` arm.
2. Add the same variant to `crate::routing::MiddlewareKind` in the server and a
   layer arm in `server/src/routing.rs`.
3. Existing `#[middleware(rate_limit)]` uses will then expand without further
   macro changes.

## Limitations

- `#[controller]` only accepts an inline module, not `mod name;`.
- Only the `auth` middleware is supported; everything else is a compile error.
- Route paths must be string literals or absent; a non-literal is left for the
  route macro to report.
- Controller-level `#[public]` is not a concept; public is a per-route marker.
- Controller-level `#[authorize]` is applied to every non-public route, so a
  controller whose routes need different permissions should keep `#[authorize]`
  at route level (or split the controller).
- The `RouteRegistrar` stores no HTTP method; the method lives in the generated
  `MethodRouter`. Tests that need the method probe the router with real requests
  (see `src/tests/routes.rs`).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
