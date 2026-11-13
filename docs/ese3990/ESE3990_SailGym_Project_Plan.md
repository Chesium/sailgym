# Learning versus Planning for Sailboat Control

**ESE3990 project plan · Shimin Chen · revised 2026-11-13**  
**Status:** M0–M4 are complete (see the [M0](M0_report.md), [M1](M1_report.md), [M2](M2_report.md), [M3](M3_report.md) and [M4](M4_report.md) reports). The frozen R comparison includes three PPO/SAC training seeds, three MPC search seeds and RuleSailor; all 2,840 final-evaluation episodes replay exactly. **M4U** (upwind comparison with an expert-data arm, Nov 14–28) is planned before M5; final presentation/packaging remains M5. PPO/SAC retain the common 1M-decision main budget; prior unequal pilot resources and MPC's exact-model privilege remain explicit.
**Working question:** In a fast physics-based sailing simulator, how do on-policy learning, off-policy learning, and decision-time planning trade off task success, experience, computation, and robustness?

The main comparison is **from-scratch PPO versus from-scratch SAC versus true-model CEM-MPC**, with the existing RuleSailor as an engineering reference. SailGym is the sole environment. **BPTT through a learned world model is retained as the final stretch goal, after the core experiments, analysis, reproducibility work, and presentation are complete.**

This replaces the pendulum-first scope in the [October proposal](ESE3990_Final_Project_Proposal_Draft.md) and the earlier PPO/BPTT version of this plan. It prioritizes explainable controller behavior and measured learning/planning tradeoffs. SuperMimic motivates the final extension rather than determining the required implementation.

## 1. What constitutes a finished project

| Priority | Deliverable | Completion condition |
|---|---|---|
| Required | Fixed reaching benchmark | Frozen task/reward/condition manifests and random/RuleSailor outcome reports |
| Required | PPO | Own implementation; checked action likelihoods, GAE and updates; at least 3 independent training seeds |
| Required | SAC | Own implementation; checked replay, soft value targets, actor/temperature updates and target networks; same training budgets as PPO |
| Required | True-model CEM-MPC | Correct isolated branches of the current episode, closed-loop control, and measured planning cost |
| Required | Explanation of tradeoffs | Learning curves, controller outcome table, MPC horizon/budget study, decision latency, and failure trajectories |
| Required | Reproducible presentation | Saved checkpoints/configs, raw summaries, figure commands, and a presentation ready for Dec 3 |
| Gated extension | Upwind sailing | Same evaluation pipeline; implement only after the reaching comparison works for all three methods. Scheduled as M4U (§6), with a labeled expert-data arm |
| Final stretch | Learned dynamics and BPTT-WM | Start only after all selected core/extension results and presentation artifacts are finished; see §9 |
| Deferred | Further algorithms and infrastructure | No TD3/Dreamer sweep, recurrent policy, full-course racing, or new browser dashboard |

The core is complete without a world model. A valid failed controller run is a result if its implementation is checked and its failures are reported. A missing SAC or MPC implementation is reduced scope, not a completed three-method comparison. The comparison concerns the implemented systems under declared resources; it does not isolate a single algorithmic mechanism.

## 2. Evidence and course alignment

The following sources ground this plan. Existing measurements are context, not new results.

| Source | Implication for this project |
|---|---|
| [Foundations slides](ESE3990_slides_L1-L13_foundations.pdf), PDF pp. 15–16 | Implement the algorithm yourself; public-implementation comparison is useful but optional; explain implementation and results in Q&A |
| [Advanced slides](ESE3990_slides_L14-L19_advanced_topics.pdf), PDF p. 3 | Nov 12 intermediate presentation and **Dec 3 final presentation**. Plan to Dec 3; verify any later instructor announcement separately |
| Advanced slides, off-policy/model-based sections; PDF pp. 246–259 for differentiable control | SAC and decision-time planning ground the core; SHAC is retained for the stretch |
| [SuperMimic](SuperMimic.pdf), §3 and Algorithm A.1 | Collect real transitions, fit a differentiable model, improve a policy through short imagined rollouts with a critic |
| [Sutton & Barto](RL_Book_Sutton&Barto.pdf), ch. 8 and policy-gradient material | Dyna, model bias, and actor–critic framing |
| [Transcript](../../ref/chatgpt-transcript.md), lines 6332–6821 | Sailing task design, reward-independent evaluation, information fairness, and runtime tradeoffs |
| [Baseline validation](../v2/baseline-validation.md) | RuleSailor completes the shipped courses under default conditions, but is less reliable under wider initial-heading sweeps |

The intermediate date has passed as of this plan. Do not assume that any earlier milestone was completed. Inventory actual work in M0 and use existing presentation feedback if available.

### What the repository already provides

| Capability | Verified location | Consequence |
|---|---|---|
| Fixed-step Rust boat physics | [`Simulation`](../../crates/sailgym-physics/src/simulation.rs) | Reuse it unchanged for every real rollout; no JAX rewrite of boat forces |
| Physical state layout | [`state.rs`](../../crates/sailgym-physics/src/state.rs) | `STATE_FIELDS` names 13 scalars; use Rust's ordering and names |
| Episode configuration and outcomes | [`episode.rs`](../../crates/sailgym-env/src/episode.rs), [`outcome.rs`](../../crates/sailgym-env/src/outcome.rs) | Route completion and failures remain Rust decisions |
| Python single/vector environments | [`env.py`](../../python/sailgym/env.py), [`vector.py`](../../python/sailgym/vector.py) | Keep the existing Gymnasium boundary and reset semantics |
| Scenario JSON and route JSON inputs | [`Spec`](../../crates/sailgym-py/src/spec.rs) | Initial conditions and uniform wind can be specified without building a new configuration system |
| Rule-based controller | [`rule_sailor.rs`](../../crates/sailgym-agent/src/pilot/rule_sailor.rs) | Reuse the implementation; add a narrow Python adapter rather than translating it |
| Course construction | [`catalogue.rs`](../../crates/sailgym-course/src/catalogue.rs), [`route.rs`](../../crates/sailgym-course/src/route.rs) | Course documents and serialized `Route` are different schemas; construct routes in Rust |
| Outcome reports and action replay | [`evaluate.rs`](../../crates/sailgym-env/src/evaluate.rs), [`decision_log.rs`](../../crates/sailgym-env/src/decision_log.rs) | Reuse outcome definitions and deterministic replay; Python export still needs plumbing |

A local runtime probe found JAX 0.11.2, Gymnasium 1.3.0, and a working `sailgym_core`. The default spec reports 18 observation fields, 3 actions, and dt = 0.005 s. **Optax and matplotlib were neither installed in the inspected environment nor declared in the root project.** Add and lock them when implementation begins; do not assume the October draft's setup is complete.

The Python API currently exposes observations and outcome metadata, but not physical-state arrays, RuleSailor, or arbitrary simulator snapshots. Those are implementation tasks, not existing features. Raw simulator throughput is not a forecast of end-to-end RL training speed.

## 3. Freeze the sailing problem before comparing methods

### 3.1 Tasks and difficulty

Both tasks use the ILCA defaults and `free_sail` as the scenario template: at rest, heading east, uniform 5 m/s wind from north. `close_hauled` is a different scenario with a 3.5 m/s westerly; do not substitute it silently.

| Task | Geometry | Budget | Role |
|---|---|---|---|
| **R: reach one gate** | Start `(0,0)`, first waypoint `(30,0)`, half-width 5 m, one lap | ~~90~~ **45** simulated seconds (frozen at M0; see [M0 report](M0_report.md) §6) | Required learning/planning comparison and MPC study |
| **U: reach an upwind gate** | Start `(0,0)`, waypoint `(0,40)`, half-width 5 m, one lap | 180 simulated seconds | Gated extension for long-horizon maneuvering |

