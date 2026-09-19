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
    /// m². Tag depends on the surface; see each owner's `Default`.
    /// KNOWN for the sail (brief §3), ASSUMED for board and rudder.
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
    /// N. DEFERRED — thrust of the M1 scaffold force model (R4). **Deleted by
    /// task 4.5 together with `forces/scaffold.rs`.** Not physics.
    pub scaffold_thrust: f64,
}

impl Default for SimParams {
    fn default() -> Self {
        Self {
            dt: 0.005,
            integrator: Integrator::Rk2Midpoint,
            scaffold_thrust: 120.0,
        }
    }
}

impl SimParams {
    fn field_mut(&mut self, path: &str) -> Option<&mut f64> {
        match path {
            "dt" => Some(&mut self.dt),
            "scaffold_thrust" => Some(&mut self.scaffold_thrust),
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

    /// Every scalar parameter field carries one of the four F7 tags.
    ///
    /// A "parameter field" is a `pub name: T,` declaration whose type is a
    /// leaf (`f64`, `bool`, `Vec3`, `Integrator`); container fields such as
    /// `pub hull: HullParams` group the catalogue and carry no tag of their
    /// own.
    #[test]
    fn every_parameter_field_is_tagged() {
        const TAGS: [&str; 4] = ["KNOWN", "ASSUMED", "TUNABLE", "DEFERRED"];
        const LEAF_TYPES: [&str; 4] = ["f64", "bool", "Vec3", "Integrator"];

        let src = include_str!("parameters.rs");
        let mut doc = String::new();
        let mut fields = 0usize;
        let mut tagged = 0usize;

        for line in src.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("///") {
                doc.push_str(rest);
                doc.push('\n');
                continue;
            }
            // Attributes sit between the doc comment and the field.
            if t.starts_with("#[") {
                continue;
            }
            if let Some(decl) = t.strip_prefix("pub ") {
                if let Some(ty) = decl.strip_suffix(',').and_then(|d| d.split_once(": ")) {
                    if !decl.contains('(') && LEAF_TYPES.contains(&ty.1) {
                        fields += 1;
                        assert!(
                            TAGS.iter().any(|tag| doc.contains(tag)),
                            "untagged parameter field: {t}"
                        );
                        tagged += 1;
                    }
                }
            }
            doc.clear();
        }

        assert_eq!(fields, tagged);
        assert!(
            fields >= 55,
            "expected the whole F7 catalogue, found {fields} tagged fields"
        );
    }
}
