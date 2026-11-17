# Prior RL work on sailboat control (upwind and tacking) and close analogues

Scope note: these notes cover published or publicly documented work up to Nov 2026. Each entry gives the venue type: **[PR]** peer-reviewed journal or conference, **[WS]** workshop, **[TH]** thesis, **[PP]** preprint, **[BL]** blog or vendor material, **[GH]** code repository. "Not stated" means the source I could read does not say. Some items were read only as abstracts or search snippets, and those are flagged.

---

## Q1. Which works apply RL to sailboats, and did any learn tacking or beating upwind end-to-end without demonstrations? What made it work?

### Takeaway
I found two documented cases where an RL agent learned to beat upwind (and tack) **from scratch, with no demonstrations**:
- **Bink (TU Delft/MARIN, 2024 MSc thesis; journal version in EAAI 2026).** SAC with rudder-rate, sheet-rate and ballast-rate actions on an Optimist, reaching a target about 20 m upwind.
- **Suda & Nikovski (MERL, IJCNN 2022).** PPO and SAC on a kinematic polar-diagram boat.

Both made the problem much easier than SailGym Task U:
- **Bink:** coarser decisions (2 Hz, at most 720 per episode), a dense per-step reward that depends on upwind distance, terminal rewards in the thousands, an explicit timeout penalty, and randomised start states.
- **MERL:** 1-minute decision steps (about 85 per episode), a point-mass "polar" model with no momentum, and a heading-change action instead of rudder control.

Most other sailing-RL work hand-designs the hard part: a tack/stay macro-action with a helming controller, action masking of no-go headings, or tacking scripted as a discrete manoeuvre. Several works only do heading control or path following.

### Cited Findings

**A. Works where upwind progress or tacking emerged from learning (no demonstrations stated)**

