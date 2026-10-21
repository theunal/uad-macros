#![allow(unused)]
include!("../support/app.rs");

#[uad_macros::controller("/users")]
mod users {
    use uad_macros::*;

    #[get("/{id}")]
    async fn show() {}

    #[get("{id}")]
    async fn show_again() {}

    // Parameter names are ignored when comparing routes.
    #[get("/:user_id")]
    async fn show_axum07() {}

    #[get("/{user_id}")]
    async fn show_renamed() {}

    // Same path, different method: allowed.
    #[delete("/{id}")]
    async fn remove() {}
}

fn main() {}
