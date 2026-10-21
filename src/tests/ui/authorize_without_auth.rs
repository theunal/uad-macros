#![allow(unused)]
include!("../support/app.rs");

#[uad_macros::controller("/users")]
mod users {
    use uad_macros::*;

    #[post]
    #[authorize(Permission::UserWrite)]
    async fn create() {}
}

fn main() {}
