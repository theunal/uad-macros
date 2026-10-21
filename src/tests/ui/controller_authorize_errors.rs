#![allow(unused)]

#[uad_macros::controller("/admin")]
#[uad_macros::authorize(Permission::AdminAccess)]
mod missing_auth {
    use uad_macros::*;

    #[get]
    async fn dashboard() {}
}

#[uad_macros::controller("/empty")]
#[uad_macros::middleware(auth)]
#[uad_macros::authorize()]
mod empty_permissions {
    use uad_macros::*;

    #[get]
    async fn index() {}
}

#[uad_macros::controller("/bad")]
#[uad_macros::middleware(auth)]
#[uad_macros::authorize(Role::Admin)]
mod not_a_permission {
    use uad_macros::*;

    #[get]
    async fn index() {}
}

#[uad_macros::authorize(Permission::AdminAccess)]
mod not_a_controller {}

fn main() {}
