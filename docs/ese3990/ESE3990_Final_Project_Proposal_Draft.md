# ESE3990 Final Project: Proposal Draft (v0)

> Historical pendulum-first draft. The [SailGym project plan](ESE3990_SailGym_Project_Plan.md) defines the current PPO/SAC/CEM-MPC comparison, milestones, and tasks. Learned-model BPTT is retained there as the final stretch goal.

**Working title:** *Pathwise vs. Likelihood-Ratio Policy Gradients: SuperMimic in Miniature*
**Stretch:** the same method on a non-differentiable sailing simulator (sailgym)
**Author:** Shimin Chen · **Date:** 2026-10-30 · **Status:** draft for my own planning, not yet submitted

---

## 0. TL;DR

SuperMimic (Liao et al., CoRL 2026, arXiv:2610.28480) trains a humanoid controller in **3 GPU-hours instead of
~21,000**. It does this by changing *how the policy gradient is estimated*. It does not use PPO's likelihood-ratio
estimate from noisy simulator rollouts. Instead it learns a differentiable world model and backpropagates the return
through **short (H = 32) imagined rollouts**. A critic bootstraps everything beyond the horizon.

This project rebuilds that mechanism **from scratch, at toy scale, where every quantity can be checked against ground
truth**. The core experiment is a 2 × 2 factorial design:

|                                   | **True simulator**              | **Learned world model**               |
|-----------------------------------|---------------------------------|---------------------------------------|
| **Likelihood-ratio gradient (PPO)** | PPO-Sim (model-free baseline)   | PPO-WM (PPO inside the model)         |
| **Pathwise gradient (BPTT + critic)** | BPTT-Sim (oracle, SHAC-like)   | **BPTT-WM (SuperMimic-lite)**         |

The main environment is a pendulum swing-up written in JAX. Its true dynamics are differentiable, so I can compute
**the true policy gradient** and measure how good each estimator is. The paper could not do this, because its
simulator gives no gradients. The stretch goal moves BPTT-WM to **sailgym**, my Rust sailing simulator. sailgym is
non-differentiable and non-smooth: its mainsheet is a rope that can go slack, and its sail and rudder can stall. That is
exactly the setting SuperMimic was designed for.

The design follows the course's "small but deeply explainable" spirit. The project comes to roughly 1,000 lines of my
own code. It has ground truth at every step, and every ablation answers one *why* question.

---

## 1. Course requirements and how this proposal meets them

Sources: L1 slides 15–18 and L14 slides 3–4.

| Requirement | How this proposal meets it |
|---|---|
| Individual project, about half the grade | Solo. Scope is sized for about 5 weeks. |
| Implement an RL algorithm **from scratch** | PPO, a world model, and BPTT with a critic, all hand-written in JAX. Libraries are used only for autodiff, the optimizer, and verification. |
| Comparing with a public implementation is optional but valued | E0 checks my PPO against CleanRL's PPO on Gymnasium `Pendulum-v1`, and my JAX pendulum against Gymnasium's step-for-step. |
| Intermediate presentation, **Nov 12**, ~5 min: idea, approach, preliminary results | Paper idea, the 2 × 2 design, the verification plot, and first learning curves (§5). |
| Final presentation, **Dec 3 (L14) or Dec 7 (syllabus): TBC**, ~10 min | Main results E1–E4, plus the sailgym stretch if it passes its go/no-go gate. |
| Graded on presentation **and Q&A**, on the code (which will be inspected), and on results | Small codebase I can defend line by line; Q&A preparation in §8; every claim is backed by a script that regenerates its plot. |
| AI tools allowed, but I am responsible for everything | Rule: no line goes in that I cannot derive or explain (§6). |

---

## 2. The SuperMimic paper explained

### 2.1 The problem

**Motion tracking** is the dominant route to general humanoid whole-body control. A policy is trained to imitate
reference clips from a large motion-capture library: walking, dancing, flips, getting up. The state of the art is
SONIC, which uses PPO. SONIC trains for **168 h on 128 GPUs (≈ 21,000 GPU-hours)**.

Why is this so expensive? Every clip is performed by *the same body with the same physics*. Model-free RL never
exploits that shared structure. It re-estimates "what does this action lead to" from noisy simulator rollouts again
and again, for every motion. More motions therefore means more compute.

### 2.2 The key idea

Learn **one differentiable world model** of the shared dynamics, fitted to logged transitions from an ordinary
(non-differentiable) simulator, MuJoCo Warp. Then improve the policy by **backpropagation through time (BPTT)** through
short imagined rollouts of that model. This gives *analytic* (first-order, pathwise) policy gradients in place of
PPO's likelihood-ratio estimate.

Because the gradients come from the *learned* model and not from the simulator, the simulator can have non-smooth
contacts or a non-differentiable implementation.

### 2.3 The training loop (Fig. 2, Algorithm A.1)

Each iteration has three phases, and all three keep updating throughout training:

