#![allow(unused)]
include!("../support/app.rs");

#[uad_macros::controller("/users")]
mod users {
    use uad_macros::*;

    #[get]
    async fn list() {}

    #[get("/")]
    async fn slash() {}

    #[get("")]
    async fn empty() {}
}

fn main() {}
