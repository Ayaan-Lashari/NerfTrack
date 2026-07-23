# Calculation and algorithm defaults

The quote is an estimate of the observed eligible Codex cost scaled to a complete allowance:

```text
weekly API equivalent = settled eligible cost delta / settled quota percentage delta × 100
```

The live graph evaluates the formula from official Codex token records within the current quota window. It requires finite embedded family pricing and a positive weekly quota observation; missing provider evidence, pricing, or quota data remains pending rather than becoming a fabricated zero. Quote points are reduced to one observation per 30-minute bucket.

Current Codex model variants use the embedded price for their compatible GPT-5 family. This keeps desktop monitoring local and functional when a new Codex variant appears before a model-specific catalog entry; values remain estimates rather than billing statements.

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
