# Nerfify graph accuracy: permanent implementation plan for GPT-5.6 Sol

You are working in the current Nerfify repository. Implement a permanent fix for every confirmed graph-accuracy defect in the audit below. This is an implementation task, not another audit and not a request for a proposed plan. Inspect the existing code, make the changes, migrate or rebuild derived data safely, run the full test suite, and verify the finished app at runtime across every graph range.

Do not use the current graph or an existing derived history response as the source of truth. Raw quota observations, eligible token events, verified pricing, and canonical time boundaries are the source data.

## Required outcome

After this patch:

- The graph, headline change, percentage change, color, and tooltips all describe the same data series.
- Historical weekly quota observations are grouped into genuine weekly windows instead of fragmenting whenever a reported reset timestamp oscillates.
- Switching among `1D`, `1W`, `1M`, `3M`, and `6M` cannot show stale statistics.
- Horizontal position represents real elapsed time.
- Tooltips show recorded observations rather than unlabeled synthetic values.
- The UI discloses incomplete pricing coverage.
- Event boundary rules are consistent and tested.
- Existing incorrect derived epochs and measurements are rebuilt from raw data after upgrade.

Preserve the independently verified pricing and weekly-normalization math unless a failing test proves a change is needed.

## Accuracy contract

Use one explicit display signal throughout the graph experience. Prefer the existing smoothed estimate, `estimatedWeeklyValueUsd`, because the product describes short-term spikes as filtered and the current headline is based on the median of the latest seven valid observations.

The following elements must all use `estimatedWeeklyValueUsd`:

- Plotted line and area
- Current headline value
- Range baseline
- Absolute and percentage change
- Positive or negative color
- Hover, held, locked, and keyboard tooltip values
- Y-axis bounds

Keep `rawEstimatedWeeklyValueUsd` available only for diagnostics or an explicitly labeled optional raw series. Never silently fall back between raw and smoothed values in user-visible calculations. If the chosen signal is unavailable, show an unavailable state rather than substituting a different signal.

For the selected range, calculate:

```text
first = first plotted point with a non-null display signal
last  = last plotted point with a non-null display signal

change = last.value - first.value

percentage_change =
  change / first.value * 100
  when first.value != 0
```

The backend must return the exact first and last points used for these statistics, including their timestamps. The frontend must render the returned values and must not recompute them from a different source such as `CurrentQuote`.

When the requested range contains less history than requested, label it as partial and compare the first and last available plotted points. The label must make this clear, for example: `Available history: 30 days`. Do not imply that a 3M or 6M result covers the full requested period when only one month exists.

## 1. Unify the plotted series and statistics

Update the history response contract so the display-series choice is unambiguous. A suitable shape is:

```text
statistics:
  signal: "smoothed"
  baseline_timestamp
  baseline_value_usd
  current_timestamp
  current_value_usd
  delta_value_usd
  delta_percent
  point_count
  requested_start_timestamp
  available_start_timestamp
  available_end_timestamp
  partial
```

Names may follow repository conventions, but do not leave both generic and conflicting versions of the same statistic.

In `src-tauri/src/storage.rs`, change range statistics to use the first and last non-null smoothed values in the exact filtered point collection returned to the frontend. Do not restrict the baseline to the active epoch. A range comparison describes the visible range, so the baseline may belong to an older genuine weekly window.

In `src/components/UsageChart.tsx`, replace the current raw-first `historySignal` behavior with the explicit display signal. Remove user-visible raw/smoothed fallback logic.

In `src/App.tsx`, stop combining `CurrentQuote` with a history baseline. The normal, non-scrubbed value and comparison must come from the selected history response’s current and baseline values. Scrubbing must compare two points from the same display series.

Add an invariant test at the Rust response boundary and another at the React rendering boundary:

```text
statistics.current == last plotted display value
statistics.baseline == first plotted display value
statistics.delta == current - baseline
statistics.delta_percent == delta / baseline * 100
```

Use cent-level tolerance for displayed dollars and `0.01` percentage-point tolerance for displayed percentages.

## 2. Reconstruct genuine weekly windows

The existing behavior in `storage.rs::window_groups` starts a new epoch whenever two reported reset timestamps differ by more than five minutes. The audited database consequently produced 119 supposed weekly epochs in 30 days, including 118 `reported_reset_changed` boundaries. Replace this rule.

Build canonical streams using every stable identity available:

```text
(account_key, limit_id)
```

