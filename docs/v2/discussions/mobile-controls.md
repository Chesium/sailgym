# Mobile controls — rate-first milestone

Updated 2026-09-20. This supersedes the position-track/hand-model-first proposal. [PRD 09](../prds/09-touch-readability.md) defines implementation; advanced experiments remain in [deferred features](deferred-features.md).

## First release

Two independent rate pads: horizontal rudder and vertical mainsheet, relative to the initial grab. Show actual rudder angle/sheet length separately from rate-command feedback. Add a large labeled hold-to-release button. Lifting ends input; it does not dump the sheet. Preserve keyboard/mouse parity and start new users in sail mode.

The Rust core already limits haul, ease, release and rudder motion. Send normalized Controls and read limits/rates from current parameters. Full-rate travel is `(max-min)/rate`; derive it after 08's geometry correction. Historical 2.4-second haul and 0.6-second release figures are not fixed targets. Rate travel, position-servo settling and human regripping are different times.

## Input and layout

Each pad owns one pointerId with capture. Two fingers steer and trim independently; unrelated pointerup events cannot cancel them. Clear transient input on cancel, lost capture, blur, hidden tab, pause, reset, scenario change, replay entry and unmount. One composition path resolves touch/mouse/keyboard priority and release precedence.

Use relative grabs, safe areas and at least 44-pixel targets. Keep camera coordinates synchronized on rotation. Preserve deliberate debug preferences while leaving new-user sailing uncluttered. Reduce wind-line density/contrast without changing physical wind. Test small portrait, landscape and desktop views; emulation is not real-device evidence.

## Why position control waits

Zero rudder-rate command currently invokes self-centering. A P controller reaching zero error cannot hold a nonzero rudder angle. Do not hide this with epsilon commands, state teleportation or toggling self-centering per frame. A future shared Rust adapter must distinguish engaged hold from released, run on simulation cadence and log gains/version. Sheet-target feedback also belongs on simulation cadence, not pointer-event cadence.

Target-versus-actual markers suit that later position mode. A rate command and a position gauge have different units and should not appear as a single position error.

## Advanced feel, separately scoped

Regripping, finite hand strokes, flick release and cleat toggles wait for usability evidence. A ratchet reduces holding effort; it is not a hands-free cleat. Effective rope path is not hand travel through a detailed tackle. [Harken reference](https://gallery.harken.com/gallery/84df8981-aaeb-43f1-8d0a-8ece55031a00.pdf).

Tension-dependent slip or haul limits change the effective plant even in TypeScript and require physical validation. Tilt is not hiking until crew position/restoring moments exist. Optional haptics may signal events, but browser vibration specifies duration/pause patterns rather than force/intensity and cannot be required. [Vibration API](https://developer.mozilla.org/en-US/docs/Web/API/Navigator/vibrate).

## Sequence

08 physics, 09 rate controls/readability, 10 truthful replay, 11 guided practice. Position adapters follow an explicit engagement contract; hand gestures follow device testing. Detailed tackle, hand forces and hiking remain separate deferred features.
