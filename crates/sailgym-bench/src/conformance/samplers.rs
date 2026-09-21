//! Branch-point samplers for the conformance bundle (v2 F16.3, task 2.4).
//!
//! > Uniform sampling over the state space almost never hits the places a
//! > port actually differs.
//!
//! Every hazard in F16.3's table gets a **named** sampler here, and the name
//! travels into `manifest.json` beside the row count, so a failing row can be
//! traced back to the trap it was written for. Each one emits cases *at*,
//! *around* and **exactly on** its boundary; [`halton`] supplies the
//! low-discrepancy background sweep that fills in everything between.
//!
//! Two rules hold throughout:
//!
//! * **`nextafter`, not epsilon.** "Just past the boundary" means the
//!   adjacent representable number, because a strict `>` and a `>=` differ on
//!   exactly one value and nothing else will find it.
//! * **Both signs, always.** The physics is mirror-symmetric (F5.2, and the
//!   symmetry suite asserts it), so a sampler that only visits `+x` cannot
//!   see a port that got `sign` wrong.
//!
//! Nothing here is a physical coefficient: these are test inputs, and
//! brief §43 governs `parameters.rs`. They are chosen to hit branches, never
//! to make a comparison look better.

use sailgym_physics::foil::EPS_FLOW;
use sailgym_physics::parameters::BoatParameters;
use sailgym_physics::rigging::mainsheet::rope_path_length;
use sailgym_physics::state::{BoatState, Controls};

/// One element of a Halton sequence in `base`, on `[0, 1)`.
///
/// A low-discrepancy sequence rather than an RNG: it is reproducible without
/// a seed, it needs no PCG32 on the reading side (F16.7), and it fills the
/// box far more evenly than 500 pseudo-random draws would.
pub fn halton(index: usize, base: u32) -> f64 {
    let b = base as f64;
    let mut f = 1.0;
    let mut r = 0.0;
    let mut i = index + 1;
    while i > 0 {
        f /= b;
        r += f * ((i % base as usize) as f64);
        i /= base as usize;
    }
    r
}

/// `n` Halton points scaled onto `[lo, hi]`.
pub fn halton_span(n: usize, base: u32, lo: f64, hi: f64) -> Vec<f64> {
    (0..n).map(|i| lo + (hi - lo) * halton(i, base)).collect()
}

/// Exactly `x`, the two adjacent representable numbers, and a ladder of
/// offsets either side of it.
///
/// The `nextafter` pair is the whole point: a strict inequality and a
/// non-strict one agree everywhere except on `x` itself, and a port that got
/// the strictness wrong is invisible to any sampler that only comes close.
pub fn around(x: f64) -> Vec<f64> {
    let mut out = vec![x, next_up(x), next_down(x)];
    for d in [1e-15, 1e-12, 1e-9, 1e-6, 1e-3, 1e-1] {
        out.push(x + d);
        out.push(x - d);
    }
    out
}

/// The next representable `f64` above `x`. Written out rather than taken
/// from `f64::next_up`, which is not stable on the pinned toolchain.
pub fn next_up(x: f64) -> f64 {
    if x.is_nan() || x == f64::INFINITY {
        return x;
    }
    if x == 0.0 {
        return f64::from_bits(1);
    }
    let bits = x.to_bits();
    f64::from_bits(if x > 0.0 { bits + 1 } else { bits - 1 })
}

/// The next representable `f64` below `x`.
pub fn next_down(x: f64) -> f64 {
    -next_up(-x)
}

// ---------------------------------------------------------------------------
// The F16.3 table, one named sampler per row
// ---------------------------------------------------------------------------

/// `wrap_pi` (`frames.rs`): exactly `±π`, the half-open `(−π, π]` convention,
/// and the in-range bit-identical fast path.
///
/// `wrap_pi(π)` must be `π` and `wrap_pi(−π)` must be `π` — the interval is
/// half-open, and that is a decision a port has to make the same way.
pub const WRAP_PI_EDGE: &str = "wrap_pi_edge";
pub fn wrap_pi_edge() -> Vec<f64> {
    use std::f64::consts::{PI, TAU};
    let mut out = Vec::new();
    for c in [
        0.0,
        PI,
        -PI,
        PI / 2.0,
        -PI / 2.0,
        TAU,
        -TAU,
        3.0 * PI,
        -3.0 * PI,
    ] {
        out.extend(around(c));
    }
    // The fast path: values already inside (−π, π] must come back bit
    // identical, so a port that always reduces will differ in the last place.
    out.extend(halton_span(120, 2, -PI, PI));
    // And far outside, where a naive `a - n*2π` loses digits.
    out.extend(halton_span(80, 3, -1000.0 * PI, 1000.0 * PI));
    out.push(-0.0);
    out
}

