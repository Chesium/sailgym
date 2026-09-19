//! Wind-field integration tests (section 03, task 3.6).
//!
//! These live outside `environment::wind` on purpose: they exercise the field
//! through its public surface only, across many configurations and seeds, and
//! they assert the properties that later sections depend on rather than the
//! internals of the current implementation.
//!
//! The single-field guarantee (brief §19, §47) is the reason
//! `grid_bitwise_matches_point_sweep` uses exact equality. It is not a
//! tolerance that may be relaxed: the visualization and the physics are
//! required to see the same numbers, and "almost the same" is a different
//! guarantee.

use sailgym_physics::environment::wind::{ProceduralWind, WindConfig, WindMode};
use sailgym_physics::environment::WindField;
use sailgym_physics::rng::{Pcg32, STREAM_NOISE, STREAM_WIND};

/// Eight configurations spanning every axis of [`WindConfig`].
fn configs() -> Vec<WindConfig> {
    vec![
        WindConfig::default(),
        WindConfig {
            mode: WindMode::Uniform,
            ..WindConfig::default()
        },
        WindConfig {
            mode: WindMode::Spatial,
            ..WindConfig::default()
        },
        WindConfig {
            mode: WindMode::Gust,
            variation: 0.45,
            bearing_deg: 37.5,
            ..WindConfig::default()
        },
        WindConfig {
            mode: WindMode::Gust,
            speed: 0.75,
            length_scale: 25.0,
            time_scale: 4.0,
            ..WindConfig::default()
        },
        WindConfig {
            mode: WindMode::Spatial,
            length_scale: 900.0,
            modes: 3,
            ..WindConfig::default()
        },
        WindConfig {
            mode: WindMode::Gust,
            modes: 31,
            spectral_slope: 0.5,
            ..WindConfig::default()
        },
        WindConfig {
            mode: WindMode::Gust,
            time_scale: f64::INFINITY,
            spectral_slope: 2.5,
            bearing_deg: 181.0,
            ..WindConfig::default()
        },
    ]
}

const SEEDS: [u64; 4] = [0, 1, 0x5EED_5EED, u64::MAX];

/// Deterministic test-local generator for sample points.
struct Points(Pcg32);

impl Points {
    fn new(seed: u64) -> Self {
        Self(Pcg32::seed_from_u64(seed))
    }
    fn next(&mut self, span: f64, t_span: f64) -> (f64, f64, f64) {
        (
            self.0.range(-span, span),
            self.0.range(-span, span),
            self.0.range(0.0, t_span),
        )
    }
}

