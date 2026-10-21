#![allow(dead_code)]

include!("support/app.rs");

use validation_rules::{ValidationError, Validator};

struct NewUser {
    name: String,
}

struct NewUserValidator;

impl Validator<NewUser> for NewUserValidator {
    fn check(&self, value: &NewUser) -> Result<(), String> {
        if value.name.trim().is_empty() {
            Err("name is required".into())
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, PartialEq)]
enum Outcome {
    Created(String),
    Invalid(String),
}

impl From<ValidationError> for Outcome {
    fn from(e: ValidationError) -> Self {
        Outcome::Invalid(e.0)
    }
}

#[uad_macros::validator(NewUserValidator, payload)]
fn create_user(payload: NewUser) -> Outcome {
    Outcome::Created(payload.name)
}

#[rustfmt::skip]
#[uad_macros::validator(NewUserValidator, payload,)]
async fn create_user_async(payload: NewUser) -> Outcome {
    Outcome::Created(payload.name)
}

struct Json<T>(T);

// The parameter name is bound inside a destructuring pattern, like axum's
// `Json(body)` extractor.
#[uad_macros::validator(NewUserValidator, body)]
fn create_user_destructured(Json(body): Json<NewUser>, _tag: u8) -> Outcome {
    Outcome::Created(body.name)
}

#[test]
fn param_bound_by_destructuring_pattern_is_found() {
    assert_eq!(
        create_user_destructured(Json(NewUser { name: "".into() }), 0),
        Outcome::Invalid("name is required".into())
    );
    assert_eq!(
        create_user_destructured(Json(NewUser { name: "eve".into() }), 0),
        Outcome::Created("eve".into())
    );
}

#[test]
fn valid_input_runs_the_original_body() {
    let out = create_user(NewUser { name: "ada".into() });
    assert_eq!(out, Outcome::Created("ada".into()));
}

#[test]
fn invalid_input_returns_early_with_converted_error() {
    let out = create_user(NewUser { name: "  ".into() });
    assert_eq!(out, Outcome::Invalid("name is required".into()));
}

#[tokio::test]
async fn works_on_async_functions_and_accepts_trailing_comma() {
    assert_eq!(
        create_user_async(NewUser { name: "".into() }).await,
        Outcome::Invalid("name is required".into())
    );
    assert_eq!(
        create_user_async(NewUser { name: "bob".into() }).await,
        Outcome::Created("bob".into())
    );
}
