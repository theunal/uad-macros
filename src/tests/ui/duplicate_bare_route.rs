#![allow(unused)]
include!("../support/app.rs");

#[uad_macros::controller("/users")]
mod users {
    use uad_macros::*;

    #[get]
    async fn list() {}

    #[get]
    async fn list_again() {}

    // A different method on the same bare path is fine.
    #[post]
    async fn create() {}
}

fn main() {}
