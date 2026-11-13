# Rebuild the slides from the saved runs (see README.md).
REPO := $(abspath ../..)
PY := cd $(REPO) && env -u PYTHONPATH MPLCONFIGDIR=/tmp/ese3990-mpl uv run --frozen --no-sync python

build/main.pdf: main.tex numbers.tex figs/r_learning.png figs/u_tracks.png
	latexmk -pdf -outdir=build -interaction=nonstopmode -halt-on-error main.tex

# Figures and numbers.tex; rerun after the M4U held-out evaluation finishes.
numbers.tex figs/r_learning.png figs/u_tracks.png &: make_figures.py
	$(PY) docs/intermediate-presentation/make_figures.py

.PHONY: data clean
data:
	$(PY) docs/intermediate-presentation/make_figures.py
	$(MAKE) build/main.pdf

clean:
	rm -rf build
