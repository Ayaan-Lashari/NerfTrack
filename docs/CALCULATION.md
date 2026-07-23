# Calculation and algorithm defaults

The quote is an estimate of the observed eligible Codex cost scaled to a complete allowance:

```text
weekly API equivalent = settled eligible cost delta / settled quota percentage delta × 100
```

The formula is evaluated only when the interval is complete, monotonic, source-stable, attributable, priced, and finite. Unknown provider evidence, missing pricing, stale quota, 100% pinning, zero/negative deltas, low usage, and incomplete boundaries produce pending or rejected diagnostics.

Defaults are algorithm inputs, not OpenAI policy facts. They are centralized in `estimator.rs` and the active algorithm version is stored with each quote:

- refresh: 10 seconds;
- reconciliation: 1 hour;
- monitoring gap: 5 minutes;
- settlement: 60 seconds, hard limit 120 seconds;
- decimal quota movement: at least 0.5 percentage points;
- whole-number quota movement: at least 3 points, with 5 points high confidence;
- eligible cost: at least $0.25;
- eligible events: at least 2;
- low-usage quarantine: at or below 3%.

Display value is the median of the latest five workload-comparable raw quotes. Comparability requires the same dominant model and cache/fast/long-context share deltas within 15/10/10 percentage points. Trend states need one comparable decline for `watching`, three consecutive declines for `possible_reduction`, five declines of at least 15% spanning two epochs for `likely_reduction`, and persistence across two reset boundaries for `sustained_trend`.
