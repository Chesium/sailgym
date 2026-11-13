# ESE3990 intermediate presentation — 3-minute video script

**Due:** Thu 13 Nov 2026, 23:59 ET (Fri 14 Nov 12:59, UTC+8).
**Requirement** (`docs/ese3990/intermediate_presentation_requirements.txt`):
minute 1 problem and motivation, minute 2 method, minute 3 preliminary results.

Slides: `build/main.pdf` (7 frames, 16:9). Every number on them comes from
`numbers.tex`, which `make_figures.py` generates from the saved runs.

## Timeline at a glance

| Time | Screen | Frame | Words (~140/min) |
|---|---|---|---|
| 0:00–0:08 | Slides | 1 Title | ~18 |
| 0:08–0:55 | **Browser: Triangle course, live** | — | ~105 |
| 0:55–1:05 | Slides | 2 Problem | ~25 |
| 1:05–1:35 | Slides | 3 Method: contract | ~70 |
| 1:35–2:00 | Slides | 4 Method: controllers | ~60 |
| 2:00–2:25 | Slides | 5 Results R | ~55 |
| 2:25–2:50 | Slides | 6 Results U | ~55 |
| 2:50–3:00 | Slides | 7 Takeaways | ~25 |

## Narration

Lines in *italics* are actions, not speech. Trim words before trimming the demo.

### 0:00 — Frame 1, title
> Hi, I'm Shimin. My project asks whether a sailboat should *learn* to sail or *plan* every move, and what each choice costs.

*Switch to the browser (Alt+Tab), course already loaded and paused.*

### 0:08 — Live demo: Triangle course
*Press P to start. Clock at 2×. Steer with A/D; trim with the on-screen mainsheet pad.*

> This is SailGym, a physics simulator I built. I only control two things: how fast the rudder turns and how fast the sail is pulled in. Everything else — heel, speed, capsize, whether I pass the gate — comes from the physics.

*Point the bow straight at mark 1: the boat slows and stalls.*

> The first mark is straight upwind, and a boat can't sail into the wind: within about 45 degrees it stalls. That's called being in irons.

*Bear away to close-hauled, about 45° off the wind; build speed; then tack across.*

> So I have to zig-zag. Each turn through the wind is a tack, and a tack done badly leaves the boat stopped and drifting backwards.

*Round mark 1 and bear away onto the reach. Switch to 4× if behind time.*

> The next leg, across the wind, is easy: point and go. These two legs are my two tasks: the upwind beat, U, and the reach, R.

*Switch to slides (frame 2).*

### 0:55 — Frame 2, problem
> So the question is: how do learning from experience and planning with a model trade off success, experience and computation on these tasks?

### 1:05 — Frame 3, method: one contract
> Every controller sails the same Rust physics, under the same rules for gates, bounds and time limits. They send the same two rates, get the same reward — minus time, plus ten for finishing, minus ten for failing, plus distance shaping — and are tested on fifty held-out wind and heading conditions plus twenty shifted ones. Every episode replays bit-exactly.

### 1:35 — Frame 4, method: three controllers
> I implemented three methods. PPO learns on-policy: it collects fresh experience, updates, and throws it away. SAC learns off-policy: it keeps everything in a replay buffer. CEM-MPC doesn't learn at all: at every decision it forks the real simulator, tries thirty-two action plans, and executes the best first step. It has the exact model, so it pays in computation, not experience.

### 2:00 — Frame 5, results R
> On the reach, SAC learns most of the task within half a million decisions and finishes 117 of 150 held-out tests; PPO never finds the gate. Planning with the true model, and the hand-written RuleSailor, finish every time.

### 2:25 — Frame 6, results U
> Upwind is much harder. With a two-second horizon, MPC never completes a tack; with eight seconds it always does, but each decision takes about 0.2 seconds, so it is not real-time. After four million decisions no learner finds the tack on its own, and even fifty expert demonstrations help SAC only a little.

### 2:50 — Frame 7, takeaways
> So: a known model with enough foresight wins, at a cost; reusing experience beats fresh batches; and upwind sailing is fundamentally an exploration problem. Thanks!

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

Rerun both after the M4U held-out evaluation finishes. Then frame 6's
"MPC 8 s (test)" row is final, and the "partial" note disappears. Before then
it shows how many MPC held-out episodes are done; the episodes it reports are
real, but the count is incomplete.

## Checklist before upload

- [ ] Numbers regenerated after the held-out run; `numbers.tex` has no "partial".
- [ ] Video ≤ 3:00; audio clear; slides readable at 1080p.
- [ ] Every spoken number matches the slide: 117/150, 0/150, 150/150; 0/60 vs 60/60; ~0.2 s; 4M; 50 demonstrations; 5/150.
