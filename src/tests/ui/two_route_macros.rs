#![allow(unused)]
include!("../support/app.rs");

#[uad_macros::get("/items")]
#[uad_macros::post("/items")]
async fn items() {}

fn main() {}
