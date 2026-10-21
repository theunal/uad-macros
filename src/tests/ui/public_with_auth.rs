#![allow(unused)]
include!("../support/app.rs");

#[uad_macros::get("/health")]
#[uad_macros::public]
#[uad_macros::middleware(auth)]
async fn health() {}

fn main() {}
