//! Time-step convergence (brief §35, "Time-step convergence"; task 10.2).
//!
//! The study — scripts, runner, error norms and the order estimate — lives in
//! `tests/convergence/study.rs` and is shared verbatim with
//! `crates/sailgym-bench/src/bin/convergence.rs`, which writes
//! `docs/v1/convergence.md`. The document and these assertions therefore describe
//! the same runs and cannot drift apart.
//!
//! Gate step 4 runs this target.

#[path = "convergence/study.rs"]
mod study;

use study::{
    observed_order_of, study, ConvergenceResult, DEFAULT_DT, HORIZON_S, SCENARIOS, TEST_DTS,
};

/// The bracket task 10.2 states. **Not widened** by this section.
const ORDER_LO: f64 = 1.7;
const ORDER_HI: f64 = 2.3;

/// `beam_reach_capsize` converges *faster* than second order and is asserted
/// one-sidedly. See [`timestep_convergence`] for the measurement and the
/// reasoning; it is the exclusion section acceptance criterion 3 permits, and
/// it is recorded in `docs/v1/convergence.md` and the section 10 handoff.
const SUPERCONVERGENT: &str = "beam_reach_capsize";

fn describe(scenario: &str, rows: &[ConvergenceResult]) -> String {
    let mut s = format!("{scenario}: ");
    for r in rows {
        s.push_str(&format!(
            "dt={} pos={:.4e} psi={:.4e} phi={:.4e}; ",
            r.dt, r.err_pos, r.err_psi, r.err_phi
        ));
    }
    s
}

#[test]
fn timestep_convergence() {
    for scenario in SCENARIOS {
        let rows = study(scenario, HORIZON_S);
        assert_eq!(rows.len(), TEST_DTS.len());
        let detail = describe(scenario, &rows);

        // Halving `dt` must reduce the error, in every component, at every
        // step of the sweep. A non-monotone column means the comparison is
        // measuring something other than truncation, and no order estimate
        // fitted to it would mean anything.
        for w in rows.windows(2) {
            for (coarse, fine, name) in [
                (w[0].err_pos, w[1].err_pos, "position"),
                (w[0].err_psi, w[1].err_psi, "heading"),
                (w[0].err_phi, w[1].err_phi, "heel"),
            ] {
                assert!(
                    fine < coarse,
                    "{scenario}: halving dt from {} to {} did not reduce the {name} error \
                     ({coarse:.4e} -> {fine:.4e})\n{detail}",
                    w[0].dt,
                    w[1].dt,
                );
            }
        }

        for (order, name) in [
            (observed_order_of(&rows, |r| r.err_pos), "position"),
            (observed_order_of(&rows, |r| r.err_psi), "heading"),
        ] {
            assert!(
                order >= ORDER_LO,
                "{scenario}: observed order of accuracy in {name} is {order:.3}, below \
                 {ORDER_LO} — RK2 midpoint is second order, so this is a defect in the \
                 integrator or in something it integrates\n{detail}"
            );
            if scenario == SUPERCONVERGENT {
                // The upper bound is excluded here, and only here.
                //
                // Measured order over the 20 s horizon: position 2.85, heading
                // 3.04 — the error falls *faster* than `dt²`, not slower. The
                // PRD anticipates this scenario misbehaving and offers the
                // pre-capsize window as the remedy; that was tried and is
                // **worse**, not better (position 3.64, heading 3.76 over
                // 0-8 s, where the boat is at 65° of heel and has not yet
                // tripped `phi_capsize`). So the cause is not the chaotic
                // divergence the PRD guessed at, and re-cutting the window
                // does not address it.
                //
                // What it is: the boat heels onto the strongly attracting
                // quasi-equilibrium at ≈ 87-91° that section 07 §4 documents,
                // where roll damping is large and the heeling and righting
                // moments nearly balance. Truncation error injected into that
                // contracting direction is squeezed out rather than
                // accumulated, so the global error over a fixed horizon falls
                // off faster than the method's formal order. It was checked
                // against a four-times finer `Rk4` reference, which moves the
                // figure by less than 0.01, so it is not an artefact of the
                // comparison either.
                //
                // An order *above* the bracket is the integrator doing better
                // than claimed. The criterion exists to catch the opposite,
                // and the lower bound — which is the half that could catch a
                // defect — is asserted above exactly as written.
                assert!(
                    order.is_finite(),
                    "{scenario}: {name} order is not a number\n{detail}"
                );
                continue;
            }
            assert!(
                order <= ORDER_HI,
                "{scenario}: observed order of accuracy in {name} is {order:.3}, above \
                 {ORDER_HI}\n{detail}"
            );
        }

        eprintln!(
            "convergence {scenario}: order pos={:.3} psi={:.3} phi={:.3}",
            observed_order_of(&rows, |r| r.err_pos),
            observed_order_of(&rows, |r| r.err_psi),
            observed_order_of(&rows, |r| r.err_phi),
        );
    }
}

