# ESE3990 intermediate presentation — 3-minute video script

**Due:** Thu 13 Nov 2026, 23:59 ET (Fri 14 Nov 12:59, UTC+8).
**Requirement** (`docs/ese3990/intermediate_presentation_requirements.txt`):
minute 1 problem and motivation, minute 2 method, minute 3 preliminary results.

Slides: `build/main.pdf` (11 frames, 16:9). Every number on them comes from
`numbers.tex`, which `make_figures.py` generates from the saved runs.

## Timeline at a glance

| Time | Screen | Frame | Words (~140/min) |
|---|---|---|---|
| 0:00–0:08 | Slides | 1 Title | ~18 |
| 0:08–0:55 | **Browser: Triangle course, live** | — | ~105 |
| 0:55–1:05 | Slides | 2 Problem | ~25 |
| 1:05–1:30 | Slides | 3 Method: contract | ~45 |
| 1:30–1:54 | Slides | 4 Method: controllers | ~50 |
| 1:54–2:00 | Slides | 5–7 Pseudo-code, one page per controller (flip ~2 s each) | ~10 |
| 2:00–2:20 | Slides | 8 Results R | ~45 |
| 2:20–2:43 | Slides | 9 Results U | ~50 |
| 2:43–3:00 | Slides | 10 Takeaways and next | ~40 |
| last 2–3 s | Slides | 11 References (flash, silent) | — |

## Narration

Lines in *italics* are actions, not speech. Trim words before trimming the demo.

### 0:00 — Frame 1, title
> Hi, I'm Shimin. My project asks whether a sailboat should *learn* to sail or *plan* every move, and what each choice costs.

*Switch to the browser (Alt+Tab), course already loaded and paused.*

### 0:08 — Live demo: Triangle course
*Press P to start. Clock at 2×. Steer with A/D; trim with the on-screen mainsheet pad.*

> This is SailGym, my sailing simulator. Its physics engine is built with Rust for high speed RL training and this is an interactive web frontend. I control only two things: how fast the rudder turns and how fast the sail comes in. The physics do the rest.

*Point the bow straight at mark 1: the boat slows and stalls.*

> See this course: The first mark is straight upwind, and a boat can't sail into the wind: within about 45 degrees it stalls. That's called being in irons.

*Bear away to close-hauled, about 45° off the wind; build speed; then tack across.*

> So I have to zig-zag. Each turn through the wind is a tack, and a tack done badly leaves the boat stopped and drifting backwards.

*Round mark 1 and bear away onto the reach. Switch to 4× if behind time.*

> The next gate, across the wind, is easy: point and go. Gate one is my upwind RL task, U; and my RL reach task, R, is a Gate like this one.

*Switch to slides (frame 2).*

### 0:55 — Frame 2, problem
> So the question is: how do learning from experience and planning with a model, trade off success, experience and computation on these tasks?

### 1:05 — Frame 3, method: one contract
> Every controller sails the same Rust simulator, sends the same two rates and gets the same reward: plus ten for finishing, minus ten for failing, minus time and plus distance shaping.

> The learners see seventeen numbers of the true boat state. And everything is tested on fifty held-out conditions plus twenty shifted ones.

### 1:30 — Frame 4, method: three controllers
> OK so I implemented these three algorithms: PPO learns on-policy: it collects fresh experience, updates, and ... throws it away. SAC learns off-policy: it keeps everything in a replay buffer. (Then the Cross Entropy Method Model Predictive Control) CEM-MPC doesn't learn: at every decision it just forks the real simulator, tries thirty-two plans and executes the best first step, spending much in computation, but not experience.

### 1:54 — Frames 5–7, pseudo-code (flip)
> And these are the Pseudo-code for the three; please pause to read it.

### 2:00 — Frame 8, results R
> OK so on the reach task, the easy one, we found that SAC learns pretty good and finishes about three quarters of held-out tests. but PPO is not that good, about one seed in four passed in pilot tests, so you can say PPO is seed-sensitive but not hopeless. The true-model MPC planner and the hand-written RuleSailor finish every time.