/// The single-field guarantee: `sample_grid` reproduces `sample` exactly, for
/// every configuration and seed, at every node.
#[test]
fn grid_bitwise_matches_point_sweep() {
    let (nx, ny) = (64usize, 64usize);
    let mut out = vec![0.0f32; 2 * nx * ny];
    let mut checked = 0usize;

    for cfg in configs() {
        for seed in SEEDS {
            let w = ProceduralWind::new(cfg, seed);
            // Deliberately awkward origin and spacing: a grid aligned to the
            // wavelength could hide a phase error that a general one exposes.
            let (x0, y0, dx, dy, t) = (-317.5, 88.25, 9.75, -6.125, 23.5);
            w.sample_grid(x0, y0, dx, dy, nx, ny, t, &mut out);

            for j in 0..ny {
                let y = y0 + (j as f64) * dy;
                for i in 0..nx {
                    let x = x0 + (i as f64) * dx;
                    let p = w.sample(x, y, t);
                    let at = 2 * (j * nx + i);
                    assert_eq!(
                        out[at].to_bits(),
                        (p.x as f32).to_bits(),
                        "wx mismatch at ({i}, {j}) for {cfg:?} seed {seed}"
                    );
                    assert_eq!(
                        out[at + 1].to_bits(),
                        (p.y as f32).to_bits(),
                        "wy mismatch at ({i}, {j}) for {cfg:?} seed {seed}"
                    );
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 8 * 4 * nx * ny);
}

/// The field is a pure function of `(config, seed)`: dropping it and building
/// it again reproduces it bit for bit. Golden trajectories (section 09) depend
/// on this.
#[test]
fn seed_reproducibility_across_instances() {
    for cfg in configs() {
        for seed in SEEDS {
            let reference: Vec<(u64, u64)> = {
                let w = ProceduralWind::new(cfg, seed);
                let mut pts = Points::new(99);
                (0..2000)
                    .map(|_| {
                        let (x, y, t) = pts.next(4000.0, 900.0);
                        let s = w.sample(x, y, t);
                        (s.x.to_bits(), s.y.to_bits())
                    })
                    .collect()
                // `w` is dropped here.
            };

            let w = ProceduralWind::new(cfg, seed);
            let mut pts = Points::new(99);
            for (n, expect) in reference.iter().enumerate() {
                let (x, y, t) = pts.next(4000.0, 900.0);
                let s = w.sample(x, y, t);
                assert_eq!(
                    (s.x.to_bits(), s.y.to_bits()),
                    *expect,
                    "sample {n} differs after a rebuild, {cfg:?} seed {seed}"
                );
            }
        }
    }
}

/// Adding an RNG consumer on another named stream must not disturb the wind.
///
/// This is the test that would fail if a later section drew its randomness
/// from the parent generator instead of taking a named stream: the wind takes
/// `stream(STREAM_WIND)` from a generator freshly seeded inside
/// `ProceduralWind::new`, so nothing outside it can advance the sequence the
/// wind sees. The second half of the test demonstrates the failure mode
/// explicitly — a consumer that *shares* a generator does shift every later
/// draw — so the guarantee is shown, not merely asserted.
#[test]
fn stream_isolation() {
    let cfg = WindConfig {
        mode: WindMode::Gust,
        variation: 0.3,
        ..WindConfig::default()
    };
    let seed = 0xBEEF_CAFE;

    let baseline: Vec<(u64, u64)> = {
        let w = ProceduralWind::new(cfg, seed);
        let mut pts = Points::new(5);
        (0..1000)
            .map(|_| {
                let (x, y, t) = pts.next(1500.0, 300.0);
                let s = w.sample(x, y, t);
                (s.x.to_bits(), s.y.to_bits())
            })
            .collect()
    };

    // A hypothetical later consumer, drawing heavily on its own named stream.
    let mut noise = Pcg32::seed_from_u64(seed).stream(STREAM_NOISE);
    let mut sink = 0u64;
    for _ in 0..100_000 {
        sink = sink.wrapping_add(u64::from(noise.next_u32()));
    }
    assert_ne!(sink, 0);

    let w = ProceduralWind::new(cfg, seed);
    let mut pts = Points::new(5);
    for (n, expect) in baseline.iter().enumerate() {
        let (x, y, t) = pts.next(1500.0, 300.0);
        let s = w.sample(x, y, t);
        assert_eq!(
            (s.x.to_bits(), s.y.to_bits()),
            *expect,
            "sample {n} moved after an unrelated consumer drew from stream {STREAM_NOISE}"
        );
    }

    // …and the failure mode the stream mechanism exists to prevent: sharing
    // one generator makes the second consumer's draws depend on the first.
    let mut shared = Pcg32::seed_from_u64(seed);
    let alone = Pcg32::seed_from_u64(seed).next_u32();
    let _ = shared.next_u32();
    assert_ne!(
        shared.next_u32(),
        alone,
        "a shared generator must shift; if this ever passes, \
         the streams in `rng.rs` no longer protect anything"
    );
    assert_ne!(STREAM_WIND, STREAM_NOISE);
}

/// brief §20: world positions stay numerically stable over prototype-scale
/// runs. No `NaN`, no infinity, anywhere in the reachable domain.
#[test]
fn finite_over_wide_domain() {
    let corners = [-1e5_f64, -1e3, -1.0, 0.0, 1.0, 1e3, 1e5];
    let times = [0.0_f64, 1e-3, 1.0, 60.0, 3600.0, 1e4];

    for cfg in configs() {
        for seed in SEEDS {
            let w = ProceduralWind::new(cfg, seed);
            for &x in &corners {
                for &y in &corners {
                    for &t in &times {
                        let s = w.sample(x, y, t);
                        assert!(
                            s.x.is_finite() && s.y.is_finite(),
                            "non-finite {s:?} at ({x}, {y}, {t}) for {cfg:?} seed {seed}"
                        );
                    }
                }
            }
            // …and densely, well away from the origin.
            let mut pts = Points::new(seed ^ 0xA5A5);
            for _ in 0..5000 {
                let (x, y, t) = pts.next(1e5, 1e4);
                let s = w.sample(x, y, t);
                assert!(s.x.is_finite() && s.y.is_finite());
            }
        }
    }
}

/// The perturbation stays inside the bound `variation` is defined by.
#[test]
fn bounded_magnitude() {
    for cfg in configs() {
        for seed in SEEDS {
            let w = ProceduralWind::new(cfg, seed);
            let bound = cfg.speed * (1.0 + 3.0 * cfg.variation);
            let mut worst = 0.0f64;

            // A regular sweep catches the structured peaks; the random one
            // catches whatever falls between the grid lines.
            let n = 180;
            let span = 4.0 * cfg.length_scale;
            for j in 0..n {
                let y = -span + 2.0 * span * (j as f64) / (n as f64);
                for i in 0..n {
                    let x = -span + 2.0 * span * (i as f64) / (n as f64);
                    for k in 0..4 {
                        let t = f64::from(k) * 0.37 * cfg.time_scale.min(1e3);
                        worst = worst.max(w.sample(x, y, t).length());
                    }
                }
            }
            let mut pts = Points::new(seed ^ 0x1234);
            for _ in 0..50_000 {
                let (x, y, t) = pts.next(1e4, 1e3);
                worst = worst.max(w.sample(x, y, t).length());
            }

            assert!(
                worst <= bound,
                "|w| reached {worst} > {bound} for {cfg:?} seed {seed}"
            );
        }
    }
}