/// The stall blend (`foil.rs`): `α_stall ± Δ_s`, where two smooth pieces are
/// joined. `smoothstep` clamps at both ends, so the joins are the only places
/// where a port's blend can disagree without being obviously wrong.
pub const STALL_BLEND_EDGE: &str = "stall_blend_edge";
pub fn stall_blend_edge(alpha_stall: f64, stall_blend: f64) -> Vec<f64> {
    let mut out = Vec::new();
    for edge in [alpha_stall, alpha_stall + stall_blend] {
        out.extend(around(edge));
        out.extend(around(-edge));
    }
    // The midpoint of the blend, where `s = 0.5` and both branches carry
    // half the weight.
    out.extend(around(alpha_stall + 0.5 * stall_blend));
    // And the F5.2 landmarks the coefficient properties are stated at.
    for c in [
        0.0,
        std::f64::consts::FRAC_PI_2,
        -std::f64::consts::FRAC_PI_2,
        std::f64::consts::PI,
        -std::f64::consts::PI,
    ] {
        out.extend(around(c));
    }
    out
}

/// The zero-flow guard (`foil.rs`): `|v|` at, just under and just over
/// `EPS_FLOW`.
///
/// `0/0` is a `NaN` in one stack and a guarded zero in the other, and the
/// guard is `v.length_squared() < EPS_FLOW²` — a comparison on the *square*,
/// which is where a port that squares in a different order lands on the other
/// side.
pub const ZERO_FLOW_EPS: &str = "zero_flow_eps";
pub fn zero_flow_eps() -> Vec<f64> {
    let mut out = vec![0.0, -0.0];
    for m in [
        EPS_FLOW,
        next_up(EPS_FLOW),
        next_down(EPS_FLOW),
        0.5 * EPS_FLOW,
        2.0 * EPS_FLOW,
        EPS_FLOW * EPS_FLOW,
        1e-300,
    ] {
        out.push(m);
        out.push(-m);
    }
    out
}

/// `phi` is not wrapped (`state.rs`, brief §17): a port that wraps every
/// angle breaks capsize, so the fixtures visit heel well past `±π`.
pub const PHI_UNWRAPPED: &str = "phi_unwrapped";
pub fn phi_unwrapped() -> Vec<f64> {
    use std::f64::consts::PI;
    let mut out = Vec::new();
    for c in [0.0, PI / 2.0, PI, 3.0 * PI / 2.0, 2.0 * PI, 3.0 * PI] {
        out.push(c);
        out.push(-c);
    }
    out.extend(halton_span(40, 5, -4.0 * PI, 4.0 * PI));
    out
}

/// The low-discrepancy background sweep every fixture carries beside its
/// boundary rows.
pub const HALTON_BACKGROUND: &str = "halton_background";

/// States drawn from the background sweep: plausible sailing, and then some.
///
/// The ranges are wide enough to include a capsized boat and a boat going
/// backwards, because a conformance fixture that only visits the comfortable
/// part of the state space certifies only the comfortable part.
pub fn background_states(n: usize, p: &BoatParameters) -> Vec<BoatState> {
    use std::f64::consts::PI;
    (0..n)
        .map(|i| {
            let h = |base: u32| halton(i, base);
            BoatState {
                x: -500.0 + 1000.0 * h(2),
                y: -500.0 + 1000.0 * h(3),
                psi: -PI + 2.0 * PI * h(5),
                phi: -2.0 * PI + 4.0 * PI * h(7),
                u: -2.0 + 10.0 * h(11),
                v: -3.0 + 6.0 * h(13),
                r: -1.5 + 3.0 * h(17),
                p: -3.0 + 6.0 * h(19),
                beta: -PI + 2.0 * PI * h(23),
                beta_dot: -6.0 + 12.0 * h(29),
                delta_r: -p.rudder.delta_r_max + 2.0 * p.rudder.delta_r_max * h(31),
                l_sheet: p.sheet.l_sheet_min + (p.sheet.l_sheet_max - p.sheet.l_sheet_min) * h(37),
                t: 60.0 * h(41),
            }
        })
        .collect()
}

/// The unilateral sheet (`mainsheet.rs`, v2 F18.1b): `e ≈ 0` on both sides
/// and **exactly on** the boundary, plus the damping-dominated case where the
/// bracket goes negative with `e > 0`.
///
/// `T = if e > 0 { max(0, k·e + c·ė) } else { 0 }` has two branches that a
/// port can get subtly wrong in opposite directions. `e = 0` with a large
/// `ė` is the one that matters: the slack set is **closed**, so an element at
/// its natural length transmits nothing however fast it is being stretched.
pub const SHEET_SLACK_BOUNDARY: &str = "sheet_slack_boundary";

