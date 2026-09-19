//! Determinism, finiteness and actuator clamping (F9, brief §34, §35).
//!
//! These land at M1 rather than M9 on purpose: retrofitting determinism after
//! the physics exists is far more expensive than maintaining it from the
//! start. Every test here must keep passing for the life of the project.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use sailgym_physics::parameters::BoatParameters;
use sailgym_physics::simulation::Simulation;
use sailgym_physics::state::{BoatState, Controls, STATE_FIELDS, STATE_LEN};

/// Deterministic test-local generator. Nothing here feeds the physics; the
/// simulation's own RNG is `rng.rs` (F9.2, section 03).
struct Lcg(u64);

impl Lcg {
    fn unit(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
    }

    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + self.unit() * (hi - lo)
    }
}

/// A scripted control sequence: `(step_index, controls)`, applied in order.
fn control_script(rng: &mut Lcg, steps: u32, every: u32) -> Vec<(u32, Controls)> {
    (0..steps)
        .step_by(every as usize)
        .map(|i| {
            (
                i,
                Controls {
                    rudder_rate_cmd: rng.range(-1.0, 1.0),
                    sheet_rate_cmd: rng.range(-1.0, 1.0),
                    sheet_release: rng.unit() < 0.1,
                },
            )
        })
        .collect()
}

/// Run `steps` steps of `sim`, applying `script`, recording the flat state at
/// each checkpoint.
fn run(
    sim: &mut Simulation,
    script: &[(u32, Controls)],
    steps: u32,
    checkpoints: &[u32],
) -> Vec<[f64; STATE_LEN]> {
    let mut out = Vec::new();
    let mut next_cmd = 0usize;
    for i in 0..steps {
        while next_cmd < script.len() && script[next_cmd].0 == i {
            sim.set_controls(script[next_cmd].1);
            next_cmd += 1;
        }
        sim.advance(1);
        if checkpoints.contains(&i) {
            out.push(sim.state().to_array());
        }
    }
    out.push(sim.state().to_array());
    out
}

#[test]
fn identical_seed_identical_trajectory() {
    let p = BoatParameters::ilca7();
    let steps = 5000;
    let script = control_script(&mut Lcg(0xD1CE_0001_0002_0003), steps, 37);
    let checkpoints: Vec<u32> = (1..=10).map(|k| k * steps / 11).collect();

    let start = BoatState {
        u: 1.5,
        psi: 0.4,
        ..Simulation::initial_state(&p)
    };

    let trace = |seed: u64| {
        let mut sim = Simulation::new(p, seed);
        sim.reset(start, seed);
        run(&mut sim, &script, steps, &checkpoints)
    };

    let a = trace(4242);
    let b = trace(4242);
    assert_eq!(a.len(), checkpoints.len() + 1);
    for (k, (sa, sb)) in a.iter().zip(b.iter()).enumerate() {
        for i in 0..STATE_LEN {
            assert_eq!(
                sa[i].to_bits(),
                sb[i].to_bits(),
                "checkpoint {k}, field {}: {} vs {}",
                STATE_FIELDS[i],
                sa[i],
                sb[i]
            );
        }
    }
    // The run must actually have gone somewhere, or the assertion is vacuous.
    let last = a.last().expect("at least one sample");
    assert!(last[0].abs() + last[1].abs() > 1.0, "the boat never moved");
}

#[test]
fn advance_batching_invariant() {
    // F9.7: advance(n) == n * advance(1), bit for bit.
    let p = BoatParameters::ilca7();
    let c = Controls {
        rudder_rate_cmd: 0.6,
        sheet_rate_cmd: -0.4,
        sheet_release: false,
    };
    let start = BoatState {
        u: 2.0,
        ..Simulation::initial_state(&p)
    };

    let mut batched = Simulation::new(p, 7);
    batched.reset(start, 7);
    batched.set_controls(c);
    batched.advance(1000);

    let mut single = Simulation::new(p, 7);
    single.reset(start, 7);
    single.set_controls(c);
    for _ in 0..1000 {
        single.advance(1);
    }

    let (a, b) = (batched.state().to_array(), single.state().to_array());
    for i in 0..STATE_LEN {
        assert_eq!(
            a[i].to_bits(),
            b[i].to_bits(),
            "field {}: {} vs {}",
            STATE_FIELDS[i],
            a[i],
            b[i]
        );
    }
    assert_eq!(batched.steps(), single.steps());
}

/// Every `.rs` file under `crates/sailgym-physics/src`.
fn physics_sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("src must be readable") {
            let path = entry.expect("readable entry").path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    walk(&root, &mut out);
    out.sort();
    assert!(out.len() > 10, "expected the whole physics module tree");
    out
}

