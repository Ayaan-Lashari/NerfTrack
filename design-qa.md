# Design QA

- Source visual truth: `/var/folders/m2/r8bvqn691ps3zh9yhzytr7yc0000gr/T/TemporaryItems/NSIRD_screencaptureui_cKKfNu/Screenshot 2026-07-23 at 2.56.01 PM.png`
- Implementation screenshot: `/Users/MAK/Desktop/Nerfify/design-qa-implementation-final.png`
- Focused comparison: `/Users/MAK/Desktop/Nerfify/design-qa-comparison-final.png`
- Browser viewport: 1280 × 720 CSS pixels
- Source pixels: 821 × 263
- Implementation pixels: 1280 × 720 at device scale factor 1
- Focused implementation crop: 925 × 300
- Density normalization: source centered without scaling; implementation chart cropped at native density
- State: desktop, dark theme, 1W selected, active hover crosshair

## Full-view comparison evidence

The final capture preserves Nerfify’s existing desktop hierarchy and range controls while adopting the reference chart’s thin green line, restrained horizontal and vertical grid, right-aligned value scale, bottom date scale, reference-value rule, and soft area fill.

## Focused comparison evidence

The combined comparison shows the reference and implementation chart regions together. Their line treatment, fill direction, grid cadence, axis placement, and label density match closely. The light-to-dark theme change is intentional because the source is Apple Stocks while Nerfify’s established product theme is dark.

## Required fidelity surfaces

- Fonts and typography: existing system-font stack retained; axis sizes, weights, and hierarchy match the compact reference treatment.
- Spacing and layout rhythm: plot proportions, right-axis gutter, bottom labels, and grid spacing align with the reference while fitting the existing dashboard.
- Colors and visual tokens: Nerfify green replaces Apple green; opacity and contrast follow the reference in the dark theme.
- Image quality and asset fidelity: no raster assets are required; the graph remains sharp native SVG at every window size.
- Copy and content: Nerfify’s weekly-equivalent labels and supported ranges are preserved.

## Interaction verification

- Mouse hover follows the pointer without requiring a click.
- Passive hover retains the normal arrow cursor.
- Pressing enters a distinct grabbed state; pointer capture keeps scrubbing active along the full drag path and release locks the selected value.
- Press-and-drag scrubbing selects continuously interpolated time and value data rather than snapping to stored vertices.
- A one-pixel pointer sweep returned six distinct values (`$392.97`, `$393.07`, `$393.48`, `$393.90`, `$394.31`, `$394.73`), confirming fine-grained tracking.
- Selection persisted after pointer release.
- Floating date/value pill, vertical guide, horizontal guide, and point marker rendered.
- Keyboard arrow scrubbing and Escape reset remain supported.
- Browser console errors: none.

## Comparison history

1. Initial comparison found the reference’s persistent dashed value rule missing (P2).
2. Added a baseline rule using the first visible weekly value.
3. Interaction review found pointer selection snapped to stored observations (P1).
4. Replaced nearest-point mouse selection with timestamp/value interpolation and hover tracking.
5. Added an explicit held/locked interaction state so press-hold-drag is distinguishable from passive hover.
6. Final live verification confirms pixel-level pointer movement, native hold-and-drag capture, normal hover cursor, and no console errors.

## Findings

No actionable P0, P1, or P2 differences remain.

## Follow-up polish

- P3: Apple Stocks exposes additional long-range tabs. Nerfify intentionally keeps only ranges backed by its current local history API.

final result: passed
