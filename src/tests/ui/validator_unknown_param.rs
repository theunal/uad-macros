#![allow(unused)]

struct NewUser;
struct NewUserValidator;
struct Json<T>(T);

#[uad_macros::validator(NewUserValidator, bdy)]
fn typo(Json(body): Json<NewUser>, tag: u8) {}

#[uad_macros::validator(NewUserValidator, payload)]
fn no_params() {}

fn main() {}