1. **Collect.** The current policy, with small Gaussian action noise added, runs in the simulator (4,096 envs per GPU).
   Transitions go into a FIFO replay buffer. Reset states are chosen by *return-based sampling*, which favours
   motion segments where the policy currently does badly.
2. **Fit the world model** on k-step windows (k = 8) sampled from the buffer (Eq. 1).
3. **Improve the policy.** Freeze the world model. Start H = 32-step imagined rollouts from buffer states, Dyna-style.
   Backpropagate the imagined return into the policy (Eq. 2), and fit a critic on the same imagined rollouts (Eq. 3).

The paper points out that old data stays valid for fitting the model: **the dynamics do not depend on the policy that
collected the data**. Model fitting is therefore naturally off-policy and needs no importance weights. Policy
improvement, by contrast, is on-policy *inside imagination*.

### 2.4 The world model (§3.2, Eq. 1)

$$\hat s_{t+1} = f_\omega(\hat s_t, a_t, \xi),\quad \hat r_{t+1} = r_\omega(\hat s_t,a_t,g_t),\quad
\hat{\bar o}_t = g_\omega(\hat s_t,\xi),\quad \hat d_t = d_\omega(\hat s_t, g_t)$$

- It is **deterministic**. The simulator's randomness has known sources, and each is handled explicitly:
  - The domain-randomisation parameters ξ (friction, calibration offsets, centre-of-mass offset) are *inputs* to the model.
  - Sensor noise η is added *outside* the model.
- It is trained **autoregressively over k = 8 steps**. The model is seeded at a logged state and then rolled forward on
  its *own* predictions using the logged actions. Training on its own errors teaches it to correct them, which is why
  rollouts stay stable well beyond k.

$$\mathcal L_{WM}(\omega)=\mathbb E_{\mathcal B}\sum_{j=1}^{k}\Big[\|\hat s_j-s_j\|^2+(\hat r_j-r_j)^2+\|\hat{\bar o}_j-\bar o_j\|^2+\mathrm{BCE}(\hat d_j,d_j)\Big]$$

- **Termination is predicted as a probability** $\hat d_t\in[0,1]$. A "survival weight"
  $c_{t+1}=c_t(1-d_{t+1})$ multiplies the rewards. This keeps the return *differentiable* even though falling over is
  a discrete event.

### 2.5 Policy objective and critic (§3.3, Eqs. 2–3)

The policy is **deterministic**, $a_t=\pi_\theta(o_t,g_t)$. The objective over imagined rollouts is

$$\hat J(\theta)=\mathbb E_{s_0\sim\rho_0}\Big[\sum_{t=0}^{H-1}\gamma^t\hat c_t\,\hat r_{t+1}+\gamma^H\hat c_H\,V_\psi(\hat s_H,g_H,\xi)\Big],\qquad H=32,\ \gamma=0.99$$

$\nabla_\theta\hat J$ is computed by ordinary reverse-mode autodiff through the H model steps. The critic is fitted to
**GAE / TD(λ) targets** (λ = 0.95), computed backward over the imagined transitions only. It has **no target network**.
The paper describes this as a short-horizon actor–critic in the spirit of **SHAC** (Xu et al., 2022). SHAC does the
same thing through a *differentiable simulator*.

The paper's own one-sentence contrast with PPO:

> "PPO treats the dynamics as a black box: it weights the score ∇θ log πθ of a stochastic policy by the advantages of
> sampled actions, a likelihood-ratio estimate. BPTT instead differentiates the return with respect to each action, so
> over short horizons its gradient has much lower variance."

### 2.6 Design details that matter, and that make good Q&A material

- **Why short H with a critic?** The model only has to be accurate for H steps, and the critic handles the rest.
  Without a critic, training *fails* at H ≤ 16. With a critic, H = 16 matches the critic-free H = 32. Going from
  H = 64 to 128 nearly *triples* the time to the target return and lowers the final return (Fig. 4b, 4c).
- **The critic matters beyond nominal return.** At matched return, the critic-free policy cannot recover from pushes,
  because the recovery reward lies beyond the imagination window (§4.3).
- **Determinism changes the reward design.** The paper removed BeyondMimic's self-collision penalty. That penalty
  guarded against a *stochastic* policy suppressing its own action noise by pressing bodies together, and a
  deterministic policy gains nothing from doing that (App. A).
- **A deterministic model cannot represent discontinuities.** For this reason the paper removed random root-velocity
  pushes from domain randomisation (App. A).
- **Action noise during collection exists only to widen the data the model is fitted on.** It is not exploration
  for the policy gradient (§3.1).

### 2.7 Headline results

