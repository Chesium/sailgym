//! Shared lift/drag foil model (F5), used unchanged by sail, centreboard and
//! rudder. Coefficients and forces are implemented in section 04.

/// Flow-speed threshold below which a velocity is treated as zero (F5.1).
/// Also the degenerate-length cut-off for [`crate::vec::Vec2::normalize`].
pub const EPS_FLOW: f64 = 1e-9; // m/s
