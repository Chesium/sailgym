//! The `rate` adapter: the identity onto `Controls` (section 05 task 5.5).
//!
//! Three scalars, `[−1, 1]^3`:
//!
//! | index | meaning | F3 |
//! |---|---|---|
//! | 0 | rudder rate command, `+1` steers the bow to starboard | `rudder_rate_cmd` |
//! | 1 | sheet rate command, `+1` eases and `−1` hauls | `sheet_rate_cmd` |
//! | 2 | release, `> 0` is released | `sheet_release` |
//!
//! # Why the release flag is a signed scalar
//!
//! F14.5 says every adapter presents `[−1, 1]^k`, and `sheet_release` is a
//! boolean. Encoding it as the **sign** of a third scalar keeps the action
//! space one box — which is what a Gymnasium `Box` space and a policy's output
//! layer both want — and makes the all-zero action mean exactly
//! `Controls::default()`: hands off, nothing commanded. A threshold anywhere
//! but zero would make "do nothing" a number a policy has to learn.
//!
//! # This adapter is the stop condition
//!
//! `rate` is the identity, so **it must reproduce a golden trajectory bit for
//! bit**. If it does not, the funnel is wrong and everything measured through
//! it afterwards is measured through a thing that changes the answer.
//! `tests::rate_reproduces_the_committed_goldens_bit_for_bit` is that
//! assertion, and it is a stop condition rather than a warning: it has no
//! tolerance to loosen (RV32).

use sailgym_physics::parameters::BoatParameters;
use sailgym_physics::state::{BoatState, Controls};

use super::Actuation;

/// The identity adapter.
#[derive(Clone, Copy, Debug, Default)]
pub struct Rate;

impl Rate {
    pub const ID: &'static str = "rate";
    /// Bumped when the denormalisation changes. It has not: this adapter is
    /// the identity and has nothing to change.
    pub const VERSION: u32 = 1;
    pub const DIM: usize = 3;

    /// The normalised action that produces a given `Controls`.
    ///
    /// The inverse of [`Actuation::to_controls`] for this adapter, exact for
    /// any `Controls` already inside F3's normalised range. It exists so that a
    /// recorded hand-flown episode can be replayed **through the funnel**
    /// rather than around it, which is what the golden test does.
    pub fn action_for(c: &Controls) -> [f64; Self::DIM] {
        [
            c.rudder_rate_cmd,
            c.sheet_rate_cmd,
            if c.sheet_release { 1.0 } else { -1.0 },
        ]
    }
}