| Claim | Evidence |
|---|---|
| Beats SONIC's return within **3 h on 1 H100** | Fig. 3a. >50× less wall-clock time and >1000× fewer GPU-hours (≈7200× by their accounting). |
| Fewer failures | On 129,514 motions: **25 failures** after 3 h on 1 GPU, and **0** after 9.8 h on 2 GPUs, against **179** for SONIC (App. F). |
| Controlled comparison against PPO in the simulator (LAFAN1) | SuperMimic reaches 0.82 in 1.2 h on 1 GPU. PPO-Sim never reaches it in 18 h: 0.60 on 1 GPU, 0.70 on 4, 0.72 on 8 (Fig. 4a). |
| Training cost grows slowly with data | 1 → 264 h of motion. The 8-step model error stays between 0.007 and 0.011, and GPU-hours to target grow slowly (Fig. 3b, 3c). |
| Transfers to a real robot | Unitree G1: skipping jump, side roll, 90° flip, 360° flip (Fig. 6). |

### 2.8 Ablations: *where* the speed-up comes from

The paper names three properties of the world model:

1. **Accuracy is stable as the motion library grows.** One body means shared dynamics: more clips add more evidence,
   not more required model capacity.
2. **It is fast.** World-model rollouts run more than 10× faster than MuJoCo Warp at practical batch sizes, and up to
   62× faster (Fig. 5a).
3. **Its gradients are analytic.** This is the cleanest experiment in the paper (Fig. 5b). They run **PPO on the same
   world model (PPO-WM)**, which isolates rollout speed from gradient type. SuperMimic reaches PPO-WM's best return
   with **≈64× fewer imagined steps**. The gain is therefore not just fast rollouts: the gradient estimator itself
   matters.

My 2 × 2 design generalises this ablation by adding the "true simulator" column.

### 2.9 Critical reading: things a careful reader, or the professor, might push on

- **Seeds.** "We ran several random seeds and found training very stable … The figures therefore do not report seed
  variation." Many curves are single runs.
- **The benchmark is the training set.** The 129,514-motion benchmark measures *coverage of training motions*, not
  generalisation. The paper says so itself (App. F.2).
- **The compute comparison is across GPU types and datasets.** SONIC used L40s and SuperMimic an H100. SONIC also
  trained on 2.3× more data. The authors argue that neither difference can explain a 1000× gap, which is plausible
  but not a controlled comparison. Fig. 4a *is* controlled.
- **The success criteria differ.** SONIC's own criterion has no orientation term; the comparison applies SuperMimic's
  termination rule to both methods.
- **The dense reward helps a lot.** The authors credit training stability to the dense per-step tracking reward. Tasks
  with sparse or delayed reward may behave differently, and sailing is one of them (§4.7).
- **The model is simulator-only.** It never learns from real data. Flat ground only: no terrain and no objects.

---

## 3. Where it sits in what we have covered

### 3.1 Lecture map

| Lecture | Concept | Where it appears in SuperMimic | Where it appears in this project |
|---|---|---|---|
| L2–L3 RL basics, MDPs | Return, discount, POMDP, termination | Goal-conditioned POMDP; survival-weighted return | Pendulum MDP; sailgym is a POMDP (§4.7) |
| L4–L5 DP | Policy iteration, Bellman backups | L14 recasts PG as approximate policy iteration | Conceptual framing |
| L6–L7 Model-free prediction | MC vs TD, TD(λ) | Critic trained on λ-returns, Eq. 3 | My critic, the same targets |
| L9 Off-policy learning | Importance sampling | *Not needed* for model fitting, because the dynamics are policy-independent | Q&A: "why can the WM use old data, when off-policy PG needs IS ratios?" |
| L10 Value-function approximation | Parametric V, regression targets | $V_\psi$ MLP | The critic |
| L11–L12 Policy gradients | REINFORCE, baselines, A2C, **GAE**, variance reduction | The PPO baseline, and the variance argument for BPTT | PPO-Sim and PPO-WM; E2 measures that variance |
| **L13 Model-based RL** | Dyna; "all models are wrong"; models learned from data; **differentiable model learning** (slide 35); sample-based planning with model-free control (slide 39); background vs decision-time planning | Dyna-style rollouts seeded from the buffer; a learned differentiable model; background planning | **The core of the project.** PPO-WM *is* slide 39, and BPTT-WM goes beyond it by using the model's *derivatives*, not just its samples |
| L14 Off-policy PG | IS for PG, state-distribution mismatch | PPO-WM's model-vs-simulator distribution shift | Interpreting the PPO-WM results |
| **L15 Advanced PG** | PPO-clip, trust regions, NPG/TRPO, "PPO implementation matters" (Engstrom et al.) | The PPO baseline that SuperMimic beats | My from-scratch PPO, with the listed tricks |
| **L16 Advanced off-policy** | DQN, target networks, deadly triad, **DDPG** | The critic has *no* target network. DDPG's actor update is a **pathwise** gradient | The H = 0 end of the spectrum in §3.3 |
| Nov 10/12 Advanced model-based RL (upcoming) | Likely Dreamer, MBPO, SHAC-type methods (to check) | Related work: Dreamer, TD-MPC, SHAC, MBPO | Update §3 after these lectures |