/// `(state, l_sheet_dot)` pairs sitting on and around the take-up boundary.
pub fn sheet_slack_boundary(p: &BoatParameters) -> Vec<(BoatState, f64)> {
    let mut out = Vec::new();
    let base = BoatState {
        l_sheet: p.sheet.l_sheet_min,
        ..BoatState::default()
    };
    for beta in [-1.2, -0.6, -0.2, 0.0, 0.2, 0.6, 1.2] {
        let length = rope_path_length(beta, p);
        // `l_sheet` exactly at the rope path makes `e` exactly zero.
        for l in around(length) {
            let l = l.clamp(p.sheet.l_sheet_min, p.sheet.l_sheet_max);
            for beta_dot in [-4.0, 4.0] {
                for rate in [0.0, -p.sheet.sheet_haul_rate, p.sheet.sheet_release_rate] {
                    out.push((
                        BoatState {
                            beta,
                            beta_dot,
                            l_sheet: l,
                            ..base
                        },
                        rate,
                    ));
                }
            }
        }
    }
    // The damping-dominated case: the rope is genuinely stretched (`e > 0`)
    // but is being eased faster than it is being stretched, so the bracket
    // `k·e + c·ė` goes negative and the `max` — not the `e > 0` test — is
    // what keeps a rope from pushing.
    for e in [1e-9, 1e-6, 1e-3, 1e-2, 5e-2] {
        for beta in [-0.8, 0.0, 0.8] {
            let l = (rope_path_length(beta, p) - e).clamp(p.sheet.l_sheet_min, p.sheet.l_sheet_max);
            let needed = p.sheet.k_sheet * e / p.sheet.c_sheet;
            for rate in [needed, next_up(needed), next_down(needed), 2.0 * needed] {
                out.push((
                    BoatState {
                        beta,
                        beta_dot: 0.0,
                        l_sheet: l,
                        ..base
                    },
                    rate,
                ));
            }
        }
    }
    out
}

/// `limit_rate` (`dynamics.rs`): a **strict** inequality whose strictness is
/// load-bearing and documented at its source.
///
/// The rate is zeroed only when the actuator is *strictly* outside its box.
/// Exactly on the stop the rate survives, which is what lets the integrator's
/// stage saturation land the actuator on the limit for any `dt`. A port with
/// `>=` freezes the actuator a timestep short, and only the exact-boundary
/// row can see it.
pub const LIMIT_RATE_BOUNDARY: &str = "limit_rate_boundary";

/// `delta_r_self_centre` (`dynamics.rs`): a **three-way** `partial_cmp`
/// including the exact-zero case. A two-way `if δr > 0 { … } else { … }` is
/// wrong at zero, where the tiller must not move at all.
pub const RUDDER_SELF_CENTRE_ZERO: &str = "rudder_self_centre_zero";

/// `sheet_release` overrides the analogue command — precedence, not addition.
pub const SHEET_RELEASE_PRECEDENCE: &str = "sheet_release_precedence";

/// `(state, controls)` pairs for the three actuator branches above.
///
/// They share one generator because they share one derivative evaluation:
/// `rudder_rate` and `sheet_rate` are both inside `dynamics::derivative`, and
/// tier 1 is where they are sampled.
pub fn actuator_boundaries(p: &BoatParameters) -> Vec<(BoatState, Controls)> {
    let mut out = Vec::new();
    let st = |delta_r: f64, l_sheet: f64| BoatState {
        u: 3.0,
        delta_r,
        l_sheet,
        ..BoatState::default()
    };
    let ctl = |rudder: f64, sheet: f64, release: bool| Controls {
        rudder_rate_cmd: rudder,
        sheet_rate_cmd: sheet,
        sheet_release: release,
    };

    // limit_rate: the rudder and the sheet exactly on, just inside and just
    // outside each stop, with the command pushing both ways.
    let dmax = p.rudder.delta_r_max;
    for d in [
        dmax,
        next_up(dmax),
        next_down(dmax),
        -dmax,
        next_up(-dmax),
        next_down(-dmax),
    ] {
        for cmd in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            out.push((st(d, p.sheet.l_sheet_min), ctl(cmd, 0.0, false)));
        }
    }
    for l in [
        p.sheet.l_sheet_min,
        next_up(p.sheet.l_sheet_min),
        next_down(p.sheet.l_sheet_min),
        p.sheet.l_sheet_max,
        next_up(p.sheet.l_sheet_max),
        next_down(p.sheet.l_sheet_max),
    ] {
        for cmd in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            out.push((st(0.0, l), ctl(0.0, cmd, false)));
        }
    }

    // delta_r_self_centre: the exact-zero case, both signed zeros, and the
    // two representable neighbours of zero — the three-way comparison's whole
    // domain.
    for d in [
        0.0,
        -0.0,
        next_up(0.0),
        next_down(0.0),
        1e-300,
        -1e-300,
        0.1,
        -0.1,
    ] {
        out.push((st(d, p.sheet.l_sheet_min), ctl(0.0, 0.0, false)));
        // A non-zero command must win over self-centring, at every δr.
        out.push((st(d, p.sheet.l_sheet_min), ctl(0.3, 0.0, false)));
    }

    // sheet_release: precedence over the analogue command, not addition. If a
    // port added them, the release rows with a hauling command would differ.
    let mid = 0.5 * (p.sheet.l_sheet_min + p.sheet.l_sheet_max);
    for cmd in [-1.0, -0.5, 0.0, 0.5, 1.0] {
        out.push((st(0.0, mid), ctl(0.0, cmd, true)));
        out.push((st(0.0, mid), ctl(0.0, cmd, false)));
    }
    // And the clamp of the normalised command, which is `[−1, 1]` (F3).
    for cmd in [-2.0, -1.0, 1.0, 2.0] {
        out.push((st(0.0, mid), ctl(cmd, cmd, false)));
    }
    out
}