Never merge records across different non-null account or limit identities. If identity is missing, keep the uncertainty visible in diagnostics and avoid inventing an identity.

Within one stream, treat `reset_at_ms` as noisy metadata. Cluster compatible reset timestamps instead of treating every change as a reset. A new weekly window requires positive evidence, such as:

- The prior canonical reset time has been reached and usage materially drops.
- Usage materially drops while the new reset is approximately one weekly duration after the prior canonical reset.
- A reset transition is supported by a monotonic sequence of observations rather than one alternating or stale record.

Reset timestamps that alternate between competing historical values must be quarantined as conflicting observations or assigned to the compatible canonical cluster. They must not create minute-long weekly windows.

Use robust clustering rather than a single adjacent-record comparison. At minimum:

- Compare a reset candidate with the canonical reset for the active group, not only the immediately previous row.
- Permit normal timestamp jitter.
- Require temporal plausibility for a seven-day window.
- Prevent a stale record from moving the canonical reset backward.
- Require more than one consistent observation before accepting a radically changed reset, unless a scheduled reset and material usage decrease make the boundary independently clear.
- Record rejected or conflicting observations in diagnostics with a reason.

Do not guess across genuinely distinct streams. If the existing schema does not retain enough source/session identity to separate them, add nullable provenance fields for future imports and keep ambiguous historical records quarantined rather than forcing them into a confident estimate.

Add focused fixture tests for:

- Alternating reset timestamps in one stream.
- Normal reset jitter.
- A genuine scheduled weekly reset.
- A stale observation arriving after newer observations.
- Two accounts or limit IDs with interleaved timestamps.
- Missing account identity.
- An app restart in the middle of a weekly window.
- A material usage regression before and after the canonical reset.

The alternating-reset fixture must produce a small, plausible number of weekly windows, no sub-minute pseudo-windows, and stable estimates.

## 3. Rebuild derived history after upgrade

Fixing reconstruction only for new records would leave existing users with misleading stored epochs. Add a versioned, idempotent derived-data migration.

On first launch after upgrade:

1. Preserve raw `quota_snapshots` and raw `usage_events`.
2. In one transaction, invalidate or replace derived epochs, measurements, and dependent annotations generated by the old reconstruction version.
3. Rebuild them from raw records with the new canonical algorithm.
4. Store a reconstruction algorithm version so the rebuild does not run on every launch.
5. Roll back the transaction on failure and leave the prior database readable.

Do not delete user-owned raw data. If annotations can be user-created, distinguish those from generated reset annotations and preserve the user-created records.

Test migration from a fixture using the old fragmented schema/data. Run the migration twice and assert that the second run is a no-op and that outputs are identical.

## 4. Eliminate stale range switching

In `src/App.tsx`, the selected range currently changes before fresh history is fetched. Replace that behavior with a request-aware range-loading path.

When a range is selected:

1. Mark that range as loading.
2. Fetch it immediately.
3. Commit the response only if it belongs to the latest selection/request generation.
4. Keep the prior graph visible with a clear loading state or show a skeleton; do not present old cached values as current.

Also refresh all cached ranges when the backend’s latest history timestamp or reconstruction version changes. A cache entry must include the source `latest_timestamp` and algorithm version. Do not rely only on the periodic ten-second refresh.

Handle rapid switching and out-of-order responses. Add frontend tests that switch through all ranges quickly and resolve requests in reverse order. The visible graph and statistics must always match the final selected range and latest backend timestamp.

## 5. Make the x-axis chronological

Remove the compressed-gap timeline from `UsageChart.tsx`. Map timestamps linearly:

```text
x = plotLeft
  + (timestamp - requestedStart)
  / (requestedEnd - requestedStart)
  * (plotRight - plotLeft)
```

Use the requested range boundary and latest backend timestamp for the domain. This means a partial 6M history occupies only the portion of the six-month axis for which data exists.

Represent inactivity as a break in the rendered line and area. Do not allocate a fixed visual width to a gap. Gap labels or markers may remain, but they cannot alter the time scale.

Add deterministic SVG-coordinate tests showing that equal time intervals have equal horizontal distances and that a multi-day gap consumes its real proportional width.

## 6. Make tooltips factual

Remove linear interpolation of monetary, quota, and observed-cost values from mouse tooltips. Hover must snap to the nearest actual plotted observation, using the same behavior as keyboard navigation.

