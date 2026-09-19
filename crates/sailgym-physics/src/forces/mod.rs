//! Generalised force assembly in a fixed summation order (F4.4, F9.4).
//!
//! Section 02 ships only the M1 placeholder (R4). The real assembly — hull,
//! board, rudder, sail, sheet, hydrostatics — lands from section 04 onward,
//! in this module, behind the same [`crate::dynamics::ForceModel`] trait.

pub mod scaffold;

pub use scaffold::ScaffoldForces;