/// Files containing any of `needles`, as `path -> first offending line`.
fn offenders(needles: &[&str]) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    for path in physics_sources() {
        let src = std::fs::read_to_string(&path).expect("source must be readable");
        for (n, line) in src.lines().enumerate() {
            if let Some(needle) = needles.iter().find(|needle| line.contains(**needle)) {
                found.insert(
                    format!("{}:{}", path.display(), n + 1),
                    format!("{needle} in `{}`", line.trim()),
                );
                break;
            }
        }
    }
    found
}

#[test]
fn no_wall_clock() {
    // F9.1: physics never reads a wall clock, and all randomness is seeded.
    let found = offenders(&["Instant", "SystemTime", "now(", "rand::thread_rng"]);
    assert!(
        found.is_empty(),
        "wall-clock or unseeded randomness: {found:#?}"
    );
}

#[test]
fn no_hash_iteration() {
    // F9.3: no HashMap/HashSet iteration inside physics.
    let found = offenders(&["HashMap", "HashSet"]);
    assert!(found.is_empty(), "hash container in physics: {found:#?}");
}

/// What one randomised episode observed.
#[derive(Clone, Copy, Debug)]
struct EpisodeStats {
    finite: bool,
    max_abs_delta_r: f64,
    min_l_sheet: f64,
    max_l_sheet: f64,
}

/// 200 randomised 30 s episodes, run once and shared by the three tests that
/// inspect them. Deterministic: the seed sequence is fixed.
fn episodes() -> &'static [EpisodeStats] {
    static CACHE: OnceLock<Vec<EpisodeStats>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let p = BoatParameters::ilca7();
        let steps = (30.0 / p.sim.dt).round() as u32;
        let mut rng = Lcg(0x5EED_0000_1111_2222);
        (0..200)
            .map(|_| {
                let start = BoatState {
                    x: rng.range(-100.0, 100.0),
                    y: rng.range(-100.0, 100.0),
                    psi: rng.range(-3.2, 3.2),
                    phi: rng.range(-1.0, 1.0),
                    u: rng.range(-3.0, 3.0),
                    v: rng.range(-1.0, 1.0),
                    r: rng.range(-0.5, 0.5),
                    p: rng.range(-0.5, 0.5),
                    beta: rng.range(-1.7, 1.7),
                    beta_dot: rng.range(-1.0, 1.0),
                    delta_r: rng.range(-p.rudder.delta_r_max, p.rudder.delta_r_max),
                    l_sheet: rng.range(p.sheet.l_sheet_min, p.sheet.l_sheet_max),
                    t: 0.0,
                };
                let script = control_script(&mut rng, steps, 60);
                let mut sim = Simulation::new(p, 1);
                sim.reset(start, 1);

                let mut stats = EpisodeStats {
                    finite: true,
                    max_abs_delta_r: 0.0,
                    min_l_sheet: f64::INFINITY,
                    max_l_sheet: f64::NEG_INFINITY,
                };
                let mut next_cmd = 0usize;
                for i in 0..steps {
                    while next_cmd < script.len() && script[next_cmd].0 == i {
                        sim.set_controls(script[next_cmd].1);
                        next_cmd += 1;
                    }
                    sim.advance(1);
                    let st = sim.state();
                    stats.finite &= st.is_finite();
                    stats.max_abs_delta_r = stats.max_abs_delta_r.max(st.delta_r.abs());
                    stats.min_l_sheet = stats.min_l_sheet.min(st.l_sheet);
                    stats.max_l_sheet = stats.max_l_sheet.max(st.l_sheet);
                }
                stats
            })
            .collect()
    })
}

#[test]
fn finite_under_random_controls() {
    // brief §35: no NaN, no Inf, under valid parameter ranges.
    for (i, e) in episodes().iter().enumerate() {
        assert!(e.finite, "episode {i} produced a NaN or Inf");
    }
}

#[test]
fn rudder_clamped() {
    let p = BoatParameters::ilca7();
    for (i, e) in episodes().iter().enumerate() {
        assert!(
            e.max_abs_delta_r <= p.rudder.delta_r_max + 1e-12,
            "episode {i}: |delta_r| reached {} > {}",
            e.max_abs_delta_r,
            p.rudder.delta_r_max
        );
    }
}

#[test]
fn sheet_length_clamped() {
    let p = BoatParameters::ilca7();
    for (i, e) in episodes().iter().enumerate() {
        assert!(
            e.min_l_sheet >= p.sheet.l_sheet_min - 1e-12,
            "episode {i}: l_sheet fell to {}",
            e.min_l_sheet
        );
        assert!(
            e.max_l_sheet <= p.sheet.l_sheet_max + 1e-12,
            "episode {i}: l_sheet rose to {}",
            e.max_l_sheet
        );
    }
}