### 3.2 The two gradient estimators, side by side

The objective is $J(\theta)=\mathbb E\big[\sum_t \gamma^t r(s_t,a_t)\big]$. For a fair comparison, both estimators
target the **same Gaussian policy** $a_t=\mu_\theta(s_t)+\sigma\varepsilon_t$.

**Likelihood ratio (score function), L11–L12 and L15.** Only *scalar* returns and $\nabla_\theta\log\pi$ are used:

$$\nabla_\theta J=\mathbb E\Big[\sum_t\nabla_\theta\log\pi_\theta(a_t|s_t)\,\hat A_t\Big]$$

- Pros: it needs no model derivatives and works with any black-box simulator.
- Cons: its variance grows with the action dimension and the horizon. It has to *discover* the direction of
  improvement through correlations between noise and outcome.

**Pathwise (reparameterisation, or BPTT).** The noise is held fixed and the return is differentiated directly. With
$s_{t+1}=f(s_t,a_t)$ and $a_t=\mu_\theta(s_t)+\sigma\varepsilon_t$:

$$\frac{ds_{t+1}}{d\theta}=\underbrace{\big(f_s+f_a\,\mu_s\big)}_{\text{closed-loop Jacobian }M_t}\frac{ds_t}{d\theta}+f_a\,\frac{\partial\mu}{\partial\theta}$$

- Pros: it uses *local derivative information*, so a single rollout already points uphill. Variance is low over short
  horizons.
- Cons:
  - **Exploding or vanishing gradients.** $ds_t/d\theta$ contains products $M_{t-1}\cdots M_1$. If the closed-loop
    dynamics are locally unstable (spectral radius > 1, as for a pendulum near upright), the gradient norm grows
    roughly like $\rho^H$. **This is why H must be short.**
  - **Bias** wherever the derivative lies, either from model error in $f_\omega$'s Jacobians or from non-smooth true
    dynamics (velocity clipping, a slack rope, contacts).

SuperMimic's answer is to keep H short, so the products cannot explode and the model only needs to be accurate for a
few steps, and to let the critic carry the long-horizon credit.

### 3.3 A unifying spectrum

This is the slide I want for the final presentation:

```
 H = 0 (no model)        0 < H < ∞ (model + critic)              H = full episode (no critic)
 ─────────────────────────────────────────────────────────────────────────────────────────────
 DDPG / SVG(0)           SHAC (true sim), SuperMimic (learned)     SuperTrack, ControlVAE,
 a* via ∇_a Q(s,a)       BPTT H steps, then γ^H V(s_H)             pure BPTT
 L16                     this project                              (explodes on long horizons)
 bias: critic error      bias: model + critic error                bias: model error only
 var: low                var: low (short H)                        var/explosion: high
```

The model-free likelihood-ratio family (REINFORCE → A2C → PPO) sits *beside* this axis, because it never
differentiates through dynamics. DDPG's actor update, $\nabla_a Q\,\nabla_\theta\mu$, is already a pathwise
gradient, with a horizon of zero. SuperMimic inserts H steps of a learned model between the policy and the critic.

---

## 4. Project proposal

### 4.1 Research question and hypotheses

**Q:** When, and why, does a pathwise gradient through a *learned* model beat a likelihood-ratio gradient, and how do
the imagination horizon and model quality limit it?

| # | Hypothesis | Tested in |
|---|---|---|
| H1 | BPTT-WM reaches PPO-Sim's final return with an order of magnitude fewer *real* simulator steps. | E1 |
| H2 | Per batch, pathwise gradients have much higher cosine similarity with the true gradient than PPO's estimator. The advantage shrinks as H grows. | E2 |
| H3 | Gradient norm grows roughly exponentially in H. Without a critic, short H fails. With a critic, an intermediate H is optimal. This is a miniature Fig. 4b/c. | E3 |
| H4 | A k-step autoregressive model loss (k = 8) gives more stable long rollouts *and* better policies than a 1-step loss, even when 1-step error is similar. Accurate predictions do not imply accurate Jacobians. | E4 |
| H5 (stretch) | The same pipeline works on sailgym. The model learns boat dynamics well except near non-smooth events (sheet going slack, stall, capsize onset), and that is where its gradients mislead. | S1–S3 |

### 4.2 Design: the 2 × 2

| Arm | Gradient | Dynamics used for the policy update | Role |
|---|---|---|---|
| **PPO-Sim** | Likelihood ratio (PPO-clip + GAE) | True simulator | Model-free baseline. This is "what everyone does". |
| **PPO-WM** | Likelihood ratio | Learned world model, with starts seeded from the buffer | Isolates *rollout source* from *gradient type*. Mirrors the paper's Fig. 5b, and is L13 slide 39 made concrete. |
| **BPTT-Sim** | Pathwise, H steps plus critic | True simulator, differentiated with JAX | **Oracle / SHAC.** The upper bound with no model error. |
| **BPTT-WM** | Pathwise, H steps plus critic | Learned world model | **SuperMimic-lite.** The method under study. |

