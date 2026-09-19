//! Time-step convergence tests (brief section 35, "Time-step convergence").
//!
//! Populated in section 02, once there is an integrator to converge.

#[test]
fn harness_is_wired() {
    assert_eq!(sailgym_physics::vec::Vec3::ZERO.length(), 0.0);
}
