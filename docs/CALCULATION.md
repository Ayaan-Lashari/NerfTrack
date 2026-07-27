# Token-based weekly API-equivalent estimator

Nerfify estimates what observed local Codex activity would cost through the OpenAI API. It does not read, infer, convert, or display Codex credits, and it is not a ChatGPT bill.

For each JSONL token event, Nerfify prices uncached input, cached input, and output at USD-per-million-token rates. When the log exposes reasoning tokens, they are treated as an output detail—not an additional billed token count—and are used only if a total output count is absent. A positive cost delta is paired only with a positive weekly `used_percent` delta in the same account/limit/reset window:

```text
cost_delta_usd = current_token_cost_usd - previous_token_cost_usd
percent_delta = current_weekly_used_percent - previous_weekly_used_percent
estimated_weekly_api_equivalent_usd = cost_delta_usd / (percent_delta / 100)
```

The visible value is the bounded median of the latest seven valid interval estimates. Raw interval cost, percentage deltas, and estimates stay local for audit. Zero or negative cost movement, no percentage movement, unknown pricing, or a reset boundary is pending/rejected rather than converted into an estimate.

## Pricing and overrides

Built-in rates were verified on 2026-07-24 from OpenAI API model pages:

- [GPT-5.3-Codex](https://developers.openai.com/api/docs/models/gpt-5.3-codex): $1.75 input, $0.175 cached input, $14 output per 1M tokens.
- [GPT-5.2-Codex](https://developers.openai.com/api/docs/models/gpt-5.2-codex): $1.75 input, $0.175 cached input, $14 output per 1M tokens.
- [codex-mini-latest](https://developers.openai.com/api/docs/models/codex-mini-latest): $1.50 input, $0.375 cached input, $6 output per 1M tokens.

The built-in text catalog also covers the currently documented GPT-5.6/5.5/5.4/5.x, GPT-4.1, GPT-4o, o1, o3, o3-mini, and o4-mini text model IDs using the [official model catalog](https://developers.openai.com/api/docs/models/all) and [model comparison](https://developers.openai.com/api/docs/models/compare) rates verified on that date. Token logs do not identify audio/image modality tokens, cache writes, or tool-call units, so those non-text charges are intentionally unavailable rather than fabricated.

User-provided model overrides are local-only and take precedence over verified built-ins. Each needs a nonempty model ID and finite non-negative input, cached-input, and output rates; an optional alias maps a local model label to the override. A model without a verified rate and override is conspicuously pending with a diagnostic: Nerfify never guesses a rate or sends model/token data to obtain one.

## Windows, reset safety, and migration

Only 10,080-minute weekly limits are used. Windows are separated by account, limit ID, and reset identity; a changed reset timestamp, material usage decline, or scheduled boundary starts a new window. The first point is a baseline, not a reset annotation. Out-of-order events are rebuilt in timestamp order and cannot form cross-window intervals.

Schema migration 7 preserves raw usage events, quota observations, accounts, and checkpoints, but invalidates incompatible derived estimates, measurements, and charts. Algorithm version: `nerfify-token-api-equivalent-v2`. All processing and override storage is local; prompts, raw JSONL, credentials, account identifiers, and complete paths are not returned.
