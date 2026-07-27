import { useEffect, useMemo, useRef, useState } from 'react';
import type { Annotation, HistoryPoint, Range } from '../domain';
import { Icon } from './Icons';

interface UsageChartProps {
  points: HistoryPoint[];
  annotations: Annotation[];
  range: Range;
  reducedMotion: boolean;
  changeValueUsd?: number | null;
  baselineEstimatedWeeklyValueUsd?: number | null;
  onScrub?: (point: HistoryPoint | null, anchor: HistoryPoint | null) => void;
}

const chartWidth = 1000;
const chartHeight = 308;
const plotTop = 18;
const plotBottom = 270;
const plotLeft = 0;
const plotRight = 944;
const rangeDurationMs: Record<Range, number> = {
  '1D': 86_400_000,
  '1W': 604_800_000,
  '1M': 2_592_000_000,
  '3M': 7_776_000_000,
  '6M': 15_552_000_000,
};

interface ChartSelection {
  point: HistoryPoint;
  coordinate: { x: number; y: number };
  pointIndex: number | null;
  source: 'hover' | 'held' | 'locked' | 'keyboard';
}

function formatDate(timestamp: number, range: Range) {
  const date = new Date(timestamp);
  if (range === '1D') {
    return date.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' });
  }
  return date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
}

function formatUsd(value: number | null) {
  return value === null ? '—' : `$${value.toFixed(2)}`;
}

function compactAnnotationLabel(label: string) {
  return label
    .replace(/^Weekly window · /, '')
    .replace(/_/g, ' ')
    .replace('reported reset changed', 'reset changed')
    .replace('usage decreased', 'usage drop');
}

function nearestPoint(points: HistoryPoint[], timestamp: number) {
  if (!points.length) return null;
  let low = 0;
  let high = points.length - 1;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    if (points[middle].timestamp < timestamp) low = middle + 1;
    else high = middle;
  }
  const index =
    low > 0 &&
    Math.abs(points[low - 1].timestamp - timestamp) < Math.abs(points[low].timestamp - timestamp)
      ? low - 1
      : low;
  return { point: points[index], index };
}

function interpolateNullable(left: number | null, right: number | null, ratio: number) {
  if (left === null || right === null) return ratio < 0.5 ? left : right;
  return left + (right - left) * ratio;
}

function interpolatePoint(points: HistoryPoint[], timestamp: number): HistoryPoint | null {
  if (!points.length) return null;
  if (timestamp < points[0].timestamp) return null;
  if (timestamp === points[0].timestamp) return points[0];
  const last = points.at(-1);
  if (!last || timestamp > last.timestamp) return null;
  if (timestamp === last.timestamp) return last;

  let low = 1;
  let high = points.length - 1;
  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    if (points[middle].timestamp < timestamp) low = middle + 1;
    else high = middle;
  }

  const left = points[low - 1];
  const right = points[low];
  const duration = Math.max(right.timestamp - left.timestamp, 1);
  const ratio = Math.max(0, Math.min(1, (timestamp - left.timestamp) / duration));
  const nearest = ratio < 0.5 ? left : right;
  if (left.epoch !== right.epoch) return nearest;

  return {
    timestamp,
    estimatedWeeklyValueUsd: interpolateNullable(
      left.estimatedWeeklyValueUsd,
      right.estimatedWeeklyValueUsd,
      ratio,
    ),
    rawEstimatedWeeklyValueUsd: interpolateNullable(
      left.rawEstimatedWeeklyValueUsd,
      right.rawEstimatedWeeklyValueUsd,
      ratio,
    ),
    observedCostUsd: interpolateNullable(left.observedCostUsd, right.observedCostUsd, ratio),
    weeklyUsedPercent: interpolateNullable(left.weeklyUsedPercent, right.weeklyUsedPercent, ratio),
    resetAt: nearest.resetAt,
    resetReason: nearest.resetReason,
    isFinalized: left.isFinalized && right.isFinalized,
    isHeartbeat: nearest.isHeartbeat,
    epoch: nearest.epoch,
    confidence: nearest.confidence,
    percentageCoverage: interpolateNullable(
      left.percentageCoverage,
      right.percentageCoverage,
      ratio,
    ),
  };
}

