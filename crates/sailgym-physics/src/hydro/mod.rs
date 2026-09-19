//! Hydrodynamics: the hull, the centreboard and the rudder (F6.5, F6.6).
//!
//! The water is still — brief §18 defers currents entirely — so every local
//! flow in this module is the negative of the surface's own velocity,
//! including the `ω × r` term that gives yaw damping and speed-dependent
//! rudder authority.
//!
//! The board and the rudder are the *same* finite lifting surface, differing
//! only in where they are mounted and which way their chord points. Both go
//! through [`foil_hydro_load`], which is the F6.5 workhorse, and through
//! `crate::foil` for the coefficients. Nothing here re-implements lift or
//! drag (section 04 acceptance criterion 6).

pub mod centerboard;
pub mod hull;
pub mod rudder;

pub use centerboard::{foil_hydro_load, local_flow, FoilLoad};
