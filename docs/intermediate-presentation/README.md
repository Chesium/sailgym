# ESE3990 intermediate presentation

3-minute video: beamer slides (`main.tex`) plus a live web demo. The narration,
demo cues and recording commands are in [`script.md`](script.md).

```bash
make          # build/main.pdf (rebuilds figures/numbers first if make_figures.py changed)
make data     # re-read the saved runs (e.g. after the M4U held-out run), then rebuild
make clean    # remove build/
```

Needs TeX Live with beamer (`texlive-latex-recommended`, `texlive-pictures`,
`latexmk`) and the repo's uv environment. `make_figures.py` writes
`figs/*.png` and `numbers.tex`; every number on the slides comes from there.
