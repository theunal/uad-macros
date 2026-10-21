#![allow(unused)]
include!("../support/app.rs");

#[uad_macros::controller("/users")]
#[uad_macros::middleware(rate_limit)]
mod users {
    use uad_macros::*;

    #[get]
    async fn list() {}
}

fn main() {}
