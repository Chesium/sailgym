//! `BoatParameters` — the complete F7 catalogue (`docs/00-foundations.md`).
//!
//! **No numeric physical literal may live outside this module and
//! `constants.rs`** (F7). Section 10 greps for violations.
//!
//! Every field carries a tag, exactly as F7 defines them:
//!
//! - **KNOWN** — published ILCA/class data or a physical constant.
//! - **ASSUMED** — physically motivated estimate; plausible, unvalidated.
//! - **TUNABLE** — expected to be adjusted by playtesting or fitting.
//! - **DEFERRED** — hook exists, not used in v1.
//!
//! Subsystems that do not exist yet (sail, board, rudder, sheet, stability)
//! are catalogued here in full. Later sections consume these fields; none of
//! them re-declares a parameter.
//!
//! ## Dotted paths
//!
//! [`BoatParameters::set_path`] and [`BoatParameters::get_path`] address a
//! scalar by `"<group>.<field>"`, matching F7's own naming: `sail.area`,
//! `board.area`, `rudder.delta_r_max`, `stability.gm`, `sim.dt`. Vector
//! parameters take a component suffix — `hull.sailor_pos_b.z`. The F7 names
//! `mast_pos_b`, `board_pos_b`, `rudder_pos_b` and `block_pos_b` live inside
//! their own group, so their paths are `sail.mast_pos_b.*`, `board.pos_b.*`,
//! `rudder.pos_b.*` and `sheet.block_pos_b.*`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::integrator::Integrator;
use crate::vec::Vec3;

/// Why a parameter operation failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParamError {
    /// No such dotted path (F8.2 `set_parameter`).
    UnknownPath(String),
    /// A `NaN` or infinite value was offered.
    NotFinite(String),
    /// The catalogue is internally inconsistent; carries the offending field
    /// and the reason.
    OutOfRange { field: &'static str, reason: String },
}

impl std::fmt::Display for ParamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownPath(p) => write!(f, "unknown parameter path: {p}"),
            Self::NotFinite(p) => write!(f, "parameter {p} must be finite"),
            Self::OutOfRange { field, reason } => write!(f, "parameter {field}: {reason}"),
        }
    }
}

impl std::error::Error for ParamError {}

/// Serialises [`Vec3`] as `{ "x": .., "y": .., "z": .. }`.
///
/// `vec.rs` is owned by section 01 and derives no serde traits; this keeps the
/// dependency one-way rather than reaching into a file this section does not
/// own.
mod vec3_serde {
    use super::Vec3;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, Deserialize)]
    struct Components {
        x: f64,
        y: f64,
        z: f64,
    }

    pub fn serialize<S: Serializer>(v: &Vec3, s: S) -> Result<S::Ok, S::Error> {
        Components {
            x: v.x,
            y: v.y,
            z: v.z,
        }
        .serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec3, D::Error> {
        let c = Components::deserialize(d)?;
        Ok(Vec3::new(c.x, c.y, c.z))
    }
}

/// `x`/`y`/`z` component of a vector parameter, by name.
fn vec3_component<'a>(v: &'a mut Vec3, component: &str) -> Option<&'a mut f64> {
    match component {
        "x" => Some(&mut v.x),
        "y" => Some(&mut v.y),
        "z" => Some(&mut v.z),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Hull and inertia (F7)
// ---------------------------------------------------------------------------

/// Hull geometry and masses.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HullParams {
    /// m. KNOWN — length overall, brief §3.
    pub loa: f64,
    /// m. KNOWN — waterline length, brief §3.
    pub lwl: f64,
    /// m. KNOWN — maximum beam, brief §3.
    pub beam: f64,
    /// kg. KNOWN — brief §3; class minimum varies 56.7–59.0 by era/source.
    pub m_hull: f64,
    /// kg. KNOWN — brief §4, by definition.
    pub m_sailor: f64,
    /// m, in B. ASSUMED — amidships, on centreline (brief §4). The sailor does
    /// not hike and does not move; see R2.
    #[serde(with = "vec3_serde")]
    pub sailor_pos_b: Vec3,
}

impl Default for HullParams {
    fn default() -> Self {
        Self {
            loa: 4.23,
            lwl: 3.81,
            beam: 1.37,
            m_hull: 58.0,
            m_sailor: 80.0,
            sailor_pos_b: Vec3::new(0.0, 0.0, 0.35),
        }
    }
}

impl HullParams {
    fn field_mut(&mut self, path: &str) -> Option<&mut f64> {
        match path {
            "loa" => Some(&mut self.loa),
            "lwl" => Some(&mut self.lwl),
            "beam" => Some(&mut self.beam),
            "m_hull" => Some(&mut self.m_hull),
            "m_sailor" => Some(&mut self.m_sailor),
            _ => path
                .strip_prefix("sailor_pos_b.")
                .and_then(|c| vec3_component(&mut self.sailor_pos_b, c)),
        }
    }
}

/// Rigid-body and added-mass inertia (F4.2).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct InertiaParams {
    /// kg·m². ASSUMED — `m·(0.25·LOA)²`.
    pub i_zz: f64,
    /// kg·m². ASSUMED — roll radius of gyration 0.45 m about the CG.
    pub i_xx: f64,
    /// kg. ASSUMED — surge added mass ≈ 5 % of Δ.
    pub a_x: f64,
    /// kg. ASSUMED — sway added mass, slender-body estimate.
    pub a_y: f64,
    /// kg·m². ASSUMED — yaw added inertia.
    pub a_psi: f64,
    /// kg·m². ASSUMED — roll added inertia.
    pub a_phi: f64,
}

impl Default for InertiaParams {
    fn default() -> Self {
        Self {
            i_zz: 155.0,
            i_xx: 28.0,
            a_x: 7.0,
            a_y: 100.0,
            a_psi: 60.0,
            a_phi: 15.0,
        }
    }
}

impl InertiaParams {
    fn field_mut(&mut self, path: &str) -> Option<&mut f64> {
        match path {
            "i_zz" => Some(&mut self.i_zz),
            "i_xx" => Some(&mut self.i_xx),
            "a_x" => Some(&mut self.a_x),
            "a_y" => Some(&mut self.a_y),
            "a_psi" => Some(&mut self.a_psi),
            "a_phi" => Some(&mut self.a_phi),
            _ => None,
        }
    }
}

