#![allow(unused)]

// An unconditional route exists in every build, so a cfg-gated route with the
// same method + path conflicts with it whenever the cfg is enabled.
#[uad_macros::controller("/users")]
mod users {
    use uad_macros::*;

    #[get("/{id}")]
    async fn show() {}

    #[cfg(test)]
    #[get("/{user_id}")]
    async fn show_test_only() {}
}

// Identical cfg sets are also both present at the same time.
#[uad_macros::controller("/posts")]
mod posts {
    use uad_macros::*;

    #[cfg(test)]
    #[get]
    async fn list() {}

    #[cfg(test)]
    #[get]
    async fn list_again() {}
}

fn main() {}
