# Value-learning stability in off-policy actor-critic RL (SAC/TD3) at long horizons and high discount factors

Legend used throughout. **[PR]** peer-reviewed (venue given). **[PP]** preprint, or no peer-reviewed venue found. **[V]** checked against the source page in this session. **[BK]** from the researcher's background knowledge and not re-fetched this session; the URL is the paper's canonical record, and the writer should treat [BK] details as needing a spot-check before quoting exact numbers. **(2023–26)** marks a 2023–2026 advance.

## Q1. Entropy reward problems: dropping entropy from the critic, fixed vs learned α, target entropy, interaction with termination and episode length, and whether r + α(H − H̄) has been proposed

### Takeaway
The literature names this failure directly. In SAC the entropy term enters the critic as a per-step intrinsic reward. Post-termination values are zero, so this reward "inflates" (if entropy > 0) or "deflates" (if entropy < 0) the value before termination and not after. Positive entropy gives a survival bias. Negative entropy, which is normal for continuous actions with target entropy −dim(A), gives pessimism and a pull toward early termination. Three documented fixes exist:
- drop the entropy reward from policy evaluation (SACLite);
- zero-mean it with a moving average (SACZero);
- turn the problem into a continuing, average-reward one with an explicit reset cost (RVI-SAC, Average-Reward SAC).

A fourth, newer fix is a separate entropy critic (TECRL/DSAC-E, 2025 preprint). No paper found proposes exactly r + α(H − H̄). SACZero (entropy reward minus its moving average) is the closest, and the two coincide in expectation once the α-constraint is satisfied. None of these papers report results at γ ≥ 0.999.