- **Bink, K.J.A. (2024), "Autonomous Sailing with Sim-to-Real Reinforcement Learning", MSc thesis, TU Delft / MARIN [TH]** (graduated 28 Feb 2024; supervisors G.D. Weymouth and B. Duz). Fast-time simulation of an Optimist dinghy, trained to reach an upwind target, then transferred to a robotised Optimist in MARIN's Offshore Basin — [TU Delft repository](https://repository.tudelft.nl/record/uuid:1284613a-d9e7-4076-b120-8349acdca4ca); [MARIN project page](https://www.marin.nl/en/research/artificial-intelligence-applications/autonomous-sailing-against-the-wind)
  - **Action space** (continuous): "Move rudder", "Let sheet out/in", "Move weight" (ballast). This is rate-style actuation very close to SailGym's rudder-rate and sheet-rate actions — [thesis Table 3.1, PDF via TU Delft repository](https://repository.tudelft.nl/record/uuid:1284613a-d9e7-4076-b120-8349acdca4ca)
  - **Observations:** rudder angle, sheet length, ballast position, boom angle, distance to target (x, y), velocity (u, v), yaw, yaw rate, roll, wind heading, wind speed. All normalised to [−1, 1] — [thesis](https://repository.tudelft.nl/record/uuid:1284613a-d9e7-4076-b120-8349acdca4ca)
  - **Algorithm:** SAC (Stable-Baselines3, RL Zoo hyperparameters, MLP with 2×64 tanh layers), trained from scratch. The training section describes no imitation or demonstration data. Imitation learning appears only in the literature review as a sim-to-real option — [thesis §5.4](https://repository.tudelft.nl/record/uuid:1284613a-d9e7-4076-b120-8349acdca4ca)
  - **Decision rate:** the simulator steps at Δt = 0.01 s with "skipped steps" = 49, so the agent acts every 0.5 s (2 Hz). With the 360 s limit, that is at most 720 decisions per episode — [thesis §5.4](https://repository.tudelft.nl/record/uuid:1284613a-d9e7-4076-b120-8349acdca4ca)
  - **Reward:**
    - A per-step "Distance Reward", which the text extraction renders as "2 × |x_ship − x1_target| + 1". The exact form is ambiguous in the extracted text; it may be a fraction.
    - A per-step "Distance Penalty" of −2 × (x_target − x_ship).
    - A gybe penalty of −100.
    - Actuator-rate penalties of −5 × the rudder, ballast and sheet rates.
    - Terminal rewards: out of time (t > 360 s) −1000; out of bounds −3000; capsize risk (roll > π/5) −3000; reaching the target (x > 10 m) +2000.
    - γ = 0.998.
    - Start at x = −10 m, so the target is about 20 m upwind — [thesis §5.4](https://repository.tudelft.nl/record/uuid:1284613a-d9e7-4076-b120-8349acdca4ca)
  - **Training scale:** 400,000 timesteps (about 4,000–5,300 episodes, about 21–23 h on CPU). In-training success rates were 95.8–98.2% across 4 validation runs — [thesis Table 5.1](https://repository.tudelft.nl/record/uuid:1284613a-d9e7-4076-b120-8349acdca4ca)
  - **Emergence:** "the agent learns an effective policy after about 400 episodes already. It explores different strategies like gybing to gain speed and/or using the sides of the basin where the wind speed is lower. Finally it learns to tack to go upwind, called 'beating'." The thesis credits γ = 0.998 for making the faster tacking strategy the highest-return one — [thesis §5.4](https://repository.tudelft.nl/record/uuid:1284613a-d9e7-4076-b120-8349acdca4ca)
  - **Seed variance and exploit:** one of 4 seeds (SAC_test_4) did not learn tacking. It used low-wind areas at the basin side and learned to **scull**, pumping the rudder "to propel itself into the wind". The thesis calls this cheating and added an energy-ratio metric to catch it — [thesis §5.4, §5.8](https://repository.tudelft.nl/record/uuid:1284613a-d9e7-4076-b120-8349acdca4ca)
  - **Domain randomisation** of initial conditions: wind 40–60%, x ∈ [−10, −6] m, y ∈ [−8, 12] m, yaw ∈ [−0.8, 0.8] rad, speed ∈ [0, 0.6] m/s. Observation noise was used in some runs — [thesis §5.4](https://repository.tudelft.nl/record/uuid:1284613a-d9e7-4076-b120-8349acdca4ca)
  - **Real-world results:**
    - SAC_RI_3 succeeded in 100% of 12 basin runs.
    - The identically trained SAC_RI_4 succeeded in only 50% of 6 runs, because it drifted after tacks and ran out of time.
    - Agents trained with observation noise often "did not demonstrate the ability to tack" in the basin.
    - The rule-based baseline (SOTON) reached 97.2% in simulation and 93.8% in the basin — [thesis §6.3](https://repository.tudelft.nl/record/uuid:1284613a-d9e7-4076-b120-8349acdca4ca)
  - **Journal version [PR]:** Bink, Weymouth & Duz, "Autonomous sailing with sim-to-real reinforcement learning", *Engineering Applications of Artificial Intelligence* (2026). Per search snippets, RL agents trained with domain randomisation "achieve comparable success rates to conventional controllers while showcasing enhanced sailing capabilities like roll tacking and recovery from wind-stalled conditions". I read only the snippet (paywalled, HTTP 403). An SSRN preprint also exists — [ScienceDirect](https://www.sciencedirect.com/science/article/pii/S0952197626020889); [SSRN](https://papers.ssrn.com/sol3/papers.cfm?abstract_id=5339032)
  - **Correction:** one search-engine summary said the 2026 paper's agent tacks when it enters "tacking zones". Per the thesis, the tacking zones belong to the rule-based **SOTON baseline** (a tack is triggered 6 m from each basin side, with a 45° beating angle), not to the RL agent — [thesis §5.5](https://repository.tudelft.nl/record/uuid:1284613a-d9e7-4076-b120-8349acdca4ca)

- **Suda, T. & Nikovski, D. (2022), "Deep Reinforcement Learning for Optimal Sailing Upwind", IEEE IJCNN 2022 [PR]** (MERL TR2022-102) — [MERL PDF](https://merl.com/publications/docs/TR2022-102.pdf); [IEEE](https://ieeexplore.ieee.org/document/9892369/)
  - **Model:** a point-mass kinematic boat. Speed comes straight from the polar diagram (Bavaria Cruiser 32, 10 kn wind) as a function of the angle between heading and wind, so there is no momentum. Heading changes by the action u each step. Wind direction follows an AR(1) process (σ = 10°) — [MERL PDF](https://merl.com/publications/docs/TR2022-102.pdf)
  - **Two MDPs:**
    - Continuous: the action is the heading correction u per step.
    - Discrete: the action is only the choice of tack (port or starboard). A hand-designed helming controller sets the heading to the optimal VMG angle α*, and a steering controller turns the boat, so a tack is a macro-action — [MERL PDF](https://merl.com/publications/docs/TR2022-102.pdf)
  - **Task and reward:** the mark is 5 nautical miles dead upwind. The time step is **1 minute**. Reward is −1 per step until the boat crosses a finish line through the mark, with timeout at K steps giving a return of −K. There is no other shaping — [MERL PDF](https://merl.com/publications/docs/TR2022-102.pdf)
  - **Algorithms:** PPO, SAC and ARS (RLlib). Training took 6.6–26 h on a 12-core CPU. No demonstrations are mentioned — [MERL PDF](https://merl.com/publications/docs/TR2022-102.pdf)
  - **Results:**
    - Mean time to goal: continuous PPO 83.2 min and SAC 83.5 min; discrete PPO 85.2 and ARS 85.4; certainty-equivalence MPC 90.25 (continuous) and 92.9 (discrete).
    - Continuous PPO beat the better MPC in 87.4% of 1,000 paired wind realisations.
    - Learned policies tack later than MPC and avoid extra tacks.
    - PPO learned "in around 10^4 episodes" in the discrete case — [MERL PDF](https://merl.com/publications/docs/TR2022-102.pdf)
  - **Exploration failure:** in the continuous case, "SAC fails to even round the mark once for the first 2,000 episodes", while PPO "start[s] to improve performance almost immediately" — [MERL PDF](https://merl.com/publications/docs/TR2022-102.pdf)

- **Gabo-Tor, "gym-sailing" (GitHub/PyPI, about 2024) [GH]**
  - Environments: Sailboat-v0 (continuous rudder in [−1, 1]), SailboatDiscrete-v0 (rudder {−1, 0, 1}) and Motorboat-v0.
  - Observations: boat speed, heading relative to wind, heading rate, course to target, normalised distance.
  - Reward: an alive penalty per step, a target reward, a course-exit penalty, and a progress reward toward the target (the README says "L8 norm").
  - Truncation at 3,000 steps. The README says the polar is "not yet accurate".
  - Reported results (SB3 defaults): policies that "tack only once" reach about 390 total reward. PPO "performs better" and SAC "converges faster". The sheet is not an action. No step counts are given beyond a 1M-step code example — [GitHub](https://github.com/Gabo-Tor/gym-sailing); [PyPI](https://pypi.org/project/gym-sailing/)

- **DockYard blog (2024), "Navigating the Seas of Reinforcement Learning: A Year with the Sailboat Problem" [BL]**
  - 2-D course with wind and currents, solved with the in-house "Rein" library (DDPG, then SAC) and PPO.
  - The stated optimal strategy in fixed wind is to sail the best-VMG angle and tack once at halfway.
  - SAC gave the best Rein results, but only on the upwind case. PPO "solved both upwind and downwind".
  - The author says the main gains came from redesigning the environment and problem modelling, not from the algorithm.
  - No action space, reward or numbers are given — [DockYard](https://dockyard.com/blog/2024/04/23/reinforcement-learning-a-year-with-the-sailboat-problem)

**B. Works where upwind or tacking was hand-designed, abstracted, or not addressed**

- **da Silva Junior, dos Santos, de Negreiros, Silva & Gonçalves (2020), "High-Level Path Planning for an Autonomous Sailboat Robot Using Q-Learning", *Sensors* 20(6):1550 [PR]** (UFRN N-Boat project)
  - Tabular Q-learning on an N×N grid. The action set "changes according to wind directions to account for the dead zone": when the target is inside the dead zone, "some actions have to be removed from the action pool". The zigzag path therefore comes from **action masking**, not from discovery.
  - Hyperparameters per scenario: α 0.5–0.9, γ 0.3–0.7, exploration 0.25–0.35, horizon 10,000.
  - The output is a global path that a low-level N-Boat controller follows — [Europe PMC full text, PMC7146235](https://pmc.ncbi.nlm.nih.gov/articles/PMC7146235/)

- **Suda & Nikovski discrete variant** (above): tack/stay macro-actions plus a helming controller. It builds on **Philpott & Mason** (stochastic DP for upwind routing, with an extra penalty time per tack) and **Ferguson & Elinas** (an MDP solved by value iteration with only two actions, "change tacks" and "stay", where a tack takes a single step plus a time penalty) — both summarised in [MERL PDF](https://merl.com/publications/docs/TR2022-102.pdf)

- **Spiking-neural-network sailboat controller (2023), *Control Theory and Technology* [PR]:** per a search summary, it trains upwind and downwind stages separately and treats tacking and gybing as discrete manoeuvres executed when the heading falls in the no-go zone. I did not read the full text (Springer redirect) — [Springer](https://link.springer.com/article/10.1007/s11768-023-00150-1)

- **Vasconcellos et al. (2023), "Reinforcement-learning robotic sailboats: simulator and preliminary results", NeurIPS 2023 Workshop on Robot Learning [WS], arXiv:2402.03337 [PP]**
  - Gazebo/Omniverse digital twin of the E-Boat robotic sailboat.
  - PPO (SB3 + Gymnasium) was trained to follow a six-waypoint mission (BCDACA). Twenty runs show consistent paths.
  - The paper gives no action space, reward, rate, or upwind or tacking result — [arXiv](https://arxiv.org/abs/2402.03337); [PDF](https://arxiv.org/pdf/2402.03337)

- **Cheng et al. (2019), ROBIO [PR]:** DQN-based "concise deep RL obstacle avoidance" with a coarse-to-fine strategy for autonomous sailboats. Details beyond the abstract were not available to me — [ResearchGate](https://www.researchgate.net/publication/338727314_Obstacle_Avoidance_for_Autonomous_Sailboats_via_Reinforcement_Learning_with_Coarse-to-fine_Strategy)

- **Chen, Liu et al. (2024), "Sail-rudder Collaborative Control of Unmanned Sailboat Based on Reinforcement Learning", OCEANS 2024 [PR]:** joint sail and rudder control with DDPG. Abstract only; nothing stated about tacking or demonstrations — [Semantic Scholar](https://www.semanticscholar.org/paper/Sail-rudder-Collaborative-Control-of-Unmanned-Based-Chen-Liu/974f2bd9126279e5ea29d569c8eb071cb351c432)

- **Yadollahi & Torabi Jahromi (2026), Iranian J. Sci. Tech., Trans. Electrical Eng. [PR]:** model-free DDPG heading control for sailing vehicles with "modified reward-shaping". Heading control only. Abstract only — [Springer](https://link.springer.com/article/10.1007/s40998-026-01170-8)

- **Deng et al. (Measurement Science and Technology, about 2025–26) [PR]:** "Difficulty-Adaptive Phasic Policy Gradient" (DA-PPG) for unmanned sailboat navigation and collision avoidance, with a fluid-structure-interaction model, **local A*-guided reward shaping** and a **difficulty-adaptive curriculum**. The abstract claims it beats geometric planners and baseline DRL. I could not confirm the year or whether upwind tacking was required — [IOPscience](https://iopscience.iop.org/article/10.1088/1361-6501/ae74f0)

- **"LOS Guidance-Based Path Following Control for Unmanned Sailboat via Neural Reinforcement Learning" (about 2025–26) [PR?]:** line-of-sight guidance (a hand-designed heading reference) plus neural RL for tracking. Details not accessible — [ResearchGate](https://www.researchgate.net/publication/399516033_LOS_Guidance-Based_Path_Following_Control_for_Unmanned_Sailboat_via_Neural_Reinforcement_Learning)

- **I-BOAT (Ital-IA 2025, CEUR-WS Vol. 4121) [WS]:** compares DRL with artificial potential fields for a water-monitoring sailboat — [CEUR](https://ceur-ws.org/Vol-4121/Ital-IA_2025_paper_85.pdf)

- **Mannocci (2023), "Sailing Boat Route Finder with Deep Reinforcement Learning", MSc thesis, Univ. Bologna [TH]**
  - Ocean-scale routing (Vendée Globe 2020/21) on real 10 km wind data.
  - A CNN over wind "images" plus 7 scalars outputs Q-values for 3 actions (turn clockwise, turn anticlockwise, don't turn).
  - Training was "not completely successful": the agent did well in the steady Southern Ocean winds but "got stuck in Pacific" near coasts and in variable wind — [thesis PDF](https://amslaurea.unibo.it/id/eprint/29903/1/Sailing_Boat_Route_Finder_with_Deep_Reinforcement_Learning.pdf)

- **Emirates Team New Zealand + McKinsey/QuantumBlack AI sailing bot (2019–21, 36th America's Cup) [BL, vendor-reported]**
  - Deep RL bot trained with RLlib in ETNZ's AC75 simulator to stand in for human sailors when testing foil designs.
  - It adjusts "14 different sail and boat controls" and "knows nothing" at the start, learning by "trial and error".
  - After about 8 weeks it was "on par with if not better than" Olympic sailors. Reports cite a thousand bots in parallel, a claimed 10× design speed-up, and sailors "learning maneuvers from the bot".
  - Algorithm, reward, curriculum and whether human data or scripted manoeuvres were used are **not stated publicly** — [Anyscale blog](https://www.anyscale.com/blog/sailing-to-victory-with-reinforcement-learning); [McKinsey case](https://www.mckinsey.com/business-functions/mckinsey-digital/how-we-help-clients/flying-across-the-sea-propelled-by-ai); [Sailing World](https://www.sailingworld.com/racing/how-a-bot-made-team-new-zealand-faster-and-smarter/)

- **Review:** Mankina et al. (2025), "AI-Based Autonomous Sailboat Navigation: A Review", *Journal of Field Robotics* [PR]. It surveys the above, including DDPG sail-rudder control. The full text was blocked (HTTP 403), so I could not extract its RL inventory — [Wiley](https://onlinelibrary.wiley.com/doi/full/10.1002/rob.70004)

**C. Comparison with SailGym Task U** (facts from the sources above; SailGym numbers from the assignment brief)

| | SailGym U | Bink 2024 (SAC) | Suda & Nikovski 2022 (PPO/SAC) | gym-sailing |
|---|---|---|---|---|
| Distance upwind | 40 m | ~20 m (x −10 → +10) | 5 nmi | not stated |
| Decision interval | 0.1 s | 0.5 s | 60 s | not stated |
| Max decisions per episode | 1,800 | 720 | K (~85–93 needed) | 3,000 |
| γ (horizon ≈ 1/(1−γ)) | 0.99975 (~4,000 steps) | 0.998 (~500 steps) | not stated | SB3 default |
| Actions | rudder rate, sheet rate | rudder, sheet, ballast moves | heading change per step / tack choice | rudder only |
| Dense reward | potential-based distance shaping | per-step distance reward and penalty, actuator-rate and gybe penalties | −1 per step only | progress + alive penalty |
| Terminal rewards | ±10 | +2000 / −3000 / timeout −1000 | timeout −K | target reward |
| Start diversity | not stated | randomised x, y, yaw, speed, wind | fixed start, random wind | not stated |
| Demonstrations | 50 (SAC+demo arm) | none described | none described | none described |
| Tacking emerged? | rarely | yes (SAC_test_2 shown beating; SAC_test_4 sculled instead; other seeds not described) | yes (continuous); macro (discrete) | "tack only once" policies |

Sources: [Bink thesis](https://repository.tudelft.nl/record/uuid:1284613a-d9e7-4076-b120-8349acdca4ca); [MERL PDF](https://merl.com/publications/docs/TR2022-102.pdf); [gym-sailing](https://github.com/Gabo-Tor/gym-sailing).

### Inferences
- No source I found reports learning end-to-end tacking at a 10 Hz decision rate over 1,800-step episodes with ±10 terminal rewards and potential-based shaping. The successful cases use 5× (Bink) to 600× (MERL) coarser decisions and an effective horizon at least 8× shorter than SailGym's (γ = 0.99975 → about 4,000 steps).
- Bink's reward design differs in two ways that plausibly matter for SailGym's "run out the clock" failure:
  - **Timeout was explicitly penalised** (−1000), about a third of the failure penalty and half the success bonus. Loitering was never a safe local optimum.
  - **The dense per-step term depended on the current upwind gap**, so every step spent far downwind cost more. This is unlike potential-based shaping, which telescopes to the same total for any loitering path.
- In MERL's model, speed comes instantly from the polar and the action is a heading change, so a tack loses only the minutes spent turning through the no-go zone. With no momentum loss or stall, the short-term cost that makes SailGym tacks hard is largely absent.
- The sculling exploit in Bink (rudder pumping at 2 Hz) suggests that a rate-actuated rudder at 10 Hz in SailGym could find similar propulsion exploits if the physics permits. Bink handled it with actuator-rate penalties plus an energy-ratio evaluation metric.
- Even Bink's identical runs diverged (1 of 4 seeds learned sculling instead of tacking; one of two identically configured sim-to-real agents, SAC_RI_4, got 50% vs 100% for SAC_RI_3). Three seeds is a small sample for judging whether tacking can be discovered.

### Gaps
- I could not read the EAAI 2026 journal version of Bink et al. (paywall). Its training setup may differ from the thesis, for example in reward, rates or the roll-tacking analysis.
- The exact form of Bink's "Distance Reward" is ambiguous in the extracted PDF text. The basin width (thesis Figure 5.5) is only in a figure.
- For the ETNZ/QuantumBlack bot, no public source gives the reward, curriculum or whether human sailing data or scripted manoeuvres seeded learning.
- I could not access the full texts of Chen 2024 (DDPG sail-rudder), Deng et al. (DA-PPG), the SNN controller, the LOS-guidance RL paper or the JFR review. Whether any of them learned tacking end-to-end is unknown.
- I found no IRSC (International Robotic Sailing Conference) paper applying RL to tacking. The search did not surface IRSC proceedings, so this may be a search gap, not an absence.

---

## Q2. Are there sailing RL benchmarks or environments, and what results do they report?

### Takeaway
There is no standard, widely adopted sailing RL benchmark with published baseline tables. The landscape is:
- small open-source Gym environments (gym-sailing, sailboat-gym, a Gym sailing gist, student projects);
- research simulators (MERL's two Gym environments, MARIN's XMF Optimist simulator, the E-Boat Gazebo twin);
- the classic discrete "sailing domain" (Vanderbei 1996), used as a planning benchmark for UCT/MCTS.

Of these, only gym-sailing and MERL report RL learning curves for an upwind target.

### Cited Findings
- **Classic "sailing domain" [PR, planning benchmark]:**
  - Kocsis & Szepesvári's UCT paper (ECML 2006) used Vanderbei's (1996) sailing problem. A sailboat moves on an 8-connected grid under fluctuating wind, and each move's duration depends on its direction relative to the wind.
  - Later MCTS papers reuse it, including BRUE simple-regret planning and Obstructed Sailing variants.
  - It is a known-model planning testbed, not a learning-from-scratch control task — [UCT paper DOI](https://dx.doi.org/10.1007/11871842_29); [Feldman & Domshlak, BRUE, arXiv 1206.3382](https://arxiv.org/pdf/1206.3382); [Tolpin & Shimony, arXiv 1108.3711](https://arxiv.org/pdf/1108.3711); [UCT-Aux, arXiv 1206.5940](https://arxiv.org/pdf/1206.5940)
- **gym-sailing [GH]:** see Q1. SB3 default PPO and SAC learn "tack only once" policies (about 390 return) on Sailboat-v0, with rudder-only control — [GitHub](https://github.com/Gabo-Tor/gym-sailing)
- **sailboat-gym (Lucas Marandat) [GH]:**
  - Gazebo-based (usv_sim_lsa, Free-Floating and Foil Dynamics plugins) in Docker.
  - Continuous rudder angle and sail angle actions; 3-D observations including wind.
  - The reward is user-defined (the example is negative distance to goal).
  - No RL results are reported. Snyk lists v1.2.0 as apparently unmaintained — [GitHub](https://github.com/lucasmrdt/sailboat-gym); [Snyk](https://snyk.io/advisor/python/sailboat-gym)
- **MERL environments [PR]:** two OpenAI Gym environments (discrete tack choice, continuous heading change) with an AR(1) wind process. They were evaluated on 1,000 fixed wind realisations; results are in Q1 — [MERL PDF](https://merl.com/publications/docs/TR2022-102.pdf)
- **MARIN XMF Optimist simulator with basin-in-the-loop [TH]:** 0.01 s physics step; the RL environment runs on the RL-Glue pattern with SB3. Not released as a public benchmark as far as the thesis states — [Bink thesis](https://repository.tudelft.nl/record/uuid:1284613a-d9e7-4076-b120-8349acdca4ca)
- **E-Boat digital twin (Gazebo/VRX-based, plus Omniverse) [WS/PP]:** PPO waypoint mission; preliminary results only — [arXiv 2402.03337](https://arxiv.org/abs/2402.03337)
- **Smaller projects [GH]:**
  - JonAsbury's "Sailing Simulator Environment for the OpenAI Gym" gist, which inspired gym-sailing — [gist](https://gist.github.com/JonAsbury/1a8102e070b1ad9888857e7cbcb48f93)
  - pfontana96/pytorch-smart-sailboat (Q-learning sailboat control) — [GitHub](https://github.com/pfontana96/pytorch-smart-sailboat)
  - trahier/RL_project_sailing — [GitHub](https://github.com/trahier/RL_project_sailing)
  - ISAE-Supaero IBoat-PMCTS: parallel MCTS weather routing; a search summary says it also lists a DQN for stall avoidance — [GitHub](https://github.com/PBarde/IBoat-PMCTS)
  - None of these report RL results for upwind tacking in what I could see.
- **Industry:** ETNZ/QuantumBlack used RLlib in a proprietary AC75 simulator (see Q1). No public benchmark — [Anyscale](https://www.anyscale.com/blog/sailing-to-victory-with-reinforcement-learning)

### Inferences
- With no shared sailing RL benchmark and no published per-seed baselines on a high-fidelity upwind task, SailGym's PPO/SAC negative result cannot be compared against prior numbers. The closest comparison is the Bink setup.

### Gaps
- I did not verify the exact specification of the original Vanderbei/UCT sailing domain, such as whether a tack-delay cost exists. Only secondary descriptions were read.
- I did not inspect the code of gym-sailing or the Gym sailing gist to determine the physics time step or how heading and speed dynamics are modelled.

---

## Q3. Closest analogues (accepting short-term loss under a directional constraint): what exploration and credit-assignment choices did they make?

### Takeaway
The successful analogues share four choices:
- coarse, physically meaningful decision intervals (seconds to minutes);
- small discrete action sets, often increments of a control angle;
- **dense, physically grounded per-step rewards** instead of sparse terminal ones (climb rate, traction power, softened distance-to-station);
- heavy randomisation of the environment (procedural wind, random starts).

Loon's hand-coded predecessor explicitly "tacked" through wind layers, and the RL controller beat it using a softened reward and a 3-minute decision interval. None used demonstrations, as far as the sources state.

### Cited Findings
- **Stratospheric balloons — Bellemare et al. (2020), "Autonomous navigation of stratospheric balloons using reinforcement learning", *Nature* 588:77–82 [PR]**
  - **Actions:** ascend, descend or stay. Controllers "receive inputs and emit commands at 3-min intervals". A trial is two simulated days (about 960 decisions).
  - **Algorithm:** QR-DQN (distributional), a 7×600 ReLU MLP, γ = 0.993, 5-step returns, 100 parallel simulated actors, replay buffers. "Using smaller networks or a non-distributional algorithm was found to degrade performance."
  - **Reward:** r = 1 within 50 km of the station. Outside it, r drops to c_cliff = 0.4 and halves every τ = 100 km. It is further decreased when consuming power. "Softening the transition at the boundary results in improved controllers", and an ablation showed robustness to the reward parameters.
  - **Training data:** ERA5 reanalysis winds plus procedural noise generate unlimited wind fields that emulate forecast error (akin to data augmentation).
  - **Baseline:** Loon's prior controller, StationSeeker, "tracks winds with headings that form an acute angle with the direction to the station, effectively tacking towards its destination."
  - **Results:** RL reached 55.1% TWR50 vs StationSeeker's 40.5% (about 3.5 h per day more time on station), plus a 39-day Pacific flight experiment. Under partial observability, an optimal agent "must still sometimes gamble" — [Nature](https://www.nature.com/articles/s41586-020-2939-8); [full-text PDF](https://gwern.net/doc/reinforcement-learning/model-free/2020-bellemare.pdf)

- **Thermal soaring (simulation) — Reddy, Celani, Sejnowski & Vergassola (2016), "Learning to soar in turbulent environments", *PNAS* 113(33):E4877 [PR]**
  - Tabular SARSA in simulated turbulent convection.
  - Actions: increase, decrease or keep the angle of attack (2.5° steps) and the bank angle (5° steps).
  - State: discretised vertical wind acceleration and roll-wise torque.
  - The reward is **local in time**: "a local-in-time reward that linearly combines the vertical wind velocity and the wind acceleration achieved at the subsequent time step, R = u_z + C·a_z". This was chosen heuristically because it gave the best soaring, and performance was insensitive to C.
  - The paper discusses the role of the discount factor in trading exploitation against exploration — [PNAS DOI](https://doi.org/10.1073/pnas.1606075113); [Salk PDF](https://papers.cnl.salk.edu/PDFs/Learning%20to%20soar%20in%20turbulent%20environments%202016-4488.pdf)

- **Thermal soaring (field) — Reddy, Wong-Ng, Celani, Sejnowski & Vergassola (2018), "Glider soaring via reinforcement learning in the field", *Nature* 562:236–239 [PR]**
  - A 2 m glider whose flight controller modulates bank angle and pitch.
  - The navigational strategy was "determined solely from the glider's pooled experiences, collected over several days in the field", using on-board estimates of vertical wind acceleration and roll torque as cues.
  - A press release reports climbs to about 700 m. No demonstrations — [Princeton record](https://collaborate.princeton.edu/en/publications/glider-soaring-via-reinforcement-learning-in-the-field/); [Salk press release](https://www.salk.edu/news-release/physicists-train-robotic-gliders-to-soar-like-birds/)

- **Cross-country soaring — Notter et al. (2023), "Hierarchical Reinforcement Learning Approach for Autonomous Cross-Country Soaring", *J. Guidance, Control, and Dynamics* 46(1):114–126 [PR]** (AIAA SciTech 2021 version)
  - The competition task (GPS triangle racing) is decomposed into subtasks and solved hierarchically by stochastic gradient ascent, "without extra heuristics from the designer". The hierarchical policy trained in simulation transferred to real flight tests.
  - The hierarchy is the engineered prior that separates "leave the thermal and accept altitude loss" from "exploit the thermal".
  - Code: ifrunistuttgart/RL_CrossCountrySoaring — [Uni Stuttgart record](https://puma.ub.uni-stuttgart.de/bibtex/23d6f0e0ba7ca4c68c202d998cad36c63/stefannotter); [GitHub](https://www.github.com/ifrunistuttgart/RL_CrossCountrySoaring); [IFR page](https://www.ifr.uni-stuttgart.de/en/Forschung/Intelligent-Flight/Autonomous-Soaring/)

- **Kites / airborne wind energy — Orzan, Leone, Mazzolini, Oyero & Celani (2023), "Optimizing airborne wind energy with reinforcement learning", *Eur. Phys. J. E* (arXiv:2203.14271, 2022) [PR]**
  - Tabular SARSA with ε-greedy exploration, visit-count-dependent learning rate and exploration, and controls on the attack and bank angles.
  - The reward is traction power along the wind (dense). Crashes get a penalty that can depend on elapsed time ("early falls are penalized more than late ones"), and the reward is reduced near the ground.
  - Episodes end on a crash or after a long time, from random initial positions.
  - The learned policy tows a vehicle long distances with helical kite motion and reduces to "a simple list of manoeuvring instructions". The supplement explores rewards not aligned with the main wind component. This is downwind or crosswind towing, not upwind — [arXiv](https://arxiv.org/abs/2203.14271); [arXiv PDF](https://arxiv.org/pdf/2203.14271); [RUG record](https://research.rug.nl/en/publications/optimizing-airborne-wind-energy-with-reinforcement-learning/)

- **Zermelo navigation in flows — Biferale, Bonaccorso, Buzzicotti, Clark Di Leoni & Gustavsson (2019), arXiv:1907.08591 [PP; journal version exists per the Tor Vergata record, venue not verified here]**
  - A fixed-speed vessel with steerable heading in 2-D turbulence.
  - Actor-critic RL finds quasi-optimal routes for frozen and time-evolving flows. It is more robust than optimal-navigation solutions, which are "unstable for the typical duration of the navigation process" — [arXiv](https://arxiv.org/pdf/1907.08591); [Tor Vergata](https://art.torvergata.it/handle/2108/230223)

- **Vortical-flow navigation — Gunnarson, Mandralis, Novati, Koumoutsakos & Dabiri (2021), "Learning efficient navigation in vortical flow fields", *Nature Communications* [PR]**
  - A fixed-speed swimmer in unsteady flow past a cylinder; deep RL with "Remember and Forget Experience Replay".
  - With local velocity sensing it reached near 100% success at near the time-efficiency of a global optimal-control planner, about 2× the success of vorticity sensing — [arXiv](https://arxiv.org/abs/2102.10536v1); [DOAJ](https://doaj.org/article/238d3bca93c848cb91cb01cd92dc5160); [Caltech news](https://lindecenter.caltech.edu/news/scientists-teach-ai-to-navigate-ocean-with-minimal-energy)

- **Mountain-car-style exploration — Eberhard, Hollenstein, Pinneri & Martius (2023), "Pink Noise Is All You Need: Colored Noise Exploration in Deep RL", ICLR 2023 (spotlight/oral) [PR]**
  - With MPO and SAC, temporally correlated pink action noise "significantly outperforms white noise, OU noise, and other alternatives on a wide range of environments"; recommended as the default — [ICLR](https://iclr.cc/virtual/2023/oral/12540); [MPI-IS](https://is.mpg.de/en/publications/eberhardetal2023-pinknoise)
  - A TU Berlin robot-learning exercise built on it reports that default SB3 SAC fails Continuous MountainCar and "only the agent that uses pink noise for exploration learns to solve the task". This is a course result (scale 0.3, 2×10⁴ steps), not from the paper — [TU Berlin exercise](https://user.tu-berlin.de/mtoussai/teaching/RobotLearning/e07-invRL.pdf)

### Inferences
- **Decision interval.** Every analogue that succeeded made decisions at a time scale comparable to the manoeuvre it needed to discover: 3 min for balloons; one angle increment per step for gliders and kites; 0.5 s for Bink's Optimist; 1 min for MERL. SailGym's 0.1 s steps mean a tack takes many tens of correlated decisions. That is exactly the regime where white Gaussian (SAC) or independent (PPO) action noise averages out, and where temporally correlated noise or action repetition is reported to help.
- **Reward.** The analogues mostly used dense, immediate, physically meaningful rewards (climb rate, traction power, softened distance). Loon showed that softening a sparse "inside radius" reward measurably improved learning.
- **Structural priors.** The deep-RL analogues used substantial scale (Loon: 100 actors, distributional RL, procedural data) or a hierarchical decomposition (Notter), not demonstrations.

### Gaps
- I did not retrieve exact decision intervals or discount factors for Reddy 2018, Notter 2023 or Orzan 2023.
- The Balloon Learning Environment (the open-sourced Loon benchmark) and Novati et al.'s gliding/perching DRL were not checked in this pass.
- I did not find the Pink-Noise paper's own MountainCar numbers; the MountainCar claim rests on the course exercise.

---

## Q4. Reported negative results (RL failing upwind) and explanations

### Takeaway
Explicit negative results are scattered but consistent:
- off-policy SAC failing to reach an upwind mark at all early in training (MERL);
- agents finding non-tacking exploits such as sculling, or failing to tack after a sim-to-real shift (Bink);
- a thesis author warning that −1-per-step sparse rewards are not feasible for continuous low-level action spaces;
- DQN routing failing in variable wind (Mannocci).

No paper I found reports a SailGym-like "never finishes, runs out the clock" failure with an analysis.

### Cited Findings
- **MERL:** with continuous actions and −1-per-step reward, "SAC fails to even round the mark once for the first 2,000 episodes", whereas PPO improved almost immediately. Even in this 1-minute-step kinematic model, an off-policy maximum-entropy agent struggled with early exploration — [MERL PDF](https://merl.com/publications/docs/TR2022-102.pdf)
- **Bink thesis on reward sparsity:** the MERL −1-per-step reward "is quite sparse, depending on exploration to reach the goal... That is feasible in the case of Suda et al. since the action and observation space are different... In our case, the action space is continuous, making it much harder for the agent to reach the target by chance. Therefore... intermediate rewards will most likely be necessary." The thesis also warns that intermediate rewards "should not be too large", because that "encourages agents to keep the episode alive to keep accumulating this intermediate reward" — [Bink thesis §3.2.3](https://repository.tudelft.nl/record/uuid:1284613a-d9e7-4076-b120-8349acdca4ca)
- **Bink seed failure and exploit:** SAC_test_4 learned sculling (rudder used "as a fin") and used low-wind basin edges instead of tacking — [Bink thesis §5.4](https://repository.tudelft.nl/record/uuid:1284613a-d9e7-4076-b120-8349acdca4ca)
- **Bink sim-to-real failures:**
  - The noise-trained agents SAC_ON_01 and SAC_ON_04 "did not demonstrate the ability to tack" in the basin. SAC_ON_01 went out of bounds quickly in all 6 trials.
  - SAC_RI_4 "was trying to go upwind by beating" but drifted after tacks and ran out of time.
  - The rule-based SOTON's single failure was speed loss after tacks.
  - These identify post-tack speed recovery as the fragile step — [Bink thesis §6.3](https://repository.tudelft.nl/record/uuid:1284613a-d9e7-4076-b120-8349acdca4ca)
- **DockYard:** early DDPG versions underperformed. The author attributes most later gains to environment and problem modelling, not the algorithm, and notes catastrophic forgetting as a risk when adapting to new conditions — [DockYard](https://dockyard.com/blog/2024/04/23/reinforcement-learning-a-year-with-the-sailboat-problem)
- **Mannocci (Bologna):** DQN routing reward fell late in training as exploration decayed. The agent "got stuck in Pacific" with spatially variable wind near coasts — [thesis PDF](https://amslaurea.unibo.it/id/eprint/29903/1/Sailing_Boat_Route_Finder_with_Deep_Reinforcement_Learning.pdf)
- **Classical context:** Ferguson & Elinas and Philpott & Mason avoided learning the tack manoeuvre altogether, using two actions (tack/stay) and a fixed tack time penalty. MERL notes this design choice is probably suboptimal — [MERL PDF](https://merl.com/publications/docs/TR2022-102.pdf)

### Inferences
- The failures fall into three patterns:
  - **Exploration with continuous low-level actions** (MERL's SAC; Bink's argument for dense rewards).
  - **Reward exploits that beat tacking** (sculling, using low-wind edges).
  - **Fragility of the post-tack speed rebuild** (drift and stall after a tack).
- SailGym's "runs out the clock, rarely tacks" pattern matches the first two. Prior work did not report it under potential-based shaping with a small terminal ratio (±10 vs a total time cost of 6 over 180 s), so there is no direct precedent.
- SailGym's SAC+demo arm "tacks but rarely finishes and forgets". This echoes DockYard's catastrophic-forgetting remark and Bink's post-tack drift failures. The specific demo-forgetting dynamic is not analysed in any sailing source found.

### Gaps
- I found no published ablation that isolates decision rate, γ, or reward type (potential-based vs level-based) for upwind sailing RL. These causal factors remain untested in the literature.
- I found no study of RL plus demonstrations (DDPGfD, RLPD, AWAC, etc.) on sailing. Whether demonstration forgetting has been seen in sailing is unknown.
