#![allow(unused)]
include!("../support/app.rs");

#[uad_macros::middleware(auth)]
async fn no_route_middleware() {}

#[uad_macros::authorize(Permission::UserRead)]
async fn no_route_authorize() {}

#[uad_macros::public]
async fn no_route_public() {}

#[uad_macros::middleware(auth)]
mod not_a_controller {}

#[uad_macros::public]
struct NotAFunction;

fn main() {}
