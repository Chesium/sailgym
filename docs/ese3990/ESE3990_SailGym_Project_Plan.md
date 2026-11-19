# Learning versus Planning for Sailboat Control

**ESE3990 project plan · Shimin Chen · revised 2026-11-14**  
**Status:** M0–M4 are complete (see the [M0](M0_report.md), [M1](M1_report.md), [M2](M2_report.md), [M3](M3_report.md) and [M4](M4_report.md) reports). The frozen R comparison includes three PPO/SAC training seeds, three MPC search seeds and RuleSailor; all 2,840 final-evaluation episodes replay exactly. **M4U** (upwind comparison with an expert-data arm): all 12 learner runs and the MPC validation sweep are done; the held-out evaluation is running. **M4X** (added 2026-11-14) tests two explanations of the upwind results with post-hoc, exploratory arms. M4X was closed early on 2026-11-17: readings settled, held-out audit and generated files skipped. **M4P** (added and completed 2026-11-17) asks, with no learning, whether random exploration with different noise and action intervals ever produces a tack on U. Per-decision white noise never tacks; correlated or held noise tacks in up to 25% of episodes, but no random episode makes real upwind progress or finishes ([M4P report](M4P_report.md)). **M5** (added 2026-11-17; stage 1 completed the same day, see [M5 report](M5_report.md)) tested two training-side changes on U without demonstrations, for PPO and SAC alike: 0.5 s decision blocks and randomised training starts. No cell finishes (0/6,319 held-out episodes), so the declared reading calls for a second stage. M5b completed on November 18. All four arms missed the threshold and [M5 closes](M5b_report.md). The [first M6 campaign](M6_first_report.md) completed on November 19. Both 400k-decision pilots stalled at the 1–3 m start band. The [M6 tuning campaign](M6_tune_report.md) (November 19, post-hoc) changed only the learner side: fixed feature scaling, 0.5 s decisions, 2 × 256 networks and a set-point action head. From-scratch PPO then finished 450/500 test episodes at 10M decisions on 10 seeds, with 9/10 seeds perfect. SAC finished 183/250 test episodes at 10M decisions on 5 seeds, with 4/5 passing. **M7** (November 20, post-hoc) narrows U to a ±10 m channel, which needs a second tack. Unchanged, the M6 recipe fails with PPO (0/10) and learns with SAC on 4/10 seeds. A width curriculum that starts in open-water U makes PPO as consistent as M6. At 40M decisions, 9/10 seeds are perfect on validation and test (180/200, 450/500, shift 134/200) ([M7 report](M7_channel_report.md)). BPTT-WM moves to after Dec 3. Final presentation/packaging remains MLast. PPO/SAC retain the common 1M-decision main budget; prior unequal pilot resources and MPC's exact-model privilege remain explicit.
**Working question:** In a fast physics-based sailing simulator, how do on-policy learning, off-policy learning, and decision-time planning trade off task success, experience, computation, and robustness?

