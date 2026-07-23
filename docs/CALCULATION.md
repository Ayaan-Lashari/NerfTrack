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

Display and graph values are cumulative weighted estimates within each normalized weekly reset:

```text
weighted weekly equivalent = sum(settled eligible cost) / sum(settled quota percentage movement) × 100
```

All priced models contribute to the same weighted value, including newly named models supplied by an authenticated official pricing source. Unknown-price usage stays pending rather than being assigned a guessed price. No model names, dollar values, or machine paths are hardwired into the aggregation.

Repeated unchanged quota heartbeats are discarded, while small consecutive movements accumulate until they meet the configured threshold. This prevents polling from postponing settlement and avoids losing several real one-point movements. Nerfify still refreshes on the configured interval and only adds a new value after the configured settlement window.

A range baseline must be at or before the requested cutoff and no more than half a range older than that cutoff. If the requested period lacks a baseline, Nerfify returns no change instead of relabeling a shorter interval as the full range. The chart uses the full selected time axis and breaks its path at weekly reset boundaries so missing time is not drawn as continuous data.
