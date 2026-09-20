# Physics and architecture tightening

Updated 2026-09-20. These are source-review findings, not a real-boat calibration study. [Section 08](../prds/08-physics-consistency.md) specifies the corrections and validation.

1. The three-harmonic GZ fit constrains initial slope, a value at `phi_peak` and a zero at `phi_vanish`. It does not force zero slope at the named peak, and the default curve can regain positive righting beyond vanishing. Validate actual extrema, roots and the full supported heel domain. Keep righting moment, derivative and energy integral consistent.
2. The 0.9 m minimum sheet length is below the configured shortest rope path, about 1.0404 m. At 20 kN/m this implies roughly 2.8 kN extension load before deliberate trim. Resolve geometry/initialization and document any intentional preload; do not conceal it with damping.
3. `max(0, k*e+c*edot)` does not guarantee zero tension for slack `e<0`. Guard slack explicitly, decide the exact boundary, and test transitions, convergence and energy behavior.

Resolve these contracts before tuning task thresholds or generating conformance fixtures. Goldens preserve behavior; they do not establish physical truth. Keep before/after evidence with source identity, resolved parameters, dt and initial conditions. Section 08 chooses replacement defaults from that evidence, not from tutorial difficulty.

Reuse the Rust simulation/recorder, web reducers and SVG renderer. Centralize input composition; give replay one consistent data source; keep task evaluation outside physics. Section 10's complete model/task/action/observation identity should serve future conformance and environment work. A parameter-only digest cannot identify changed equations.

Planing, crew righting, sail immersion and quantitative ILCA certification remain in [deferred features](deferred-features.md).