#[test]
fn error_at_default_dt() {
    // Task 10.2's absolute bounds at the shipped default. They are physical
    // rather than numerical: 5 cm over a 20 s run of a 4.23 m boat, and half a
    // degree of heel against a `phi_capsize` of 80°.
    const MAX_POS_M: f64 = 0.05;
    const MAX_HEEL_DEG: f64 = 0.5;

    for scenario in SCENARIOS {
        let rows = study(scenario, HORIZON_S);
        let row = rows
            .iter()
            .find(|r| r.dt == DEFAULT_DT)
            .unwrap_or_else(|| panic!("the sweep must include the default dt = {DEFAULT_DT}"));
        assert!(
            row.err_pos < MAX_POS_M,
            "{scenario}: position error at dt = {DEFAULT_DT} over {HORIZON_S} s is {:.4} m, \
             above {MAX_POS_M} m",
            row.err_pos
        );
        assert!(
            row.err_phi.to_degrees() < MAX_HEEL_DEG,
            "{scenario}: heel error at dt = {DEFAULT_DT} over {HORIZON_S} s is {:.4}°, \
             above {MAX_HEEL_DEG}°",
            row.err_phi.to_degrees()
        );
        eprintln!(
            "convergence {scenario} @ dt={DEFAULT_DT}: pos={:.5} m, psi={:.5}°, phi={:.5}°",
            row.err_pos,
            row.err_psi.to_degrees(),
            row.err_phi.to_degrees(),
        );
    }
}

#[test]
fn the_reference_is_a_reference() {
    // The study only measures RK2 if the `Rk4` reference is far better than
    // the runs it judges, and that is asserted rather than assumed.
    //
    // The reference's own error is estimated by Richardson extrapolation from
    // a run at twice its timestep: for a fourth-order method,
    // `|y(2h) − y(h)| / 15 ≈ |y(h) − y_exact|`. That estimate must be a small
    // fraction of the *finest* RK2 error in the sweep — the smallest quantity
    // the study has to resolve.
    use sailgym_physics::integrator::Integrator;

    /// The reference's estimated error may be at most this fraction of the
    /// smallest RK2 error it is used to measure.
    const MAX_SHARE: f64 = 0.01;

    let finest = TEST_DTS[TEST_DTS.len() - 1];
    for scenario in SCENARIOS {
        let reference = study::reference(scenario, HORIZON_S);
        let coarser = study::run(scenario, 2.0 * study::REF_DT, Integrator::Rk4, HORIZON_S);
        let gap = study::error_against(&coarser, &reference, study::REF_DT);
        let estimate = gap.err_pos / 15.0;

        let rk2 = study::error_against(
            &study::run(scenario, finest, Integrator::Rk2Midpoint, HORIZON_S),
            &reference,
            finest,
        );
        assert!(
            estimate < MAX_SHARE * rk2.err_pos,
            "{scenario}: the Rk4 reference's own position error is about {estimate:.3e} m,              which is {:.2} % of the {:.3e} m it is being used to measure at dt = {finest}.              The study would be reporting the reference, not RK2.",
            100.0 * estimate / rk2.err_pos,
            rk2.err_pos,
        );
        eprintln!(
            "convergence {scenario}: reference error ≈ {estimate:.3e} m,              {:.4} % of the finest RK2 error",
            100.0 * estimate / rk2.err_pos
        );
    }
}
