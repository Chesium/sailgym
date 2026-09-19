//! Physics invariant tests (brief section 35).
//!
//! Section 01 lands the harness only; the invariants themselves arrive with
//! the physics they guard, from section 02 onward. Gate step 4 runs this
//! target, so it must exist and pass from M0.

#[test]
fn harness_is_wired() {
    // Proves the integration-test target builds against the crate.
    assert_eq!(sailgym_physics::vec::Vec2::new(1.0, 0.0).length(), 1.0);
}