impl Actuation for Rate {
    fn id(&self) -> &'static str {
        Self::ID
    }

    fn version(&self) -> u32 {
        Self::VERSION
    }

    fn dim(&self) -> usize {
        Self::DIM
    }

    /// The identity. No arithmetic at all: a multiplication by 1.0 would be
    /// exact too, but "no arithmetic" is the property the golden test is
    /// asserting, and it is easier to keep true if it is literally true.
    fn to_controls(&mut self, a: &[f64], _st: &BoatState, _p: &BoatParameters) -> Controls {
        Controls {
            rudder_rate_cmd: a[0],
            sheet_rate_cmd: a[1],
            sheet_release: a[2] > 0.0,
        }
    }

    /// No state to clear.
    fn reset(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actuation::{apply_values, ActionError};
    use sailgym_physics::recording::ToolchainInfo;
    use sailgym_physics::scenario::load_shipped;
    use sailgym_physics::simulation::Simulation;
    use sailgym_physics::state::{STATE_FIELDS, STATE_LEN};
    use serde::Deserialize;
    use std::path::PathBuf;

    /// The six scenarios that carry a committed golden trajectory.
    const GOLDENS: [&str; 6] = [
        "beam_reach_capsize",
        "close_hauled",
        "free_sail",
        "gybe",
        "sheet_release_recovery",
        "tack",
    ];

    /// Just enough of the committed golden file to replay it.
    ///
    /// Deliberately **not** the physics crate's own `Golden` struct, and
    /// deliberately not `#[path]`-included from its test tree: this crate reads
    /// the goldens as *data*, so a change to the generator's private shape
    /// cannot break this test's compilation, and a change to the recorded
    /// numbers breaks it exactly the way it should.
    #[derive(Debug, Deserialize)]
    struct Golden {
        schema_version: u32,
        toolchain: ToolchainInfo,
        dt: f64,
        duration_s: f64,
        sample_hz: f64,
        /// `[t, rudder_rate, sheet_rate, release]` per cue.
        script: Vec<[f64; 4]>,
        fields: Vec<String>,
        samples: Vec<Vec<f64>>,
    }

    fn golden_path(scenario: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../sailgym-physics/tests/golden")
            .join(format!("{scenario}.json"))
    }

    /// How a control cue is delivered: straight onto the simulation, or
    /// through the `rate` adapter.
    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Via {
        Direct,
        Funnel,
    }

    /// Replay a golden's own recorded script, sampling as the generator did.
    ///
    /// The loop is `tests/golden/script.rs::run_golden`'s, restated here rather
    /// than shared, for the reason the struct above is: this crate consumes the
    /// committed artefact, it does not link the tool that wrote it. Cue times
    /// and sample points are counted in **steps**, never in accumulated
    /// floating-point time.
    fn replay(scenario: &str, g: &Golden, via: Via) -> Vec<[f64; STATE_LEN]> {
        let sc = load_shipped(scenario).expect("a shipped scenario");
        let params = sc.to_parameters().expect("a valid catalogue");
        let mut sim = Simulation::new(params, sc.seed);
        sim.load_scenario(&sc).expect("the scenario loads");

        let dt = params.sim.dt;
        assert_eq!(
            dt, g.dt,
            "{scenario}: the golden was recorded at another dt"
        );
        let steps = (g.duration_s / dt).round() as u64;
        let every = (1.0 / (g.sample_hz * dt)).round().max(1.0) as u64;
        let cue_steps: Vec<u64> = g
            .script
            .iter()
            .map(|c| (c[0] / dt).round() as u64)
            .collect();

        let mut adapter = Rate;
        let mut out = Vec::new();
        let mut next_cue = 0usize;

        for i in 0..=steps {
            while next_cue < g.script.len() && cue_steps[next_cue] == i {
                let cue = g.script[next_cue];
                let controls = Controls {
                    rudder_rate_cmd: cue[1],
                    sheet_rate_cmd: cue[2],
                    sheet_release: cue[3] != 0.0,
                };
                let applied = match via {
                    Via::Direct => controls,
                    Via::Funnel => apply_values(
                        &mut adapter,
                        &Rate::action_for(&controls),
                        sim.state(),
                        sim.params(),
                    )
                    .expect("the recorded script is inside the normalised range"),
                };
                sim.set_controls(applied);
                next_cue += 1;
            }
            if i % every == 0 {
                out.push(sim.state().to_array());
            }
            if i < steps {
                sim.advance(1);
            }
        }
        out
    }

    fn load(scenario: &str) -> Golden {
        let path = golden_path(scenario);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("{} is not a golden file: {e}", path.display()))
    }

    /// The property that makes everything after this affordable, asserted on
    /// day one: driving a scenario **through the funnel** gives the same
    /// trajectory as driving it directly, bit for bit.
    ///
    /// This half needs no committed file and no toolchain match, so it is the
    /// half that can never be skipped.
    #[test]
    fn the_funnel_changes_no_bit_of_a_trajectory() {
        for scenario in GOLDENS {
            let g = load(scenario);
            let direct = replay(scenario, &g, Via::Direct);
            let funnelled = replay(scenario, &g, Via::Funnel);
            assert_eq!(direct.len(), funnelled.len());
            for (k, (a, b)) in direct.iter().zip(funnelled.iter()).enumerate() {
                for i in 0..STATE_LEN {
                    assert_eq!(
                        a[i].to_bits(),
                        b[i].to_bits(),
                        "{scenario}: sample {k}, field {}: {} vs {} — the `rate` adapter is \
                         not the identity, and the funnel is wrong (RV32)",
                        STATE_FIELDS[i],
                        a[i],
                        b[i]
                    );
                }
            }
            // The trajectory has to have gone somewhere, or a match proves
            // nothing.
            let last = direct.last().expect("at least one sample");
            assert!(
                last[0].abs() + last[1].abs() > 1e-3,
                "{scenario}: the boat never moved"
            );
        }
    }

    /// The same property against the **committed** goldens: `rate` reproduces
    /// a golden trajectory bit for bit (section acceptance 3).
    ///
    /// R7: a golden file is only evidence about the build that wrote it, so a
    /// toolchain mismatch **skips with a message** rather than failing — the
    /// same branch `tests/regression.rs` has, for the same reason. What it must
    /// never do is pass silently, so the message is the deliverable.
    #[test]
    fn rate_reproduces_the_committed_goldens_bit_for_bit() {
        let current = ToolchainInfo::current();
        let mut compared = 0usize;
        for scenario in GOLDENS {
            let g = load(scenario);
            assert_eq!(g.schema_version, 2, "{scenario}: another golden schema");
            assert_eq!(
                g.fields,
                STATE_FIELDS
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>(),
                "{scenario}: the golden's field order is not the F8.3 layout"
            );
            if g.toolchain != current {
                eprintln!(
                    "skip: {scenario} golden was recorded on a different toolchain.\n  \
                     expected: {}\n  actual:   {}\n  \
                     Golden trajectories are only valid for the build that produced them \
                     (F9, R7).",
                    g.toolchain.describe(),
                    current.describe(),
                );
                continue;
            }

            let produced = replay(scenario, &g, Via::Funnel);
            assert_eq!(
                produced.len(),
                g.samples.len(),
                "{scenario}: {} samples produced against {} recorded",
                produced.len(),
                g.samples.len()
            );
            for (k, (now, then)) in produced.iter().zip(g.samples.iter()).enumerate() {
                assert_eq!(then.len(), STATE_LEN, "{scenario}: sample {k} is malformed");
                for i in 0..STATE_LEN {
                    assert_eq!(
                        now[i].to_bits(),
                        then[i].to_bits(),
                        "{scenario}: sample {k}, field {}: {} vs recorded {} — the `rate` \
                         adapter perturbed a bit, and every committed golden is now wrong \
                         (RV32). This assertion has no tolerance to loosen.",
                        STATE_FIELDS[i],
                        now[i],
                        then[i]
                    );
                }
            }
            compared += 1;
        }
        eprintln!(
            "rate: {compared} of {} goldens compared bit for bit",
            GOLDENS.len()
        );
    }

    #[test]
    fn the_adapter_is_the_identity_onto_controls() {
        let p = BoatParameters::ilca7();
        let st = BoatState::ZERO;
        let mut r = Rate;
        assert_eq!(r.id(), "rate");
        assert_eq!(r.dim(), 3);
        assert_eq!(r.version(), Rate::VERSION);

        // The all-zero action is `Controls::default()`: hands off.
        assert_eq!(
            apply_values(&mut r, &[0.0, 0.0, 0.0], &st, &p),
            Ok(Controls::default())
        );

        // Every value passes through untouched, bit for bit.
        for a in [
            [1.0, -1.0, -1.0],
            [-1.0, 1.0, 1.0],
            [0.375, -0.125, -0.5],
            [-0.2, 0.7, 0.000_1],
        ] {
            let c = apply_values(&mut r, &a, &st, &p).expect("in bounds");
            assert_eq!(c.rudder_rate_cmd.to_bits(), a[0].to_bits());
            assert_eq!(c.sheet_rate_cmd.to_bits(), a[1].to_bits());
            assert_eq!(c.sheet_release, a[2] > 0.0);
            // …and the round trip through `action_for` is exact.
            let back = Rate::action_for(&c);
            assert_eq!(back[0].to_bits(), a[0].to_bits());
            assert_eq!(back[1].to_bits(), a[1].to_bits());
            assert_eq!(back[2] > 0.0, a[2] > 0.0);
        }
    }

    #[test]
    fn the_declared_dim_matches_the_emitted_action_width() {
        let r = Rate;
        assert_eq!(Rate::action_for(&Controls::default()).len(), r.dim());
        let mut r = Rate;
        assert_eq!(
            apply_values(
                &mut r,
                &[0.0, 0.0],
                &BoatState::ZERO,
                &BoatParameters::ilca7()
            ),
            Err(ActionError::WrongDim {
                adapter: "rate",
                got: 2,
                want: 3
            })
        );
    }
}