The main comparison is **from-scratch PPO versus from-scratch SAC versus true-model CEM-MPC**, with the existing RuleSailor as an engineering reference. SailGym is the sole environment. **BPTT through a learned world model is future work after the Dec 3 presentation** (amended 2026-11-14; reasons in [M4X](#m4x--explain-the-upwind-failures-let-learning-shorten-planning--812-h)).

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
| Post-hoc extension | M4X: critic-stability arm and value-tail planner | Labeled exploratory everywhere; starts after M4U's report; whatever is unfinished on Nov 28 is presented as planned work (§6) |
| Post-hoc extension | M4P: exploration probe, no learning | Labeled exploratory everywhere; starts after M4X's report; whatever is unfinished on Nov 28 is presented as planned work (§6) |
| Post-hoc extension | M5: upwind without demonstrations (decision blocks × training starts, PPO and SAC) | Labeled exploratory everywhere; starts after M4P's report; runs before MLast (§6) |
| Future work (after Dec 3) | Learned dynamics and BPTT-WM | Moved out of the course schedule on 2026-11-14 (§6 M4X, §9) |
| Deferred | Further algorithms and infrastructure | No TD3/Dreamer sweep, recurrent policy, full-course racing, or new browser dashboard |
| Not scheduled (2026-11-14) | Harder tasks and observation | Multi-gate or trajectory-following tasks, random/gusty wind, deliberate partial observation; reasons in §6 M4X |

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

Enable U only if, by Nov 24, PPO, SAC and MPC all have valid R evaluations and the core report pipeline works. Train PPO/SAC independently from scratch on U with matched budgets; retain the same MPC planning convention and declare any separately tuned settings. **Amended 2026-11-14 by [M4U](#m4u--upwind-comparison-with-an-expert-data-arm--1216-h):** U uses γ = 0.99975, a 4M budget and 400k SAC replay, and adds labeled expert-data arms. All are frozen before any U learning run. At least three training seeds are required for a comparative learning claim.

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

Milestones run in the order below; the presentation is Dec 3. Budget roughly **55–75 focused implementation/analysis hours**, plus measured unattended training. These are estimates. BPTT has no reserved time in the core schedule; it begins only if the entire selected project is ready early, otherwise after the course deadline.

### M0 · Freeze the benchmark and audit MPC branching · 9–12 h

- [x] Inventory actual code/results and any Nov 12 feedback; do not treat earlier draft milestones as completed. *No earlier ESE3990 code existed. Nov 12 feedback is expected after Nov 19.*
- [x] Freeze R/U configs, evaluation manifests, reward/masks, action/cadence and cost units after nominal baseline checks. *Contract v1: R budget cut to 45 s; shift set uses 15–30° heading offsets; the shaping target is the gate centre.*
- [x] Add Rust route construction/export and research-state readout; bind the existing RuleSailor.
- [x] Implement the shared wrapper, transition reward, explicit-reset collector, final-state handling and checkpoint metadata. *`experiments/ese3990/sailing.py`; checkpoint metadata comes from `run_metadata`.*
- [x] Audit a complete native planning branch; make one branch-versus-replay proof including a pending observer/gate state. Record missing state and expected implementation cost. *`Episode::fork`. Costs: 4.5 µs per fork and ~25 µs per branch decision, so ~130–190 ms per baseline planner decision single-threaded.*
- [x] Verify nominal restricted RuleSailor R completion and random-control reports; add/lock Optax and matplotlib in the root uv workspace.
- [x] **Viz (§10):** record the episode-log requirements (actions, condition, seed, build identity, checksum) as part of the frozen contract, so that every later run can be replayed.

**Exit (met 2026-11-13):** the benchmark is reproducible and there is an evidence-backed branching approach. If exact branching remains unresolved after two days, make it the first M1 integration task and defer batching/browser work; do not hide the risk until the last week.

### M1 · PPO baseline and complete branching proof · 10–13 h

- [x] Implement PPO, transformed action likelihoods and GAE; pass the E0 numerical checks. *`ppo.py`, `tests/test_ppo.py`.*
- [x] Save/reload a policy and reproduce its evaluation; produce the first valid PPO R curve. *Seed 0, 1M decisions: 0/20 validation throughout. It found the gate around 500k, then avoided it because of the −10 mark-miss penalty ([M1 report](M1_report.md) §4).*
- [x] Complete isolated branch/rollout parity at gate, capsize and step-budget boundaries; verify candidate order cannot change the live episode. *Done in M0: `crates/sailgym-env/tests/fork.rs` (6 tests) and `tests/test_branch.py`.*
- [x] Measure a small candidate batch and a 100k-decision PPO pilot; set realistic planning and training budgets. *`RawEnv.rollout`. The baseline planner costs 158 ms per decision single-threaded, so it runs offline; threading K is the first M3 task. PPO takes 14 s per 100k and 93 s per 1M, so runtime does not limit it. The common budget is set in M2.*
- [x] Freeze the shared task/normalization protocol after any pilot-only corrections and record all changes. *[M1 report](M1_report.md) §5. No corrections were made. Two PPO pilot configurations remain (§4).*
- [x] **Viz (§10):** episode store, deterministic replay with build/checksum verification, logging hooks in the evaluator and collector, and a static trajectory plot. Prove it with stored RuleSailor R/U episodes that replay exactly. This is the debugging tool for the remaining milestones. *`experiments/ese3990/viz/`. All 18 rendered RuleSailor R/U episodes replayed exactly, at ~18 B per decision. A basic gif/mp4 overlay landed early. The PPO collector must call `EpisodeRecorder` when it is written.*
- [x] **Viz (§10):** log the probe set (nominal plus 5 validation conditions) at every saved PPO checkpoint. *All 20 validation documents are stored; the probes are tagged `kind="probe"`. Calibration and training episodes are logged too, and all of them replay exactly.*

**Exit (met 2026-11-13):** PPO is reproducible and correct native hypothetical rollouts exist. If branches are correct but slow, proceed with an explicitly offline planner and retain latency measurement. If branches cannot be made correct, the planned MPC comparison is blocked and must be recorded as unfinished scope.

### M2 · SAC on the same task · 10–13 h

- [x] Implement replay, twin Q/target Q networks, actor, temperature and Polyak updates. *`sac.py`; comments cite the local slides by PDF page.*
- [x] Pass target/mask/gradient/replay checks and a short finite-loss training smoke run. *61 experiment tests pass, including 17 SAC tests.*
- [x] Audit pilot/tuning work; log Q scale, alpha and action saturation. *Deviation: existing PPO/SAC runs exceeded the bounded allowance; resources were unequal. Twelve baseline SAC seeds at 1M were audited, without further tuning ([M2 report](M2_report.md)).*
- [x] Freeze PPO/SAC settings and the common final budget using end-to-end runtime. *§4.5 baselines, 1M decisions including calibration; `configs/M2_freeze.json`.*
- [x] Queue the first independent-seed main learner runs; retain failed seeds. *Seeds [0,1,2] for each learner under `runs/ese3990/main/`; main completion is M4 work. PPO seed 0's prior diagnostic use is disclosed.*
- [x] **Viz (§10):** SAC logs training rollouts and checkpoint probes through the same store (no new code); check that a SAC probe episode replays exactly. *138 selected/final pilot probes replay exactly; all tiny-run calibration/training/evaluation episodes replay too.*

**Exit (met; see [M2 report](M2_report.md)):** both learning methods train/evaluate through the same contract, and their collected experience and optimization costs are traceable. The pilot-allowance deviation remains explicit. No world model or BPTT implementation starts here.

### M3 · Close the CEM-MPC loop · 9–12 h

- [x] Implement blocked candidate sampling, elite refitting, warm starts, release restriction and episode resets. *`mpc.py`; comments cite advanced PDF pp. 120, 178–179 and 199–201.*
- [x] Score through the same transition reward and Rust outcomes; add ranking/reproducibility checks. *18 MPC checks; full experiment suite 79 passed. Independent candidates use the existing native Rayon pool.*
- [x] Run closed-loop R on nominal/validation conditions and compare predicted and executed paths. *All 300 sweep episodes and six selected probes finish and replay exactly; all 1,628 probe first-step positions match exactly.*
- [x] Measure K/I/H, actual hypothetical work, latency distributions and 50 ms deadline misses. *11.45B hypothetical physics steps; default sweep p50/p95 15.2/18.8 ms with 0/16,768 misses. Separate probes have 3/1,628 misses; no hard real-time guarantee.*
- [x] Run the five-setting validation horizon/budget sweep and freeze the default planner. *H=40/K=32/I=3, selected on validation finishes then p50 latency; `configs/M3_freeze.json`. No test/shift evaluation.*
- [x] **Viz (§10):** for selected probe episodes, store MPC's planned path at each decision; render the predicted path against the executed one (required artifact 4). *Six probe stores retain every selected plan; `mpc_figures.py` renders cost/latency and the nominal predicted/executed continuation.*

**Exit (met 2026-11-13; see [M3 report](M3_report.md)):** all three methods produce comparable real R outcome reports; MPC's known-model advantage and online cost are explicit. Success is not required to assert a correct implementation, but failure must be diagnosed through trajectories and horizon/cost evidence. No failures occurred in the M3 validation/probe set; held-out outcomes remain M4 work.

### M4 · Finish comparisons and failure analysis · 10–14 h

- [x] Complete three PPO/SAC seed pairs; extend to five only if core runtime and analysis remain on schedule. *All six runs complete at 1M; audited against M2 settings. No extension to five.*
- [x] Evaluate selected MPC/RuleSailor and all predeclared learner checkpoints after the configuration freeze. *36 learner checkpoints + three search seeds + RuleSailor, 71 conditions each; `configs/M4_freeze.json` precedes held-out outcomes. All 2,840 episodes replay exactly; all 36 nominal checkpoint reloads match.*
- [x] Finish the MPC cost/horizon figure, outcome table and representative trajectory analysis. *M3 frontier/prediction figures retained; M4 test/time learning curves, selected/final tables and deterministic first-failure/control figures generated.*
- [x] Make the Nov 24 upwind decision; run all three methods on U only if the stated readiness gate passes. *Decision made early: omit optional U to keep the required R comparison/presentation on scope. This is a scope choice, not a claim that the readiness gate failed.*
- [x] Audit training versus search seeds, pilot/calibration/evaluation cost, hypothetical rollout counts and failed runs. *6M main decisions including 60k calibration; paired calibration/normalization exact. M4 adds 2.117M evaluation decisions / 2.179B hypothetical steps; prior pilot deviations remain disclosed.*
- [x] Regenerate the five required artifacts and identify unsupported/missing claims. *`M4_claims.json` maps figures/tables/media to sources/freezes/seeds/commands; final presentation/archive/rehearsal explicitly remain MLast.*
- [x] **Viz (§10):** batch compositor and video specs: a checkpoint overlay per learner, a controller overlay (PPO/SAC/MPC/RuleSailor), a seed overlay, and the first failing episode per controller. Output gif/mp4 from stored episodes only. *Eight GIF/MP4 pairs, static grid and control plots; no invented MPC/RuleSailor failures. Fixed specs reproduce first frames and frame counts.*

**Exit (met 2026-11-13; see [M4 report](M4_report.md)):** reproducible core results exist. After Nov 28, add no core method, reward, wind model or task. BPTT remains deferred until the presentation and reproduction work below are finished too.

### M4U · Upwind comparison with an expert-data arm · 12–16 h

Reopens the optional U experiment (E4), which M4 deferred to protect the R comparison. R's results, freezes and stores are not touched. **MLast's dates do not move: if M4U is not complete by Nov 28, stop. MLast then presents R only, and U becomes post-presentation work.**

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
- [x] MPC on U validation: H=40 and H=160, three search seeds each. Selection is predeclared as in M3 (most finishes, then lower p50 latency). Only the selected setting goes to held-out sets; H=160 is labeled offline if selected. *H=40 0/60 (59 truncated, 1 out of bounds), H=160 60/60. H=160 selected and labeled offline; `configs/M4U_mpc_freeze.json`.*
- [x] Write `configs/M4U_train_freeze.json` (arms, settings, seeds, demonstration hash) **before** the 12 main learner runs. Queue the runs and retain failed ones. *Written, then all 12 launched concurrently 2026-11-14 00:43 (+08:00) under `runs/ese3990/M4U/main/`.*
- [x] Write the held-out freeze before opening test/shift, as in M4. Evaluate every predeclared checkpoint, the selected MPC and RuleSailor on nominal/test/shift, then run the exact-replay audit. *76 groups, 5,396 episodes; all replay exactly (`runs/ese3990/M4U/heldout/replay_audit.json`). Selected test finishes: PPO, PPO + BC, BC only, SAC 0/150 each; SAC + demonstrations 5/150; MPC H=160 150/150 test, 60/60 shift; RuleSailor 38/50, 2/20.*
- [x] U diagnostics from replay, with thresholds frozen before held-out results: tacks completed, time in irons, mark misses, out-of-bounds exits, and final distance for timeouts. *All 5,396 held-out episodes ([M4U_diagnostics.md](M4U_diagnostics.md)), plus the validation sweep ([M4U_mpc_sweep_diagnostics.json](M4U_mpc_sweep_diagnostics.json)). Episodes with a tack (selected arms, 213 each): SAC + demonstrations 87, PPO 49, PPO + BC 22, SAC 1, BC only 0. MPC H=40 never attempts a tack (0/60).*
- [x] Optional, if the binding exposes RuleSailor's mode: predict that mode from the 17 learner features on the demonstration episodes. This tests whether its memory adds information the learners lack. *[M4U_mode_check.json](M4U_mode_check.json): a 2×64 MLP reaches 0.82 balanced accuracy on held-out demonstration episodes (linear 0.38, majority 0.14). The features carry most of the mode.*
- [x] `M4U_report.md`, claim-index entries, and handoff `docs/handoffs/ese3990-m4u.md`. *[M4U report](M4U_report.md), [M4U_claims.json](M4U_claims.json). Readings: the horizon-bound reading applies; demonstrations removed the tack-discovery bottleneck only in part (5/150 finishes, decayed to 0 by 4M); RL did not improve on RuleSailor. The 2,882 R/viz_demo episodes still replay exactly.*

**Schedule.**

1. Contract version, plumbing and tests
2. Demonstrations; RuleSailor/MPC validation (≤5 h); training freeze; launch the main runs (≈1.5 h wall for six concurrent SAC runs)
3. Held-out freeze and evaluation. MPC H=40 held-out takes ≤3.2 h; H=160 takes ≤13 h if selected
4. Analysis, figures and media
5. Report and handoff

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

**Not in M4U:** a learned value tail, MPC with RuleSailor as its base policy, residual RL, a γ sweep or any other U tuning, and BPTT. *(2026-11-14: the value tail and a critic-LayerNorm arm are scheduled as M4X, outside M4U's freezes; BPTT moves to after Dec 3.)*

**Exit:** every arm has a valid U evaluation that replays exactly; R evidence still replays unchanged; the report states which reading applies.

### M4X · Explain the upwind failures; let learning shorten planning · 8–12 h

Added 2026-11-14, after M4U's training runs and MPC sweep and before any M4X run. M4/M4U freezes, rows and stores are not touched. U's validation outcomes are already open, so **every M4X row is post-hoc and labeled exploratory**, and is never merged into an M4/M4U table. **Start gate:** M4U's held-out evaluation, replay audit and report are done. **Stop rule:** MLast's dates do not move; whatever is unfinished on Nov 28 is presented as planned work.

**Evidence that motivates M4X**, all from existing logs:

| Observation | Source |
|---|---|
| SAC's critic stays bounded on R and blows up on U. Max \|Q\| over each run is 18–25 on R, 2,200–5,000 for SAC from scratch on U and 6,900–9,100 for SAC + demonstrations. The three demonstration runs end with mean Q of +743 to +3,314; no U return from any state can exceed about +16 (finish bonus plus the largest shaping gain) | `metrics.jsonl` of all 9 SAC main runs (3 R, 6 U) |
| The learners settle on running out the clock. A timeout returns about −5.3 from the start; a failure costs −10. 357 of SAC-from-scratch's 360 U validation episodes (all checkpoints, all seeds) are timeouts. PPO did the same on R ([M1 report](M1_report.md) §4) | [M4U_main_audit.json](M4U_main_audit.json) |
| SAC + demonstrations learned, then lost it: seed 0 reached 7/20 validation finishes at 1M decisions and 0/20 from 2M on | Same audit |
| The learners are not missing RuleSailor's information: its hidden mode is recoverable from their 17 features (M4U mode check, 0.82 balanced accuracy) | [M4U_mode_check.json](M4U_mode_check.json) |
| Planning is horizon-bound on U: H=40 0/60, H=160 60/60 on validation, at ≈48 vs ≈193 ms p50 per decision. M4U's declared reading "a value tail is the next step" applies | M4U MPC sweep |

**Arms.** Each differs from its M4U parent in one declared change only.

| Arm | Question | The one change |
|---|---|---|
| **X1** SAC + demonstrations + critic LayerNorm | Is SAC's upwind failure caused by its critic blowing up? | LayerNorm after each hidden layer of both critics and their targets, as in RLPD (Ball et al., 2023), the component M4U deliberately omitted. Actor, demonstration store (`69575c91…`), seeds [0, 1, 2], 4M budget, 400k replay, γ, checkpoints and selection rule unchanged |
| **X1b** SAC + critic LayerNorm, from scratch (optional) | Does a stable critic help without demonstrations? | The same LayerNorm change on M4U's SAC from scratch |
| **X2** CEM-MPC H=40 + value tail | Can a learned value replace foresight, in real time? | Score adds γ^H·V(φ(s_H)) for candidates still running after H decisions. No tail after a finish, failure or the time limit, as in M3. V is the critic of PPO + BC's initial checkpoint for the same seed: already fitted by regression to the 50 demonstrations' returns-to-go under the U contract (M4U). It needs no new fitting and has no bootstrapping. Search seed s uses PPO + BC seed s's V and normalization. K, I, blocks and warm start unchanged |

**Thresholds declared before results.** A critic is *bounded* if its max \|Q\| stays ≤ 250 (10× R's largest) and its mean Q never exceeds +20 during the run. *Real time* means p50 < 50 ms, with 50 ms deadline misses reported as in M3/M4. X2 records predicted versus executed return, so a planner exploiting V's errors is visible.

*Amended 2026-11-17, before any X run (user decision):* SAC's critic estimates a soft value, which includes the discounted entropy term α·H·γ/(1−γ). At γ = 0.99975, with H at its −2 target, an accurate critic reads about task return − 8000·α, so the rule above cannot tell a blow-up from a large α. The freeze keeps that rule and adds an **entropy-adjusted** one, which the readings use. Its final form is freeze amendment 1, written before any X1 run and before any M4X result was read: subtracting the current window's o = α·H·γ/(1−γ) called a lagging critic a blow-up. Each window's o is now computed from 400k decisions on. A critic is bounded if Q stays within 250 (max/min) and +20 (mean) of the interval [min(0, o), max(0, o)] spanned by those offsets. New runs log per-window max and min Q for it. [`configs/M4X_freeze.json`](../../experiments/ese3990/configs/M4X_freeze.json), [`M4X_freeze_amendment1.json`](../../experiments/ese3990/configs/M4X_freeze_amendment1.json)

- [ ] *Skipped 2026-11-17 (M4X closed early, user decision; see [M4X report](M4X_report.md#not-done-skipped-2026-11-17)):* the figure file was not generated; the numbers for all 15 SAC runs are in the M4X report. Critic-scale figure for all 9 SAC main runs (3 R, 6 U): max \|Q\| and mean Q against decisions, with the feasible return range marked. Existing logs only.
- [x] Optional critic LayerNorm in `sac.py`, default off, so every existing config and checkpoint reloads unchanged. Unit test; the existing experiment tests still pass.
- [x] Optional terminal value in `mpc.py`, default off, so the frozen planners are unchanged. Unit test: the tail is added only to candidates still running at H; the existing MPC tests still pass.
- [x] V check on RuleSailor's 20 U validation episodes (never used in fitting): returns-to-go error versus a constant predictor. Reported either way; V is not refitted on planner outcomes.
- [x] Write `configs/M4X_freeze.json` (arms, seeds, V checkpoint hashes, the thresholds above) **before** any X run.
- [ ] Validation and selection done for X1, X1b and X2. *Skipped 2026-11-17 (M4X closed early, user decision; see [M4X report](M4X_report.md#not-done-skipped-2026-11-17)):* the held-out stage was stopped at 2,763 of 2,769 episodes, and the replay audit was not run. Run X1 (X1b if time) and X2 on U validation; select learner checkpoints by M4U's rule. Then evaluate on U test/shift, labeled exploratory, and run the exact-replay audit.
- [ ] `M4X_report.md` (closed, with a not-done list) and the handoff are written. *Skipped 2026-11-17 (M4X closed early, user decision; see [M4X report](M4X_report.md#not-done-skipped-2026-11-17)):* claim-index entries and the generated report files. `M4X_report.md`, claim-index entries and handoff `docs/handoffs/ese3990-m4x.md`.

Compute is small: three concurrent SAC runs take ≈100 min; X2 on validation takes about an hour. **Cut order:** X1b, then X2's held-out evaluation (keep validation), then X1's held-out evaluation.

**Readings declared before results.**

| Observation | Reading |
|---|---|
| X1 bounded, and finishes > SAC + demonstrations | The critic blowing up was a cause of the upwind failure |
| X1 bounded, finishes unchanged | The blow-up was a symptom; the bottleneck is elsewhere (exploration, the timeout trap) |
| X1 still unbounded | LayerNorm alone does not stabilize SAC at γ = 0.99975; report it, with no further fixes in M4X |
| X2 finishes ≈ H=160 in real time | A learned value substitutes for foresight: learning buys the planner back real time |
| X2 tacks but finishes less often than H=160 | V points the right way but is imprecise near the gate |
| X2 never tacks, like H=40 | V does not encode the worth of a tack; the horizon gap remains |

**Ideas considered on 2026-11-14 and not scheduled.**

| Idea | Decision | Reason |
|---|---|---|
| BPTT through a learned world model (§9) | After Dec 3 | Its default horizon (1.6 s) is shorter than the 2 s at which even the exact-model planner never tacks, so on U it would rest on a bootstrapped critic, the part observed to blow up. The model would also have to represent stalls and gate events, its worst case. On R, SAC and MPC already succeed. Fitting alone was budgeted at two days |
| Multi-gate or trajectory-following tasks, random wind | Not scheduled | No learner clears one upwind gate reliably, so more legs add no information. Tasks and wind models freeze after Nov 28 (M4 exit); gusty wind breaks §3.3's information contract |
| Deliberate partial observation | Not scheduled | The mode check shows the features already carry RuleSailor's hidden mode; removing information would predictably make the learners worse |
| General tuning of PPO/SAC | Replaced by X1 | With U's outcomes open, only single, hypothesis-driven changes labeled post-hoc are defensible |

**Not in M4X:** BPTT-WM, a γ/α/learning-rate sweep, LayerNorm for PPO, refitting V, new tasks, wind models or observation sets, and replacing any M4/M4U row.

**Exit:** every M4X arm that ran has a validation result, labeled exploratory; held-out results, if run, replay exactly; the report states which reading applies.

### M4P · Exploration probe: can undirected noise find a tack? · 3–5 h

Added 2026-11-17, after M4X's training runs and before any probe episode. **No learning.** M4P measures whether the kind of exploration a learner starts with could ever produce the behaviour U needs. Sparse-outcome RL mostly amplifies successes its own sampling already produces. If random exploration never tacks, choosing a better learner is not the next step. Every row is post-hoc and labeled exploratory. No task, reward, condition manifest, controller or M4/M4U/M4X freeze changes. Probe code goes in new files only, so `baselines.py` and the other M4X hash-frozen sources stay untouched. **Start gate:** M4X's report is done, and its held-out stage no longer uses the CPU. *(2026-11-17: met by closing M4X early; its held-out stage was stopped at 10:58.)* **Stop rule:** as for M4X, MLast's dates do not move.

**Evidence that motivates M4P**, from existing results and the literature:

| Observation | Source |
|---|---|
| U decides at 20 Hz, so 3,600 decisions per episode. A tack pays off 2–8 s later, i.e. after 40–160 decisions: MPC never tacks at H=40 and always finishes at H=160 | [M0 report](M0_report.md); M4U MPC sweep |
| The learners explore with independent per-decision noise on rudder and sheet *rates*. White Gaussian rate noise resampled at 20 Hz "mostly averages out in the actuators" | [M1 report](M1_report.md) §4 |
| PPO from scratch crosses head to wind in 40/150 test episodes, but its furthest upwind progress is 8.2 m of 40. SAC from scratch tacks in 1/213 | [M4U report](M4U_report.md) |
| The planner that finishes holds each action for 0.5 s | M3/M4U planner settings |
| On MountainCar with an action cost, a "do nothing" trap like U's timeout, the noise process alone turns DDPG/TD3/SAC from failure into success (Hollenstein et al., 2022). Pink noise is the recommended default for off-policy learners (Eberhard et al., 2023) | Literature |

**Probe grid**, frozen in `configs/M4P_freeze.json` before any probe episode:

| Factor | Levels |
|---|---|
| Noise colour (power spectrum ∝ 1/f^β per action dimension, drawn per episode) | white β = 0 (the learners' per-decision noise), pink β = 1, red β = 2 |
| Hold (sample-and-hold interval) | 0.05 s (every decision), 0.5 s, 1 s, 2 s, i.e. 1, 10, 20, 40 decisions |
| Scale σ (rates then clipped to [−1, 1]) | 0.3, 0.6, 1.0 |
| Reference cell | M1's calibration controller, `SmoothRandom(hold=10)`: uniform rates held for 0.5 s |

- **Episodes:** 36 cells plus the reference cell, 100 episodes each. Conditions come from `make_conditions.TrainingSampler` with probe seeds; no validation, test or shift conditions are used. That is at most 13.3M decisions. Time one cell before the full run.
- **Recording:** every action passes through `ActionAdapter`, and every episode is stored and replays exactly.
- **Measures per cell,** from `u_diagnostics` with the frozen M4U thresholds:
  - episodes with a settled tack;
  - failed tacks and time in irons;
  - mean maximum y, and the share of episodes reaching y ≥ 20 m;
  - outcomes: finish, out of bounds, capsize, mark missed, timeout.

**Threshold declared before results.** A process *reaches* if ≥ 10% of its episodes contain a settled tack and ≤ 10% end in a failure (out of bounds, capsize or mark missed).

**Readings declared before results.**

| Observation | Reading |
|---|---|
| White noise at a 0.05 s hold already reaches | Exploration noise is not U's bottleneck. The next arm targets the critic and the decision horizon: entropy kept out of the critic target, 0.5 s action blocks |
| White noise at 0.05 s does not reach, but a correlated or held process does | Per-decision white noise is a cause. That process (colour, hold, scale) is the exploration setting for the next RL arm |
| Tacks appear only together with more than 10% failures | Coherent exploration runs into terminal states. Start-state curricula or resets come before noise changes |
| No cell reaches 10% tacks | Undirected exploration cannot find the tack. The next step is reset-based discovery (a reverse-start curriculum, Go-Explore), labeled as using simulator resets |
| Any random finish | Reported with its process. It is the success rate a sparse-outcome learner would start from |

- [x] *(No tack, failure or irons in the 9 calibration episodes; mean max y 4.5 m.)* First look, at zero cost: `u_diagnostics` on the M4U learners' stored calibration episodes (`SmoothRandom(hold=10)`, 10,000 decisions per run). Did any contain a tack?
- [x] New `probe.py`: power-law noise via FFT (numpy only), sample-and-hold, runner and episode stores. Unit tests:
  - spectral slope within ±0.2 of β;
  - actions constant within each hold window;
  - same seed, same actions;
  - a short probe episode replays exactly.
- [x] Write `configs/M4P_freeze.json` (grid, seeds, episodes, threshold, readings, source hashes) **before** any probe episode. *(2026-11-17 03:05 UTC.)*
- [x] Run the grid and the replay audit. *(3,700 episodes in 85 s on 12 cores; all replay exactly. One cell was rerun after a pilot collision; see the report.)*
- [x] [`M4P_report.md`](M4P_report.md) (tack and failure rates over hold × colour for each scale, and which reading applies), claim-index entries ([`M4P_claims.json`](M4P_claims.json)) and handoff `docs/handoffs/ese3990-m4p.md`.

**Not in M4P:** any training run, reward or task change, noise processes outside the grid, or reset-based exploration. Those are candidates for later work after Dec 3, chosen by M4P's reading. *(Amended 2026-11-17: two training-side changes are scheduled as [M5](#m5--upwind-without-demonstrations-decision-interval-and-start-distribution--610-h).)*

**Exit:** every cell has a stored, replayed result, labeled exploratory; the report states which reading applies.

### M5 · Upwind without demonstrations: decision interval and start distribution · 6–10 h

Added 2026-11-17, after M4P and before any M5 run. Every row is post-hoc and labeled exploratory, and none is merged into an M4/M4U/M4X/M4P table. No task, reward, condition manifest or earlier freeze changes. New code goes in new files only, because `sailing.py`, `evaluate.py` and `probe.py` are hash-frozen. This amends M4P's "later work after Dec 3" line and M4X's "general tuning of PPO/SAC: replaced by X1" decision: M5 is two hypothesis-driven changes, not a sweep. **Start gate:** M4P's report is done *(met 2026-11-17)*.

**Question.** Which of two training-side changes lets PPO or SAC finish U from scratch: deciding every 0.5 s, or seeing starts from which a finish is reachable?

**Evidence that motivates M5.**

| Observation | Source |
|---|---|
| The planner holds each action for 0.5 s and finishes 150/150 on test. Bink's SAC learned to tack from scratch at 0.5 s decisions | [M4U report](M4U_report.md); `reports/RL methods for sparse upwind sailing.md` |
| PPO's GAE window is 1/(1 − γλ) ≈ 20 decisions = 1 s at γ = 0.99975, λ = 0.95. The exact-model planner never tacks at a 2 s horizon | `train.py` config; M4U MPC sweep |
| A PPO rollout is 2,048 decisions from one environment, about 0.57 of one U episode (3,600 decisions) | `train.py` config |
| No random process finishes from the frozen start: 0/3,700 episodes | [M4P report](M4P_report.md) |
| At 0.5 s blocks, SAC's entropy charge for a 50 s finish is about 177·α instead of 1,770·α, so the α above which the soft value prefers failing now to finishing rises from about 0.011 to about 0.11. U's SAC runs ended at α = 0.018–0.096 | Arithmetic on [M4X report](M4X_report.md) § Critic scale |

**Factors**, each applied identically to PPO and SAC.

| Factor | Off (M4U) | On |
|---|---|---|
| **B** decision blocks | One action per 0.05 s, γ = 0.99975 | Each action is held for 10 decisions (0.5 s). The block reward is the discounted sum of its 10 per-decision rewards and γ per block is 0.99975¹⁰ ≈ 0.9975, so the objective is unchanged. A block ends early at a finish, failure or the time limit. Every other setting keeps its value, now counted in blocks (PPO rollout 2,048 blocks; SAC replay 400k blocks, one update per block) |
| **S** training starts | Frozen start (0, 0) | Half of the training episodes use the frozen start. The other half draw x ∈ [−10, 10] m, y ∈ [25, 39] m and a uniform heading, at rest. Wind is sampled as before. Calibration and every evaluation use the frozen start |

The original S draft box, x ∈ [−25, 25] m and y ∈ [0, 35] m, failed the no-learning start-box check (0/100 finishes). Before the main freeze, it was amended to x ∈ [−10, 10] m and y ∈ [25, 39] m, which passed (2/100). All 200 episodes replay exactly; both checks are retained in [M5_start_box.json](M5_start_box.json). Calibration remains the common 10,000 raw decisions at the frozen start in every cell; B cells aggregate those transitions for SAC replay.

**Arms.** 2 learners × 4 cells (neither, B, S, B + S) × training seeds [0, 1, 2, 3, 4]: 40 runs, 34 of them new. The "neither" cell reuses M4U's seeds 0–2 and adds seeds 3–4 under the M4U config, as M5 rows. Each run gets 4M decisions, calibration included, which is 400k blocks in the B cells. PPO and SAC are matched by decisions and seeds, not by CPU time (§5, resource fairness).

**Disclosed consequence of B.** At equal decisions the B cells make ten times fewer gradient updates: about 195 PPO updates and 400k SAC updates. PPO's B and B + S cells therefore get one extra row each at **40M decisions** (about 1,950 updates, M4U's count), labeled as an extended budget and never compared with a 4M row as if equal: 10 more runs, about 70 min each by M4U's PPO timing.

**Evaluation.** Every arm is evaluated on the frozen U contract: frozen start, 20 Hz, γ = 0.99975, the M4U validation set at M4U's checkpoints (in decisions), and M4U's selection rule. A B-cell policy is queried every 10 decisions and its action held. Held-out test/shift is opened only for selected and final checkpoints, after a held-out freeze.

- [x] Start-box check, no learning: M4P's best noise cell (pink, 0.5 s hold, σ = 0.3) from the S start box, 100 episodes. If fewer than 1% finish, amend the box (recorded) before the freeze.
- [x] Block wrapper, start sampler and a held-action evaluation controller, in new files. *`m5.py`, `m5_train.py`, `m5_study.py`; 146 experiment tests pass.* Unit tests: the block return equals the 20 Hz discounted return for the same actions; a stored block episode replays exactly at 20 Hz; the start sampler never emits a held-out condition; the existing experiment tests still pass.
- [x] One 100k smoke run per cell on pilot seed 100, for plumbing and timing only. *All eight runs complete (PPO and SAC × four cells), 572/572 stored episodes replay exactly; budgets, validation aggregates and saved-policy nominal reloads agree.* Not used for tuning or reported as results.
- [x] Write `configs/M5_freeze.json` (cells, seeds, start box, thresholds, readings, source hashes) **before** any main run. *Frozen 2026-11-17 at 03:53:58 UTC. The 34-new-run 4M queue started at 03:54:12 UTC with six workers; the 40M queue is separate.*
- [x] Main runs, 4M rows first, then PPO's 40M rows. Retain failed runs. *All 34 new 4M rows and the ten 40M PPO rows finished on 2026-11-17, with no failed run.*
- [x] Held-out freeze, test/shift for selected and final checkpoints, replay audit, `u_diagnostics`, and per-run rudder travel to catch sculling. *Done on 2026-11-17 by `m5_followup.py`. 89 selected/final groups were frozen before any test/shift episode, and all 6,319 episodes replay exactly. Including the training audits, 169,586 episodes replay exactly. There are no finishes, so no sculling finish exists to flag.*
- [x] `M5_report.md`, claim-index entries and handoff `docs/handoffs/ese3990-m5.md`. *[M5 report](M5_report.md); [`M5_results.json`](M5_results.json), generated by `m5_report.py` from the audits; [`M5_claims.json`](M5_claims.json).*

**Automatic continuation (queued 2026-11-17 at 07:04 UTC, user request).** After the existing main queue finishes, `m5_followup.py` runs the ten extended PPO jobs, parallel main/extended replay audits (including the six reused M4U rows), selected/final held-out freeze and evaluation, and exact held-out replay with behaviour/rudder-travel diagnostics. The follow-on code passed 158 experiment tests and a six-episode real-checkpoint nominal/validation preflight. Its new sources are sealed in `runs/ese3990/M5/followup_plan.json`; training sources/freezes are unchanged. Progress and failure/restart details are in the [M5 handoff](../handoffs/ese3990-m5.md). *Completed on 2026-11-17 at 10:20 UTC, with no failed stage.*

**Cut order:** PPO's 40M rows, then held-out evaluation (keep validation), then seeds 3–4.

**Threshold declared before results.** A cell *finishes* if at least 3 of its 5 seeds have a selected checkpoint with ≥ 1/20 validation finishes. Also reported per cell: episodes with a tack, mean max y, SAC's Q against the feasible band [−22.3, +15.7], and rudder travel.

**Readings declared before results.** They are read on the 4M rows; the 40M rows are reported beside them.

| Observation | Reading |
|---|---|
| B finishes, S does not | The decision interval was the bottleneck |
| S finishes, B does not | Discovering a first finish was the bottleneck |
| Only B + S finishes | Both are needed, as in Bink's setup |
| No cell finishes | Neither change is sufficient; run the second stage |
| A PPO cell finishes only at 40M | PPO's B result is update-bound, not decision-interval-bound |
| One learner finishes where the other does not | Report it with the Q band and tack diagnostics; no ranking claim beyond these seeds |

**Second stage, only if the readings call for it.** One change each, on that learner's best cell, under its own freeze: for SAC, the entropy term dropped from the critic target, then pink exploration noise at σ = 0.3; for PPO, 8 environments per update, then initial σ = 0.3.

**Stage-1 result (2026-11-17).** The reading "no cell finishes" fired for both learners: 0/1,000 selected validation episodes and 0/6,319 held-out episodes finish ([M5 report](M5_report.md)). On 2026-11-17 the user chose to have the second stage designed but not run. The exploratory design below was subsequently approved for execution on November 18.

#### M5 second stage (M5b) · complete November 18

The approved protocol uses the “neither” baseline for both learners, additive pink noise at fixed σ = 0.3 and 8 × 256 PPO rollouts. All four pilot runs passed, and 296 stored episodes replayed exactly. The [M5b freeze](../../experiments/ese3990/configs/M5b_freeze.json) was written before the first main run. [Execution handoff](../handoffs/ese3990-m5b.md) · [Live status](../../runs/ese3990/M5b/status.json)

**Best cell.** The plan does not define a best cell, and all 40 stage-1 rows have zero finishes, so any rule chosen now is post-hoc. Proposed rule: rank cells by (1) seeds meeting the finish threshold, then (2) selected-checkpoint validation episodes with a settled tack, then (3) mean selected max y. Validation return is not used: every cell's mean lies between −5.17 and −5.34, so it ranks near-identical timeouts.

- **PPO → "neither"**: 20/100 tack episodes and 10.1 m, against at most 2 and 4.8 m for the other cells.
- **SAC → "neither"**: 1/100 tack episodes and 3.7 m, against 0 and 2.6 m for B. A one-episode margin is noise. The deciding reason is mechanical: in SAC B, α ends at ≤ 0.0006 and Q stays in the band, so removing the entropy term from the critic target would change almost nothing there.
- **Baselines** are the stage-1 "neither" rows, seeds 0–4. Nothing is rerun.

**Arms.** Each arm makes one change to its learner's best cell. All arms use seeds [0, 1, 2, 3, 4], 4M decisions, and M4U's configuration and checkpoints otherwise.

| Arm | Change | Code (new files only) | Updates at 4M |
|---|---|---|---|
| SAC-E | Critic target without the entropy term: y = r + γ·b·min Q̄(s′, a′), a′ ~ π(·\|s′). Actor and α losses are unchanged | A variant of `sac.soft_target` and `make_update` | 4M, unchanged |
| SAC-P | Pink exploration noise at σ = 0.3, M4P's best process: during collection, a = clip(tanh μ(s) + 0.3·ε, −1, 1), with ε from `probe.powerlaw_noise` (β = 1), one realisation per episode and action dimension. Updates are unchanged | Collection loop only | 4M, unchanged |
| PPO-8 | 8 environments per update at 256 decisions each. The rollout stays 2,048 decisions, so the update count is unchanged. Conditions come from the same training stream | Collection loop only | about 1,950, unchanged |
| PPO-σ | Initial σ = 0.3: `log_std` = ln 0.3 ≈ −1.20 instead of −0.5 (σ ≈ 0.61) | Configuration only | about 1,950, unchanged |

**Readings, to be frozen before results.** The threshold is stage 1's: an arm *finishes* if at least 3 of its 5 seeds have a selected checkpoint with at least 1/20 validation finishes.

| Observation | Reading |
|---|---|
| An arm finishes | That change was the blocking factor for that learner at 4M. Report it beside its stage-1 baseline, on the same seeds |
| SAC-E keeps Q in the band but does not finish | Critic scale can be fixed without B, and is still not sufficient, as SAC B showed |
| Both arms of one learner finish | Report both. Stacking them is outside M5 |
| No arm finishes | M5 closes: from-scratch PPO/SAC do not solve U under these budgets and changes. Demonstrations, start curricula that move toward the frozen start, and reset-based exploration move to after Dec 3 |

Each arm also reports the stage-1 diagnostics: tack episodes, mean max y, rudder travel, training finishes, and SAC's Q band and α.

**Points for review before the freeze.**

1. **The best-cell rule.** The alternative is SAC B, which costs 16–18 min per run instead of 119–195 min, but SAC-E tests almost nothing there.
2. **SAC-P's form.** The draft adds M4P's process, at a fixed σ, to the policy's mean action (the literal reading). The alternative is the policy's own learned σ with pink ε in place of Gaussian ε (Eberhard et al., 2023).
3. **PPO-8's shape.** The draft uses 8 × 256 decisions, so only batch diversity changes. The alternative is 8 × 2,048, which gives one-eighth of the updates at 4M.
4. **PPO-σ.** M4P found that per-decision white noise never tacks at σ = 0.3 either. On "neither", PPO-σ therefore tests learning dynamics, not discovery.

**Order and cut.** All four arms are independent and share one queue: SAC-E and PPO-8 first, then SAC-P and PPO-σ. Cut order: PPO-σ, then SAC-P, then held-out evaluation (validation is kept), then seeds 3–4.

**Cost.**

- Implementation and tests: about 3–4 h.
- Ten SAC runs on "neither": about 4–7 h wall with six workers.
- Ten PPO runs: under 1 h.
- Audits and the held-out evaluation: about 1 h.

- [x] Review this draft: the best cell, SAC-P's form and PPO-8's shape.
- [x] New code in new files (`m5b.py` and tests):
  - SAC-E's target equals `sac.soft_target` at α = 0.
  - The pink sampler reproduces `probe.powerlaw_noise` for a given seed.
  - A PPO-8 batch holds 8 × 256 rows from 8 conditions, with GAE cut at each environment boundary.
  - Stored episodes replay exactly.
  - The existing experiment tests still pass.
- [x] A 100k smoke run per arm on pilot seed 100, for plumbing and timing only.
- [x] Write `configs/M5b_freeze.json` (arms, best-cell rule and its result, seeds, budgets, threshold, readings, source hashes) before any main run.
- [x] Main runs, held-out evaluation and replay audits complete. [M5b report](M5b_report.md) · [Claims](M5b_claims.json) · [Handoff](../handoffs/ese3990-m5b.md)

**Not in M5:** reward or timeout changes, demonstrations, simulator-state restore, γ/α/learning-rate sweeps, hierarchy or new action semantics, and replacing any earlier row.

**Exit:** every cell that ran has a validation result that replays exactly, labeled exploratory; the report states which reading applies.

#### M5 SAC budget extension · five seeds, about three hours of CPU training

**Training and validation complete.** All five seeds reached 40M decisions in 2 h 31 min wall time. The five final policies finished 0/100 validation episodes, and no earlier checkpoint finished. The [study writeup](M5_sac_budget_report.md) records the results and links the held-out comparison.

This exploratory study tests whether more training improves gate completion for blocked, from-scratch SAC on U. The comparison pairs stage 1's five 4M SAC B runs with five fresh 40M runs on seeds 0–4. Each uses the standard start, 0.5 s action holds, the original reward and a budget counted in raw 20 Hz decisions. The 10k calibration is included. Five CPU workers run concurrently using the existing M5 trainer. The wall-time estimate covers training and checkpoint validation. Replay auditing and held-out evaluation follow. The four M5b arms use their own freeze.

The [training freeze](../../experiments/ese3990/configs/M5_sac_budget_freeze.json) records the configuration, checkpoint schedule, inputs and outcomes to report before launch. Each run starts from its seed because stage-1 policy snapshots omit optimizer and replay state. The extension retains stage-1 checkpoints and adds 10M, 20M, 30M and 40M. Compare the matching early checkpoints through 3M, and record any difference at 4M from the shorter run's final partial block.

Use the original validation-only selection rule and finish threshold. At least three of five seeds must each finish at least one of 20 validation episodes. Report matched-seed 4M and 40M final-checkpoint results alongside selected results because the longer run has more selection opportunities. Freeze selected and final checkpoint identities before held-out evaluation. Report test and shift finishes, tacking, upwind progress, gate approach, training finishes and critic scale. A tack or training finish alone counts as behavioral evidence. The conclusion about reliably finishing U depends on validation and held-out outcomes. [Launch handoff](../handoffs/ese3990-m5-sac-budget.md)

### M6 · Learn the gate approach with a reverse-start curriculum · planning draft

**Question.** Can a learner that reliably crosses the gate from a few metres away learn to do so from the original 40 m start if training starts move back gradually? M6 is exploratory, uses privileged simulator resets during training, and requires no demonstrations. The November 19 launch authorization brings the first campaign forward from the post-presentation schedule. Its [run record](../handoffs/ese3990-m6-first-campaign.md) gives the start time and links the saved evidence. The distances, allocations and thresholds below are proposals to check on pilots and freeze before main training.

**What to build on.**

| Observation | Implication for M6 |
|---|---|
| At 4M, standard 20 Hz PPO tacked in 20/100 selected validation episodes and 40/250 test episodes. One seed supplied 17 of the 20 validation tacks. Another reached 27.9 m upwind on average on validation while remaining at least 33 m from the gate. Blocked PPO at 40M produced one standard-start training finish, followed by zero evaluation finishes. | Keep the original single-environment PPO as the first learner. It can sustain a turn; the new test is whether it can time the turn and sail the next leg through the gate. Track lateral error and closest approach. [M5 results](M5_report.md) |
| In M5's 20 Hz near-gate arm, PPO and SAC each recorded 11 training finishes. Pooling their eligible random starts gives 22/2,381 finishes from at most 7 m downwind and 0/3,282 from farther away. This is a post-hoc split on longitudinal distance, not distance to the gate centre. | Start with a short, learnable final approach. M5 alternated a fixed box with the full task and never trained a sequence of intermediate starts. The counts establish reachability; a training-only control must establish whether learning beats accidental crossings. [M5 table](M5_report.md#results-main-rows-4m-decisions) · [Episode indexes](../../runs/ese3990/M5/main) |
| SAC with demonstrations could tack and occasionally finish before that skill decayed. From scratch, removing critic entropy kept Q in the reference band in 0/5 seeds, pink noise produced zero selected validation tacks, and 40M blocked decisions produced no training finish. | Use standard 20 Hz SAC on the same curriculum. Its 11 near-gate finishes are the best direct reason to try it. Log whether it retains successes as the start moves back, alongside critic scale. Expert outcomes only inform the hypothesis. [M4U](M4U_report.md) · [M5b](M5b_report.md) · [SAC budget extension](M5_sac_budget_report.md) |

**Proposed first experiment.** Reuse M5's position and heading reset support and the original 20 Hz PPO and SAC configurations. Train fresh weights for each seed. Keep the route, dynamics, learner observations, action space, reward, discount and time limit fixed. The curriculum supplies start positions and headings only. No prior checkpoint, stored action sequence, planner or scripted controller supplies training targets. The design adapts the reverse-curriculum idea to the existing positional resets; its effectiveness on U remains an empirical question. [M5 reset implementation](../../experiments/ese3990/m5.py) · [Research notes](<../../research_notes/RL methods for sparse upwind sailing/exploration_methods.md>) · [Florensa et al., CoRL 2017](https://proceedings.mlr.press/v78/florensa17a.html)

1. **Calibrate the first starts cheaply.** On training-only conditions, try starts at rest 1–3 m below the gate line, with lateral offsets on both sides drawn from the M5 range and headings sampled as in M5-S. Run 100 episodes each with neutral controls and the existing M4P pink-noise probe. Record valid crossings, start side, elapsed time and rudder travel. Check both sides are represented and no episode is already finished at reset. If crossings are essentially absent or passive drift routinely scores, adjust the box on pilot conditions before freezing it.
2. **Pilot transfer before reserving five seeds.** Give each learner a separate seed-100 pilot of up to 400k raw decisions. Candidate stages are 1–3, 3–7, 7–15, 15–30 and 30–40 m below the gate, then the exact original (0, 0) start and original heading distribution. Keep lateral offsets symmetric and include starts on both sides. Allocate 80% of episodes to the current stage and 20% to the preceding stage to check retention; use only the current stage at the first stage and only the original start in the final stage. Use the ordinary training wind stream. Separately record probe decisions and wall time.
3. **Advance on demonstrated learning.** After every 25 completed current-stage training episodes, evaluate the deterministic policy on 20 fresh training-only probe conditions balanced by side. Proposed promotion requires at least 6/20 finishes on two successive probes and a finish rate above both fixed probe controllers for that stage. Measure the previous stage again after promotion. Advance a learner to the main study only if it passes the first stage and finishes on the next stage in its pilot. If it stalls, record where and stop that arm. Pilot geometry and promotion rules are selected without using validation or held-out outcomes.
4. **Main comparison.** For each passing learner, freeze settings and run seeds 0–4 from scratch at 4M raw decisions per seed including the common 10k standard-start calibration. Compare within learner to the saved M5 standard-start and fixed near-gate cells on the same seeds. These test the practical benefit of the whole curriculum; isolating adaptive ordering would require an additional control sampling the same stages in a fixed mixture. Select and evaluate using the original U start and M5 rules. Report both selected and fixed-final policies, then test and shift after freezing checkpoint identities. Exact replay includes the reset geometry.

**Decision and cost.** Retain the M5 criterion of at least 3/5 seeds each finishing at least 1/20 selected validation episodes from the original start. Report held-out transfer separately. Per stage and seed, record finishes, closest gate distance, lateral error at closest approach or at a missed crossing, settled tacks, rudder travel, and SAC's Q range. If a policy learns the last few metres but stalls farther downwind, the useful result is the first distance at which transfer breaks. If it repeatedly succeeds and then loses that ability, a subsequent single-change experiment could retain each seed's own successes in a SAC replay buffer or add a PPO self-imitation loss. M6's first experiment tests the curriculum alone. [Self-imitation notes](<../../research_notes/RL methods for sparse upwind sailing/exploration_methods.md>)

#### First campaign · 150 minutes on the existing CPU machine

**Completed November 19, 2026.** Both seed-100 pilots reached 400k decisions in the first band. PPO finished 5/100 fresh promotion episodes, including one finish with a settled tack, and SAC finished 0/80. Neither passed the first promotion. Final original-start validation was 0/20 for each. The conditional five-seed batch was skipped under the pilot rule. Wall time for the campaign was 8 min 33 s. [First campaign report](M6_first_report.md) · [Results](M6_first_results.json)

**Tack-transfer hypothesis.** Exact replay of the 22 M5 near-gate training finishes found zero classified settled tacks in all 11 PPO finishes and in 10 of 11 SAC finishes. The remaining SAC finish had one. Expect the 1–3 m stage to teach the final approach first. The 3–7 m and 7–15 m bands are the first places to look for a policy that chooses when to tack. For a centered boat sailing a steady 45° ground track, one leg from 3 m below the gate crosses 3 m off-center, inside the 5 m half-width. From 10 m it crosses 10 m off-center and misses. From farther back, the boat can sail longer before turning onto a final leg it already knows. Gate bearing relative to wind, boat speed and heading are all available to the policy. Starting offset, wind and speed through the turn make the switch point vary even at the same depth. [M5 episode stores](../../runs/ese3990/M5/main) · [Tack classifier](../../experiments/ese3990/u_diagnostics.py) · [Policy observations](../../experiments/ese3990/sailing.py)

**Paired transfer probe.** Just before a promotion, freeze the current checkpoint and evaluate it on the next band before collecting any training transitions there. Use a separate training-only panel of 20 starts. Make ten pairs matched on depth, wind and starting heading, with five pairs on each starting tack. Vary lateral offset within the pair so that continuing on the initial close-hauled course projects inside the gate on one start and outside it on the other, with a feasible return leg on the opposite tack. Calibrate these projections against measured ground tracks in the setup pilot and save the geometry before scoring learners. Apply this probe to bands with both geometries. Use the original start distribution for the final transfer to U. Probe labels and trajectories are for scoring only.

For each geometry and side, report finishes / attempts and finishes with a settled tack / attempts. On exact replay record the time, position, gate bearing relative to wind, and speed at the first tack, plus whether the boat regained speed and finished. Repeat the panel after training on the band. Finishing the direct cases on one leg and the missed-course cases after a well-placed tack **before further training** is evidence of transfer. Success only after training on the new band measures local learning. Report the paired probe separately from the fresh conditions used for promotion, including cases where favorable starts alone let a policy pass.

| Elapsed wall time | Work and decision |
|---|---|
| 0–20 min | Start the clock with the first simulator job once the curriculum runner and logging are ready. Check reset/replay on both sides, run the neutral and pink-noise controls from step 1, and verify the paired geometries. Score both controls on the planned promotion stages to establish the stage-specific baselines. Save the reset distributions, probe conditions and promotion rule. |
| 20–50 min | Start fresh PPO and SAC seed-100 pilots concurrently, up to 400k raw decisions each. Record training, promotion-probe and replay time separately. Apply step 3 to each learner. Save the last checkpoint and stage reached if a pilot exhausts its allocation. |
| 50–110 min | If PPO passes, freeze the main settings and run PPO seeds 0–4 concurrently at up to 4M decisions each. Use the measured pilot cost, including probes, to decide whether the batch fits. SAC's first-campaign allocation is its pilot. A passing SAC pilot qualifies it for a separately budgeted five-seed run. If PPO stalls or the batch cannot fit, use this window for replay and analysis of the completed pilots. |
| 110–150 min | Reserve 40 min for original-start evaluation, exact replay of scored trajectories, and a short results table. Stop training at minute 110 at a saved checkpoint. For completed main runs, freeze validation-selected and final checkpoint identities before test and shift. Mark any unfinished evaluation or replay as pending at minute 150. Preserve checkpoints, completed counts and remaining conditions for resumption. |

M5's comparable 4M near-gate runs took about 8 min per PPO seed and 94–136 min per SAC seed with six CPU workers. That supports prioritizing replicated PPO while measuring SAC on the same curriculum. The new promotion probes can dominate wall time when episodes finish quickly. Report actual elapsed time, decisions and stage per seed, paired-probe counts, and original-start finishes against the matched M5 rows. Apply the 3/5 decision only to a completed five-seed main comparison. For a stalled pilot, report the first band it failed to pass and whether the observed failures were missed turns, stalls during the turn, or missed gates after recovery. [M5 timings](M5_report.md#results-main-rows-4m-decisions)

#### M6 follow-up · Learn the first approach before moving downwind

**Completed November 19, 2026.** Nine 400k-decision PPO runs finished in the three planned geometries. Each geometry qualified 0/3 seeds. On identical starts the earlier seed-100 policy lost all 16 of its initial finishes by 219,759 decisions. All nine final policies finished 0/20 on original-start validation. The conditional transfer and SAC branches stayed closed. [Follow-up report](M6_width_report.md) · [Numerical results](M6_width_results.json)

**Question.** Does narrowing the first-stage lateral offset help a from-scratch policy learn a repeatable finish? In the first seed-100 PPO campaign, the fresh probes yielded 5/54 finishes from starts inside the gate's lateral span and 0/46 outside. This post-hoc split motivates the experiment. [First campaign](M6_first_report.md)

1. **Check retention on identical starts.** Freeze 80 training-only broad starts in mirrored pairs, half inside the gate span and half outside. Rescore all five saved seed-100 PPO probe checkpoints and the fixed final policy. Report inside/outside counts and which individual conditions gained or lost a finish.
2. **Isolate width.** Train PPO from scratch with lateral offsets uniform within ±3 m or ±10 m, on matched seeds 0, 1 and 2, for 400k raw decisions per run including the common 10k standard-start calibration. Hold depth at 1–3 m for the full run. Keep uniform reset headings, training wind, 20 Hz actions, at-rest dynamics and the U reward. Use the same random-number streams within each seed. Score each fixed final policy on the same narrow and broad panels, with matched wind, heading, depth and scaled lateral offset. Score neutral and pink-noise controls on 100 training-only starts per geometry. The broader panel and original 40 m validation remain separate outcomes.
3. **Decide promotion on fresh conditions.** Every 25 training episodes, use 20 new starts balanced by side. A seed qualifies on two successive probes with at least 6/20 finishes each and a finish rate strictly above both controls on its training geometry. Keep the first qualifying snapshot. If at least two of the three narrow PPO seeds qualify, score those snapshots on the frozen 3–7 m direct/switch pairs before any training at that depth. Count direct finishes without a settled tack and switch finishes with one, on both boards. Run one fresh 400k-decision SAC pilot on that geometry (seed 100).
4. **Conditional heading pilot.** If fewer than two narrow PPO seeds qualify, keep the narrow width and rerun the three PPO seeds with the only training change being the initial heading. Start at ±45° to the wind on a board whose measured ground track projects through the gate; sample either board when both work. Recalibrate controls and promotion probes on those headings. Score the common random-heading panels as before and also a fixed matched heading-specific panel to measure what was learned locally. Apply the same two-of-three transfer and SAC rule. A later widening of headings depends on this pilot demonstrating repeatable learning. This campaign closes after these predefined branches.

Checkpoint, panel and training-source identities are frozen before scoring. Report one row per seed and arm, all promotion probes, the fixed-final original-start validation and exact replay of scored episodes. [Campaign runs](../../runs/ese3990/M6/width)

#### M6 follow-up · Opening maneuver and PPO self-imitation

**Completed November 19, 2026.** Switching from early to late PPO after 2 s lost all 16 early finishes. The reverse switch recovered 10. At 400k decisions, self-imitation retained 12/17 previously successful seed–start pairs versus 6/17 for ordinary PPO, and finished 26/240 versus 3/240 fresh near-gate evaluations. Two seeds supplied the improvement. No seed met the repeated promotion threshold, and original-start validation remained 0/60 per arm. [Campaign report](M6_imitation_report.md) · [Numerical results](M6_imitation_results.json)

**Opening test.** On the same 80 mirrored broad starts, run the seed-100 policies from 77,991 and 400,000 decisions uninterrupted and in both orders. Switch once after 40 decisions (2 s); both policies act on the live state. Verify uninterrupted action sequences against the width campaign. Score all 80 starts and separately the 16 original early finishes. Report recovery, losses and new finishes by condition.

**Training test.** Match the broad, random-heading PPO baselines on seeds 0–2 at 400k decisions. An entire successful online training episode enters its own seed's buffer once it finishes. Add a single actor loss to each regular PPO minibatch, with coefficient 0.1, using an independently sampled uniform minibatch of buffered transitions. Weight the original action's negative log probability by the positive part of discounted return-to-go minus current value, detached from the gradient. Preserve the ordinary PPO optimizer, number of updates, environment, reward and reset sequence. Record first success, buffer growth, and sampled advantage. Probe and validation experience is exclusively for scoring.

**Reading.** Before the first replay-weighted update, freeze a common policy per seed and verify the pre-intervention training prefix against the saved baseline. Score its 80 broad starts and count the same finishes retained by each arm at 400k. Also report retention from each arm's first promotion-probe snapshot, fixed-final performance on an independent fresh 80-start panel, fresh promotion probes, and original 40 m validation. Run the 3–7 m paired transfer on first qualifying snapshots if at least two imitation seeds meet the existing fresh-probe rule and pooled retained common-anchor finishes exceed baseline. An anchor with zero finishes supplies no observable retention cases. [Campaign run](../../runs/ese3990/M6/imitation)

#### M6 follow-up · Simulator-verified mirrored self-imitation

**Completed November 19, 2026.** All 65 mirrored attempts finished, costing 19,159 extra simulator decisions. On the original diagnostic panel seed 2 improved west from 0/40 to 3/40 and fell east from 11/40 to 9/40. On the new panel west improved from 1/40 to 13/40 and east went from 4/40 to 3/40. On the 40 mirrored pairs, seed 2 finished both sides of 11 pairs, up from zero. Across three seeds, new-panel finishes rose from 13/240 to 30/240. No seed passed promotion and original-start validation remained 0/60 per arm. [Campaign report](M6_mirror_report.md) · [Numerical results](M6_mirror_results.json)

Run broad-start PPO at 400k online decisions on matched seeds 0–2. After every successful training episode, reflect its lateral offset, wind bearing and heading across the gate centerline. Replay the commands with reversed rudder rate and unchanged sheet rate for at most their original duration. Store the simulator's actual states, pre-squash reflected actions and discounted rewards in that seed's imitation buffer only after a verified finish. Keep uniform replay sampling, minibatch and PPO update count, coefficient 0.1, original reward and all promotion rules. Count every extra mirror simulator decision separately, including attempts that run out of actions.

Compare final ordinary and mirrored self-imitation on the archived 40 mirrored start pairs, the earlier balanced panel where seed 2 scored west 0/40 and east 11/40, and a newly frozen panel with 40 starts per side. Report west and east separately for each seed, the number of mirrored pairs finished on both sides, and original 40 m validation. The primary question is whether seed 2 gains west-side finishes while retaining east-side finishes. At least two seeds must qualify on the existing fresh promotion probes to open the 3–7 m transfer panel. [Campaign directory](../../runs/ese3990/M6/mirror)

#### M6 tuning · Learner-side changes from the original start · November 19

**Completed November 19, 2026.** This is a post-hoc, exploratory pilot campaign under the user's 8-hour tuning goal. Route, dynamics, reward, discount, time limit and the 20 Hz rate actions sent to Rust are unchanged. It used no demonstrations, and the final configuration uses no privileged resets. Training starts at the original (0, 0) start. Four learner-side changes together made from-scratch PPO learn U:

- fixed physical feature scaling instead of the calibration statistics, which clip starboard-tack headings and near-gate positions;
- 0.5 s decisions;
- 2 × 256 networks;
- a rudder-angle/sheet-length target head with a dead-beat inner loop on the observed actuators.

At 10M decisions, 9/10 seeds finished every held-out episode: 180/200 validation, 450/500 test and 180/200 shift overall. Every finish contained one settled tack. Mean finish time was 40 s, against 49 s for offline MPC H = 160. At the 4M main budget, 7/10 seeds met M5's threshold, with 293/500 test finishes. With the original rate actions and initial log σ −1.5 instead of the target head, PPO passed on 5/5 seeds, finishing 195/250 test episodes. With calibration scaling, the M5 B cell (vectorized) stayed at 0/40 through 20M. At 2 × 64, about half the seeds learned. SAC with the same changes passed on 4/5 seeds at 10M: 73/100 validation and 183/250 test, but only 27/100 on shift. Checkpoints were selected on a training-distribution pilot panel before validation, test and shift were opened. [Tuning report](M6_tune_report.md) · [Results](M6_tune_results.json) · [Run record](../handoffs/ese3990-m6-tune.md)

### M7 · Channel beat: string several tacks together · started November 20

**Question:** does the M6 tuning recipe go from one tack to a chain of tacks when space forces them? Post-hoc and exploratory, like M6 tuning.

**Task:** U with its x bounds narrowed from [−80, 120] to [−10, 10] m. Route, start, gate, reward, discount, the 180 s budget and the 20 Hz rate actions sent to Rust are unchanged. Leaving the channel is the existing out-of-bounds failure (−10). The features already contain x through the target offset, so a fixed-width channel needs no new observation.

**Evidence before training** (November 20):

- *Zero-shot.* The nine converged M6 PPO policies finish 0/450 U test episodes at ±15, ±10 and ±7.5 m; every episode goes out of bounds. In open water they sail about 21 m to one side and tack once on the layline. The same policies finish 449/450 under the default gust field and 395/450 with the gate moved to 80 m.
- *Feasibility.* The frozen M4U planner (H160_K32, search seed 0) finishes 8/8 of validation-000 to -007 at ±10 m with 2 tacks each. Mean finish is 49.1 s, against 48.3 s in open water. Only the right wall forces a tack; the second tack lines up the gate. [Tracks](figs/M7_channel_mpc_half10_tracks.png) · [script](../../experiments/ese3990/m7_channel_mpc.py)
- *Diagnostic caveat.* The frozen tack classifier counted 0 tacks on validation-006 because the planner sailed at 20–28° true wind angle, inside the classifier's 30° head-to-wind zone. Tack counts in the channel are checked against tracks.

**Arms:** PPO (10 seeds) and SAC (5 seeds) from scratch with the M6 tuning configuration: fixed feature scaling, 0.5 s decisions, 2 × 256 networks and the target head, for 10M decisions. Checkpoints are selected on the 40-condition training-distribution panel in the channel. Selected checkpoints are then scored in the channel on validation, test and shift, with exact replay and tack diagnostics.

**Reading:** a seed passes with at least 1/20 channel validation finishes, as in M5 and M6. "As consistent as M6" means at least 9/10 PPO seeds and 4/5 SAC seeds pass. If either learner fails consistently, learner-side tuning iterates within a 4-hour budget that started at 01:10 on November 20.

**Follow-ups if ±10 m is learned:** ±10 m forces only one extra tack, so a narrower channel (±7.5 or ±5 m) or the gate at 80 m is the next test of a genuine chain.

**Result (November 20, 01:10–04:28, exploratory).** Every learned policy sails the planner's two-tack route; the frozen classifier undercounts tacks sailed at 20–28° true wind angle, so counts are read with the tracks.

- *PPO, M6 configuration unchanged.* 0/10 seeds at 10M; every panel episode is a timeout. The wall's −10 makes hovering the safe policy. Entropy 0.01, random starts, the rate head, per-episode width mixing and mirror-equivariant nets also stayed at 0.
- *PPO with a width curriculum.* This works:
  - narrowing from ±30 m in 2.5 m steps passes 6/10 seeds at 20M;
  - training first in open-water U, which is bit-identical to M6 tuning until the switch, then narrowing with a back-off, passes 8/10 seeds at 30M (7/10 perfect). **At 40M it passes 9/10, all nine perfect: 180/200 validation, 450/500 test, 134/200 shift.** These are M6's open-water totals at 4× its budget; shift is weaker;
  - PPO breaks at ±12.5 → ±10 m, where a second tack first becomes necessary. 5 m steps collapse there for good. Most remaining failures are seeds still mid-transition when the budget ends.
- *SAC, M6 configuration unchanged.* SAC learns the channel directly on 4/10 seeds at 10M: 80/200 validation, 200/500 test, 62/200 shift. Each learned seed is perfect on validation and test. Width mixing and the U-first curriculum did not help SAC; the curriculum collapses at each width switch.

[Report](M7_channel_report.md) · [Results](M7_channel_results.json) · [Run record](../handoffs/ese3990-m7-channel.md)

### Optional project stretch · after M7, not required for Dec 3

- **Wider wind-direction range.** Train with the bearing drawn from ±20° instead of ±5°. This is a training-condition change only. It targets the one known weakness of the M6 policies: with the wind backed 15°, the nine converged PPO policies finish 26/72 stress episodes, because every policy sails the same side first.
- **Changing wind as a contest.** The M6 policies already survive the default gust field (449/450), so survival is not the question. The question is whether a policy trained in gusts uses the shifts, for example by tacking on headers, to beat the current policy on the same wind. Score paired wins on identical realizations, as Suda and Nikovski (2022) did against MPC. Prerequisite: every episode currently gets the same gust realization because the scenario seed is fixed, so per-episode seeds must come first.
- **Multi-leg courses.** `courses/windward_leeward.json` and `courses/triangle.json` already exist, and the Rust course layer tracks marks and rounding sides. `sailing.Task` handles one gate only. A multi-leg variant needs the current mark as the target feature and shaping per leg. The downwind gybe adds a capsize risk.

### MLast · Freeze, explain and present · 7–10 h

- [ ] Freeze checkpoints/results and regenerate all core figures/tables from saved data.
- [ ] Finish the claim-to-command index and a clean saved-checkpoint evaluation invocation.
- [ ] Prepare a 10-minute presentation with sailing video/replay or static trajectory fallback.
- [ ] **Viz (§10):** regenerate every presentation video and trajectory figure from the frozen episode stores with one command; record each spec in the claim index.
- [ ] Rehearse PPO versus SAC data use, SAC entropy/targets, MPC foresight/compute and all known-model fairness limitations.
- [ ] Reproduce a report, rehearse Q&A and confirm final logistics; Dec 3: present measured findings.

Suggested presentation: boat/question (1 min), task/information contract (1), three controller mechanisms (3), outcomes and learning cost (2), MPC horizon/latency and failures (2), limitations (1). Upwind can replace part of the failure discussion if completed. M4X's critic-scale figure and value-tail result, and M4P's tack-rate grid, all labeled exploratory, belong in the failure and horizon slots. BPTT is not attempted before Dec 3 (§9).

## 7. Implementation footprint and verification

Keep project-specific rewards/trainers in `experiments/ese3990/`, using the existing root uv workspace. Core M0–M4 modules and media are implemented; final presentation/packaging remains MLast:

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
| Scope or compute exceeds deadline | Milestone exits and measured pilots | Cut M5 first (its own cut order), then M4P, then M4X (its own cut order), then U and five-seed expansion; reduce common budgets before dropping independent seeds. BPTT is already outside the schedule |
| Post-hoc arm read as confirmatory | An M4X, M4P or M5 row without its exploratory label, or merged into an M4/M4U table | Label M4X, M4P and M5 rows everywhere; report them separately |
| Planner exploits a learned value | X2's predicted versus executed return | Report the gap; do not refit V on planner outcomes |
| Visualization consumes core time | Hours spent against the §10 estimates | Cut the browser export first, then the grid layout, then mp4 (keep gif and static figures). Keep the episode store and replay, because the analysis depends on them |

A missing required method or MPC study remains incomplete work; do not relabel a reduced experiment as the full project. A measured failure with checked code is valid evidence. Any robustness claim is limited to the explicit uniform-wind conditions tested. The core cannot establish real-world transfer, a universal algorithm ranking, equal prior knowledge, or SuperMimic-like speedups.

The strongest supported story is which implemented controller is useful under which **experience and computation budget**, and how its sailing behavior explains the result.

## 9. Final stretch: learned world model and BPTT-WM

**Amended 2026-11-14: future work after Dec 3.** The reasons are in [M4X](#m4x--explain-the-upwind-failures-let-learning-shorten-planning--812-h). The design below is kept unchanged for that later work. If it resumes, start from S1's model diagnostics, not from policy learning: on U, check whether the model represents tacks and stalls before any BPTT run.

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

The simulator is deterministic, so an episode is fully described by its condition, seed and action sequence on a given physics build. Store that, and re-simulate when rendering. Re-simulating a 45 s R episode costs about 30 ms, so rendering is the only expensive step. The harness serves debugging (M1–M3), the failure analysis (M4) and the presentation (MLast) from one code path.

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

Estimate: 3–4 h in M1, 1–2 h in M3, 3–4 h in M4, 1 h in MLast, and 2–3 h for the browser export stretch.