function historySignal(point: HistoryPoint) {
  return point.rawEstimatedWeeklyValueUsd ?? point.estimatedWeeklyValueUsd;
}

// harn:assume raw-history-stable-headline ref=history-chart-rendering scope=function
export function UsageChart({
  points,
  annotations,
  range,
  reducedMotion,
  changeValueUsd = null,
  baselineEstimatedWeeklyValueUsd = null,
  onScrub,
}: UsageChartProps) {
  const svgRef = useRef<SVGSVGElement>(null);
  const isDragging = useRef(false);
  const anchorRef = useRef<HistoryPoint | null>(null);
  const [selection, setSelection] = useState<ChartSelection | null>(null);
  const [anchorPoint, setAnchorPoint] = useState<HistoryPoint | null>(null);

  useEffect(() => {
    isDragging.current = false;
    anchorRef.current = null;
    setSelection(null);
    setAnchorPoint(null);
  }, [range]);

  const values = useMemo(
    () => points.map(historySignal).filter((value): value is number => value !== null),
    [points],
  );
  const bounds = useMemo(() => {
    if (!values.length) return { min: 0, max: 1 };
    const minValue = Math.min(...values);
    const maxValue = Math.max(...values);
    const padding = Math.max((maxValue - minValue) * 0.12, maxValue * 0.04, 1);
    return { min: Math.max(0, minValue - padding), max: maxValue + padding };
  }, [values]);

  const rangeEnd = points.at(-1)?.timestamp ?? Date.now();
  const rangeStart = rangeEnd - rangeDurationMs[range];
  const coordinates = useMemo(() => {
    const valueRange = Math.max(bounds.max - bounds.min, 1);
    return points.map((point) => ({
      x:
        plotLeft +
        ((point.timestamp - rangeStart) / rangeDurationMs[range]) * (plotRight - plotLeft),
      y:
        historySignal(point) === null
          ? plotBottom
          : plotTop +
            ((bounds.max - (historySignal(point) ?? bounds.min)) / valueRange) *
              (plotBottom - plotTop),
    }));
  }, [bounds.max, bounds.min, points, range, rangeStart]);

  const segments = useMemo(() => {
    const result: { x: number; y: number }[][] = [];
    points.forEach((point, index) => {
      if (historySignal(point) === null) return;
      if (index === 0 || points[index - 1].epoch !== point.epoch) result.push([]);
      result.at(-1)?.push(coordinates[index]);
    });
    return result;
  }, [coordinates, points]);
  const linePath = segments
    .map((segment) =>
      segment
        .map(
          (point, index) =>
            `${index === 0 ? 'M' : 'L'} ${point.x.toFixed(2)} ${point.y.toFixed(2)}`,
        )
        .join(' '),
    )
    .join(' ');
  const areaPath = segments
    .filter((segment) => segment.length > 1)
    .map((segment) => {
      const line = segment
        .map(
          (point, index) =>
            `${index === 0 ? 'M' : 'L'} ${point.x.toFixed(2)} ${point.y.toFixed(2)}`,
        )
        .join(' ');
      return `${line} L ${segment.at(-1)?.x ?? plotRight} ${plotBottom} L ${segment[0].x} ${plotBottom} Z`;
    })
    .join(' ');
  const visibleAnnotations = useMemo(() => {
    let previousX = -Infinity;
    let lane = 0;
    return annotations
      .map((annotation) => {
        const ratio = (annotation.timestamp - rangeStart) / rangeDurationMs[range];
        if (ratio < 0 || ratio > 1) return null;
        const x = plotLeft + ratio * (plotRight - plotLeft);
        lane = x - previousX < 146 ? (lane + 1) % 3 : 0;
        previousX = x;
        return {
          annotation,
          x,
          lane,
          label: compactAnnotationLabel(annotation.label),
        };
      })
      .filter((item): item is NonNullable<typeof item> => item !== null)
      .slice(-10);
  }, [annotations, range, rangeStart]);
  const selected = selection?.point ?? null;
  const selectedCoordinate = selection?.coordinate ?? null;
  const anchorCoordinate = useMemo(() => {
    if (!anchorPoint || points.length === 0) return null;
    const valueRange = Math.max(bounds.max - bounds.min, 1);
    return {
      x:
        plotLeft +
        ((anchorPoint.timestamp - rangeStart) / rangeDurationMs[range]) * (plotRight - plotLeft),
      y:
        historySignal(anchorPoint) === null
          ? plotBottom
          : plotTop +
            ((bounds.max - (historySignal(anchorPoint) ?? bounds.min)) / valueRange) *
              (plotBottom - plotTop),
    };
  }, [anchorPoint, bounds.max, bounds.min, points.length, range, rangeStart]);
  const baselineCoordinate =
    baselineEstimatedWeeklyValueUsd === null ||
    baselineEstimatedWeeklyValueUsd < bounds.min ||
    baselineEstimatedWeeklyValueUsd > bounds.max
      ? null
      : {
          y:
            plotTop +
            ((bounds.max - baselineEstimatedWeeklyValueUsd) /
              Math.max(bounds.max - bounds.min, 1)) *
              (plotBottom - plotTop),
        };
  const dragChange =
    anchorPoint &&
    historySignal(anchorPoint) != null &&
    selected &&
    historySignal(selected) != null &&
    (selection?.source === 'held' || selection?.source === 'locked')
      ? (historySignal(selected) ?? 0) - (historySignal(anchorPoint) ?? 0)
      : null;
  const isNegative = (dragChange ?? changeValueUsd ?? 0) < 0;
  const chartColor = isNegative ? '#ff5d73' : '#5cf07a';

  const selectPoint = (index: number) => {
    const point = points[index];
    const coordinate = coordinates[index];
    if (!point || !coordinate) return;
    setSelection({ point, coordinate, pointIndex: index, source: 'keyboard' });
    onScrub?.(point, null);
  };

  const updateSelection = (
    clientX: number,
    source: ChartSelection['source'],
    anchor: HistoryPoint | null | 'self' = null,
  ) => {
    const svg = svgRef.current;
    if (!svg || points.length === 0) return null;
    const rect = svg.getBoundingClientRect();
    if (rect.width <= 0) return null;
    const svgX = ((clientX - rect.left) / rect.width) * chartWidth;
    const ratio = Math.max(0, Math.min(1, (svgX - plotLeft) / (plotRight - plotLeft)));
    const timestamp = rangeStart + ratio * rangeDurationMs[range];
    const point = interpolatePoint(points, timestamp);
    if (!point) return null;
    const valueRange = Math.max(bounds.max - bounds.min, 1);
    const y =
      historySignal(point) === null
        ? plotBottom
        : plotTop +
          ((bounds.max - (historySignal(point) ?? bounds.min)) / valueRange) *
            (plotBottom - plotTop);
    setSelection({
      point,
      coordinate: { x: plotLeft + ratio * (plotRight - plotLeft), y },
      pointIndex: null,
      source,
    });
    onScrub?.(point, anchor === 'self' ? point : anchor);
    return point;
  };

  useEffect(() => {
    if (selection?.pointIndex !== null && selection?.pointIndex !== undefined) {
      if (selection.pointIndex >= points.length) {
        const nextIndex = points.length ? points.length - 1 : null;
        if (nextIndex === null) {
          setSelection(null);
        } else {
          const point = points[nextIndex];
          const coordinate = coordinates[nextIndex];
          setSelection({ point, coordinate, pointIndex: nextIndex, source: 'keyboard' });
        }
      }
    }
  }, [coordinates, points, selection?.pointIndex]);

  const keyHandler = (event: React.KeyboardEvent<SVGSVGElement>) => {
    if (!points.length) return;
    const current =
      selection?.pointIndex ??
      nearestPoint(points, selection?.point.timestamp ?? points.at(-1)?.timestamp ?? 0)?.index ??
      points.length - 1;
    if (event.key === 'ArrowLeft' || event.key === 'ArrowDown') {
      event.preventDefault();
      const next = Math.max(0, current - 1);
      selectPoint(next);
    }
    if (event.key === 'ArrowRight' || event.key === 'ArrowUp') {
      event.preventDefault();
      const next = Math.min(points.length - 1, current + 1);
      selectPoint(next);
    }
    if (event.key === 'Escape') {
      setSelection(null);
      anchorRef.current = null;
      setAnchorPoint(null);
      onScrub?.(null, null);
    }
  };

  const gridValues = [
    bounds.max,
    bounds.max - (bounds.max - bounds.min) / 4,
    bounds.min + (bounds.max - bounds.min) / 2,
    bounds.min + (bounds.max - bounds.min) / 4,
    bounds.min,
  ];
  const labelRatios = [0, 0.25, 0.5, 0.75, 1];

  return (
    <div
      className={`usage-chart chart-${isNegative ? 'negative' : 'positive'} ${
        reducedMotion ? 'reduced-motion' : ''
      }`}
      style={{ '--chart-color': chartColor } as React.CSSProperties}
    >
      <div className="chart-value-label">
        <span>Estimated weekly API-equivalent value</span>
        <small>USD · local token-derived estimate</small>
      </div>
      <div className="chart-canvas-wrap">
        {selected && selectedCoordinate && (
          <div
            className="scrub-readout"
            style={
              {
                '--scrub-x': `${(selectedCoordinate.x / chartWidth) * 100}%`,
              } as React.CSSProperties
            }
          >
            <Icon name="calendar" size={14} />
            <span>{formatDate(selected.timestamp, range)}</span>
            <strong>{formatUsd(historySignal(selected))}</strong>
            <small>Observed: {formatUsd(selected.observedCostUsd)}</small>
          </div>
        )}
        {!points.length && <div className="chart-empty">Waiting for weekly observations</div>}
        <svg
          ref={svgRef}
          className={`chart-canvas ${isDragging.current ? 'is-scrubbing' : ''}`}
          viewBox={`0 0 ${chartWidth} ${chartHeight}`}
          role="img"
          aria-label="Estimated weekly API-equivalent value history chart. Use arrow keys to move between points."
          aria-grabbed={isDragging.current}
          tabIndex={0}
          onPointerDown={(event) => {
            event.currentTarget.setPointerCapture?.(event.pointerId);
            isDragging.current = true;
            anchorRef.current = updateSelection(event.clientX, 'held', 'self');
            setAnchorPoint(anchorRef.current);
          }}
          onPointerMove={(event) => {
            if (isDragging.current || event.pointerType === 'mouse') {
              const coalesced = event.nativeEvent.getCoalescedEvents?.();
              updateSelection(
                coalesced?.at(-1)?.clientX ?? event.clientX,
                isDragging.current ? 'held' : 'hover',
                isDragging.current ? anchorRef.current : null,
              );
            }
          }}
          onPointerUp={(event) => {
            if (event.currentTarget.hasPointerCapture?.(event.pointerId)) {
              event.currentTarget.releasePointerCapture?.(event.pointerId);
            }
            isDragging.current = false;
            setSelection((current) =>
              current?.source === 'held' ? { ...current, source: 'locked' } : current,
            );
          }}
          onPointerCancel={() => {
            isDragging.current = false;
          }}
          onLostPointerCapture={() => {
            isDragging.current = false;
            setSelection((current) =>
              current?.source === 'held' ? { ...current, source: 'locked' } : current,
            );
          }}
          onPointerLeave={() => {
            if (!isDragging.current && selection?.source === 'hover') {
              setSelection(null);
              onScrub?.(null, null);
            }
          }}
          onDoubleClick={() => {
            setSelection(null);
            anchorRef.current = null;
            setAnchorPoint(null);
            onScrub?.(null, null);
          }}
          onKeyDown={keyHandler}
        >
          <defs>
            <linearGradient id="usage-area" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0" stopColor={chartColor} stopOpacity="0.3" />
              <stop offset="0.72" stopColor={chartColor} stopOpacity="0.09" />
              <stop offset="1" stopColor={chartColor} stopOpacity="0" />
            </linearGradient>
          </defs>
          {gridValues.map((_, index) => {
            const y = plotTop + (index / 4) * (plotBottom - plotTop);
            return (
              <line
                key={`grid-${index}`}
                className="chart-grid"
                x1={plotLeft}
                x2={plotRight}
                y1={y}
                y2={y}
              />
            );
          })}
          {labelRatios.slice(1, -1).map((ratio) => (
            <line
              key={`vertical-${ratio}`}
              className="chart-grid chart-grid-vertical"
              x1={plotLeft + ratio * (plotRight - plotLeft)}
              x2={plotLeft + ratio * (plotRight - plotLeft)}
              y1={plotTop}
              y2={plotBottom}
            />
          ))}
          {baselineCoordinate && (
            <line
              className="chart-baseline"
              x1={plotLeft}
              x2={plotRight}
              y1={baselineCoordinate.y}
              y2={baselineCoordinate.y}
            />
          )}
          <path className="chart-area" d={areaPath} />
          <path className="chart-line" d={linePath} />
          {segments
            .filter((segment) => segment.length === 1)
            .map(([point], index) => (
              <circle
                key={`point-${index}`}
                className="chart-point"
                cx={point.x}
                cy={point.y}
                r="3"
              />
            ))}
          {visibleAnnotations.map(({ annotation, x, lane, label }) => {
            const markerY = 20 + lane * 30;
            const labelX = Math.max(4, Math.min(x - 64, plotRight - 132));
            return (
              <g key={annotation.id} className="chart-annotation">
                <title>{annotation.label}</title>
                <line x1={x} x2={x} y1={markerY} y2={plotBottom} />
                <circle cx={x} cy={markerY} r={4.4} />
                <g transform={`translate(${labelX}, ${markerY - 15})`}>
                  <rect width="132" height="28" rx="7" />
                  <text x="66" y="18" textAnchor="middle">
                    {label}
                  </text>
                </g>
              </g>
            );
          })}
          {anchorCoordinate && (selection?.source === 'held' || selection?.source === 'locked') && (
            <g className="chart-anchor-marker">
              <line x1={anchorCoordinate.x} x2={anchorCoordinate.x} y1={plotTop} y2={plotBottom} />
              <circle cx={anchorCoordinate.x} cy={anchorCoordinate.y} r={5.5} />
              <circle cx={anchorCoordinate.x} cy={anchorCoordinate.y} r={2.25} />
            </g>
          )}
          {selectedCoordinate && selected && (
            <g
              className={`chart-crosshair ${
                selection?.source === 'held' ? 'chart-crosshair-held' : ''
              }`}
            >
              <line
                x1={selectedCoordinate.x}
                x2={selectedCoordinate.x}
                y1={plotTop}
                y2={plotBottom}
              />
              <line
                className="chart-crosshair-horizontal"
                x1={plotLeft}
                x2={plotRight}
                y1={selectedCoordinate.y}
                y2={selectedCoordinate.y}
              />
              <circle cx={selectedCoordinate.x} cy={selectedCoordinate.y} r={5.5} />
              <circle cx={selectedCoordinate.x} cy={selectedCoordinate.y} r={2.25} />
            </g>
          )}
          <line
            className="chart-axis"
            x1={plotLeft}
            x2={plotRight}
            y1={plotBottom}
            y2={plotBottom}
          />
          {gridValues.map((value, index) => {
            const y = plotTop + (index / 4) * (plotBottom - plotTop);
            return (
              <text key={`y-${index}`} className="chart-y-label" x="963" y={y + 4}>
                ${Math.round(value)}
              </text>
            );
          })}
          {labelRatios.map((ratio, index) => {
            const x = plotLeft + ratio * (plotRight - plotLeft);
            const timestamp = rangeStart + ratio * rangeDurationMs[range];
            return (
              <text
                key={`x-${index}`}
                className="chart-x-label"
                x={x}
                y="292"
                textAnchor={
                  index === 0 ? 'start' : index === labelRatios.length - 1 ? 'end' : 'middle'
                }
              >
                {formatDate(timestamp, range)}
              </text>
            );
          })}
        </svg>
      </div>
    </div>
  );
}