### 2:20 — Frame 9, results U
> Then, The second, Upwind task is much harder. For MPC, a two-second horizon never tacks; After increasing it to eight seconds, it can finish every run, but at 0.2 seconds per decision, it's a lots of computation, and can't be done in real time. And No PPO or SAC learner finishes reliably: fifty demonstrations from the our handmade rule sailor does teach SAC to tack in four runs out of ten, but it reaches the gate only five times in 150 tests.

### 2:43 — Frame 10, takeaways and next
> So in conclusion, clearly MPC works, which is a known model with foresight. but at a compute cost, and SAC performs better then PPO. The next steps is trying some standard fix to stop SAC's values from blowing up in the harder task, and see whether a **learned** trajectory-end value let the two-second MPC tack in real time? Thanks!

*Advance to frame 11 (References) for 2–3 s, then stop recording. If the take runs over 3:00, take those seconds from the takeaways.*

## Demo preparation (web frontend)

```bash
pnpm --dir web dev            # then open http://localhost:5173
```

1. Choose the **Triangle** course challenge. Its marks are (0, 40) dead upwind,
   then (30, 10) and (0, −10). Fit the camera to the whole course.
2. Controls: `A`/`←` and `D`/`→` steer; the **mainsheet pad** (mouse) hauls in
   or eases out; `P` pauses; `R` resets; clock speeds 0.25/1/2/4×.
   The study's controllers never use the release (`Space`); avoid it on camera.
3. Rehearse leg 1 twice. RuleSailor needs about 55–65 s of simulated time for
   the beat, so at 2× it takes about 30 s on screen. Show one deliberate stall
   head-to-wind before the first tack — it makes the point in two seconds.
4. Leave it paused at t = 0 before recording.

## Recording (X11, screen 2560×1600)

One continuous take, switching windows with Alt+Tab. Slides full screen with
`evince --presentation docs/intermediate-presentation/build/main.pdf`.

```bash
# start / stop with q; check the microphone level first
ffmpeg -f x11grab -framerate 30 -video_size 2560x1600 -i :1 \
       -f pulse -i default \
       -vf scale=1920:1200 -c:v libx264 -preset veryfast -crf 20 \
       -c:a aac -b:a 160k take1.mp4

ffprobe -v error -show_entries format=duration -of csv=p=0 take1.mp4   # must be ≤ 180 s

# if needed: trim the head/tail (re-encode for an exact cut)
ffmpeg -ss 2.0 -to 182.0 -i take1.mp4 -c:v libx264 -crf 20 -c:a aac final.mp4
```

Two takes instead (demo, then slides):

```bash
printf "file 'demo.mp4'\nfile 'slides.mp4'\n" > parts.txt
ffmpeg -f concat -safe 0 -i parts.txt -c copy final.mp4
```

## Rebuilding the slides

```bash
cd docs/intermediate-presentation
env -u PYTHONPATH MPLCONFIGDIR=/tmp/ese3990-mpl uv run --frozen --no-sync python make_figures.py
latexmk -pdf -outdir=build main.tex
```

Rerun both after the M4U held-out evaluation finishes. Then frame 9's
"MPC 8 s (test)" row is final, and the "partial" note disappears. Before then
it shows how many MPC held-out episodes are done; the episodes it reports are
real, but the count is incomplete.

## Checklist before upload

- [x] After the M4U held-out run finishes (`runs/ese3990/M4U/heldout/complete.json` exists): `make data`.
- [x] The MPC 8 s shift episodes all finished (`\uMPCshift` in `numbers.tex` is n/n). If not, change "every run so far" (frame 9 narration) and "finished every test so far" (slide 10).
- [x] The U held-out replay audit passes before saying "every episode replays bit-exactly" (slide 3 only claims the 2,840 R episodes).
- [ ] Slide 10 "Next" still matches the plan's M4X arms (X1 critic LayerNorm, X2 value tail).
- [ ] Video ≤ 3:00; audio clear; slides readable at 1080p.
- [ ] Every spoken number matches the slide: 117/150, 0/150, 150/150; ~1 in 4 PPO pilot seeds; 0/60 vs 60/60; ~0.2 s; 4M; 50 demonstrations; tacks in ~4/10 runs; 5/150.
