#![allow(unused)]
include!("../support/app.rs");

#[uad_macros::controller("/users")]
#[uad_macros::middleware(auth)]
mod users {
    use crate::CurrentUser;
    use axum::Extension;
    use uad_macros::*;

    // #[public] drops the controller-level auth, so CurrentUser is never set.
    #[public]
    #[get("/me")]
    async fn me(Extension(_user): Extension<CurrentUser>) {}
}

fn main() {}
