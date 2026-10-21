// Compile-fail tests: every file in src/tests/ui must fail to compile with the
// exact error in its .stderr snapshot. After an intentional message change,
// regenerate snapshots with `TRYBUILD=overwrite cargo test --test ui`.
#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("src/tests/ui/*.rs");
}
