# Token-based weekly API-equivalent estimator

NerfTrack estimates what observed local Codex activity would cost through the OpenAI API. It does not read, infer, convert, or display Codex credits, and it is not a ChatGPT bill.

For each JSONL token event, NerfTrack prices uncached input, cached input, and output at USD-per-million-token rates. When the log exposes reasoning tokens, they are treated as an output detail—not an additional billed token count—and are used only if a total output count is absent. A positive cost delta is paired only with a positive weekly `used_percent` delta in the same account/limit/reset window:

```text
cost_delta_usd = current_token_cost_usd - previous_token_cost_usd
percent_delta = current_weekly_used_percent - previous_weekly_used_percent
estimated_weekly_api_equivalent_usd = cost_delta_usd / (percent_delta / 100)
```

Each history point shows the unsmoothed cumulative cost-per-usage estimate for that observation. The headline remains the median of the latest seven valid cumulative estimates so short-lived noise does not redefine the current projection. Raw interval cost, percentage deltas, cumulative estimates, confidence, and coverage stay local for audit. Zero or negative cost movement, no percentage movement, unknown pricing, or a reset boundary is pending/rejected rather than converted into an estimate.

## Pricing and overrides

Built-in rates were verified on 2026-08-08 from OpenAI API model pages:

- [GPT-5.6 Luna](https://developers.openai.com/api/docs/models/gpt-5.6-luna): $0.20 input, $0.02 cached input, $1.20 output per 1M text tokens.
- [GPT-5.6 Terra](https://developers.openai.com/api/docs/models/gpt-5.6-terra): $2 input, $0.20 cached input, $12 output per 1M text tokens.

- [GPT-5.3-Codex](https://developers.openai.com/api/docs/models/gpt-5.3-codex): $1.75 input, $0.175 cached input, $14 output per 1M tokens.
- [GPT-5.2-Codex](https://developers.openai.com/api/docs/models/gpt-5.2-codex): $1.75 input, $0.175 cached input, $14 output per 1M tokens.
- [codex-mini-latest](https://developers.openai.com/api/docs/models/codex-mini-latest): $1.50 input, $0.375 cached input, $6 output per 1M tokens.

The built-in text catalog also covers the currently documented GPT-5.6/5.5/5.4/5.x, GPT-4.1, GPT-4o, o1, o3, o3-mini, and o4-mini text model IDs using the [official model catalog](https://developers.openai.com/api/docs/models/all), [model comparison](https://developers.openai.com/api/docs/models/compare), and [API pricing](https://openai.com/api/pricing/) rates verified on that date. Token logs do not identify audio/image modality tokens, cache writes, or tool-call units, so those non-text charges are intentionally unavailable rather than fabricated.

User-provided model overrides are local-only and take precedence over verified built-ins. Each needs a nonempty model ID and finite non-negative input, cached-input, and output rates; an optional alias maps a local model label to the override. A model without a verified rate and override is conspicuously pending with a diagnostic: NerfTrack never guesses a rate or sends model/token data to obtain one.

## Windows, reset safety, and migration

Only 10,080-minute weekly limits are used. Windows are separated by account and limit ID. Reported reset timestamps within five minutes are treated as one reset identity so normal server jitter cannot fragment a weekly allowance. A larger reset-time change or an observed scheduled boundary starts a new window. A usage regression before the reported reset is retained in raw quota history but excluded from estimation as stale/out-of-order data. Event costs are attributed by the accepted epoch's time bounds rather than exact reset timestamp equality.

Range changes are calculated only when both endpoints have medium or high confidence, the baseline lies inside the selected range, and it precedes the current estimate. Otherwise the comparison is unavailable rather than inferred from stale or low-coverage history.

Schema migration 8 preserves raw usage events, quota observations, accounts, settings, user annotations, and checkpoints, but invalidates and rebuilds incompatible derived estimates, measurements, epochs, and charts. The persisted estimator algorithm identifier remains stable across this branding change. All processing and override storage is local; prompts, raw JSONL, credentials, account identifiers, and complete paths are not returned.