Construct each with the existing `Route::waypoints` path, preserving directed crossing and mark-miss behavior. R is the first leg of `courses/reach.json`; U is the first leg of `courses/windward_leeward.json`. They remain ordinary SailGym routes. A single gate keeps navigation and outcome evaluation small while the controllers are established.

Use an initial rectangular sailing area of x ∈ [−80,120] m and y ∈ [−80,100] m. M0 must verify that the action-restricted RuleSailor can finish the nominal tasks within the proposed bounds and budgets. Adjust geometry/budgets only during M0, record the reason, and freeze before learning curves. A gate must not be replaced by an easier Python distance-threshold success rule.

**Task U is a separate experiment.** Train PPO and SAC from scratch on U with the same initialization protocol if it is enabled, and evaluate CEM-MPC under the declared planning budget. Do not compare transfer-initialized and freshly initialized learners without labeling that difference. Full shipped multi-leg courses are a later demonstration only if a correct leg-transition treatment is implemented; they are not required.

### 3.2 Timing and action contract

- Read dt and cadence from `Spec`. Default: 10 physics steps per decision = 0.05 s = 20 Hz.
- Learners output two rates in [−1,1]: rudder and sheet. Append **−1** for the release flag at the shared action adapter.
- Apply the same release restriction to random control, RuleSailor, data collection, learned policies, and every MPC live/candidate action. Historical RuleSailor numbers with release available are not measurements of this restricted benchmark.
- Keep the physical actuator limits and rate semantics. Do not interpret actions as rudder angle or sheet length.
- Use `max_steps` in physics steps: derive it from the simulated-time budget and dt, with divisibility by cadence checked. Log actual executed steps, including shortened terminal decisions.
- Begin with autoreset **Disabled**, explicit resets, and a modest batch of single environments. Add the native vector path only if the measured collector cost justifies it, preserving terminal state and reward parity.

### 3.3 Information: controlled, privileged-state study

The required experiment studies learned policies and known-model planning under uniform wind. It does not simultaneously attempt state estimation from the default sensor suite.

Expose a read-only research state through the existing binding. Its schema must include Rust field names/units/version, the 13 physical state values, current controls, and the capsize observer's pending threshold duration. Supply the frozen route geometry, bounds and uniform true-wind vector as task context. Outcome, executed steps and final state must correspond to the transition just collected, before any reset.

**Important distinction:** `BoatState` is enough for the smooth physical evolution under fixed context, but is not an entire episode snapshot. Capsize uses a private pending-crossing timer; `CapsizeState.since` is not populated during all pending crossings. Add a read-only elapsed-duration accessor if necessary rather than guessing it from decision-rate heel samples. Do not call a 13-value export a fully restorable environment state.

PPO and SAC receive the **same actor feature vector**, derived from this research state and the fixed goal: target-relative position, heading, heel, velocities/rates, boom state, actuator state, capsize duration, and wind. Convert wrapped yaw and boom angles to sine/cosine; retain unwrapped heel. Exclude absolute episode time from a stationary actor in uniform wind. Freeze normalization from the common calibration data in §4.5 and store its values. MPC uses the same physical context for scoring, plus the opaque episode branch state described in §4.3.

RuleSailor continues to use its existing sensor inputs. It has less information and internal maneuver memory, so label it an engineering reference, not an information-matched learning ablation. Resolve all sensor columns by their names. Neither learner gets a privileged field unavailable to the other.

Spatial/gusty wind and sensor-only policies are deferred: local true wind alone is not a sufficient description of a hidden procedural field's future evolution.

### 3.4 Reward and terminal handling

Use one experiment reward wrapper with access to the previous and ending state and Rust outcome. The existing observation-only `reward_fn` is insufficient for this transition reward. Keep reward code out of the physics crate.

Proposed initial contract, to freeze in M0:

$$
r_t=-\Delta t_t/30
 +10\,\mathbf1[\mathrm{finished}]
 -10\,\mathbf1[\mathrm{failed}]
 +\gamma\Phi(s_{t+1})-\Phi(s_t),
\qquad \Phi(s)=-d(s,\mathrm{target})/30,\quad\gamma=0.999.
$$

Distances are in metres and elapsed time in seconds. `failed` means a Rust capsize, out-of-bounds, or mark-miss termination. Set the potential of genuine terminal states, including success, to zero. Keep the ending state's potential at a time-limit truncation and bootstrap from its value. This is a discounted task objective with potential shaping; it is not exactly a minimum-time objective.

No extra heel or action-smoothness reward penalties in the required comparison. Measure those behaviors separately. This keeps reward design from becoming an additional experiment.

Store separate masks for value bootstrap and trajectory continuation. Success/failure have no bootstrap. Time-limit truncations bootstrap from the actual final state, but GAE must not continue into the reset episode. A zero-physics-step autoreset record is not a training transition. Copy retained vector buffers. Include hand-calculated tests for all these cases.

## 4. Required controllers

### 4.1 PPO: learn from fresh rollout batches

Implement Gaussian-policy PPO with tanh-transformed actions, GAE, clipped policy objective, critic regression, advantage normalization and gradient clipping. Store pre-transform actions or otherwise compute consistent transformed-action likelihoods. PPO can optimize a collected batch for several epochs; it does not reuse a long-lived replay buffer as SAC does.

Use a 2 × 64 tanh MLP for actor and critic initially. Verify log-probability ratios are one before an update, hand-calculate a short GAE example, and confirm finite reproducible updates on SailGym. A public PPO implementation on the same task is optional verification, not the project implementation or another environment.

### 4.2 SAC: learn from replayed experience

Implement the modern SAC variant with a tanh-transformed stochastic actor, two Q networks, slowly updated target Q networks, and an entropy temperature. Share the actor feature transform and initial hidden widths with PPO; Q networks additionally take the two executed action rates. Keep observations/actions/rewards/final observations and the termination/truncation distinction in replay.

The target is `r + gamma * bootstrap * (min(target_Q1, target_Q2) - alpha * log_pi(next_action))`, with the bracketed term detached. Bootstrap after time limits from their final state, not the reset state. The actor minimizes `alpha * log_pi(action) - min(Q1, Q2)`. Stop Q-parameter updates during the actor step while preserving the Q gradient with respect to action. Use a detached entropy error for temperature fitting and test the sign of the update.

Use Polyak averaging for target Q parameters. Record the target entropy, temperature convention, update-to-data ratio and exploration warmup. These are implementation choices that must be explainable in Q&A. SAC's entropy-regularized training objective differs from PPO's objective; compare task outcomes rather than interpreting their losses or training returns as interchangeable.

Tests: hand-check a soft Bellman target, verify terminal/truncated masks, check a Polyak update, ensure replay owns its data, and verify actor/Q/temperature gradient destinations. Log actor loss, both Q losses, alpha, action saturation and Q scale. Do not interpret poor control as a theoretical SAC weakness until these checks pass.

### 4.3 CEM-MPC: plan with the actual Rust simulator

At each 20 Hz decision, branch from the live episode, sample candidate rate sequences, roll them forward with Rust physics, rank discounted reward, refit a sampling distribution to elite candidates, and execute only the first action. Repeat at the next decision. CEM is derivative-free; it needs neither neural dynamics nor differentiable boat physics.

Start with action blocking: one pair of rates per 0.5 s block, expanded to the same 0.05 s decision cadence used by the learners. This reduces search dimension without changing how often the live controller can replan. Reject inconsistent block/cadence settings. Warm-start by shifting the previous expanded plan by one decision, extending its tail and projecting it back to block means. Reset the plan at each episode boundary.

Optimize the same discounted shaped reward as the learners. Stop candidate rollouts at failure, finish, or the live episode's remaining step budget; preserve shortened terminal decisions and never simulate through a reset. A time limit is not a failure bonus/penalty. Initially set the nonterminal tail value to zero: a finite lookahead approximates the longer-horizon learning objective. Do not import a PPO/SAC critic into the required planner. A later terminal heuristic would be a separately declared ablation, not an unreported fix.