/// Reduced empirical hull resistance (F6.6). Linear + quadratic, every
/// coefficient replaceable by towing-tank data later.
///
/// Sanity anchor: at `u = 2.06 m/s` (4 kn) total surge resistance ≈ 48 N. The
/// model has no planing regime and over-predicts resistance above ≈ 5 m/s
/// (R6).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResistanceParams {
    /// N·s/m. TUNABLE — linear surge resistance.
    pub x_u: f64,
    /// N·s²/m². TUNABLE — quadratic surge resistance.
    pub x_uu: f64,
    /// N·s/m. TUNABLE — linear sway resistance.
    pub y_v: f64,
    /// N·s²/m². TUNABLE — quadratic sway resistance.
    pub y_vv: f64,
    /// N·m·s. TUNABLE — linear yaw damping.
    pub n_r: f64,
    /// N·m·s². TUNABLE — quadratic yaw damping.
    pub n_rr: f64,
    /// N·m·s. TUNABLE — linear roll damping.
    pub k_p: f64,
    /// N·m·s². TUNABLE — quadratic roll damping.
    pub k_pp: f64,
}

impl Default for ResistanceParams {
    fn default() -> Self {
        Self {
            x_u: 5.0,
            x_uu: 9.0,
            y_v: 40.0,
            y_vv: 512.0,
            n_r: 250.0,
            n_rr: 180.0,
            k_p: 60.0,
            k_pp: 40.0,
        }
    }
}

impl ResistanceParams {
    fn field_mut(&mut self, path: &str) -> Option<&mut f64> {
        match path {
            "x_u" => Some(&mut self.x_u),
            "x_uu" => Some(&mut self.x_uu),
            "y_v" => Some(&mut self.y_v),
            "y_vv" => Some(&mut self.y_vv),
            "n_r" => Some(&mut self.n_r),
            "n_rr" => Some(&mut self.n_rr),
            "k_p" => Some(&mut self.k_p),
            "k_pp" => Some(&mut self.k_pp),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Foils (F5) — one section shape, used by sail, board and rudder
// ---------------------------------------------------------------------------

/// The F5.3 `FoilParams` coefficient set: everything the shared lift/drag
/// model needs about one surface. Section 04 implements the model itself.
///
/// Flattened into its owner when serialised, so the JSON keys and the dotted
/// paths agree: `sail.area`, `board.cd0`, `rudder.oswald`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FoilSection {
    // `area` carries **no F7 tag of its own**, deliberately: a tag describes a
    // *value*, and the three surfaces sharing this struct do not share a value
    // or a provenance — the sail's is class data, the board's and the rudder's
    // are estimates. Each owner states the tag with a `Tag override:` note on
    // its own `section` field, which `catalogue` reads. A surface that forgets
    // to leaves an untagged leaf, which `provenance::every_field_tagged`
    // refuses. (A plain comment, not a doc comment: the parser must not see a
    // tag here.)
    /// m². The reference area the F5.3 coefficients act on.
    pub area: f64,
    /// dimensionless. ASSUMED — aspect ratio; the board's is doubled for the
    /// free-surface mirror.
    pub ar: f64,
    /// rad. TUNABLE — onset of the stall blend, F5.2 `α_s`.
    pub alpha_stall: f64,
    /// rad. TUNABLE — width of the stall blend, F5.2 `Δ_s`.
    pub stall_blend: f64,
    /// dimensionless. ASSUMED — flat-plate normal force, F5.2 `C_N,max`.
    pub cn_max: f64,
    /// dimensionless. ASSUMED — zero-lift profile drag.
    pub cd0: f64,
    /// dimensionless. ASSUMED — Oswald span efficiency.
    pub oswald: f64,
    /// rad. DEFERRED — camber hook `α_0` of F5.2; zero in v1.
    pub alpha_camber: f64,
    /// rad. DEFERRED — camber blend width `α_b` of F5.2. Not tabulated in F7
    /// because the hook is inert while `alpha_camber = 0`; it is non-zero only
    /// so `tanh(α / α_b)` stays defined at `α = 0`.
    pub camber_blend: f64,
}

impl FoilSection {
    fn field_mut(&mut self, path: &str) -> Option<&mut f64> {
        match path {
            "area" => Some(&mut self.area),
            "ar" => Some(&mut self.ar),
            "alpha_stall" => Some(&mut self.alpha_stall),
            "stall_blend" => Some(&mut self.stall_blend),
            "cn_max" => Some(&mut self.cn_max),
            "cd0" => Some(&mut self.cd0),
            "oswald" => Some(&mut self.oswald),
            "alpha_camber" => Some(&mut self.alpha_camber),
            "camber_blend" => Some(&mut self.camber_blend),
            _ => None,
        }
    }

    fn validate(&self, group: &'static str) -> Result<(), ParamError> {
        if self.area <= 0.0 {
            return Err(ParamError::OutOfRange {
                field: group,
                reason: "area must be positive".into(),
            });
        }
        if self.ar <= 0.0 {
            return Err(ParamError::OutOfRange {
                field: group,
                reason: "aspect ratio must be positive".into(),
            });
        }
        if self.alpha_stall <= 0.0 || self.stall_blend <= 0.0 {
            return Err(ParamError::OutOfRange {
                field: group,
                reason: "stall angle and blend width must be positive".into(),
            });
        }
        if self.oswald <= 0.0 || self.oswald > 1.0 {
            return Err(ParamError::OutOfRange {
                field: group,
                reason: "Oswald efficiency must lie in (0, 1]".into(),
            });
        }
        if self.camber_blend <= 0.0 {
            return Err(ParamError::OutOfRange {
                field: group,
                reason: "camber blend must be positive so tanh(α/α_b) is defined".into(),
            });
        }
        Ok(())
    }
}

/// A foil plus where it is mounted. The centreboard (F6.5).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FoilMountParams {
    /// The F5.3 coefficient block, flattened (section 02 §2.8).
    ///
    /// Tag override: `area` ASSUMED — 0.20 m² is a plan-form estimate for the
    /// ILCA centreboard, not published class data.
    #[serde(flatten)]
    pub section: FoilSection,
    /// m, in B. ASSUMED — centre of effort relative to the CG.
    #[serde(with = "vec3_serde")]
    pub pos_b: Vec3,
}

impl Default for FoilMountParams {
    /// Centreboard defaults (F7).
    fn default() -> Self {
        Self {
            section: FoilSection {
                area: 0.20,
                ar: 4.9,
                alpha_stall: 0.209,
                stall_blend: 0.087,
                cn_max: 1.9,
                cd0: 0.012,
                oswald: 0.90,
                alpha_camber: 0.0,
                camber_blend: 0.2,
            },
            pos_b: Vec3::new(0.45, 0.0, -0.45),
        }
    }
}

impl FoilMountParams {
    fn field_mut(&mut self, path: &str) -> Option<&mut f64> {
        if let Some(c) = path.strip_prefix("pos_b.") {
            return vec3_component(&mut self.pos_b, c);
        }
        self.section.field_mut(path)
    }
}

// ---------------------------------------------------------------------------
// Sail and rig (F6.3, F6.9)
// ---------------------------------------------------------------------------

/// Sail, boom and rig geometry.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SailParams {
    /// The F5.3 coefficient block, flattened (section 02 §2.8).
    ///
    /// Tag override: `area` KNOWN — the ILCA 7 sail is 7.06 m² by class rule
    /// (brief §3).
    #[serde(flatten)]
    pub section: FoilSection,
    /// m. KNOWN — boom length.
    pub boom_length: f64,
    /// m. ASSUMED — centre of effort along the boom from the mast,
    /// ≈ 0.38 × boom length.
    pub d_ce: f64,
    /// m. ASSUMED — CE height above the CG.
    pub z_ce: f64,
    /// m, in B. ASSUMED — mast foot relative to the CG.
    #[serde(with = "vec3_serde")]
    pub mast_pos_b: Vec3,
    /// kg·m². ASSUMED — boom 3 kg + sail 3 kg about the mast axis.
    pub i_boom: f64,
    /// N·m·s/rad. TUNABLE — gooseneck friction, F6.9 `c_β`.
    pub c_beta: f64,
    /// rad. ASSUMED — boom swing limit (100°), F6.9.
    pub beta_max: f64,
    /// N·m/rad. TUNABLE — soft limit stiffness beyond `beta_max`.
    pub k_lim: f64,
    /// N·m·s/rad. TUNABLE — soft limit damping beyond `beta_max`.
    pub c_lim: f64,
}

