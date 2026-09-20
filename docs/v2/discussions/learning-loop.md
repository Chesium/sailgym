# Learning loop — what to borrow from Vibesail

Updated 2026-09-20. Sailgym should make cause and effect inspectable: **goal → sail → result → inspect → same-conditions retry**. This recommendation comes from the repository/browser review, not a claim of validated real-boat accuracy.

Vibesail offers useful precedents for [guided progression](https://vibesail.com/blog/changelog-059-tutorial-ui-and-guided-race/), [compact controls](https://vibesail.com/blog/changelog-074-cleaner-game-controls/) and [replay inspection](https://vibesail.com/blog/changelog-063-daily-race-replay-spectator-mode/). Its [removal of ghost collisions](https://vibesail.com/blog/changelog-073-race-collisions-removed/) supports trying recorded visual ghosts before interacting fleets. Borrow these interaction lessons; photorealism and a full 3-D world are outside Sailgym's direction.

M-next uses current scenarios for three challenges: get moving, complete a tack, recover from excessive heel before capsize. Present one instruction, measurable progress and one useful result. Inspect the relevant recorded moment, then retry with the exact model, parameters, initial state and wind seed. Keep two attempts in memory; no account or leaderboard is needed.

The immediate blocker is truthful replay: App selects recorded pose while other consumers can still show live diagnostics. Section 10 fixes every consumer and labels unavailable legacy fields. Section 11 scores physics-step events, not interpolated poses or browser frames. Recovery must not imply physically righting a capsized boat.

After M-next, add a recorded ghost and one short course with a rule sailor. Existing course/agent/env PRDs are research infrastructure; the browser integration still needs a bounded PRD.