Reading across a row shows the cost of model error. Reading down a column shows the effect of gradient type.

### 4.3 Main environment: pendulum swing-up in JAX

- **Dynamics:** identical to Gymnasium `Pendulum-v1`: g = 10, m = l = 1, dt = 0.05, torque in [−2, 2], angular
  velocity clipped to ±8, 200 steps. The cost is $\theta^2+0.1\dot\theta^2+0.001u^2$ and the observation is
  $(\cos\theta,\sin\theta,\dot\theta)$. It is about 30 lines of `jax.numpy`, and E0 checks it step-for-step against
  Gymnasium.
- **Why this toy:**
  - Its state is 2-D and its action 1-D, so the true dynamics are small enough to differentiate exactly.
  - It is locally **unstable** near upright, so gradient explosion with H is real and measurable.
  - It is **mildly non-smooth**: the torque and velocity clips are a miniature version of "contacts".
  - It trains in minutes on a laptop CPU, so 5+ seeds per configuration is affordable.
- **Optional second toy**, only if time allows: Acrobot swing-up, which is chaotic, so it stresses H3 harder.
- **Optional "motion-library" analogue (E5):** make the task goal-conditioned, so that the pendulum tracks
  reference trajectories θ_ref(t) drawn from a library of N sinusoids. Then sweep N ∈ {1, 4, 16, 64} and test the
  paper's amortisation claim (Fig. 3c): world-model error should stay flat as N grows, because the dynamics are shared.

### 4.4 Algorithms, all from scratch in JAX

**World model** $f_\omega$: an MLP that predicts $\Delta s$, the residual. Reward and termination are analytic heads,
since they are known for the pendulum. It is trained on k-step autoregressive windows (Eq. 1).

**BPTT-WM training loop**, which follows Algorithm A.1:

```
init policy μ_θ (deterministic MLP), critic V_ψ, world model f_ω, replay buffer B
loop:
  # 1. collect: real simulator, small Gaussian action noise; push (s, a, r, s', d) into B
  # 2. fit world model: sample k-step windows from B; roll f_ω forward on the logged actions;
  #    ω ← ω − α ∇ω Σ_j ||ŝ_j − s_j||²
  # 3. improve policy:
  #    s_0 ~ B;  for t < H:  a_t = μ_θ(s_t);  s_{t+1} = f_ω(s_t, a_t)   # f_ω frozen (stop_gradient on ω)
  #    Ĵ = Σ γ^t r̂_t + γ^H V_ψ(s_H)
  #    θ ← θ + α ∇θ Ĵ            # jax.grad through the whole unroll
  #    fit V_ψ to TD(λ) targets computed backward over the imagined trajectory (targets held fixed)
```

- **BPTT-Sim** is the same loop with $f_\omega$ replaced by the true JAX dynamics, and no step 2.
- **PPO-Sim** follows L15 slide 25: PPO-clip with GAE, value-function and gradient clipping, observation
  normalisation, and an entropy bonus. CleanRL's `ppo_continuous_action` serves as the reference.
- **PPO-WM** uses the same PPO code, but its "environment" is $f_\omega$ started from buffer states, with H-step
  rollouts plus bootstrapping.

### 4.5 Experiments

**E0 · Verification.** This answers "is my code right?" before any science.
- My JAX pendulum matches Gymnasium `Pendulum-v1` to within 1e-6 over random action sequences.
- My PPO matches CleanRL's PPO on `Pendulum-v1`: return curves over 5 seeds should overlap within their confidence
  bands.
- My world-model gradients are checked against finite differences.

**E1 · The 2 × 2 main result.** Plot return against **real-simulator steps**, against **imagined steps**, and against
**wall-clock time**, for all four arms with 5 seeds each (mean ± 95% CI). Report steps-to-threshold, for example to a
return of −200.

**E2 · Gradient quality.** This is the centrepiece, and the experiment the paper could not run. At fixed policy
checkpoints (early, middle, late):
- Compute a reference **true gradient** $g^\star$ of the shared stochastic-policy objective. Use a very large batch of
  full-horizon pathwise gradients through the true dynamics, and cross-check it with central finite differences using
  common random numbers.
- For batch sizes N ∈ {16, 64, 256, 1024}, measure each estimator's **cosine similarity to $g^\star$** and its
  **variance**:
  - the PPO (likelihood-ratio) gradient;
  - BPTT-Sim at H ∈ {4, 16, 64};
  - BPTT-WM at the same horizons.
- The expected picture is that pathwise wins at short H and degrades at long H. BPTT-WM sits below BPTT-Sim, and the
  gap between them measures *model gradient error*.

**E3 · Horizon ablation**, a miniature of Fig. 4b/c. Use H ∈ {1, 4, 8, 16, 32, 64, 128}, each with and without a
critic, and report three things:
1. final return;
2. steps to threshold;
3. the gradient-norm distribution against H.

