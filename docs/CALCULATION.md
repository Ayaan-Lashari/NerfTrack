# Calculation and algorithm defaults

The quote is an estimate of the observed eligible Codex cost scaled to a complete allowance:

```text
weekly API equivalent = settled eligible cost delta / settled quota percentage delta × 100
```

The live graph evaluates the formula between consecutive official Codex observations for the same normalized weekly reset. It requires finite embedded family pricing, monotonic quota movement, settled sources, and the configured minimum movement, cost, event count, and low-usage thresholds. Quote points are reduced to one observation per 30-minute bucket. The first 100% observation may close an interval; later cost at the same saturated reset cannot create or inflate another quote.

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

Display and graph values require at least three quotes for the same dominant model and normalized weekly reset. They use the median of the latest five comparable quotes, so an isolated low-movement interval cannot become the headline or a chart baseline.

Range comparisons use the current dominant model only. A baseline must be at or before the requested cutoff and no more than half a range older than that cutoff. If the requested period lacks a comparable baseline, Nerfify returns no change instead of relabeling a shorter interval as the full range. The graph is rebuilt from local observations on every configured refresh; it does not contain fixed dollar values or machine-specific paths.

Cache/fast/long-context share comparability and multi-epoch trend classification remain estimator capabilities for a future schema that persists those shares; they are not claimed by the current quote store.
