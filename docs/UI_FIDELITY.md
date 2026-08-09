# UI fidelity review

NerfTrack’s supplied dashboard screenshot is the visual reference for the first
implementation pass. The reference is stored at
`design-reference/dashboard-reference.png`; the first-run setup image is kept
separately at `design-reference/first-run-concept.png` and is a concept, not an
approval artifact.

The implementation carries forward the following reference decisions:

- near-black desktop canvas with a quiet graphite navigation rail;
- green accent for active navigation, connected/valid state, chart stroke, and
  primary action;
- large estimated full-week value with compact range controls above the chart;
- responsive SVG area chart with reset annotation, keyboard scrubbing, crosshair
  state, and reduced-motion support;
- metric cards for weekly usage, observed credits, reset timing, estimated
  credits, and confidence;
- setup cards for Codex data folder, executable, provider/model context, and
  refresh monitoring;
- iconography implemented as local SVG components rather than emoji or a chart
  dependency.

The concept’s amber setup state is represented as a semantic `Needs setup`
state in the product. Detected/connected states remain green, so the setup
screen communicates the same distinction without treating the concept as an
approved visual specification.

The responsive pass was reviewed at the browser QA viewport used for this
build. At narrower desktop widths the metric cards intentionally collapse to
two columns; at the supplied wide reference aspect ratio they retain four
columns. The final browser screenshot is a local QA artifact and is not part
of the repository.