Have the native batch API return decision-end states, outcomes, executed durations and validity masks. Score them with a vectorized form of the same Python experiment reward used for live transitions, checked against its scalar path. This keeps physics stepping in Rust and avoids implementing the reward twice. Include transfer/scoring overhead in planner latency.

#### Branching is a required early feasibility check

The current Python episode interface cannot fork an arbitrary live episode. A 13-field state export or `Simulation::reset` does not preserve capsize history, route tracking, step budgets and all other future-relevant state. Add a narrow Rust planning branch/rollout API that reuses the existing action adapter, physics stepping, route passage and outcome logic.

Audit the copied state explicitly: simulation parameters/controls/wind/seed and capsize observer, route tracker and previous position, miss flag, episode counters/outcome, and any RNG/sensor state that affects returned observations or future evolution. Recording and old log arrays need not be copied into each hypothetical branch. Keep the live episode isolated from candidate actions. Branches are manually driven and do not need to clone a learned or rule controller's internal policy state.

`Simulation` is not currently `Clone`; `Episode` contains trait objects, so adding a blanket derive is not a complete design. Choose the smallest correct native branch implementation after inspecting the owned fields. Reuse the existing step path; do not create a second Python implementation of capsizes, gate passage or actuator physics. Put candidate batches in Rust before considering parallel execution; clone branches without serializing through JSON in the hot loop.

Required branch checks: compare identical future action sequences from the branch and deterministic reset/replay of the live prefix; cover pending capsize duration, a near-gate state, failure/finish, and a nearly exhausted step budget. Candidate order and candidate evaluation must leave the live trajectory unchanged. A reset/replay implementation is acceptable as a correctness reference or explicitly slow fallback, not assumed to meet the planner runtime budget.

#### Planning cost and horizon limits

For K candidates, I CEM iterations, H decisions and cadence C, the upper-bound work is `K * I * H * C` physics steps per live decision, before early stops and elite reuse. Record actual work. Increasing H may improve foresight while making optimization harder and more expensive; it is not automatically beneficial.

The 20 Hz real-time target is **50 ms per decision**. Measure end-to-end p50/p95/max latency and the fraction above 50 ms. Simulation can pause while planning, so a slow controller can still be evaluated offline, but it must be labeled non-real-time. Preserve identical simulated action cadence across methods; do not silently give MPC longer action holds to make latency look acceptable.

### 4.4 RuleSailor and random control

Bind the existing RuleSailor reset/decide contract rather than translating it. Preserve maneuver memory and the current layout/cadence; reset it with the episode. Apply the same release-off action restriction as every other controller. Its smaller sensor set, hand tuning and internal memory make it an engineering reference, not an information-matched learning ablation.

Random control is the environment sanity check. Use a versioned smooth-random rate schedule for calibration, holding each command for several decisions. It receives no special release or safety controller.

### 4.5 Concrete pilot starting values

These are unvalidated starting values. Freeze any changes using validation evidence before final runs. Use JAX/Optax for PPO/SAC; candidate physics rollouts stay in Rust and scoring shares the experiment reward. There is no new learned dynamics model in the core.

| Setting | Initial value |
|---|---|
| Actor architecture | Same 2 × 64 tanh hidden layers for PPO and SAC |
| Value/Q architectures | PPO value: 2 × 64; each SAC Q: 2 × 64 with state/action inputs |
| Optimizer | Adam 3e−4; log gradient norms and any clipping, initially cap at 1.0 |
| Shared calibration | 10,000 smooth-random real decisions per training seed; identical data for both learners |
| PPO updates | 2,048 new decisions per rollout; minibatch 256; 4 epochs; clip 0.2; GAE lambda 0.95 |
| PPO losses | Value coefficient 0.5; entropy coefficient 0.001; initial log standard deviation −0.5 |
| SAC replay/updates | Capacity 100k transitions; minibatch 256; one gradient update per real decision after warmup |
| SAC targets | Polyak tau 0.005; gamma from the shared task contract |
| SAC entropy | Learn alpha, initially 0.2; target entropy −2 in normalized two-action coordinates |
| MPC baseline | H = 40 decisions = 2 s; block 10 decisions; K = 64; I = 3; retain 8 elites |
| CEM distribution | Clipped Gaussian rate proposals; initial standard deviation 0.6; standard-deviation floor 0.1 |
| MPC tail | Zero nonterminal tail value; same true terminal reward/outcome semantics |

**M3 freeze:** the five-setting validation sweep selected H=40, K=32, I=3
(four elites), retaining the other settings above. The baseline row remains
the original starting point. See [`configs/M3_freeze.json`](../../experiments/ese3990/configs/M3_freeze.json)
and the [M3 measurements](M3_report.md); `evaluate.py --controller mpc` loads
the frozen default.

Freeze feature normalization from the common calibration data, training split only, and save its values. PPO uses calibration for normalization; SAC additionally puts the same transitions in replay. Charge calibration to each learner's budget. This is a disclosed difference in how the algorithms use available experience, not an uncontrolled extra dataset for SAC. Neither receives expert demonstrations in the core.

## 5. Experiments, fairness and budgets

### E0 · Correctness and integration

- Identical scenario/seed/action log reproduces Rust state and outcomes; state reads and hypothetical branches do not change the live episode.
- Rust decides directed gate completion, misses, capsize, bounds and time limits. Test these at episode boundaries, not just ordinary transitions.
- PPO numerical checks, SAC numerical checks, and CEM candidate ranking/distribution updates pass small deterministic examples.
- Single/batch collectors agree if batching is enabled; reset records, final states and retained buffers obey the declared contract.
- Restricted RuleSailor finishes nominal R. Measure its evaluation reports under the frozen conditions rather than importing historical times.

A CEM check can use a tiny deterministic score array to verify elite selection and a fixed seed to verify reproducibility. Branch-versus-replay checks establish the planner uses the real environment semantics; a visually plausible path is insufficient.

### E1 · PPO versus SAC learning on R

At least **3 independent training seeds** `[0,1,2]`, targeting `[0,1,2,3,4]` if runtime permits. Initial final budget: **500,000 real decisions per run**, including calibration. Evaluate saved checkpoints at 10k, 100k, 250k and 500k. Give both algorithms the same condition distribution, actor information, cadence, reward and tuning allowance.

Allow at most three 100k-decision pilot configurations per learner on validation conditions. Record pilot cost separately. Preserve diverged runs and their original seeds. A common extension to 1M decisions or reduction to 250k must be chosen before final runs using measured runtime, not test rankings.

Six main learner runs cost **3M real decisions**; ten runs cost 5M. Enabling U at three seeds adds six learner runs and 3M decisions. Calibration is included; pilots and evaluation are additional. The core no longer requires six extra BPTT horizon training runs.

Benchmark a 100k-decision pilot including compilation, collection and optimization. Use a two-hour target per 500k run as an initial scheduling gate, not a performance claim. Reduce common budgets before dropping independent seeds. GPU/cloud provisioning is not assumed or authorized by this document.

### E2 · MPC planning budget and horizon on R

Choose one validation-selected default planner and run it on the same final condition set as the learners. Repeat stochastic CEM search with three declared optimizer seeds; these are search repetitions, not training seeds.

Keep the sweep bounded. At fixed K = 64 and I = 3, compare H = 40/80/160 decisions (2/4/8 s). At default H = 40 and I = 3, compare K = 32/64/128, with elite count `max(2, ceil(K/8))`. This is five distinct settings including the shared baseline, **not** a full cross product. Run the sweep on the same 20 validation conditions and three search seeds before choosing the default; retain the whole sweep as evidence. Repeat selected extremes on final conditions only if needed for a predeclared final figure.

