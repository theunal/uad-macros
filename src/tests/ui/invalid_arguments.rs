#![allow(unused)]
include!("../support/app.rs");

#[uad_macros::get(42)]
async fn numeric_path() {}

#[uad_macros::get("/a")]
#[uad_macros::middleware(logging)]
async fn unknown_middleware() {}

#[uad_macros::get("/b")]
#[uad_macros::middleware(auth)]
#[uad_macros::authorize(UserRead)]
async fn authorize_missing_enum() {}

#[uad_macros::get("/c")]
#[uad_macros::middleware(auth)]
#[uad_macros::authorize()]
async fn authorize_empty() {}

#[uad_macros::get("/d")]
#[uad_macros::public(yes)]
async fn public_with_args() {}

#[uad_macros::controller(users)]
mod controller_ident_prefix {}

#[uad_macros::validator(SomeValidator)]
fn validator_missing_param(payload: u8) {}

#[uad_macros::validator(SomeValidator, payload, extra)]
fn validator_extra_tokens(payload: u8) {}

fn main() {}