Overlay $\rho(M)^H$ on the gradient norms, using the closed-loop Jacobian's spectral radius near upright measured from
the true dynamics. This turns "gradients explode" into an explained number.

**E4 · World-model ablation.**
- **Loss horizon:** k ∈ {1, 4, 8}. Report 1-step error, *open-loop* error at 32 and 128 steps, **Jacobian error**
  $\|\partial f_\omega/\partial(s,a)-\partial f/\partial(s,a)\|$, and the resulting policy return.
- **Collection noise:** on vs off, to test the paper's claim that collection noise "widens the range of actions the
  model is fit on".

**E5 · (optional) Amortisation sweep.** Goal-conditioned tracking of N reference trajectories, as described in §4.3.

**Protocol.**
- 5 seeds minimum per configuration.
- Fixed budgets, in both real steps and wall-clock time.
- One script per figure, writing to `results/`. The README maps each claim on the slides to the script that produced it.
- Everything runs on CPU. A GPU is optional.

### 4.6 What I am *not* claiming

- I am not reproducing SuperMimic. There is no humanoid, no motion capture, no hardware, and none of the paper's
  scale.
- I am not claiming the 1000× number transfers. The point is the *mechanism*: which estimator wins, why, and where it
  breaks.

### 4.7 Stretch: sailgym (go/no-go on **Nov 19**)

**Why it fits SuperMimic's setting exactly.** sailgym is a from-scratch Rust simulator of an ILCA 7 dinghy:
surge, sway, yaw and roll, plus a dynamic boom.
- It is **non-differentiable** and has **non-smooth** physics:
  - the mainsheet is a unilateral rope, $T=\max(0,k e+c\dot e)$;
  - sail and rudder stall;
  - a binary capsize event.
- Learning a differentiable surrogate from logged transitions is therefore the *only* way to get pathwise gradients,
  which is precisely SuperMimic's argument for learning the model instead of differentiating the simulator.
- It is fast: about 3,000× real time on one core, and 5.25 M steps/s batched. Data collection is cheap.