The tooltip timestamp must be the stored observation timestamp. It must never present an interpolated value as an observation. If interpolation is intentionally retained for another visual purpose, label it `Interpolated` and keep it out of the accuracy comparison; snapping is preferred.

Test hover near both sides of a midpoint, across epoch boundaries, and inside an inactivity gap.

## 7. Align event boundaries

Choose and document one interval convention for token-cost attribution. Prefer:

```text
window_start < event_timestamp <= observation_timestamp
```

Apply it consistently to:

- Estimate cost accumulation
- Displayed observed token cost
- Coverage calculations
- Migration rebuilds
- Verification queries and tests

If an event exactly at `window_start` belongs to the preceding window under this convention, ensure it is not displayed as part of the new window’s observed cost.

Add exact-boundary fixtures for events at one millisecond before, exactly at, and one millisecond after the start and observation boundaries.

## 8. Disclose pricing coverage

The audit found 1,525 events without usable pricing. Do not silently imply that the estimate covers all usage.

For each current estimate and history point, calculate enough metadata to state:

- Eligible events priced
- Eligible events excluded for unavailable pricing
- Priced share by token count or another defensible denominator
- Pricing source status (`official`, `custom`, `mixed`, or incomplete)

Do not fabricate prices for unknown models. Show a concise notice near the estimate when coverage is incomplete, with details available in diagnostics. Account for the fact that cache-write tokens cannot currently be identified; state that limitation rather than applying an unsupported multiplier.

Add tests for fully priced, partly priced, and completely unpriced windows.

## 9. Data volume and downsampling

The current API advertises bucket sizes but returns every quote. Either implement real deterministic downsampling or remove the false bucket metadata.

If downsampling is implemented, it must preserve:

- First and last display points.
- Genuine epoch boundaries.
- Minima and maxima that would otherwise hide spikes.
- Null/gap boundaries.
- The exact points referenced by range statistics.

Downsampling must never change the headline or comparison statistics. Test raw and downsampled responses against the same source data.

## Verification procedure

Run formatting, linting, type checking, frontend tests, Rust tests, and any repository-specific checks. Do not stop after unit tests.

Then launch the real app against a copied test database containing:

- The audited alternating-reset pattern.
- At least two genuine weekly windows.
- Gaps of several hours and several days.
- Partial history for 3M and 6M.
- Unknown-price events.

Inspect `1D`, `1W`, `1M`, `3M`, and `6M`. For every range independently recompute the expected first value, last value, dollar change, and percentage change from the returned plotted display series. Confirm the UI matches.

Verify:

- No range reports a decline when its displayed endpoints rise, or vice versa.
- Rapid range switching never mixes a graph with another range’s statistics.
- Refreshing while switching ranges cannot commit an obsolete response.
- Gaps use chronological width and break the line.
- Mouse and keyboard tooltips resolve to stored points.
- 3M and 6M clearly report partial history when applicable.
- The old 119-epoch fixture is rebuilt into plausible weekly windows.
- No raw quota snapshots or usage events are lost.

Capture before-and-after evidence and include exact commands and results in the final response.

## Definition of done

The work is complete only when:

- All confirmed defects in this document are fixed in production code.
- Existing users’ derived graph data is safely rebuilt.
- All old and new tests pass.
- Runtime verification passes for every available range.
- The graph and every associated statistic obey the same display-signal contract.
- Remaining uncertainty is disclosed in the UI and diagnostics.

Do not weaken or delete a valid existing test merely to obtain a green build. Do not paper over fragmented history by hiding annotations or filtering inconvenient points in the frontend. Correct the reconstruction and data contract at their source.

## Audit facts to reproduce before declaring success

The original audit found:

- The current headline estimate was mathematically correct.
- All 46,286 eligible priced events matched independent repricing.
- The plotted line used raw estimates while the headline and range comparison used smoothed estimates.
- The 1W line rose from `$16.458` to `$114.394`, while the UI reported a `13.6%` decline.
- Thirty days of data became 119 reconstructed weekly epochs.
- 118 boundaries were labeled `reported_reset_changed`.
- 87 epochs lasted less than one minute.
- 98 epochs lasted less than five minutes.
- Range switching could temporarily show a dollar-change error of `$9.42`.
- Long-range gaps were visually compressed.
- Mouse tooltips could show synthetic interpolated values.

Use these as regression targets. If the current repository or database differs, build fixtures that preserve the failure patterns and prove the fixes against those fixtures.