impl Default for SailParams {
    fn default() -> Self {
        Self {
            section: FoilSection {
                area: 7.06,
                ar: 3.7,
                alpha_stall: 0.262,
                stall_blend: 0.105,
                cn_max: 1.8,
                cd0: 0.06,
                oswald: 0.85,
                alpha_camber: 0.0,
                camber_blend: 0.2,
            },
            boom_length: 2.72,
            d_ce: 1.05,
            z_ce: 2.40,
            mast_pos_b: Vec3::new(1.20, 0.0, 0.0),
            i_boom: 12.0,
            c_beta: 2.0,
            beta_max: 1.745,
            k_lim: 400.0,
            c_lim: 40.0,
        }
    }
}

impl SailParams {
    fn field_mut(&mut self, path: &str) -> Option<&mut f64> {
        if let Some(c) = path.strip_prefix("mast_pos_b.") {
            return vec3_component(&mut self.mast_pos_b, c);
        }
        match path {
            "boom_length" => Some(&mut self.boom_length),
            "d_ce" => Some(&mut self.d_ce),
            "z_ce" => Some(&mut self.z_ce),
            "i_boom" => Some(&mut self.i_boom),
            "c_beta" => Some(&mut self.c_beta),
            "beta_max" => Some(&mut self.beta_max),
            "k_lim" => Some(&mut self.k_lim),
            "c_lim" => Some(&mut self.c_lim),
            _ => self.section.field_mut(path),
        }
    }
}

// ---------------------------------------------------------------------------
// Rudder (F6.5, F2.2)
// ---------------------------------------------------------------------------

/// Rudder foil, mounting and actuator limits.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RudderParams {
    /// The F5.3 coefficient block, flattened (section 02 §2.8).
    ///
    /// Tag override: `area` ASSUMED — 0.105 m² is a plan-form estimate for the
    /// ILCA rudder blade, not published class data.
    #[serde(flatten)]
    pub section: FoilSection,
    /// m, in B. ASSUMED — rudder centre of effort relative to the CG.
    #[serde(with = "vec3_serde")]
    pub pos_b: Vec3,
    /// rad. ASSUMED — maximum rudder deflection (40°), brief §13.
    pub delta_r_max: f64,
    /// rad/s. TUNABLE — maximum commanded rudder rate (120°/s), brief §13.
    pub delta_r_rate_max: f64,
    /// rad/s. TUNABLE — rate at which the tiller returns to neutral (90°/s)
    /// when no steering key is held.
    pub delta_r_return_rate: f64,
    /// TUNABLE — whether the tiller self-centres at all (brief §13 leaves the
    /// choice to playtesting). Non-zero through `set_path` means `true`.
    pub delta_r_self_centre: bool,
}

impl Default for RudderParams {
    fn default() -> Self {
        Self {
            section: FoilSection {
                area: 0.105,
                ar: 3.9,
                alpha_stall: 0.209,
                stall_blend: 0.087,
                cn_max: 1.9,
                cd0: 0.012,
                oswald: 0.90,
                alpha_camber: 0.0,
                camber_blend: 0.2,
            },
            pos_b: Vec3::new(-2.00, 0.0, -0.28),
            delta_r_max: 0.698,
            delta_r_rate_max: 2.09,
            delta_r_return_rate: 1.57,
            delta_r_self_centre: true,
        }
    }
}

impl RudderParams {
    fn field_mut(&mut self, path: &str) -> Option<&mut f64> {
        if let Some(c) = path.strip_prefix("pos_b.") {
            return vec3_component(&mut self.pos_b, c);
        }
        match path {
            "delta_r_max" => Some(&mut self.delta_r_max),
            "delta_r_rate_max" => Some(&mut self.delta_r_rate_max),
            "delta_r_return_rate" => Some(&mut self.delta_r_return_rate),
            _ => self.section.field_mut(path),
        }
    }
}

// ---------------------------------------------------------------------------
// Mainsheet (F6.8)
// ---------------------------------------------------------------------------

/// Mainsheet rope model. Mechanical advantage, block friction, ratchets,
/// cleats and hand force are DEFERRED (brief §11); `l_sheet` is the effective
/// available length at the boom.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SheetParams {
    /// N/m. TUNABLE — rope stiffness. R1: raising this above 3e4 is the most
    /// likely cause of a blow-up at `dt = 0.005`.
    pub k_sheet: f64,
    /// N·s/m. TUNABLE — rope damping.
    pub c_sheet: f64,
    /// m. ASSUMED — boom attachment distance from the mast, near the clew.
    pub d_sheet: f64,
    /// m. ASSUMED — boom height above the CG.
    pub z_boom: f64,
    /// m, in B. ASSUMED — transom block position.
    #[serde(with = "vec3_serde")]
    pub block_pos_b: Vec3,
    /// m. ASSUMED — shortest available sheet length.
    pub l_sheet_min: f64,
    /// m. ASSUMED — longest available sheet length.
    pub l_sheet_max: f64,
    /// m/s. TUNABLE — hauling rate under player command.
    pub sheet_haul_rate: f64,
    /// m/s. TUNABLE — easing rate under player command.
    pub sheet_ease_rate: f64,
    /// m/s. TUNABLE — emergency release rate (Space), brief §12.
    pub sheet_release_rate: f64,
}

