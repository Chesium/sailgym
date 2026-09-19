//! Physical constants (`docs/00-foundations.md` F1).
//!
//! These are the only numeric physical literals permitted outside
//! `parameters.rs`; see F7.

/// Standard gravity, m/s^2. KNOWN.
pub const G: f64 = 9.806_65;
/// Air density at ISA sea level, 15 degC, kg/m^3. KNOWN.
pub const RHO_AIR: f64 = 1.225;
/// Seawater density, kg/m^3. KNOWN. Fresh water = 998.0.
pub const RHO_WATER: f64 = 1025.0;