### Cited Findings
**Yu, Zhang & Xu (2022), "Do You Need the Entropy Reward (in Practice)?"** [PP; arXiv, no venue on the arXiv record] [V]
- Main recommendation: "entropy rewards should be applied with caution to policy evaluation." Using entropy regularization only in policy improvement "leads to comparable or even better performance and robustness." — [arXiv 2201.12434](https://arxiv.org/abs/2201.12434)
- Mechanism (they call it "reward inflation"): SAC's entropy reward "inflates (in expectation) the reward function before termination while not after it," because post-termination Q values are zero in TD backups. "An opposite behavior may happen if the entropy is negative initially," which makes the policy pessimistic and drives earlier termination. — [ar5iv full text](https://ar5iv.arxiv.org/html/2201.12434)
- Toy evidence (SimpleChain): entropy bonus ≈ +0.14 per step against task reward −0.05 gives a net ≈ +0.09 per step, so the agent "never wants to reach the terminal state." In BipedalWalker, SAC learned to do the splits and stall so it could keep collecting the entropy bonus over a long episode. — [ar5iv](https://ar5iv.arxiv.org/html/2201.12434)
- The two variants:
  - **SACZero** subtracts "the entropy rewards by their moving average (i.e., zero-mean normalization)." This is a running mean, not the target entropy.
  - **SACLite** "simply discard[s] the entropy reward but still keep[s] the entropy cost in policy optimization." — [ar5iv](https://ar5iv.arxiv.org/html/2201.12434)
- Benchmarks: 26 tasks (4 Fetch manipulation, 4 Box2D, 4 MuJoCo locomotion, 8 DM Control, 6 Safety Gym) plus toy tasks. Results are curves only, with no aggregate numbers.
  - Manipulation: SAC is "much worse than SACLite" on FetchPush and FetchPickAndPlace. On PickAndPlace the order is SACLite ≫ SACZero ≫ SAC.
  - Locomotion: curves are "very close."
  - DMC: SACLite is "much better than SAC" on 3 tasks and about the same on the rest.
  - Robustness test: 9.46 (SAC) vs 9.68 (SACLite). — [ar5iv](https://ar5iv.arxiv.org/html/2201.12434)
- Discount and α settings in that paper:
  - γ = 0.99 (0.98 for manipulation). No analysis of how the entropy reward's magnitude scales with 1/(1−γ). No evidence at γ ≥ 0.999.
  - A fixed α = 0.2 "hardly makes any progress" on SimpleChain.
  - Learned α "typically decreases from a large initial value to a small one." The official implementation starts at α = 1.0 with learning rate 3e-4.
  - Target entropies used: −1 per action dimension for locomotion/Safety Gym, ≈ −0.916 for manipulation, ≈ −1.609 for BipedalWalker. — [ar5iv](https://ar5iv.arxiv.org/html/2201.12434)

**Earlier SAC papers and library defaults** [BK]
- Haarnoja et al., original SAC [PR, ICML 2018]: temperature was fixed, and reward scale (equivalent to inverse temperature) was the hyperparameter SAC was most sensitive to. — [arXiv 1801.01290](https://arxiv.org/abs/1801.01290)
- Haarnoja et al., "SAC Algorithms and Applications" [PP, 2018]: α is learned as the dual variable of a minimum-entropy constraint, with the target entropy heuristic −dim(A). — [arXiv 1812.05905](https://arxiv.org/abs/1812.05905)
- Stable-Baselines3: `ent_coef='auto'` (initial value 1.0; `'auto_0.1'` sets the initial value) and `target_entropy='auto'` = −dim(A). CleanRL's SAC autotune uses the same −dim(A) target. — [SB3 SAC docs](https://stable-baselines3.readthedocs.io/en/master/modules/sac.html); [CleanRL SAC docs](https://docs.cleanrl.dev/rl-algorithms/sac/)

**Termination bias from the sign of the per-step reward**
- Kostrikov et al., Discriminator-Actor-Critic [PR, ICLR 2019] [BK]: with zero-valued terminal states, strictly positive per-step rewards cause a survival bias and strictly negative ones cause an early-termination bias. Their fix is an explicit absorbing state whose value is learned by bootstrapping instead of being fixed at 0. The same reasoning applies to an entropy reward of fixed sign. — [arXiv 1809.02925](https://arxiv.org/abs/1809.02925)
- Sun et al. [PR, NeurIPS 2022] [BK]: shifting rewards by a constant is equivalent to shifting the Q initialization. A negative shift makes the learner conservative and a positive shift makes it optimistic. An entropy reward pinned at H̄ < 0 acts as a constant negative shift of α·H̄. — [arXiv 2209.07288](https://arxiv.org/abs/2209.07288)
- Ma (2022), "Entropy Augmented RL" [PP] [V, search snippet only]: the −α log π term is attached to a specific time step, so the soft backup is not "absorbing" in the sense GAE needs. — [arXiv 2208.09322](https://arxiv.org/pdf/2208.09322)

**Average-reward and separate-critic versions of SAC**
- RVI-SAC, Hisaki & Ono [PR, ICML 2024] (2023–26) [V]:
  - Folds entropy into the reward (R − log π) and learns an average-reward soft Q with RVI Q-learning plus a "Delayed f(Q) Update."
  - For terminating tasks it adds a reset cost, turning them into continuing tasks. The cost is a Lagrange multiplier that enforces a reset-frequency constraint ρ_reset ≤ ε_reset, which "is independent of reward scale." By contrast, ATRPO hard-coded a reset cost of 100 for MuJoCo. — [arXiv 2408.01972](https://arxiv.org/html/2408.01972v1); [PMLR](https://proceedings.mlr.press/v235/hisaki24a.html)
- Average-Reward Soft Actor-Critic, Adamczyk, Makarenko, Tiomkin & Kulkarni [PR, accepted at RLC 2025 journal track] (2023–26) [V, abstract only]: a deep actor-critic for the entropy-regularized average-reward objective. It reports "superior performance for the average-reward criterion" against other average-reward methods. Details were not read. — [arXiv 2501.09080](https://arxiv.org/abs/2501.09080)
- TECRL / DSAC-E, Zhan et al. [PP, Oct 2025] (2023–26) [V, abstract]:
  - Names the problem as "non-stationary Q-value estimation caused by jointly injecting entropy and updating its weighting parameter," plus temperature tuning that looks only at single-step entropy.
  - Learns separate reward and entropy Q-functions, so reward targets are unaffected by α updates, and constrains trajectory-level cumulative entropy.
  - OpenAI Gym results only; the abstract gives no numbers. — [arXiv 2511.11592](https://arxiv.org/abs/2511.11592)
- Against centering: SimbaV2 [PR, ICML 2025] deliberately does not center rewards, because "shifting the reward can alter the optimal policy in episodic tasks." — [arXiv 2502.15280 (HTML)](https://arxiv.org/html/2502.15280v2)
- A targeted search for "subtract target entropy in the critic target" found no paper that proposes r + α(H − H̄). — [search results incl. MathWorks SAC docs](https://au.mathworks.com/help/reinforcement-learning/ug/sac-agents.html), [Revisiting Discrete SAC](https://ar5iv.labs.arxiv.org/html/2209.10081)

### Inferences
**The offset is the correct value, not a divergence.** The soft value decomposes as Q_soft = Q_task + α·E[Σ γ^t H_t]. With H pinned near H̄ = −2, the entropy part is ≈ −2α·γ/(1−γ). That is −8,000α at γ = 0.99975, −2,000α at γ = 0.999, and −200α at γ = 0.99. So the mean Q of about −8,000α that the user sees is the correct soft-Bellman fixed point. A normalization layer will not "fix" it, because the target really is that large.

**What goes wrong because of the offset.** It scales linearly with the effective horizon, while the task signal stays inside about [−22, +16]. It creates two problems:
1. Every change of Δα in α moves all targets by about 8,000·Δα. This is exactly the non-stationarity TECRL identifies.
2. Termination avoids the remaining discounted entropy cost. At 10 Hz, γ^1800 = e^(1800·ln 0.99975) ≈ 0.638. So an episode cut at 1,800 steps still carries 2α(1 − 0.638)/(1 − γ) ≈ 2,900α of entropy cost. If timeouts are bootstrapped, the cost of continuing is the full ≈ 8,000α. A failure at −10 versus a finish at +10 is a gap of only 20. So for α greater than roughly 0.007 (or about 0.0025 with bootstrapped timeouts), ending early, even by failing, can beat a long successful run under the soft objective. This is the Yu et al. pessimism mechanism, made larger by the horizon.

**Ranking the fixes by how directly they address this setup.**
1. SACLite. The critic learns only Q_task, which is bounded by the feasible return range, and entropy still regularizes the actor. α-tuning is unchanged because it lives in the actor/α losses.
2. Average-reward SAC (RVI-SAC or ARSAC) with an automatic reset cost. This removes the offset by construction and replaces the arbitrary terminal-zero with a learned penalty.
3. SACZero, or the proposed r + α(H − H̄). Either removes the mean offset. The residual bias flips sign whenever H drifts from H̄ (that is, while α lags), and SimbaV2 warns that shifting rewards changes the optimal policy in episodic tasks. Here, though, that "change" is the removal of the entropy-induced termination bias, which is the goal.
4. A separate entropy critic (TECRL). This is preprint-level evidence only.

### Gaps
- No paper reports any of these entropy variants at γ ≥ 0.999 or with horizons of thousands of steps.
- Yu et al. give no aggregate statistics, and no peer-reviewed venue was found for that paper.
- The full ARSAC text, including how entropy enters the gain and the comparison with discounted SAC, was not read.
- No published analysis of r + α(H − H̄) specifically was found.

## Q2. Time limits and truncation: Pardo et al. 2018, partial-episode bootstrapping, and how timeouts create local optima

### Takeaway
Whether a timeout should be a real terminal depends on whether the task itself is time-limited:
- If the task is time-limited, the remaining time must be in the observation; otherwise the state is aliased and the value is not Markov.
- If the task is time-unlimited but training uses timeouts, timeouts must be bootstrapped (partial-episode bootstrapping, PEB), never treated as terminal.

With an entropy cost, the two choices give very different values: a bootstrapped timeout carries the full −α|H̄|/(1−γ), while a terminal carries 0. This makes any true termination look attractive.

### Cited Findings
- Pardo, Tavakoli, Levdik & Kormushev, "Time Limits in Reinforcement Learning" [PR, ICML 2018] [BK]:
  - Time-limited tasks: add time-awareness (the remaining time) to the observation.
  - Time-unlimited tasks trained with time limits: use partial-episode bootstrapping, meaning bootstrap from the value at a timeout instead of setting it to 0.
  - Treating timeouts as terminal without time in the observation causes state aliasing, invalid targets and degraded or unstable policies. Shown with PPO on gridworlds and MuJoCo.
  — [arXiv 1712.00378](https://arxiv.org/abs/1712.00378); [PMLR v80](http://proceedings.mlr.press/v80/pardo18a.html)
- Yu et al. (2022) [PP] [V]: when an episode ends by timeout and time is not in the observation, "the discount of the final step should be γ instead of 0." Whether a task is episodic is a property of the MDP, not of the truncation used for training. — [ar5iv 2201.12434](https://ar5iv.arxiv.org/html/2201.12434)
- Gymnasium separates `terminated` from `truncated`, and SB3's replay buffer bootstraps through truncations (`handle_timeout_termination=True`, the default) [BK]. — [Gymnasium: handling time limits](https://gymnasium.farama.org/tutorials/gymnasium_basics/handling_time_limits/); [SB3 SAC docs](https://stable-baselines3.readthedocs.io/en/master/modules/sac.html)
- RVI-SAC [PR, ICML 2024] [V] avoids the episodic/continuing ambiguity in MuJoCo (episodes capped at 1,000 steps) by turning terminations into resets with a learned cost. — [arXiv 2408.01972](https://arxiv.org/html/2408.01972v1)
- DAC [PR, ICLR 2019] [BK]: absorbing-state handling (an explicit absorbing state with a learned value) removes the termination bias that fixed-sign per-step rewards cause. — [arXiv 1809.02925](https://arxiv.org/abs/1809.02925)

### Inferences
- Task U has 1,800-step episodes against an effective horizon of 4,000 (γ^1800 ≈ 0.64). With PEB, about 64% of the discounted entropy offset at a start state comes from bootstrapped value beyond the timeout, which never appears in the data. That is exactly where TD error compounds over many bootstraps.
- If reaching 1,800 decisions is part of the task (for example, a timeout counted as a −10 failure), it is a real terminal. Pardo et al. then require remaining time in the observation. Otherwise the critic cannot tell that a state 10 steps from timeout has no future entropy cost, while the same state at step 10 has about 8,000α of it.
- Whichever convention is chosen, a negative per-step soft reward and a zero terminal value together create the early-termination local optimum. Removing the entropy from the critic (SACLite) or centering it makes the timeout convention matter much less.

### Gaps
- No quantitative study of PEB or time-awareness with SAC at γ ≥ 0.999 was found.
- Pardo et al.'s exact numbers were not re-fetched.

## Q3. High discount factors and long horizons: instability as γ → 1, smaller planning horizons, logarithmic mapping, discount curricula, hyperbolic discounting and average-reward actor-critics

### Takeaway
There is direct evidence that γ close to 1 destabilizes off-policy continuous control. In RVI-SAC, SAC at γ = 0.999 was worse than at γ = 0.99 on 5 of 6 MuJoCo tasks with 1,000-step episodes. State-of-the-art continuous-control methods cap γ at about 0.99–0.997. The two fixes with peer-reviewed support for going further are:
- the average-reward reformulation (RVI-SAC, ARO-DDPG, ARSAC) and reward centering, whose benefit "increases further as the discount factor approaches one";
- value transforms. A transformed Bellman operator made γ = 0.999 stable in Atari, and Agent57 went to about 0.9997.

A separate, under-appreciated factor for 10 Hz control is that the action gap shrinks in proportion to the decision interval.

### Cited Findings
**Evidence that large γ destabilizes**
- RVI-SAC [PR, ICML 2024] (2023–26) [V]:
  - On Ant, HalfCheetah, Hopper, Walker2d, Humanoid and Swimmer (10 seeds, 1,000-step episodes), SAC with γ = 0.99 "performs better than the other rates in environments other than Swimmer." Swimmer "requires" γ = 0.999, but "in the environments other than Swimmer, a setting of γ=0.999 shows lower performance."
  - "Setting a large discount rate can lead to destabilization of learning and slow convergence" (citing Fujimoto et al. 2018 and Dewanto & Gallagher 2021).
  - RVI-SAC matches SAC(0.999) on Swimmer and matches or beats SAC(0.99) elsewhere, except on HalfCheetah, where it is weaker. It beats ARO-DDPG "in all environments."
  — [arXiv 2408.01972](https://arxiv.org/html/2408.01972v1)
- Fujimoto, van Hoof & Meger, TD3 [PR, ICML 2018] [BK]: the variance of TD estimates grows with γ. "Given a large discount factor γ, the variance can grow rapidly with each update if the error from each update is not tamed." — [arXiv 1802.09477](https://arxiv.org/abs/1802.09477)
- Dewanto & Gallagher (2021), comparing average and discounted optimality criteria [PP] [cited by RVI-SAC, not fetched]. — [arXiv 2107.01348](https://arxiv.org/abs/2107.01348)

**Average-reward methods and reward centering**
- Naik, Wan, Tomar & Sutton, "Reward Centering" [PR, RLC 2024] (2023–26) [V, abstract]:
  - Subtracting the empirical average reward gives gains that are "substantial at commonly used discount factors" and that "increase further as the discount factor approaches one."
  - Methods with centering are unaffected by constant reward shifts, which make standard methods "perform much worse."
  - The paper gives an off-policy estimator, and the authors "expect almost every reinforcement-learning algorithm to benefit."
  — [arXiv 2405.09999](https://arxiv.org/abs/2405.09999)
- ARO-DDPG, Saxena et al. [PR, ICML 2023] [BK]: off-policy average-reward actor-critic with a deterministic policy. RVI-SAC describes it as the only prior off-policy, continuous-action, average-reward deep RL method. — [arXiv 2305.12239](https://arxiv.org/abs/2305.12239); [V via RVI-SAC](https://arxiv.org/html/2408.01972v1)
- Average-Reward SAC [PR, RLC 2025] (2023–26) [V, abstract]. — [arXiv 2501.09080](https://arxiv.org/abs/2501.09080)

**Value transforms that enable very high γ**
- Pohlen et al., "Observe and Look Further" [PP, 2018] [V]: a transformed Bellman operator for "rewards of varying densities and scales," plus a temporal-consistency loss, lets the agent "train stably using a discount factor of γ = 0.999 (instead of γ = 0.99), extending the effective planning horizon by an order of magnitude." It exceeds average human performance on 40 of 42 Atari games. — [arXiv 1805.11593](https://arxiv.org/abs/1805.11593)
- Agent57, Badia et al. [PR, ICML 2020] [BK]: a family of discount factors up to about γ = 0.9997, made workable by the transformed Bellman operator and separate extrinsic and intrinsic value heads. — [arXiv 2003.13350](https://arxiv.org/abs/2003.13350)

**Defaults in current methods (evidence of practical γ ceilings)**
- DreamerV3 [PR, Nature 2025; arXiv 2023] (2023–26) [BK]: γ = 0.997 (horizon 333), with symlog/two-hot critics and return normalization. — [arXiv 2301.04104](https://arxiv.org/abs/2301.04104)
- TD-MPC2 [PR, ICLR 2024] (2023–26) [BK]: a heuristic γ computed from episode length and clipped to [0.95, 0.995]. — [arXiv 2310.16828](https://arxiv.org/abs/2310.16828)

**Why a smaller γ can be better, and how to make one workable**
- Jiang, Kulesza, Singh & Lewis [PR, AAMAS 2015] [BK]: the planning horizon acts as complexity control. With a model or data of limited accuracy, planning with a γ smaller than the evaluation γ can give better policies, because the size of the effective policy class grows with γ. — [AAMAS 2015 proceedings](https://www.ifaamas.org/Proceedings/aamas2015/aamas/p1181.pdf)
- Amit, Meir & Ciosek [PR, ICML 2020] [BK]: a lower γ acts as a regularizer in TD learning and helps most when data are limited. — [arXiv 2007.02040](https://arxiv.org/abs/2007.02040)
- van Seijen, Fatemi & Tavakoli [PR, NeurIPS 2019] [BK]: argue that low γ performs poorly mainly because the action gap varies widely across states. Learning in a logarithmic value space makes lower γ work well on Atari. A follow-up, Orchestrated Value Mapping (Fatemi & Tavakoli [PR, ICLR 2022]), generalizes this. — [arXiv 1906.00572](https://arxiv.org/abs/1906.00572); [arXiv 2203.07171](https://arxiv.org/abs/2203.07171)

**Fine time steps**
- Tallec, Blier & Ollivier [PR, ICML 2019] [BK]: as the decision interval δt → 0, Q(s,a) collapses onto V(s) (the action gap is O(δt)), and Q-learning and DDPG degrade. Their fix is advantage updating with δt-scaled advantages and γ = γ₀^δt. — [arXiv 1901.09732](https://arxiv.org/abs/1901.09732)

**Discount schedules, multiple horizons and fixed horizons**
- François-Lavet, Fonteneau & Ernst [workshop, NeurIPS 2015 Deep RL WS] [BK]: raising γ during training, with a decaying learning rate, improved DQN. — [arXiv 1512.02011](https://arxiv.org/abs/1512.02011)
- Xu, van Hasselt & Silver [PR, NeurIPS 2018] [BK]: meta-gradients adapt γ and λ online. — [arXiv 1805.09801](https://arxiv.org/abs/1805.09801)
- Fedus et al. [PP, 2019] [BK]: learn values for many γ at once. Hyperbolic discounting is approximated by integrating over γ, and multi-horizon auxiliary heads improved Atari. — [arXiv 1902.06865](https://arxiv.org/abs/1902.06865)
- De Asis, Chan, Pitis, Sutton & Graves, fixed-horizon TD [PR, AAAI 2020] [BK]: learn values for horizons h = 1..H, each bootstrapping from the shorter one. This removes the self-bootstrapping feedback loop and is stable with function approximation. — [arXiv 1909.03906](https://arxiv.org/abs/1909.03906)

**Horizon as a scaling bottleneck**
- Park, Frans, Mann, Eysenbach, Kumar & Levine, "Horizon Reduction Makes RL Scalable" [PR, NeurIPS 2025] (2023–26) [V]: "the horizon is the main cause behind the poor scaling of offline RL." "Long horizons indeed present a fundamental barrier." Horizon-reduction methods, including SHARSA, give the best asymptotic performance and scaling, on datasets up to 1000× larger than usual. This is offline RL only. — [arXiv 2506.04168](https://arxiv.org/abs/2506.04168)

### Inferences
- The SailGym γ = 0.99975 sits above everything validated in continuous control in this literature:
  - SAC at 0.999 is already degraded on 5 of 6 MuJoCo tasks;
  - TD-MPC2 caps γ at 0.995 and DreamerV3 uses 0.997;
  - only Atari value-based agents with value transforms reach 0.999–0.9997.
- The same SAC being stable at γ = 0.999 but not at 0.99975 fits two facts: the entropy offset is 4× smaller at 0.999, and variance grows with 1/(1−γ) (TD3).
- At 10 Hz, the Tallec et al. argument adds an action gap that is small relative to |Q|. With γ near 1 and a large offset, the critic's actor-relevant signal is a tiny ripple on a large, noisy constant.
- Candidate fixes, ordered by the strength of evidence:
  1. Average-reward soft actor-critic (RVI-SAC, ARSAC) or reward centering. These remove the r̄/(1−γ) constant by construction and target exactly the γ → 1 regime.
  2. A lower training γ (for example, 0.999, where the user's SAC is stable), justified by Jiang et al. and Amit et al. If the shaping is potential-based, its γ must match the new γ so the optimal policy is preserved.
  3. Value transforms (h-transform/log mapping) or fixed-horizon or multi-horizon critics.
  4. A δt-aware advantage parameterization.

### Gaps
- No study of SAC or TD3 at γ ≥ 0.9997, or with continuous-control episodes of thousands of decisions, was found.
- The specific γ values tested in Reward Centering (the abstract gives none) and ARSAC's benchmarks were not verified.
- RVI-SAC's numerical scores sit in figures and appendix tables that were not read.

## Q4. Scale-robust value learning: PopArt, symlog/two-hot (DreamerV3), HL-Gauss ("Stop Regressing"), and reward scaling for SAC

### Takeaway
Scale-robust losses and normalizers (PopArt, symlog/two-hot, HL-Gauss/categorical, SimbaV2's return-std reward scaling) stabilize gradient magnitudes across reward scales. They do not, by themselves, solve a large constant offset riding on a small task signal:
- PopArt is the exception. Its adaptive mean/scale moves the offset into normalization statistics without changing the outputs.
- Symlog compression actually lowers resolution at |Q| in the thousands.
- SimbaV2 deliberately avoids centering, so it keeps the offset.

### Cited Findings
- PopArt, van Hasselt, Guez, Hessel, Mnih & Silver, "Learning values across many orders of magnitude" [PR, NeurIPS 2016] [BK]: adaptively normalizes value targets (running mean μ and scale σ) and rescales the last layer so outputs are preserved. It enabled DQN without reward clipping on Atari. — [arXiv 1602.07714](https://arxiv.org/abs/1602.07714)
- Multi-task PopArt, Hessel et al. [PR, AAAI 2019] [BK]: one IMPALA agent across 57 Atari games exceeded median human performance. — [arXiv 1809.04474](https://arxiv.org/abs/1809.04474)
- Pohlen et al. [PP, 2018] [V]: the h-transformed Bellman operator handles "rewards of varying densities and scales" and enabled γ = 0.999. — [arXiv 1805.11593](https://arxiv.org/abs/1805.11593)
- DreamerV3 [PR, Nature 2025] (2023–26) [BK]: symlog-transformed predictions, a two-hot categorical critic over exponentially spaced bins, and return normalization that divides by max(1, P95 − P5) so small returns are not amplified. Fixed hyperparameters, including a fixed entropy scale, work across domains. — [arXiv 2301.04104](https://arxiv.org/abs/2301.04104)
- Imani & White [PR, ICML 2018] [BK]: HL-Gauss, a histogram loss with Gaussian smoothing for regression. — [arXiv 1806.04613](https://arxiv.org/abs/1806.04613)
- Farebrother et al., "Stop Regressing" [PR, ICML 2024] (2023–26) [BK]: training value functions with cross-entropy, especially HL-Gauss, instead of MSE improves performance and scalability. Shown on Atari (single-task with SoftMoE, multi-task with ResNets), Q-transformer robotic manipulation, chess without search, and a Wordle agent. The authors attribute the gains to robustness to noisy targets and non-stationarity. — [arXiv 2403.03950](https://arxiv.org/abs/2403.03950)
- SimbaV2, Lee et al. [PR, ICML 2025 spotlight] (2023–26) [V]: a SAC-based method, state of the art on 57 continuous-control tasks across 4 domains.
  - Reward scaling: keep a running discounted return G_t ← γG_{t−1} + r_t (reset each episode), track its running variance σ²_G and running maximum G_max,t, and scale r̄_t = r_t / max(√(σ²_G + ε), G_max,t / G_max). The goal is to "maintain unit variance of the target."
  - Rewards are not centered, because "shifting the reward can alter the optimal policy in episodic tasks."
  - Critic: categorical over [G_min, G_max] with a KL loss, adopted because an MSE Bellman loss "is susceptible to outliers."
  - Reward scaling helps most in high-reward-variance environments such as MuJoCo.
  — [arXiv 2502.15280](https://arxiv.org/abs/2502.15280); [HTML](https://arxiv.org/html/2502.15280v2)
- Reward scale matters a lot for off-policy actor-critics:
  - Henderson et al. [PR, AAAI 2018] [BK]: reward rescaling has large and inconsistent effects on DDPG. — [arXiv 1709.06560](https://arxiv.org/abs/1709.06560)
  - Engstrom et al. [PR, ICLR 2020] [BK]: PPO's reward scaling by the running std of the discounted return is a key "code-level optimization," the same idea SimbaV2 uses. — [arXiv 2005.12729](https://arxiv.org/abs/2005.12729)
  - The original SAC was most sensitive to reward scale [BK]. — [arXiv 1801.01290](https://arxiv.org/abs/1801.01290)
- Reward centering [PR, RLC 2024] [V]: centered methods are invariant to constant reward shifts. — [arXiv 2405.09999](https://arxiv.org/abs/2405.09999)

### Inferences
- A per-decision reward of ≈ 0.003 with terminals of ±10 is badly scaled for a default SAC. Under SimbaV2-style scaling, the return-std normalizer would be set by the ±10 terminals and the entropy term.
- It is not clear whether SimbaV2 scales the entropy term along with r (gap). If it does not, the entropy-to-task ratio gets worse.
- A categorical or HL-Gauss critic on bounded support needs a support wide enough for the −8,000α offset. Its bins must still resolve a task range of about 38. Symlog compresses magnitudes in the thousands, so it makes this worse.
- So the order should be: remove the offset first (SACLite, centering, average-reward, or PopArt's μ), then apply scale robustness. PopArt alone covers both offset and scale while keeping the policy unchanged, but it is not validated in SAC at γ ≥ 0.999.

### Gaps
- None of these scale methods was found evaluated at γ ≥ 0.999 in continuous control.
- SimbaV2's G_min/G_max values, atom count, γ, and treatment of the entropy term in the scaled target were not read (truncated appendix).
- No direct study of PopArt with SAC was found.

## Q5. Critic regularization and normalization (LayerNorm/RLPD, CrossQ, BRO, SimBa, weight normalization, "Is high variance unavoidable"): do they target long-horizon/high-γ problems or sample-efficiency/high-UTD problems?

### Takeaway
Almost all of this work targets high update-to-data (UTD) overestimation, extrapolation from offline data, plasticity loss, or early-training numerical instability. It is evaluated at γ ≈ 0.99 on roughly 1,000-step tasks. None targets the γ/(1−γ) magnitude of a correct soft value. LayerNorm bounds hidden features, not the output, which the last layer can still scale to any value. A failure to keep Q within ±250 of the offset is therefore expected, not a sign that the normalization was implemented wrongly. Bounded architectures (SimbaV2) move the problem into reward scaling and a fixed categorical support.

### Cited Findings
**High-UTD and offline-data work**
- RLPD, Ball, Smith, Kostrikov & Levine [PR, ICML 2023] (2023–26) [BK]: LayerNorm in the critic limits catastrophic overestimation from extrapolating to out-of-distribution actions when online learning is seeded with offline data. It also uses high UTD and a random-subset critic ensemble. The motivation is offline-data extrapolation and sample efficiency. — [arXiv 2302.02948](https://arxiv.org/abs/2302.02948)
- DroQ [PR, ICLR 2022] [BK]: dropout plus LayerNorm Q-functions as a cheaper REDQ at high UTD. REDQ [PR, ICLR 2021] [BK]: ensembles with in-target minimization over random subsets for UTD = 20. — [arXiv 2110.02034](https://arxiv.org/abs/2110.02034); [arXiv 2101.05982](https://arxiv.org/abs/2101.05982)
- CrossQ, Bhatt et al. [PR, ICLR 2024] (2023–26) [BK]: Batch Renormalization in the critic, no target networks, wider critics. It matches REDQ/DroQ sample efficiency at UTD = 1 with less compute. — [arXiv 1902.05605](https://arxiv.org/abs/1902.05605)
- BRO, Nauman et al. [PR, NeurIPS 2024] (2023–26) [BK]: large critics with LayerNorm residual blocks (BroNet), plus optimistic exploration. Reported as the first model-free agent near-optimal on the DMC dog and humanoid tasks. — [arXiv 2405.16158](https://arxiv.org/abs/2405.16158)
- SimBa, Lee et al. [PR, ICLR 2025] (2023–26) [BK]: running-statistics observation normalization, residual feed-forward blocks with pre-LayerNorm, and a post-LayerNorm, so that parameter scaling works. — [arXiv 2410.09754](https://arxiv.org/abs/2410.09754)
- SimbaV2 [PR, ICML 2025] (2023–26) [V]: hyperspherical normalization "constraining the growth of weight and feature norm," plus a distributional critic with reward scaling. Its stated motivation is overfitting and "unstable optimization" when scaling models. — [arXiv 2502.15280](https://arxiv.org/abs/2502.15280)
- Hussing, Voelcker, Gilitschenski, Farahmand & Eaton [PR, RLC 2024] (2023–26) [V]: at high update ratios the core problem is value divergence. "Overinflated Q-values are found not only on out-of-distribution but also in-distribution data," linked to overestimation of unseen actions driven by optimizer momentum. A "simple unit-ball normalization" allows large update ratios without resets and is strong on the DMC dog tasks. — [arXiv 2403.05996](https://arxiv.org/abs/2403.05996)

**Early-training instability**
- Bjorck, Gomes & Weinberger, "Is High Variance Unavoidable in RL?" [PR, ICLR 2022] [V]: in pixel-based continuous control, variance across runs comes mainly from early "outlier" runs. One cause is numerical instability that saturates nonlinearities. Normalizing the penultimate features is a simple, effective fix that also allows larger learning rates. — [arXiv 2110.11222](https://arxiv.org/abs/2110.11222)

**Broader analyses**
- Nauman, Bortkiewicz, Miłoś, Trzciński, Ostaszewski & Cygan, "Overestimation, Overfitting, and Plasticity in Actor-Critic" [PR, ICML 2024] (2023–26) [BK]: a large empirical study. Generic network regularization of the critic (for example, LayerNorm) often beats RL-specific overestimation fixes. The setting is high-UTD sample efficiency. — [arXiv 2403.00514](https://arxiv.org/abs/2403.00514)
- Gallici et al., PQN [PR, ICLR 2025] (2023–26) [BK]: LayerNorm plus L2 regularization provably stabilizes off-policy TD, addressing the "deadly triad," without target networks or replay. — [arXiv 2407.04811](https://arxiv.org/abs/2407.04811)
- Lyle et al. [PR, NeurIPS 2024] (2023–26) [BK]: normalization layers implicitly decay the effective learning rate as parameter norms grow. They propose normalize-and-project. — [arXiv 2407.01800](https://arxiv.org/abs/2407.01800)

### Inferences
- With Q = w·LN(φ) + b, LayerNorm bounds ‖LN(φ)‖. Large |Q| comes through ‖w‖ and b, so a 2×64 tanh critic with LayerNorm can and will represent −8,000α if that is the target.
- These techniques reduce divergence (Q growing without bound away from its fixed point) and high-UTD overestimation. The SailGym symptom (mean Q tracks −α|H̄|γ/(1−γ)) is convergence to the correct fixed point of a badly conditioned objective.
- A 2×64 tanh critic is also much smaller than the 256–1,024-wide critics in BRO and SimBa. Tanh can saturate early, which is Bjorck et al.'s failure mode, so a wider ReLU-family critic with LayerNorm is worth testing, but only after removing the offset.

### Gaps
- None of these methods was found evaluated at γ ≥ 0.999 or on horizons of thousands of steps; their published settings use γ ≈ 0.99, and per-paper γ values were not re-checked.
- No weight-normalization study specific to long-horizon SAC was found.

## Q6. Distributional critics (e.g., TQC) and overestimation control at long horizons

### Takeaway
Distributional critics (TQC, categorical/HL-Gauss, DSAC) give finer control over overestimation and more robust losses. Overestimation or underestimation bias compounds over roughly 1/(1−γ) bootstraps, so control matters more at long horizons. But no evidence was found for any of them at γ ≥ 0.999 in continuous control. In the SailGym symptom, |Q| is dominated by a known entropy offset, not by overestimation.

### Cited Findings
- TQC, Kuznetsov, Shvechikov, Grishin & Vetrov [PR, ICML 2020] [BK]: pools the quantile atoms of several distributional critics and drops the top atoms, so overestimation can be tuned finely. Reports state-of-the-art MuJoCo results, including about a 25% improvement on Humanoid. Implemented in SB3-contrib. — [arXiv 2005.04269](https://arxiv.org/abs/2005.04269)
- C51, Bellemare, Dabney & Munos [PR, ICML 2017] [BK]: the categorical distributional critic that SimbaV2 builds on. — [arXiv 1707.06887](https://arxiv.org/abs/1707.06887)
- SimbaV2 [PR, ICML 2025] [V]: a categorical critic chosen because an MSE Bellman loss is "susceptible to outliers," paired with return-std reward scaling. — [HTML](https://arxiv.org/html/2502.15280v2)
- DSAC-T, distributional SAC with three refinements [BK]: Duan et al.; peer-review status not verified. DSAC-E (2025 preprint) builds on it and adds a separate entropy critic. — [arXiv 2310.05858](https://arxiv.org/abs/2310.05858); [arXiv 2511.11592](https://arxiv.org/abs/2511.11592)
- TD3 [PR, ICML 2018] [BK]: clipped double-Q exists because estimation error accumulates over bootstrapping, and its variance grows with γ. — [arXiv 1802.09477](https://arxiv.org/abs/1802.09477)
- "Stop Regressing" [PR, ICML 2024] [BK]: classification-based (HL-Gauss) value losses are more robust to noisy, non-stationary targets. — [arXiv 2403.03950](https://arxiv.org/abs/2403.03950)

### Inferences
- In SAC the clipped double-Q minimum is applied to the full soft value, entropy included. At a horizon of 4,000, a small per-step pessimistic bias accumulates through 1/(1−γ). TQC's truncation fraction is a tuned hyperparameter (it varies by environment), so it would need re-tuning at γ = 0.99975.
- A quantile or categorical critic over a value dominated by −8,000α spends its resolution on the offset. Removing the entropy from the critic first (SACLite or average-reward) makes distributional critics far more useful, because the support can then be fitted to the feasible range of about [−22, +16].

### Gaps
- No evidence was found for TQC or any distributional critic at γ ≥ 0.999 or with horizons of thousands of steps.
- No study was found that separates overestimation bias from entropy-offset effects in long-horizon SAC.
- TQC's exact truncation defaults and the per-environment numbers were not re-fetched.