impl Default for SheetParams {
    fn default() -> Self {
        Self {
            k_sheet: 2.0e4,
            c_sheet: 300.0,
            d_sheet: 2.45,
            z_boom: 0.70,
            block_pos_b: Vec3::new(-2.10, 0.0, 0.10),
            l_sheet_min: 0.90,
            l_sheet_max: 4.50,
            sheet_haul_rate: 1.5,
            sheet_ease_rate: 3.0,
            sheet_release_rate: 6.0,
        }
    }
}

impl SheetParams {
    fn field_mut(&mut self, path: &str) -> Option<&mut f64> {
        if let Some(c) = path.strip_prefix("block_pos_b.") {
            return vec3_component(&mut self.block_pos_b, c);
        }
        match path {
            "k_sheet" => Some(&mut self.k_sheet),
            "c_sheet" => Some(&mut self.c_sheet),
            "d_sheet" => Some(&mut self.d_sheet),
            "z_boom" => Some(&mut self.z_boom),
            "l_sheet_min" => Some(&mut self.l_sheet_min),
            "l_sheet_max" => Some(&mut self.l_sheet_max),
            "sheet_haul_rate" => Some(&mut self.sheet_haul_rate),
            "sheet_ease_rate" => Some(&mut self.sheet_ease_rate),
            "sheet_release_rate" => Some(&mut self.sheet_release_rate),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Stability (F6.7, F6.10)
// ---------------------------------------------------------------------------

/// Hydrostatic righting and the capsize report.
///
/// Anchor: `Δ·g·GZ_max ≈ 406 N·m`. With `z_ce = 2.4 m` that is balanced by
/// ≈ 169 N of sail side force. This is correct and intended — brief §4 fixes
/// the sailor amidships with no hiking (R2).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct StabilityParams {
    /// m. ASSUMED — metacentric height, the slope of `GZ` at `φ = 0`.
    pub gm: f64,
    /// rad. ASSUMED — angle of maximum righting arm (45°).
    pub phi_peak: f64,
    /// m. ASSUMED — maximum righting arm.
    pub gz_max: f64,
    /// rad. ASSUMED — angle of vanishing stability (80°).
    pub phi_vanish: f64,
    /// rad. TUNABLE — heel beyond which the capsize timer runs (80°).
    pub phi_capsize: f64,
    /// s. TUNABLE — how long `|φ| > phi_capsize` must hold before the boat is
    /// *reported* capsized. Reported, never acted on (F6.10).
    pub t_capsize: f64,
}

impl Default for StabilityParams {
    fn default() -> Self {
        Self {
            gm: 1.00,
            phi_peak: 0.785,
            gz_max: 0.30,
            phi_vanish: 1.396,
            phi_capsize: 1.396,
            t_capsize: 1.0,
        }
    }
}

impl StabilityParams {
    fn field_mut(&mut self, path: &str) -> Option<&mut f64> {
        match path {
            "gm" => Some(&mut self.gm),
            "phi_peak" => Some(&mut self.phi_peak),
            "gz_max" => Some(&mut self.gz_max),
            "phi_vanish" => Some(&mut self.phi_vanish),
            "phi_capsize" => Some(&mut self.phi_capsize),
            "t_capsize" => Some(&mut self.t_capsize),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Integration (F7)
// ---------------------------------------------------------------------------

/// Numerical integration settings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimParams {
    /// s. TUNABLE — fixed physics timestep; brief §21 range 0.005–0.01.
    pub dt: f64,
    /// KNOWN — brief §21 prefers RK2 midpoint. `Rk4` exists only as the
    /// section 10 convergence reference and is never the default.
    pub integrator: Integrator,
}

impl Default for SimParams {
    fn default() -> Self {
        Self {
            dt: 0.005,
            integrator: Integrator::Rk2Midpoint,
        }
    }
}

impl SimParams {
    fn field_mut(&mut self, path: &str) -> Option<&mut f64> {
        match path {
            "dt" => Some(&mut self.dt),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// The catalogue
// ---------------------------------------------------------------------------

/// The complete F7 parameter catalogue.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BoatParameters {
    pub hull: HullParams,
    pub inertia: InertiaParams,
    pub resistance: ResistanceParams,
    pub sail: SailParams,
    pub board: FoilMountParams,
    pub rudder: RudderParams,
    pub sheet: SheetParams,
    pub stability: StabilityParams,
    pub sim: SimParams,
}

/// The one path whose value is a flag rather than a number.
const SELF_CENTRE_PATH: &str = "rudder.delta_r_self_centre";

impl BoatParameters {
    /// The F7 defaults: an ILCA 7 with an 80 kg sailor sitting amidships.
    pub fn ilca7() -> Self {
        Self::default()
    }

    /// Displacement `Δ = m_hull + m_sailor` (F4.2).
    pub fn total_mass(&self) -> f64 {
        self.hull.m_hull + self.hull.m_sailor
    }

    /// Internal consistency of the catalogue. Cheap, and called wherever
    /// parameters enter from outside the crate.
    pub fn validate(&self) -> Result<(), ParamError> {
        let positive = |v: f64, field: &'static str| -> Result<(), ParamError> {
            if v > 0.0 && v.is_finite() {
                Ok(())
            } else {
                Err(ParamError::OutOfRange {
                    field,
                    reason: format!("must be finite and positive, got {v}"),
                })
            }
        };

        positive(self.hull.loa, "hull.loa")?;
        positive(self.hull.lwl, "hull.lwl")?;
        positive(self.hull.beam, "hull.beam")?;
        positive(self.hull.m_hull, "hull.m_hull")?;
        positive(self.hull.m_sailor, "hull.m_sailor")?;
        positive(self.inertia.i_zz, "inertia.i_zz")?;
        positive(self.inertia.i_xx, "inertia.i_xx")?;
        positive(self.sail.i_boom, "sail.i_boom")?;
        positive(self.sail.beta_max, "sail.beta_max")?;
        positive(self.rudder.delta_r_max, "rudder.delta_r_max")?;
        positive(self.rudder.delta_r_rate_max, "rudder.delta_r_rate_max")?;
        positive(self.sheet.k_sheet, "sheet.k_sheet")?;
        positive(self.stability.gm, "stability.gm")?;
        positive(self.stability.gz_max, "stability.gz_max")?;
        positive(self.sim.dt, "sim.dt")?;

        if self.inertia.a_x < 0.0 || self.inertia.a_y < 0.0 {
            return Err(ParamError::OutOfRange {
                field: "inertia",
                reason: "added masses must not be negative".into(),
            });
        }
        if self.hull.lwl > self.hull.loa {
            return Err(ParamError::OutOfRange {
                field: "hull.lwl",
                reason: "waterline length exceeds length overall".into(),
            });
        }
        if self.sheet.l_sheet_min >= self.sheet.l_sheet_max {
            return Err(ParamError::OutOfRange {
                field: "sheet.l_sheet_min",
                reason: "must be strictly below l_sheet_max".into(),
            });
        }
        if self.sheet.l_sheet_min < 0.0 {
            return Err(ParamError::OutOfRange {
                field: "sheet.l_sheet_min",
                reason: "must not be negative".into(),
            });
        }
        // F6.7 solves the GZ curve from these; the ordering is what makes the
        // 3×3 system meaningful.
        if self.stability.phi_peak <= 0.0 || self.stability.phi_peak >= self.stability.phi_vanish {
            return Err(ParamError::OutOfRange {
                field: "stability.phi_peak",
                reason: "require 0 < phi_peak < phi_vanish".into(),
            });
        }
        if self.stability.t_capsize < 0.0 {
            return Err(ParamError::OutOfRange {
                field: "stability.t_capsize",
                reason: "must not be negative".into(),
            });
        }

        self.sail.section.validate("sail")?;
        self.board.section.validate("board")?;
        self.rudder.section.validate("rudder")?;
        Ok(())
    }

    /// Dotted-path setter for live editing (F8.2 `set_parameter`, brief §31).
    ///
    /// Returns `Ok(true)` when the change invalidates simulation continuity
    /// and the caller must reset. Only `sim.*` does: changing the timestep or
    /// the integrator changes the meaning of every recorded trajectory
    /// (F9, brief §33). Everything else may be edited while running.
    pub fn set_path(&mut self, path: &str, value: f64) -> Result<bool, ParamError> {
        if !value.is_finite() {
            return Err(ParamError::NotFinite(path.to_string()));
        }
        if path == SELF_CENTRE_PATH {
            self.rudder.delta_r_self_centre = value != 0.0;
            return Ok(false);
        }
        match self.field_mut(path) {
            Some(slot) => *slot = value,
            None => return Err(ParamError::UnknownPath(path.to_string())),
        }
        Ok(path.starts_with("sim."))
    }

    /// Dotted-path getter, the inverse of [`BoatParameters::set_path`].
    pub fn get_path(&self, path: &str) -> Result<f64, ParamError> {
        if path == SELF_CENTRE_PATH {
            return Ok(if self.rudder.delta_r_self_centre {
                1.0
            } else {
                0.0
            });
        }
        // `BoatParameters` is `Copy`, so probing through a scratch copy costs
        // one stack move and keeps a single path table instead of two.
        let mut probe = *self;
        match probe.field_mut(path) {
            Some(slot) => Ok(*slot),
            None => Err(ParamError::UnknownPath(path.to_string())),
        }
    }

    /// The single dotted-path table. `get_path` and `set_path` share it.
    fn field_mut(&mut self, path: &str) -> Option<&mut f64> {
        let (group, rest) = path.split_once('.')?;
        match group {
            "hull" => self.hull.field_mut(rest),
            "inertia" => self.inertia.field_mut(rest),
            "resistance" => self.resistance.field_mut(rest),
            "sail" => self.sail.field_mut(rest),
            "board" => self.board.field_mut(rest),
            "rudder" => self.rudder.field_mut(rest),
            "sheet" => self.sheet.field_mut(rest),
            "stability" => self.stability.field_mut(rest),
            "sim" => self.sim.field_mut(rest),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// The editable catalogue, derived from this file's own source (section 08)
// ---------------------------------------------------------------------------

/// One editable leaf of the catalogue: its dotted path, its F7 tag, its unit
/// and its documentation.
///
/// **Nothing here is hand-written.** [`catalogue`] parses the struct
/// declarations and the doc comments of this very file, so a parameter added
/// above appears in the panel (brief §31) with its tag the moment it compiles,
/// and one deleted disappears. That is the whole point: a second, hand-kept
/// list of parameter names is precisely the kind of duplication that goes
/// stale, and the panel's job is to be a faithful view of `parameters.rs`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ParamMeta {
    /// The dotted path `set_path`/`get_path` accept.
    pub path: String,
    /// `KNOWN`, `ASSUMED`, `TUNABLE`, `DEFERRED`, or empty if the doc comment
    /// carries none (which `every_parameter_field_is_tagged` forbids).
    pub tag: String,
    /// The unit as the doc comment states it — `m`, `kg·m²`, `rad/s`,
    /// `dimensionless` — or empty for a dimensionless flag.
    pub unit: String,
    /// The field's whole doc comment, for the control's tooltip.
    pub doc: String,
    /// `f64` or `bool`. `sim.integrator` is neither and is not addressable
    /// (section 02 handoff §2.8), so it is absent.
    pub kind: String,
    /// Whether editing this path invalidates simulation continuity, matching
    /// what [`BoatParameters::set_path`] returns.
    pub reset_required: bool,
}

/// The four F7 tags, in no particular order.
const PARAM_TAGS: [&str; 4] = ["KNOWN", "ASSUMED", "TUNABLE", "DEFERRED"];

/// Longest a leading phrase may be before it stops being a unit and starts
/// being prose. `N·s²/m²` and `m, in B` are units; a sentence is not.
const MAX_UNIT_CHARS: usize = 24;

/// One field as it is written in the source.
struct SourceField {
    name: String,
    ty: String,
    doc: String,
    flatten: bool,
}

/// Every `pub struct` in this file, with its fields in declaration order.
fn source_structs(src: &str) -> BTreeMap<String, Vec<SourceField>> {
    let mut out: BTreeMap<String, Vec<SourceField>> = BTreeMap::new();
    let mut open: Option<(String, Vec<SourceField>)> = None;
    let mut doc = String::new();
    let mut flatten = false;

    for line in src.lines() {
        let t = line.trim();
        let Some((_, fields)) = open.as_mut() else {
            if let Some(name) = t
                .strip_prefix("pub struct ")
                .and_then(|r| r.strip_suffix(" {"))
            {
                open = Some((name.to_string(), Vec::new()));
                doc.clear();
                flatten = false;
            }
            continue;
        };
        // Struct bodies in this file contain no nested braces, so a `}` in
        // column 0 closes the declaration.
        if line == "}" {
            let (name, fields) = open.take().expect("a declaration is open");
            out.insert(name, fields);
            continue;
        }
        if let Some(rest) = t.strip_prefix("///") {
            doc.push_str(rest.trim());
            doc.push(' ');
            continue;
        }
        if t.starts_with("#[") {
            flatten |= t.contains("flatten");
            continue;
        }
        if let Some((name, ty)) = t
            .strip_prefix("pub ")
            .and_then(|d| d.trim_end_matches(',').split_once(": "))
        {
            fields.push(SourceField {
                name: name.to_string(),
                ty: ty.to_string(),
                doc: doc.trim().to_string(),
                flatten,
            });
        }
        doc.clear();
        flatten = false;
    }
    out
}

/// Split a doc comment into `(tag, unit)`. The tag is the first of the four
/// F7 words to appear; the unit is the phrase in front of it, when that phrase
/// is short enough to be a unit rather than a sentence.
fn tag_and_unit(doc: &str) -> (String, String) {
    let at = PARAM_TAGS
        .iter()
        .filter_map(|tag| doc.find(tag).map(|i| (i, *tag)))
        .min_by_key(|(i, _)| *i);
    let (tag_at, tag) = match at {
        Some((i, tag)) => (i, tag.to_string()),
        None => (doc.len(), String::new()),
    };
    let lead = doc[..tag_at].trim().trim_end_matches('.').trim();
    // A unit is what stands before the first sentence break. Anything longer
    // is prose, and prose is not a unit.
    let candidate = lead.split_once(". ").map_or(lead, |(head, _)| head).trim();
    let unit = if candidate.chars().count() <= MAX_UNIT_CHARS {
        candidate.to_string()
    } else {
        String::new()
    };
    (tag, unit)
}

fn leaf(
    path: String,
    field: &SourceField,
    kind: &str,
    overrides: &BTreeMap<String, TagOverride>,
) -> ParamMeta {
    let (mut tag, unit) = tag_and_unit(&field.doc);
    let mut doc = field.doc.clone();
    if let Some(o) = overrides.get(&field.name) {
        tag = o.tag.clone();
        // The override sentence is prepended rather than appended, because it
        // is the more specific statement and it is what the panel's tooltip
        // should lead with.
        doc = format!("{} {}", o.note, doc).trim().to_string();
    }
    ParamMeta {
        reset_required: path.starts_with("sim."),
        path,
        tag,
        unit,
        doc,
        kind: kind.to_string(),
    }
}

/// A per-owner tag for one leaf of a shared, flattened struct.
///
/// `FoilSection` is shared by the sail, the centreboard and the rudder, and
/// `area` is KNOWN for one of them and ASSUMED for the other two: the tag
/// belongs to the value, and there are three values behind one declaration.
/// F5.3 pins `FoilParams`'s field list, so splitting `area` out is not
/// available (F13.1); instead each owner states the tag on its own `section`
/// field and this carries it.
struct TagOverride {
    tag: String,
    note: String,
}

/// Read `Tag override: \`name\` TAG — reason.` entries out of a doc comment.
///
/// The marker is machine-first on purpose. A tag that a human has to infer
/// from prose is a tag that goes stale without anything noticing, and the
/// whole point of `catalogue` is that nothing about a parameter is kept in two
/// places.
fn tag_overrides(doc: &str) -> BTreeMap<String, TagOverride> {
    const MARKER: &str = "Tag override:";
    let mut out = BTreeMap::new();
    for (i, _) in doc.match_indices(MARKER) {
        let rest = &doc[i + MARKER.len()..];
        // The sentence runs to the next marker, so several overrides can sit
        // in one doc comment.
        let end = rest.find(MARKER).unwrap_or(rest.len());
        let sentence = rest[..end].trim();
        let Some(name) = sentence
            .split('`')
            .nth(1)
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
        else {
            continue;
        };
        let (tag, _) = tag_and_unit(sentence);
        if tag.is_empty() {
            continue;
        }
        out.insert(
            name,
            TagOverride {
                tag,
                note: format!("{MARKER} {sentence}"),
            },
        );
    }
    out
}

fn walk(
    structs: &BTreeMap<String, Vec<SourceField>>,
    ty: &str,
    prefix: &str,
    overrides: &BTreeMap<String, TagOverride>,
    out: &mut Vec<ParamMeta>,
) {
    let Some(fields) = structs.get(ty) else {
        return;
    };
    for field in fields {
        let own = format!("{prefix}{}", field.name);
        match field.ty.as_str() {
            "f64" => out.push(leaf(own, field, "f64", overrides)),
            "bool" => out.push(leaf(own, field, "bool", overrides)),
            "Vec3" => {
                for c in ["x", "y", "z"] {
                    out.push(leaf(format!("{own}.{c}"), field, "f64", overrides));
                }
            }
            // Not a scalar, so `set_path` has never accepted it.
            "Integrator" => {}
            group => {
                // `#[serde(flatten)]` puts the child's fields at the parent's
                // level in the JSON, and F7's own naming follows the JSON —
                // `sail.area`, not `sail.section.area` (section 02 §2.8).
                let next = if field.flatten {
                    prefix.to_string()
                } else {
                    format!("{own}.")
                };
                // A tag override reaches exactly the struct it is written on,
                // and no further: the owner knows what its own `area` is, not
                // what a nested group's might be.
                let inherited = if field.flatten {
                    tag_overrides(&field.doc)
                } else {
                    BTreeMap::new()
                };
                walk(structs, group, &next, &inherited, out);
            }
        }
    }
}

/// Every editable leaf of the F7 catalogue, in declaration order.
///
/// The paths are exactly those [`BoatParameters::set_path`] accepts;
/// `catalogue_agrees_with_set_path` proves it in both directions.
pub fn catalogue() -> Vec<ParamMeta> {
    let structs = source_structs(include_str!("parameters.rs"));
    let mut out = Vec::new();
    walk(&structs, "BoatParameters", "", &BTreeMap::new(), &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Paths spanning every group, used by the round-trip test.
    const SAMPLE_PATHS: [&str; 14] = [
        "hull.loa",
        "hull.m_sailor",
        "hull.sailor_pos_b.z",
        "inertia.i_zz",
        "resistance.y_vv",
        "sail.area",
        "sail.mast_pos_b.x",
        "sail.beta_max",
        "board.area",
        "board.pos_b.z",
        "rudder.delta_r_max",
        "sheet.k_sheet",
        "stability.gz_max",
        "sim.dt",
    ];

    #[test]
    fn ilca7_validates() {
        assert_eq!(BoatParameters::ilca7().validate(), Ok(()));
    }

    #[test]
    fn ilca7_matches_the_f7_anchors() {
        let p = BoatParameters::ilca7();
        assert_eq!(p.total_mass(), 138.0);
        assert_eq!(p.sail.section.area, 7.06);
        assert_eq!(p.sim.dt, 0.005);
        assert_eq!(p.sim.integrator, Integrator::Rk2Midpoint);
    }

    #[test]
    fn set_path_get_path_round_trip() {
        let mut p = BoatParameters::ilca7();
        for (i, path) in SAMPLE_PATHS.iter().enumerate() {
            let value = 1.5 + i as f64;
            let reset = p.set_path(path, value).expect("path must resolve");
            assert_eq!(reset, path.starts_with("sim."), "reset flag for {path}");
            assert_eq!(p.get_path(path), Ok(value), "round trip for {path}");
        }
        assert!(SAMPLE_PATHS.len() >= 10);
    }

    #[test]
    fn unknown_paths_are_errors() {
        let mut p = BoatParameters::ilca7();
        for bad in [
            "",
            "nope",
            "hull",
            "hull.nope",
            "sail.mast_pos_b.w",
            "sim.integrator",
        ] {
            assert!(matches!(
                p.set_path(bad, 1.0),
                Err(ParamError::UnknownPath(_))
            ));
            assert!(matches!(p.get_path(bad), Err(ParamError::UnknownPath(_))));
        }
    }

    #[test]
    fn non_finite_values_are_rejected() {
        let mut p = BoatParameters::ilca7();
        assert!(matches!(
            p.set_path("sail.area", f64::NAN),
            Err(ParamError::NotFinite(_))
        ));
        assert_eq!(p.sail.section.area, 7.06);
    }

    #[test]
    fn self_centre_flag_round_trips_through_the_path_api() {
        let mut p = BoatParameters::ilca7();
        assert_eq!(p.get_path(SELF_CENTRE_PATH), Ok(1.0));
        assert_eq!(p.set_path(SELF_CENTRE_PATH, 0.0), Ok(false));
        assert!(!p.rudder.delta_r_self_centre);
        assert_eq!(p.get_path(SELF_CENTRE_PATH), Ok(0.0));
    }

    #[test]
    fn validate_rejects_an_inconsistent_catalogue() {
        let mut p = BoatParameters::ilca7();
        p.sheet.l_sheet_min = p.sheet.l_sheet_max + 1.0;
        assert!(p.validate().is_err());

        let mut p = BoatParameters::ilca7();
        p.stability.phi_vanish = p.stability.phi_peak * 0.5;
        assert!(p.validate().is_err());

        let mut p = BoatParameters::ilca7();
        p.sail.section.area = 0.0;
        assert!(p.validate().is_err());
    }

    #[test]
    fn parameters_round_trip_through_json() {
        let p = BoatParameters::ilca7();
        let json = serde_json::to_string(&p).expect("serialisable");
        // The flattened foil section keeps the JSON keys and the dotted paths
        // in step: `sail.area` is `sail: { area: .. }`, not `sail.section.area`.
        assert!(json.contains("\"area\":7.06"));
        let back: BoatParameters = serde_json::from_str(&json).expect("deserialisable");
        assert_eq!(back, p);
    }

    /// The derived catalogue and the dotted-path table describe the same set.
    ///
    /// This is what lets the parameter panel be generated rather than written
    /// (brief §31, task 8.5): a path the panel offers must be one the core
    /// accepts, and a path the core accepts must be one the panel offers.
    #[test]
    fn catalogue_agrees_with_set_path() {
        let meta = catalogue();
        let mut p = BoatParameters::ilca7();

        // Forwards: every catalogued path resolves, round-trips, and reports
        // the reset flag the metadata promised.
        for m in &meta {
            let before = p
                .get_path(&m.path)
                .unwrap_or_else(|e| panic!("{}: {e}", m.path));
            let probe = if m.kind == "bool" { 0.0 } else { before + 1.0 };
            let reset = p
                .set_path(&m.path, probe)
                .unwrap_or_else(|e| panic!("{}: {e}", m.path));
            assert_eq!(reset, m.reset_required, "reset flag for {}", m.path);
            assert_eq!(p.get_path(&m.path), Ok(probe), "round trip for {}", m.path);
            p.set_path(&m.path, before).expect("restore");
            assert!(m.kind == "f64" || m.kind == "bool", "kind for {}", m.path);
            assert!(
                PARAM_TAGS.contains(&m.tag.as_str()),
                "untagged catalogue entry: {} ({:?})",
                m.path,
                m.doc
            );
        }

        // Backwards: the paths the rest of the crate already relies on are all
        // present, and so is every group.
        let paths: Vec<&str> = meta.iter().map(|m| m.path.as_str()).collect();
        for path in SAMPLE_PATHS.iter().chain([&SELF_CENTRE_PATH]) {
            assert!(paths.contains(path), "catalogue is missing {path}");
        }
        for group in [
            "hull.",
            "inertia.",
            "resistance.",
            "sail.",
            "board.",
            "rudder.",
            "sheet.",
            "stability.",
            "sim.",
        ] {
            assert!(
                paths.iter().any(|p| p.starts_with(group)),
                "catalogue is missing the {group} group"
            );
        }

        // `sim.integrator` is not a scalar and `set_path` rejects it, so the
        // panel must not offer it (section 02 handoff §2.8).
        assert!(!paths.contains(&"sim.integrator"));

        // Same floor as `every_parameter_field_is_tagged`: the whole F7
        // catalogue, not a subset of it.
        assert!(meta.len() >= 55, "only {} catalogued leaves", meta.len());
    }

    /// Units and tags are read off the doc comments, not invented.
    #[test]
    fn catalogue_reads_units_and_tags_from_the_doc_comments() {
        let meta = catalogue();
        let by = |path: &str| {
            meta.iter()
                .find(|m| m.path == path)
                .unwrap_or_else(|| panic!("no {path}"))
                .clone()
        };
        assert_eq!(
            (by("hull.loa").tag.as_str(), by("hull.loa").unit.as_str()),
            ("KNOWN", "m")
        );
        assert_eq!(by("inertia.i_zz").unit, "kg·m²");
        assert_eq!(by("resistance.x_uu").unit, "N·s²/m²");
        assert_eq!(by("resistance.x_uu").tag, "TUNABLE");
        assert_eq!(by("sheet.k_sheet").unit, "N/m");
        assert_eq!(by("rudder.delta_r_rate_max").unit, "rad/s");
        assert_eq!(by("hull.sailor_pos_b.y").unit, "m, in B");
        assert_eq!(by("sail.alpha_camber").tag, "DEFERRED");
        assert_eq!(by("sim.dt").tag, "TUNABLE");
        assert!(by("sim.dt").reset_required);
        assert!(!by("sail.area").reset_required);
        // A doc comment that is prose rather than a unit yields no unit.
        assert_eq!(by(SELF_CENTRE_PATH).kind, "bool");
        assert_eq!(by(SELF_CENTRE_PATH).unit, "");
        assert_eq!(by(SELF_CENTRE_PATH).tag, "TUNABLE");
        // Vector parameters expand into their three components, in order.
        let block: Vec<&str> = meta
            .iter()
            .map(|m| m.path.as_str())
            .filter(|p| p.starts_with("sheet.block_pos_b."))
            .collect();
        assert_eq!(
            block,
            [
                "sheet.block_pos_b.x",
                "sheet.block_pos_b.y",
                "sheet.block_pos_b.z"
            ]
        );
    }

    /// Every scalar parameter field carries one of the four F7 tags.
    ///
    /// A "parameter field" is a `pub name: T,` declaration whose type is a
    /// leaf (`f64`, `bool`, `Vec3`, `Integrator`); container fields such as
    /// `pub hull: HullParams` group the catalogue and carry no tag of their
    /// own.
    ///
    /// Scoped to the structs [`catalogue`] actually reaches from
    /// `BoatParameters`. `ParamMeta` and the serde helper live in this file
    /// too and are not parameters; scanning the file as a whole would demand
    /// an F7 tag on `ParamMeta::reset_required`, which has no physical
    /// meaning. Reusing the same parser the catalogue uses also means this
    /// test fails if that parser ever stops seeing a field.
    /// A shared, flattened field takes its tag from its **owner** (section 10).
    ///
    /// `FoilSection::area` is one declaration behind three values: 7.06 m² by
    /// class rule for the sail, and plan-form estimates for the board and the
    /// rudder. Section 08's handoff §5 recorded that all three showed `KNOWN`
    /// in the panel and that two of them were wrong; this is the guard on the
    /// fix.
    #[test]
    fn a_shared_field_takes_its_tag_from_its_owner() {
        let by_path: BTreeMap<String, String> =
            catalogue().into_iter().map(|m| (m.path, m.tag)).collect();
        assert_eq!(by_path["sail.area"], "KNOWN");
        assert_eq!(by_path["board.area"], "ASSUMED");
        assert_eq!(by_path["rudder.area"], "ASSUMED");
        // The coefficients really are shared, so they must *not* be
        // overridden — otherwise the mechanism is leaking.
        for surface in ["sail", "board", "rudder"] {
            assert_eq!(by_path[&format!("{surface}.cd0")], "ASSUMED");
        }
        // And the base declaration carries no tag of its own, which is what
        // makes a missing override a failure rather than a silent default.
        let structs = source_structs(include_str!("parameters.rs"));
        let area = structs["FoilSection"]
            .iter()
            .find(|f| f.name == "area")
            .expect("FoilSection::area");
        assert!(
            !PARAM_TAGS.iter().any(|t| area.doc.contains(t)),
            "FoilSection::area must carry no tag of its own: {}",
            area.doc
        );
    }

    #[test]
    fn every_parameter_field_is_tagged() {
        const LEAF_TYPES: [&str; 4] = ["f64", "bool", "Vec3", "Integrator"];

        let structs = source_structs(include_str!("parameters.rs"));
        let mut reachable: std::collections::BTreeSet<String> = Default::default();
        let mut queue = vec!["BoatParameters".to_string()];
        while let Some(name) = queue.pop() {
            let Some(fields) = structs.get(&name) else {
                continue;
            };
            if !reachable.insert(name) {
                continue;
            }
            for f in fields {
                if structs.contains_key(&f.ty) {
                    queue.push(f.ty.clone());
                }
            }
        }
        assert!(
            reachable.contains("FoilSection") && reachable.contains("StabilityParams"),
            "the walk must reach the flattened and the nested groups: {reachable:?}"
        );

        // The assertion is on the **resolved** catalogue, not on the
        // declaration, because since section 10 a tag may legitimately live on
        // the owner rather than on the field: `FoilSection` is shared by three
        // surfaces and `area` is KNOWN for one of them and ASSUMED for the
        // other two (see `TagOverride`). Resolving first is strictly stronger
        // than scanning declarations — it is what the panel actually shows —
        // and it still fails on a field nobody tagged anywhere.
        let catalogue = catalogue();
        let relevant = |path: &str, field: &str| -> bool {
            path == field
                || path.ends_with(&format!(".{field}"))
                || path.contains(&format!(".{field}."))
                || path.starts_with(&format!("{field}."))
        };

        let mut fields = 0usize;
        let mut tagged = 0usize;
        for name in &reachable {
            for f in &structs[name] {
                if !LEAF_TYPES.contains(&f.ty.as_str()) {
                    continue;
                }
                fields += 1;
                // `sim.integrator` is not a scalar and has never been
                // addressable (section 02 §2.8), so it produces no leaf; its
                // own doc comment must therefore carry the tag.
                let leaves: Vec<&ParamMeta> = catalogue
                    .iter()
                    .filter(|m| relevant(&m.path, &f.name))
                    .collect();
                if leaves.is_empty() {
                    assert!(
                        PARAM_TAGS.iter().any(|tag| f.doc.contains(tag)),
                        "untagged parameter field: {name}.{}",
                        f.name
                    );
                } else {
                    for m in leaves {
                        assert!(
                            PARAM_TAGS.contains(&m.tag.as_str()),
                            "untagged parameter leaf: {} (from {name}.{})",
                            m.path,
                            f.name
                        );
                    }
                }
                tagged += 1;
            }
        }

        assert_eq!(fields, tagged);
        assert!(
            fields >= 55,
            "expected the whole F7 catalogue, found {fields} tagged fields"
        );
    }
}