Record success, conditional completion time, actual hypothetical physics steps and p50/p95/max decision latency. Keep the block duration, objective and terminal-value convention fixed. Report the measured success/cost frontier; a planner that succeeds only offline remains a useful but clearly qualified result.

### E3 · Held-out and shifted-condition outcomes

Freeze explicit scenario documents, not just seed integers:

- Nominal debugging: original `free_sail`, uniform 5 m/s wind from north, east heading.
- Training: uniform wind 4.5–5.5 m/s, wind bearing within ±5° of north, initial heading within ±15° of east; store the sampling/seed mapping.
- Validation: 20 disjoint condition documents within the training range.
- Final test: 50 disjoint documents within that range; separate 20-condition shift set at speeds 4/6 m/s and heading offsets up to ±30°.

Generate manifests in M0 and verify unique document content across splits. Different reset seeds alone do not vary uniform wind or initial heading. Change scenario JSON explicitly. Begin with modest single-env collection, batched network inference where useful, and explicit resets; use native vector stepping only if the runtime evidence warrants it.

Tune and choose checkpoints on validation only. Keep final tests closed until methods/settings are frozen. Evaluate all predeclared saved checkpoints together after that freeze to draw test curves without feeding results back into training. Apply the same test conditions to RuleSailor and the selected MPC.

Report success/failure counts, per-training-seed success, conditional completion time, timeouts, capsize/miss/bounds rates, peak heel, and rudder/sheet travel separately. Decision-boundary actuator travel is an approximation to the Rust evaluator's physics-step total; label the sampling rate. Failed attempts must not disappear from outcome tables or make a method look fast because it has few successful episodes.

Show individual PPO/SAC training curves plus their means/ranges. Three training seeds support limited statistical claims; evaluation episodes are not extra training seeds. Distinguish CEM optimizer variability, training variability and condition variability in every interval or error bar.

### E4 · Gated upwind experiment