**Interface facts** (checked with `sailgym_core.Spec(scenario="close_hauled")`):
- **Observation:** 18-D. It contains IMU rates, heel and accelerations, apparent wind angle and speed, boom angle and
  rate, sheet length and slack, rudder angle and rate command, and guidance (cross-track error, bearing and distance to
  the target, the leg's angle to the wind, and rounding side).
- **Action:** 3-D in [−1, 1]: rudder *rate*, sheet *rate*, and a release flag (sign > 0).
- **Timing:** dt = 5 ms with a decision every 10 steps, which is **20 Hz**.
- **Scenarios:** `close_hauled`, `tack`, `gybe`, `free_sail`, `beam_reach_capsize`, `sheet_release_recovery`.
- **Termination reasons:** `capsized`, `out_of_bounds`, `mark_missed`.
- **No reward is shipped.** The reward is an experiment parameter, which I would design.

**Task.** On the `close_hauled` scenario, sail toward an upwind mark: maximise velocity made good, with a heel penalty
and an action-smoothness penalty. Reward: $r_t=(d_{t-1}-d_t)/\Delta t-w_h\,\text{heel}^2-w_a\|a_t-a_{t-1}\|^2$, where
$d$ is `distance_to_target`.

The release flag is fixed to "not released", which removes the binary action dimension. Capsize is handled with the
paper's **learned termination probability and survival weight**, which is the one SuperMimic component the pendulum
cannot exercise.

**Stages:**
- **S1 · Can an MLP learn the boat?** Fit $f_\omega$ to transitions collected with noisy rule-based or random
  policies. Report k-step and open-loop error, *broken down by regime*: sheet taut vs slack, attached vs stalled flow,
  near capsize. This is already a meaningful result on its own.
- **S2 · BPTT-WM vs PPO-Sim** on the upwind task, compared at an equal real-simulator-step budget.
- **S3 · Compare with sailgym's rule sailor** (`crates/sailgym-agent/src/pilot/rule_sailor.rs`), a hand-written expert
  baseline.

**Open design issues**, to resolve in week 3:
1. **State vs observation.** SuperMimic's model works on the simulator *state* $s_t$, and observations come from a
   head. sailgym's Python API exposes only the 18-D observation. It lacks, for example, boat speed and heading, so
   the observation is not Markov. There are two options:
   - (a) a model over a short observation and action history. SuperMimic's *policy* uses 10 frames.
   - (b) add a privileged full-state readout to the `sailgym_core` binding. The observation layout already carries a
     `privileged` flag per column.

   Option (b) is cleaner and closer to the paper, if the binding change is small.
2. **Delayed reward.** A tack takes several seconds, which is 40–100 decisions at 20 Hz. That is longer than H = 32
   imagined steps, so the critic is essential. This is a direct test of the paper's §4.3 argument, and a contrast with
   its dense tracking reward (§2.9).
3. **Rule-sailor access from Python** may need a small binding addition.

**Go/no-go:** start the stretch only if E0–E3 are complete by Nov 19. Otherwise, S1 alone becomes a "future work"
slide, a model-fit plot plus the open issues above, which still connects the project to Microtransat-style autonomy.

---

## 5. Plan and milestones

Today is Fri Oct 30.

| Week | Dates | Deliverables |
|---|---|---|
| 1 | Oct 30 – Nov 5 | JAX pendulum and the Gymnasium equivalence test. PPO from scratch, checked against CleanRL (**E0**). BPTT-Sim, which is easy once the env is in JAX. |
| 2 | Nov 6 – Nov 12 | World model with k-step loss, BPTT-WM, and PPO-WM. First E1 curves. **Nov 12: intermediate presentation** (below). |
| 3 | Nov 13 – Nov 19 | **E2** gradient quality, **E3** horizon ablation, **E4** model ablation, all with 5 seeds. **Nov 19: go/no-go on sailgym.** |
| 4 | Nov 20 – Nov 26 (Thanksgiving) | sailgym **S1**, then **S2** if S1 looks healthy. Otherwise E5 and polish. |
| 5 | Nov 27 – Dec 3 | Freeze results by ~Nov 30. Slides. Q&A rehearsal (§8). Code clean-up so it matches every claim. |

**Intermediate presentation (~5 min, Nov 12):**
1. The problem, and the SuperMimic idea in one figure.
2. The 2 × 2 design and why it isolates the mechanism.
3. E0 verification plots: my env matches Gymnasium, and my PPO matches CleanRL.
4. First learning curves.
5. The plan and the questions I want feedback on:
   - Is E2's "true gradient" protocol convincing?
   - Is the sailgym stretch worth the risk?

**Final presentation (~10 min):**
1. Motivation, 1 min.
2. The two estimators and the spectrum (§3.2–3.3), 2 min.
3. The 2 × 2 result (E1), 1.5 min.
4. Gradient quality (E2), 1.5 min.
5. Horizon (E3), 1.5 min.
6. Model ablation (E4), 1 min.
7. sailgym, 1 min.
8. Takeaways and limitations, 0.5 min.

---

## 6. Code structure and explainability rules

```
ese3990-project/
  envs/pendulum.py        # ~40 lines, JAX, matches Pendulum-v1
  envs/sail_wrapper.py    # (stretch) reward + state/obs handling over sailgym's Gymnasium env
  nets.py                 # hand-written MLP init/apply (no Flax), ~40 lines
  ppo.py                  # PPO-clip + GAE, ~250 lines
  world_model.py          # k-step autoregressive fit, ~120 lines
  bptt.py                 # short-horizon actor-critic through f (true or learned), ~150 lines
  experiments/e0_verify.py ... e4_model.py   # one script per figure
  results/                # generated; never hand-edited
  README.md               # claim → script → figure table
```

- **Stack:** JAX, with `jax.grad`, `jit` and `vmap`, plus Optax for Adam. Both are already in sailgym's venv
  (jax 0.11.2). Plotting uses matplotlib. CleanRL and Gymnasium are used *only* in E0.
- **Rule 1:** every loss in the code has a matching equation in the slides, and I can derive it on a whiteboard.
- **Rule 2:** AI-assisted code is allowed only once I can explain every line. If I can't, it gets rewritten.
- **Rule 3:** every number on a slide is regenerated by a script, with a fixed seed list.

---

## 7. Risks and mitigations

| Risk | Likelihood | Mitigation |
|---|---|---|
| My PPO underperforms, which would make the baseline unfair | Medium | E0 against CleanRL comes *first*. Use the L15 "implementation matters" tricks. |
| BPTT through the pendulum explodes even at moderate H | Medium (and it is *part* of the result) | Gradient-norm clipping, as the paper does (global norm 1.0). Report the explosion; do not hide it. |
| A pendulum is "too easy" to show differences | Low–medium | E2 measures gradient quality, which differs even when final returns tie. Acrobot as the optional harder toy. |
| sailgym's observation is not Markov, so the model cannot be fitted | Medium | Option (b), the privileged state readout, or fall back to an observation history. S1 alone is still a result. |
| Time | Medium | Hard go/no-go on Nov 19. The pendulum part is a complete project on its own. |
| The final presentation date is unclear (Dec 3 vs Dec 7) | Already present | Ask the instructors this week, and plan for Dec 3. |

---

## 8. Anticipated Q&A, with answer sketches to rehearse

1. **"Why is the BPTT gradient lower-variance than PPO's?"**
   The likelihood-ratio estimator only sees scalar returns and has to infer the uphill direction from correlations
   between injected noise and outcome. Its variance grows with the horizon and the action dimension. The pathwise
   estimator uses the Jacobians of the dynamics directly, so one rollout already gives a direction. E2 quantifies this.
2. **"Then why not BPTT through the whole episode?"**
   The gradient contains products of closed-loop Jacobians. If they are locally unstable, as near upright, the norm
   grows like $\rho^H$. Model error also compounds. E3 shows both effects. This is exactly why SuperMimic uses H = 32
   plus a critic, and why H = 128 was worse.
3. **"What does the critic buy you?"**
   It replaces the truncated tail with $\gamma^H V(s_H)$. That makes a short H non-myopic, so the model only needs to
   be accurate for H steps. The paper shows no-critic training fails at H ≤ 16, and that the no-critic policy can't
   recover from pushes.
4. **"The model is wrong. Why does the policy still work?"**
   Four reasons:
   - Rollouts are short and start from *real* buffer states (Dyna-style).
   - The model is refit continually on the data the current policy visits.
   - The k-step loss trains the model to correct its own drift.
   - The policy is evaluated in the *real* simulator.

   BPTT-Sim vs BPTT-WM measures what is left of the model error.
5. **"Is SuperMimic on-policy or off-policy?"**
   Both. Model fitting is off-policy and needs no importance weights, because $p(s'|s,a)$ does not depend on the
   behaviour policy. Policy improvement is on-policy *in imagination*. Contrast this with off-policy PG (L14), which
   needs IS ratios over actions.
6. **"How does this relate to DDPG?"**
   DDPG's actor update $\nabla_a Q\,\nabla_\theta\mu$ is already a pathwise gradient with H = 0, and it trusts the
   critic's action-gradient entirely. SuperMimic inserts H steps of a differentiable model before the critic. That
   trades critic bias for model bias.
7. **"Why a deterministic policy? How does it explore?"**
   The pathwise gradient doesn't need exploration noise to estimate the gradient. Exploration for *data* comes from
   action noise during collection, plus return-based reset sampling. A side effect is that reward exploits that rely
   on noise disappear, which is why the paper dropped the self-collision penalty.
8. **"Why a k-step model loss instead of 1-step?"**
   At imagination time the model consumes its *own* predictions. A 1-step loss never trains it on that input
   distribution, so its errors compound. E4 measures this, including the Jacobian error.
9. **"What happens at non-smooth dynamics, like your slack sheet or capsize?"**
   The learned model is a *smoothed* surrogate. Its gradients are the gradients of that surrogate, which can help
   by giving informative gradients near a kink, or mislead when the kink matters. Termination becomes a predicted
   probability, which keeps the return differentiable. S1 breaks model error down by regime to show where this bites.
10. **"How do you know your PPO is correct?"**
    Through E0: it matches CleanRL on the same task across 5 seeds, and my environment matches Gymnasium to 1e-6.
11. **"Is your comparison fair in compute?"**
    I report three x-axes: real steps, imagined steps, and wall-clock time. Each one answers a different fairness
    question, and the paper's own Fig. 5b uses imagined steps.
12. **"What would you criticise in the paper?"**
    See §2.9:
    - single-seed curves;
    - a training-set benchmark;
    - a compute comparison across GPU types and data sizes;
    - a dense reward that may hide difficulties that sparse-reward tasks expose.

---

## 9. To confirm or decide

- [ ] Final presentation date: Dec 3 (L14) or Dec 7 (syllabus)?
- [ ] Whether the instructors' presentation template imposes a structure on the slides.
- [ ] After the Nov 10/12 lectures: update §3 with the advanced model-based RL content (Dreamer, MBPO, SHAC?).
- [ ] sailgym: state readout, option (a) or (b), and rule-sailor access from Python.
- [ ] Whether checking against CleanRL counts as "comparison with a public implementation". Ask at the intermediate
      presentation.

---

## References

- Q. Liao, Y. Shao, J. Long, K. Zakka, R. Horowitz, P. Abbeel, K. Sreenath. *SuperMimic: Scaling Humanoid Whole-Body
  Control with One Differentiable World Model.* CoRL 2026. arXiv:2610.28480. Local copy: `SuperMimic.pdf`.
- J. Xu et al. *Accelerated Policy Learning with Parallel Differentiable Simulation (SHAC).* ICLR 2022.
- J. Schulman et al. *Proximal Policy Optimization Algorithms.* 2017. *High-Dimensional Continuous Control Using GAE.*
  ICLR 2016.
- R. S. Sutton. *Dyna, an Integrated Architecture for Learning, Planning, and Reacting.* 1991.
- M. Janner et al. *When to Trust Your Model: Model-Based Policy Optimization.* NeurIPS 2019.
- T. Lillicrap et al. *Continuous Control with Deep RL (DDPG).* 2016.
- L. Engstrom et al. *Implementation Matters in Deep Policy Gradients.* ICLR 2020. Cited in L15.
- S. Huang et al. *CleanRL.* JMLR 2022. Used for verification in E0.
- ESE3990 lectures L1, L11–L16. Sutton & Barto, *Reinforcement Learning: An Introduction*, 2nd ed., ch. 8 (Dyna) and
  ch. 13 (policy gradients).