Enable U only if, by Nov 24, PPO, SAC and MPC all have valid R evaluations and the core report pipeline works. Train PPO/SAC independently from scratch on U with matched budgets; retain the same MPC planning convention and declare any separately tuned settings. **Amended 2026-11-14 by [M4U](#m4u--nov-1428--upwind-comparison-with-an-expert-data-arm--1216-h):** U uses γ = 0.99975, a 4M budget and 400k SAC replay, and adds labeled expert-data arms. All are frozen before any U learning run. At least three training seeds are required for a comparative learning claim.

Success remains a directed gate passage. Inspect tacking, irons, backward drift, miss events and planned-versus-executed paths. Short-horizon MPC failure during tacking can be an interpretable result; do not silently add an expert tack script. Omit U or label individual trajectories exploratory if the gate fails. Full multi-leg courses remain outside the required scope.

### Resource fairness: known dynamics are a capability, not free experience

Give PPO/SAC the same actor features and uniform-wind context. MPC may internally branch a richer episode state, including tracker/capsize history; expose the relevant metadata to both learners where practical and disclose anything that remains planner-only. Even with matched observable information, MPC has an exact dynamics model by construction. Label it **known-model/oracle-model planning**, never a guaranteed performance upper bound or a model learned at zero cost.

Report these resources separately:

| Resource | PPO/SAC | CEM-MPC |
|---|---|---|
| Real task interaction | All calibration, training and evaluation decisions | Live closed-loop evaluation decisions |
| Hypothetical simulator work | None in core | All candidate physics steps, branch overhead and candidate evaluation calls |
| Offline optimization | Policy/value/Q updates and tuning | Planner tuning, without policy training |
| Online cost | Measured actor inference and adapter cost | Branching, sampling, rollouts, elite fitting and action selection |
| Model access | No dynamics model | Exact current simulator parameters and fixed uniform wind |

Do not plot MPC as a miraculous zero-sample learner at x = 0. Plot PPO/SAC learning curves; show the fixed MPC outcome as an annotated reference or a separate success-versus-planning-cost plot. Compare cold-start total time, steady-state training cost and online latency separately. If deriving a deployment break-even episode count, state assumptions about training amortization and hardware.

### Required final artifacts

1. PPO/SAC success-versus-real-decisions and success-versus-training-time curves, with RuleSailor and selected MPC reference levels clearly labeled.
2. Outcome table for all controllers on nominal, held-out and shifted conditions, including failures and completion times.
3. MPC horizon/budget success-versus-compute plot and per-controller online latency table, including 50 ms deadline misses.
4. Representative real trajectories and controls; one MPC predicted path versus executed continuation. Select one nominal success if available and the first failing condition by manifest ID for each controller.
5. Source → config/checkpoint/seed → generating command → figure → supported claim index, plus a ready presentation.

Each artifact identifies task/reward/version, model privilege, seeds and cost units. Upwind and BPTT figures are additional and never prerequisites for these five artifacts.

## 6. Milestones and task backlog

Dates assume a Nov 13 start, core results frozen Nov 29, and a Dec 3 presentation. Budget roughly **55–75 focused implementation/analysis hours**, plus measured unattended training. These are estimates. BPTT has no reserved time in the core schedule; it begins only if the entire selected project is ready early, otherwise after the course deadline.

### M0 · Nov 13–14 · Freeze the benchmark and audit MPC branching · 9–12 h

- [x] Inventory actual code/results and any Nov 12 feedback; do not treat earlier draft milestones as completed. *No earlier ESE3990 code existed. Nov 12 feedback is expected after Nov 19.*
- [x] Freeze R/U configs, evaluation manifests, reward/masks, action/cadence and cost units after nominal baseline checks. *Contract v1: R budget cut to 45 s; shift set uses 15–30° heading offsets; the shaping target is the gate centre.*
- [x] Add Rust route construction/export and research-state readout; bind the existing RuleSailor.
- [x] Implement the shared wrapper, transition reward, explicit-reset collector, final-state handling and checkpoint metadata. *`experiments/ese3990/sailing.py`; checkpoint metadata comes from `run_metadata`.*
- [x] Audit a complete native planning branch; make one branch-versus-replay proof including a pending observer/gate state. Record missing state and expected implementation cost. *`Episode::fork`. Costs: 4.5 µs per fork and ~25 µs per branch decision, so ~130–190 ms per baseline planner decision single-threaded.*
- [x] Verify nominal restricted RuleSailor R completion and random-control reports; add/lock Optax and matplotlib in the root uv workspace.
- [x] **Viz (§10):** record the episode-log requirements (actions, condition, seed, build identity, checksum) as part of the frozen contract, so that every later run can be replayed.

**Exit (met 2026-11-13):** the benchmark is reproducible and there is an evidence-backed branching approach. If exact branching remains unresolved after two days, make it the first M1 integration task and defer batching/browser work; do not hide the risk until the last week.

### M1 · Nov 15–17 · PPO baseline and complete branching proof · 10–13 h

- [x] Implement PPO, transformed action likelihoods and GAE; pass the E0 numerical checks. *`ppo.py`, `tests/test_ppo.py`.*
- [x] Save/reload a policy and reproduce its evaluation; produce the first valid PPO R curve. *Seed 0, 1M decisions: 0/20 validation throughout. It found the gate around 500k, then avoided it because of the −10 mark-miss penalty ([M1 report](M1_report.md) §4).*
- [x] Complete isolated branch/rollout parity at gate, capsize and step-budget boundaries; verify candidate order cannot change the live episode. *Done in M0: `crates/sailgym-env/tests/fork.rs` (6 tests) and `tests/test_branch.py`.*
- [x] Measure a small candidate batch and a 100k-decision PPO pilot; set realistic planning and training budgets. *`RawEnv.rollout`. The baseline planner costs 158 ms per decision single-threaded, so it runs offline; threading K is the first M3 task. PPO takes 14 s per 100k and 93 s per 1M, so runtime does not limit it. The common budget is set in M2.*
- [x] Freeze the shared task/normalization protocol after any pilot-only corrections and record all changes. *[M1 report](M1_report.md) §5. No corrections were made. Two PPO pilot configurations remain (§4).*
- [x] **Viz (§10):** episode store, deterministic replay with build/checksum verification, logging hooks in the evaluator and collector, and a static trajectory plot. Prove it with stored RuleSailor R/U episodes that replay exactly. This is the debugging tool for the remaining milestones. *`experiments/ese3990/viz/`. All 18 rendered RuleSailor R/U episodes replayed exactly, at ~18 B per decision. A basic gif/mp4 overlay landed early. The PPO collector must call `EpisodeRecorder` when it is written.*
- [x] **Viz (§10):** log the probe set (nominal plus 5 validation conditions) at every saved PPO checkpoint. *All 20 validation documents are stored; the probes are tagged `kind="probe"`. Calibration and training episodes are logged too, and all of them replay exactly.*

**Exit (met 2026-11-13):** PPO is reproducible and correct native hypothetical rollouts exist. If branches are correct but slow, proceed with an explicitly offline planner and retain latency measurement. If branches cannot be made correct, the planned MPC comparison is blocked and must be recorded as unfinished scope.

### M2 · Nov 18–20 · SAC on the same task · 10–13 h

- [x] Implement replay, twin Q/target Q networks, actor, temperature and Polyak updates. *`sac.py`; comments cite the local slides by PDF page.*
- [x] Pass target/mask/gradient/replay checks and a short finite-loss training smoke run. *61 experiment tests pass, including 17 SAC tests.*
- [x] Audit pilot/tuning work; log Q scale, alpha and action saturation. *Deviation: existing PPO/SAC runs exceeded the bounded allowance; resources were unequal. Twelve baseline SAC seeds at 1M were audited, without further tuning ([M2 report](M2_report.md)).*
- [x] Freeze PPO/SAC settings and the common final budget using end-to-end runtime. *§4.5 baselines, 1M decisions including calibration; `configs/M2_freeze.json`.*
- [x] Queue the first independent-seed main learner runs; retain failed seeds. *Seeds [0,1,2] for each learner under `runs/ese3990/main/`; main completion is M4 work. PPO seed 0's prior diagnostic use is disclosed.*
- [x] **Viz (§10):** SAC logs training rollouts and checkpoint probes through the same store (no new code); check that a SAC probe episode replays exactly. *138 selected/final pilot probes replay exactly; all tiny-run calibration/training/evaluation episodes replay too.*

**Exit (met; see [M2 report](M2_report.md)):** both learning methods train/evaluate through the same contract, and their collected experience and optimization costs are traceable. The pilot-allowance deviation remains explicit. No world model or BPTT implementation starts here.

### M3 · Nov 21–23 · Close the CEM-MPC loop · 9–12 h

- [x] Implement blocked candidate sampling, elite refitting, warm starts, release restriction and episode resets. *`mpc.py`; comments cite advanced PDF pp. 120, 178–179 and 199–201.*
- [x] Score through the same transition reward and Rust outcomes; add ranking/reproducibility checks. *18 MPC checks; full experiment suite 79 passed. Independent candidates use the existing native Rayon pool.*
- [x] Run closed-loop R on nominal/validation conditions and compare predicted and executed paths. *All 300 sweep episodes and six selected probes finish and replay exactly; all 1,628 probe first-step positions match exactly.*
- [x] Measure K/I/H, actual hypothetical work, latency distributions and 50 ms deadline misses. *11.45B hypothetical physics steps; default sweep p50/p95 15.2/18.8 ms with 0/16,768 misses. Separate probes have 3/1,628 misses; no hard real-time guarantee.*
- [x] Run the five-setting validation horizon/budget sweep and freeze the default planner. *H=40/K=32/I=3, selected on validation finishes then p50 latency; `configs/M3_freeze.json`. No test/shift evaluation.*
- [x] **Viz (§10):** for selected probe episodes, store MPC's planned path at each decision; render the predicted path against the executed one (required artifact 4). *Six probe stores retain every selected plan; `mpc_figures.py` renders cost/latency and the nominal predicted/executed continuation.*

**Exit (met 2026-11-13; see [M3 report](M3_report.md)):** all three methods produce comparable real R outcome reports; MPC's known-model advantage and online cost are explicit. Success is not required to assert a correct implementation, but failure must be diagnosed through trajectories and horizon/cost evidence. No failures occurred in the M3 validation/probe set; held-out outcomes remain M4 work.

### M4 · Nov 24–28 · Finish comparisons and failure analysis · 10–14 h

- [x] Complete three PPO/SAC seed pairs; extend to five only if core runtime and analysis remain on schedule. *All six runs complete at 1M; audited against M2 settings. No extension to five.*
- [x] Evaluate selected MPC/RuleSailor and all predeclared learner checkpoints after the configuration freeze. *36 learner checkpoints + three search seeds + RuleSailor, 71 conditions each; `configs/M4_freeze.json` precedes held-out outcomes. All 2,840 episodes replay exactly; all 36 nominal checkpoint reloads match.*
- [x] Finish the MPC cost/horizon figure, outcome table and representative trajectory analysis. *M3 frontier/prediction figures retained; M4 test/time learning curves, selected/final tables and deterministic first-failure/control figures generated.*
- [x] Make the Nov 24 upwind decision; run all three methods on U only if the stated readiness gate passes. *Decision made early: omit optional U to keep the required R comparison/presentation on scope. This is a scope choice, not a claim that the readiness gate failed.*
- [x] Audit training versus search seeds, pilot/calibration/evaluation cost, hypothetical rollout counts and failed runs. *6M main decisions including 60k calibration; paired calibration/normalization exact. M4 adds 2.117M evaluation decisions / 2.179B hypothetical steps; prior pilot deviations remain disclosed.*
- [x] Regenerate the five required artifacts and identify unsupported/missing claims. *`M4_claims.json` maps figures/tables/media to sources/freezes/seeds/commands; final presentation/archive/rehearsal explicitly remain M5.*
- [x] **Viz (§10):** batch compositor and video specs: a checkpoint overlay per learner, a controller overlay (PPO/SAC/MPC/RuleSailor), a seed overlay, and the first failing episode per controller. Output gif/mp4 from stored episodes only. *Eight GIF/MP4 pairs, static grid and control plots; no invented MPC/RuleSailor failures. Fixed specs reproduce first frames and frame counts.*

**Exit (met 2026-11-13; see [M4 report](M4_report.md)):** reproducible core results exist. After Nov 28, add no core method, reward, wind model or task. BPTT remains deferred until the presentation and reproduction work below are finished too.

### M4U · Nov 14–28 · Upwind comparison with an expert-data arm · 12–16 h

Reopens the optional U experiment (E4), which M4 deferred to protect the R comparison. R's results, freezes and stores are not touched. **M5's dates do not move: if M4U is not complete by Nov 28, stop. M5 then presents R only, and U becomes post-presentation work.**

**Questions.** (1) Does the R ranking survive a task that needs a tack? (2) Is the learners' U bottleneck *discovering* the tack? This is tested by giving SAC RuleSailor's episodes in replay and PPO the same episodes as an initialization. (3) Is MPC limited by its horizon or by its search?

**Settings frozen before any U learning run.** Each is recorded with its reason, and no U outcome may change it.

| Setting | R (unchanged) | U | Reason |
|---|---|---|---|
| γ per decision | 0.999: horizon 1,000 decisions = 50 s | **0.99975**: 4,000 decisions = 200 s | Keeps R's ratio of effective horizon to budget (1,000/900). At 0.999, a +10 finish at RuleSailor's typical 70–90 s is worth only 1.7–2.5 from the start; at 0.99975 it is worth 6.4–7.0 |
| Budget | 1M decisions | **4M**, including calibration and any demonstrations | Same number of full-length episodes as R (≈1,100). Measured runtime: ≈80 min per SAC run and ≈7 min per PPO run |
| SAC replay capacity | 100k | **400k** | Holds the same ≈110 full-length episodes as R |
| Checkpoints | 10k … 1M | Initial (after calibration and any demonstrations), 400k, 1M, 2M, 3M, 4M | R's schedule ×4 |
| Everything else | M2 freeze | Unchanged | Networks, optimizer, PPO/SAC hyperparameters, 10k smooth-random calibration and normalization, release off, condition manifests, checkpoint-selection rule, MPC conventions |

Normalization was checked on 2026-11-14 and needs no amendment. Against seed-0 U calibration statistics, no RuleSailor U position feature reaches the ±5 clip; 4% of its states clip some other feature.

**Arms.** Training seeds are [0, 1, 2]; MPC uses search seeds [0, 1, 2]. Expert-data arms are labeled as such everywhere (§9) and never replace the from-scratch rows.

| Arm | Role | What differs from from-scratch |
|---|---|---|
| PPO, SAC from scratch | Primary comparison (§3.1) | Only the U settings above |
| SAC + demonstrations | Expert-data arm | A separate, never-evicted buffer holds the demonstrations. Each minibatch is 128 demonstration rows + 128 online rows: RLPD-style symmetric sampling only, without RLPD's LayerNorm, ensembles or higher update ratio |
| PPO + BC | The same data offered to PPO (§9) | Actor mean fitted to the demonstrations by MSE on atanh(clip(a, ±0.99)), and value fitted to their discounted returns-to-go: 20 epochs, minibatch 256, Adam 3e−4. log σ stays −0.5, then frozen PPO |
| BC only | Pure-imitation reference | PPO + BC's initial checkpoint; no extra run |
| CEM-MPC | Known-model planning | Frozen M3 default (H=40, K=32, I=3) under the U γ. H=160 (K=32) runs on validation as a diagnostic |
| RuleSailor | Reference and demonstration source | None |

**Demonstrations.** 50 RuleSailor episodes on `TrainingSampler(1000)` conditions, which are drawn from the training distribution and exclude held-out documents. M0's U validation/test/shift episodes are never used. Every outcome is kept (finishes, misses, timeouts) with its true U-contract reward. Transitions come from exact replay of an ordinary episode store (`kind="demo"`); there is no new data format. All seeds share one demonstration set, whose store hash is frozen. Cost: ≈91k decisions (10 episodes measured 18,253), charged to each expert-data run's 4M budget. RuleSailor's tack recipe (a 1,344-script sweep, `docs/v2/practice-validation.md`) is disclosed as uncounted human prior work.

- [x] **U contract version.** Leave `configs/tasks.json` byte-identical and add no global identity key, because both are replay identity for every existing store. U stores record the contract version they used, and replay uses that version. *Accept:* all 2,840 M4 episodes and the 21 stored U episodes in `runs/ese3990/viz_demo/store_U` still replay exactly; a test hand-checks one U transition reward at γ = 0.99975 and confirms R still uses 0.999. *`configs/tasks_v2.json`; `Task.load("U")` is contract 2, R stays 1 with unchanged records. `replay_audit.py`: all 2,840 M4 + 42 viz_demo episodes (21 U) replay exactly.*
- [x] Take the task from the freeze in `mpc_study.py` (hard-coded R) and confirm that `compare.py` and `comparison_report.py` handle U. *`mpc_study.py --task U`. `compare.py` is hash-frozen by M4 and stays R-only; U uses new `m4u_compare.py`/`m4u_report.py`, which reuse its helpers.*
- [x] Demonstration store and loader, the SAC demonstration buffer with symmetric sampling, and PPO's BC/value pretraining. *Accept:* demonstration transitions match replayed rewards and checksums; demonstration rows are never evicted and make up exactly half of each minibatch; BC loss falls and the BC actor reloads exactly; the existing 83 experiment tests still pass. *`demos.py`, `sac.symmetric_sample`, `ppo.behavior_clone`. Store sha256 `69575c91…`, D = 83,462 (37 finished, 7 mark missed, 6 truncated). 119 experiment tests pass.*
- [x] One 100k smoke run per new arm on pilot seed 100, to check plumbing and runtime. These runs are not used for tuning or reported as results. *`runs/ese3990/M4U/smoke/`, all four arms; projected 4M: SAC ≈97 min, PPO ≈10 min.*
- [x] RuleSailor and smooth-random on U validation under the U contract. Outcomes should match M0; only returns change. *Matched: RuleSailor 13/20 (5 mark missed, 2 truncated), random 0/20.*
- [ ] MPC on U validation: H=40 and H=160, three search seeds each. Selection is predeclared as in M3 (most finishes, then lower p50 latency). Only the selected setting goes to held-out sets; H=160 is labeled offline if selected.
- [x] Write `configs/M4U_train_freeze.json` (arms, settings, seeds, demonstration hash) **before** the 12 main learner runs. Queue the runs and retain failed ones. *Written, then all 12 launched concurrently 2026-11-14 00:43 (+08:00) under `runs/ese3990/M4U/main/`.*
- [ ] Write the held-out freeze before opening test/shift, as in M4. Evaluate every predeclared checkpoint, the selected MPC and RuleSailor on nominal/test/shift, then run the exact-replay audit.
- [ ] U diagnostics from replay, with thresholds frozen before held-out results: tacks completed, time in irons, mark misses, out-of-bounds exits, and final distance for timeouts.
- [ ] Optional, if the binding exposes RuleSailor's mode: predict that mode from the 17 learner features on the demonstration episodes. This tests whether its memory adds information the learners lack.
- [ ] `M4U_report.md`, claim-index entries, and handoff `docs/handoffs/ese3990-m4u.md`.

**Schedule.**

| Dates | Work |
|---|---|
| Nov 14–15 | Contract version, plumbing and tests |
| Nov 16–17 | Demonstrations; RuleSailor/MPC validation (≤5 h); training freeze; launch the main runs (≈1.5 h wall for six concurrent SAC runs) |
| Nov 18–21 | Held-out freeze and evaluation. MPC H=40 held-out takes ≤3.2 h; H=160 takes ≤13 h if selected |
| Nov 22–26 | Analysis, figures and media |
| Nov 27–28 | Report and handoff |

Compute is not the constraint; the slack is for debugging. **Cut order:** H=160 diagnostic, then media, then the optional mode check. PPO + BC can be dropped only by amending §9 in writing.

**Readings declared before results.**

| Observation | Reading |
|---|---|
| SAC + demonstrations ≫ SAC from scratch | The learner bottleneck was discovering the tack, not representation or memory |
| SAC + demonstrations > RuleSailor | RL improved on its expert; check whether it removed RuleSailor's mark misses |
| PPO + BC ≪ SAC + demonstrations | Off-policy learning keeps using expert data; on-policy learning cannot (advanced slides p. 10) |
| BC only < RuleSailor | Compounding error, or averaging over RuleSailor's hidden mode |
| MPC H=40 fails, H=160 succeeds | Planning is horizon-bound; a value tail is the next step |
| Both MPC settings fail | Planning is search-bound: the tack is not being sampled |
| Every arm fails, including the expert-data arms | A valid result. Report it with the tack/irons diagnostics; do not add a hidden tack script |

**Not in M4U:** a learned value tail, MPC with RuleSailor as its base policy, residual RL, a γ sweep or any other U tuning, and BPTT.

**Exit:** every arm has a valid U evaluation that replays exactly; R evidence still replays unchanged; the report states which reading applies.

### M5 · Nov 29–Dec 3 · Freeze, explain and present · 7–10 h

- [ ] Nov 29–30: freeze checkpoints/results and regenerate all core figures/tables from saved data.
- [ ] Finish the claim-to-command index and a clean saved-checkpoint evaluation invocation.
- [ ] Prepare a 10-minute presentation with sailing video/replay or static trajectory fallback.
- [ ] **Viz (§10):** regenerate every presentation video and trajectory figure from the frozen episode stores with one command; record each spec in the claim index.
- [ ] Rehearse PPO versus SAC data use, SAC entropy/targets, MPC foresight/compute and all known-model fairness limitations.
- [ ] Dec 1–2: reproduce a report, rehearse Q&A and confirm final logistics; Dec 3: present measured findings.

Suggested presentation: boat/question (1 min), task/information contract (1), three controller mechanisms (3), outcomes and learning cost (2), MPC horizon/latency and failures (2), limitations (1). Upwind can replace part of the failure discussion if completed. A late BPTT attempt never displaces the core explanation.

## 7. Implementation footprint and verification

Keep project-specific rewards/trainers in `experiments/ese3990/`, using the existing root uv workspace. Core M0–M4 modules and media are implemented; final presentation/packaging remains M5:

```text
experiments/ese3990/
  README.md       # commands, manifests and claim → figure map
  configs/        # task/condition documents and seed lists
  sailing.py      # transforms, transition reward and collection
  ppo.py          # PPO/GAE and networks
  sac.py          # SAC, replay and networks
  mpc.py          # CEM search over a native candidate-rollout interface
  train.py        # learner entry point: PPO or SAC
  evaluate.py     # one real-simulator evaluator for every controller
  compare.py      # frozen M4 evaluation of every predeclared checkpoint
  comparison_report.py # outcomes, costs, curves and saved video specs
  plots.py        # figures from raw summaries
  viz/            # §10: episode store, replay, rendering, video composition
  tests/          # focused numerical/boundary checks
  model_based.py  # only created for the final BPTT stretch
```

| Potential code change | Scope and required checks |
|---|---|
| `crates/sailgym-py/src/{env,spec,lib}.rs` | Research-state readout, route and RuleSailor helpers, native branch/batch-rollout interface; preserve existing contracts |
| `crates/sailgym-env/src/episode.rs` | Narrow planning branch using the existing runner; preserve causal state, outcome order and live-episode isolation |
| `crates/sailgym-physics/src/simulation.rs` | Owned-state clone/snapshot support only if needed; preserve capsize/wind/control/cache state and deterministic stepping |
| `crates/sailgym-physics/src/stability/capsize.rs` | At most a read-only pending-duration accessor for research features; no new termination behavior |
| Native/Python vector path | Only if measurements justify it; preserve final state and reset semantics |
| Root `pyproject.toml` / `uv.lock` | Add actual experiment dependencies through uv; record resolved versions |
| Existing and experiment tests | Extend binding, replay, determinism and array checks; explicitly invoke experiment tests outside current default discovery |

Do not solve branching by rewriting forces, passage rules or capsize conditions in Python. No simulator derivative port, Rust neural inference, general snapshot file format or new browser UI is required. Branches can be opaque native in-memory objects. Browser trajectory playback is gated on existing export plumbing; plots/video suffice.

Run focused numerical and integration tests, affected Rust package tests, and `scripts/py-test.sh` for binding/wrapper changes. Keep experiment tests explicitly discoverable. Save large generated buffers/runs outside source control, with small manifests and selected figures under this documentation directory. This documentation-only edit does not require a simulator-wide test run.

## 8. Risks, cut order and defensible claims

| Risk | Detection | Response |
|---|---|---|
| Incomplete MPC branch | Replay disagrees near gate/capsize/budget boundaries | Fix causal state and shared stepping before comparing controller quality |
| Planning too slow | Latency and hypothetical step counts | Reduce candidate budget/block search dimension, retain common live cadence, label offline performance honestly |
| Short horizon misses a tack | Trajectory and horizon sweep | Report foresight/runtime tradeoff; no hidden expert maneuver script |
| SAC/PPO implementation error | Numerical checks, Q/entropy/ratio diagnostics | Debug on SailGym; optionally cross-check a public implementation on the same task |
| Information/model advantage hidden | State/context audit | Match learner inputs and disclose richer planner state plus exact model access |
| Reward exploits or misleading times | Rust outcomes, failed episodes and controls | Freeze geometry/evaluator first; report success and failures alongside conditional times |
| Scope or compute exceeds deadline | Milestone exits and measured pilots | Cut BPTT first, then U and five-seed expansion; reduce common budgets before dropping independent seeds |
| Visualization consumes core time | Hours spent against the §10 estimates | Cut the browser export first, then the grid layout, then mp4 (keep gif and static figures). Keep the episode store and replay, because the analysis depends on them |

A missing required method or MPC study remains incomplete work; do not relabel a reduced experiment as the full project. A measured failure with checked code is valid evidence. Any robustness claim is limited to the explicit uniform-wind conditions tested. The core cannot establish real-world transfer, a universal algorithm ranking, equal prior knowledge, or SuperMimic-like speedups.

The strongest supported story is which implemented controller is useful under which **experience and computation budget**, and how its sailing behavior explains the result.

## 9. Final stretch: learned world model and BPTT-WM

**Start gate:** all required R results, the MPC study, selected U results (or a documented decision to omit U), reproducibility checks, figures and presentation are complete and frozen. Confirm spare time against the final deadline. If these conditions are not met, this section remains future work. No world-model data collection, fitting or gradient study belongs to the core critical path.

Keep the completed PPO/SAC/MPC study unchanged. Perform stretch work separately, starting on task R with the same action/state/reward contract. If final tests are reused, label the stretch exploratory; reserve a new held-out set before tuning if making a fresh confirmatory comparison. The expected default is to attempt this after the course project unless everything is ready early.

### S1 · Fit a differentiable boat model first

Fit a small residual MLP to physical-state transitions, conditioned on action and uniform wind. Reward and event heads also receive the fixed goal/gate geometry and bounds. Time advancement and fixed context are known. Encode angular targets without wrap discontinuities. Include the capsize-duration variable for the event model; do not infer episode outcome from boat pose alone.

Use a separate reward head and a three-class event head: continuing / successful / failed within the next decision. Reward targets are the exact wrapper rewards, including shaping and actual terminal decision duration. A time limit is a collection cutoff, not a learned physical failure class. Physical-transition loss excludes shortened terminal transitions initially, while their reward/event labels remain available.

Start with two hidden layers of width 128 and a k = 4 autoregressive state loss. Keep k = 1 as the one fitting ablation. Standardize with training data only. Split by **whole episodes and condition manifests**, never by adjacent transition rows. Windows stop at episode boundaries; masked padded elements contribute no loss.

For an online BPTT comparison, recollect the same 10,000-decision smooth-random calibration protocol per seed, freeze the same feature normalization convention, and charge all calibration and subsequent real collection to the BPTT budget. Refit on newly visited states throughout training. If data from completed PPO/SAC runs is used instead, label it an offline or pretrained variant and account for the cost of producing those data; do not claim a from-scratch sample-efficiency gain.

RuleSailor trajectories may be used for a **separate model-fit diagnostic dataset**, with their origin and collection cost recorded. They do not enter the main learning runs. A rescue using expert initialization or demonstrations must be labeled separately, offered to both learners under a defined protocol, and must not replace the original comparison invisibly.

Proposed validation gate: dynamic-state MSE at most 0.5× persistence at one decision and 0.8× persistence at eight decisions, reward error below a constant training-mean predictor, and explicitly reported event support. Freeze thresholds before fitting and exclude deterministic time/context from the state metric. Budget at most two focused days for fitting/debugging before deciding whether control is feasible. Model fitting alone is a partial stretch result, not BPTT policy learning.

### S2 · Short-horizon actor–critic through that model

Use a deterministic tanh actor with the same input features and hidden widths as PPO's mean network. Add logged Gaussian action noise during real collection, clip collection actions to the valid rates, and keep release off. Collection noise is not the policy-gradient estimator.

Start short imagined trajectories from real buffer states; freeze model parameters during actor updates while preserving derivatives through model inputs. With predicted event probabilities, let `q_cont` be the probability that the next state is continuing. Optimize:

$$
c_0=1,\quad c_{t+1}=c_t q_{\mathrm{cont},t},\qquad
\hat J=\sum_{t=0}^{H-1}\gamma^t c_t\hat r_t
       +\gamma^H c_H V(\hat s_H).
$$

The reward head already predicts expected success/failure reward; do not add event bonuses twice. Fit the critic with stopped-gradient bootstrapped targets and terminal/survival masks. Keep the critic parameters frozen for the actor update, but retain its input derivative. Use real-transition TD targets first; any imagined critic targets are an explicit later change, not an undocumented mixture. Record that this is a simplified SHAC/SuperMimic-inspired variant.

Default H = 32 decisions = 1.6 simulated seconds. A tack can take much longer, so the critic is essential to the proposed long-horizon mechanism. Record real versus predicted return, actor-gradient norms, and event calibration. Short rollouts do not guarantee that the policy cannot exploit the model.

Pilot starting values: collection noise standard deviation 0.2; replay capacity 100k; world-model Adam 1e−3 with batch 256/k = 4, 200 initial updates and 20 updates per 2,048 new real decisions; actor Adam 3e−4, four imagined updates with 64 starts per collection round; eight real-transition critic minibatches of 256. Standardize state/reward losses separately and convert predicted rewards back to task units for the actor objective. These settings are unvalidated and all model-fitting/imagined computation must be logged.

Start with one pilot seed and H = 8 or 32. Only after real control is valid run three independent BPTT seeds at the frozen core interaction budget and shared evaluation protocol. Three runs at 500k add 1.5M real decisions, plus separately reported diagnostics/evaluation; they are not included in the core run count. Use the original learner initialization convention and disclose deterministic-versus-stochastic policy differences. A single seed supports only an exploratory demonstration.

### S3 · Model and horizon diagnostics, if control is ready

At early and final checkpoints, report normalized state error at 1, 8, 16 and 32 decisions on held-out recorded action sequences. Compare with a persistence predictor that holds dynamic state fixed and advances known time. Break down errors by sheet taut/slack and heel regime using existing Rust/sensor labels. Add near-capsize or stall subsets only if enough independent episodes and reliable Rust diagnostics exist; otherwise state that coverage is absent.

Report reward error, event confusion/calibration, and class counts. A head tested on zero capsizes has no demonstrated capsize accuracy. Compare k = 1 versus k = 4 on the **same fixed dataset**, architecture and fitting budget; this is a model-fitting ablation, not an extra full policy-training grid.

Retain the original BPTT horizon question: H = 8/32/64, or 0.4/1.6/3.2 s. Reuse the frozen baseline H and add the other horizons only after a valid BPTT-versus-core comparison exists. A full three-seed sweep to 250k adds six runs/1.5M real decisions; it is optional and cannot consume core deadlines. Report real outcomes, predicted/real return gap and actor-gradient norms, not a claimed guaranteed exponential gradient law.

### S4 · Local sensitivity, optional after S1–S3

Use a small set of logged episode prefixes and the validated native branch API, with reset/replay as the correctness reference. Perturb a feasible rudder/sheet action by ±ε, then replay the same short continuation. Compare the real finite-difference change in a scalar short-horizon objective with the learned model's prediction at multiple ε values.

Separate smooth regimes from event boundaries and use float64 simulator outputs. A model-only finite-difference check tests autodiff correctness; this diagnostic tests agreement with the boat. Neither provides a ground-truth full policy gradient. Omit this diagnostic if its runtime is excessive; no additional snapshot subsystem is needed.

### S5 · PPO-WM attribution control, last optional addition

If time remains, reuse PPO inside the same frozen learned model with the same start-state distribution, horizon, event/reward heads and model checkpoint as BPTT-WM. Use a common reparameterized stochastic actor for an estimator-focused diagnostic. PPO-WM and deterministic BPTT-WM learning curves alone still do not isolate gradient type. There is no BPTT-Sim arm: SailGym has no complete differentiable dynamics implementation.

No stretch outcome changes whether the three-controller core is complete. Do not add these stages to the dated milestones or postpone the final presentation to finish them.

**First concrete action:** freeze the common transition/reward contract and establish native planning-branch correctness alongside the restricted RuleSailor R baseline. This unlocks the required three-controller comparison; the learned-model option waits until the complete core is ready.

## 10. Visualization and episode-replay harness

The simulator is deterministic, so an episode is fully described by its condition, seed and action sequence on a given physics build. Store that, and re-simulate when rendering. Re-simulating a 45 s R episode costs about 30 ms, so rendering is the only expensive step. The harness serves debugging (M1–M3), the failure analysis (M4) and the presentation (M5) from one code path.

### 10.1 What is stored

| Per episode | Size (R, ≤ 900 decisions) | Purpose |
|---|---|---|
| Actions (rudder rate, sheet rate; release is always −1) | ≤ 7.2 KB as f32 for learners, ≤ 14.4 KB as f64 for RuleSailor | Exact replay; the dtype is chosen so storage loses nothing |
| Condition, seed, controller, run, checkpoint, kind, outcome, reason, return, steps | < 0.5 KB in an index line | Querying without opening shards |
| Final research-state checksum | 8 B | Detects a changed physics build or contract |
| Pose (x, y, ψ, φ) at 5 Hz | ~3.6 KB | Fallback plots after a physics change |

Each store records the physics source identity (`ModelIdentity`), the `tasks.json` hash, and the feature and reward versions. Replay refuses an identity or checksum mismatch and falls back to the stored poses, labelling them as such.

Layout: one directory per run, compressed `.npz` shards holding concatenated actions and poses with per-episode offsets, plus an `index.jsonl` with one line per episode. Logging every training rollout costs about 4–8 MB per 500k-decision run before compression, so everything is kept, including failed and diverged runs. Stores live under the gitignored `runs/`.

### 10.2 Two kinds of episode

- **Training rollouts:** the stochastic episodes a learner actually collected, logged automatically.
- **Probe evaluations:** a fixed probe set (nominal plus 5 validation conditions) evaluated deterministically at every saved checkpoint. RuleSailor and MPC run the same probes. The progress videos come from probes, so each frame shows the same conditions. Test conditions stay closed until the configuration freeze.

MPC additionally stores its planned path at each decision for selected probe episodes only, because it costs about 40 points per decision.

### 10.3 Modules (`experiments/ese3990/viz/`)

| Module | Role | Milestone |
|---|---|---|
| `episode_store.py` | Writer and reader; `query(run=…, checkpoint=…, condition=…)` | M1 |
| `replay.py` | Re-simulation from actions to state arrays, with identity and checksum checks | M1 |
| `render.py` | Top-down frame and static trajectory figure: hull, boom, gate, track, wind, readout | M1 (static), M4 (animated) |
| `compose.py` | Checkpoint, controller and seed overlays; small-multiples grid; gif (Pillow) and mp4 (system ffmpeg) | M4 |
| `compose.py` CLI (consolidated) | Command-line entry driven by a video spec, recorded in the claim index; no separate wrapper needed | M4 |
| Recording export to the web viewer | Optional single-episode inspection in the browser replay UI | Stretch |

### 10.4 Acceptance

- A stored episode re-simulates to its logged final state, outcome, executed steps and return.
- Replay refuses a mismatched build identity or contract hash.
- Store, then load, then replay is lossless for both f32 and f64 action logs.
- A fixed video spec gives the same frame count and the same first frame.

Estimate: 3–4 h in M1, 1–2 h in M3, 3–4 h in M4, 1 h in M5, and 2–3 h for the browser export stretch.

