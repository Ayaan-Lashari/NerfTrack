import { useEffect, useMemo, useRef, useState } from 'react';
import type { Annotation, HistoryPoint, Range } from '../domain';
import { Icon } from './Icons';

interface UsageChartProps {
  points: HistoryPoint[];
  annotations: Annotation[];
  range: Range;
  reducedMotion: boolean;
  onScrub?: (point: HistoryPoint | null) => void;
}

const chartWidth = 1000;
const chartHeight = 308;
const plotTop = 18;
const plotBottom = 270;
const plotLeft = 0;
const plotRight = 944;

interface ChartSelection {
  point: HistoryPoint;
  coordinate: { x: number; y: number };
  pointIndex: number | null;
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
  if (timestamp <= points[0].timestamp) return points[0];
  const last = points.at(-1);
  if (!last || timestamp >= last.timestamp) return last ?? null;

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

  return {
    timestamp,
    valueUsd: interpolateNullable(left.valueUsd, right.valueUsd, ratio),
    rawValueUsd: interpolateNullable(left.rawValueUsd, right.rawValueUsd, ratio),
    weeklyUsedPercent: interpolateNullable(left.weeklyUsedPercent, right.weeklyUsedPercent, ratio),
    isFinalized: left.isFinalized && right.isFinalized,
    isHeartbeat: nearest.isHeartbeat,
    dominantModel: nearest.dominantModel,
  };
}

export function UsageChart({
  points,
  annotations,
  range,
  reducedMotion,
  onScrub,
}: UsageChartProps) {
  const svgRef = useRef<SVGSVGElement>(null);
  const isDragging = useRef(false);
  const [selection, setSelection] = useState<ChartSelection | null>(null);

  const values = useMemo(
    () => points.map((point) => point.valueUsd).filter((value): value is number => value !== null),
    [points],
  );
  const bounds = useMemo(() => {
    if (!values.length) return { min: 0, max: 1 };
    const minValue = Math.min(...values);
    const maxValue = Math.max(...values);
    const padding = Math.max((maxValue - minValue) * 0.12, maxValue * 0.04, 1);
    return { min: Math.max(0, minValue - padding), max: maxValue + padding };
  }, [values]);

  const coordinates = useMemo(() => {
    const firstTimestamp = points[0]?.timestamp ?? 0;
    const duration = Math.max((points.at(-1)?.timestamp ?? firstTimestamp) - firstTimestamp, 1);
    const valueRange = Math.max(bounds.max - bounds.min, 1);
    return points.map((point) => ({
      x: plotLeft + ((point.timestamp - firstTimestamp) / duration) * (plotRight - plotLeft),
      y:
        point.valueUsd === null
          ? plotBottom
          : plotTop + ((bounds.max - point.valueUsd) / valueRange) * (plotBottom - plotTop),
    }));
  }, [bounds.max, bounds.min, points]);

  const linePath = useMemo(
    () =>
      coordinates
        .map(
          (point, index) =>
            `${index === 0 ? 'M' : 'L'} ${point.x.toFixed(2)} ${point.y.toFixed(2)}`,
        )
        .join(' '),
    [coordinates],
  );
  const areaPath = linePath
    ? `${linePath} L ${coordinates.at(-1)?.x ?? plotRight} ${plotBottom} L ${coordinates[0]?.x ?? plotLeft} ${plotBottom} Z`
    : '';
  const selected = selection?.point ?? null;
  const selectedCoordinate = selection?.coordinate ?? null;
  const baselineCoordinate = coordinates.find((_, index) => points[index]?.valueUsd !== null);

  const selectPoint = (index: number) => {
    const point = points[index];
    const coordinate = coordinates[index];
    if (!point || !coordinate) return;
    setSelection({ point, coordinate, pointIndex: index });
    onScrub?.(point);
  };

  const updateSelection = (clientX: number) => {
    const svg = svgRef.current;
    if (!svg || points.length === 0) return;
    const rect = svg.getBoundingClientRect();
    if (rect.width <= 0) return;
    const svgX = ((clientX - rect.left) / rect.width) * chartWidth;
    const ratio = Math.max(0, Math.min(1, (svgX - plotLeft) / (plotRight - plotLeft)));
    const firstTimestamp = points[0].timestamp;
    const lastTimestamp = points.at(-1)?.timestamp ?? firstTimestamp;
    const timestamp = firstTimestamp + ratio * (lastTimestamp - firstTimestamp);
    const point = interpolatePoint(points, timestamp);
    if (!point) return;
    const valueRange = Math.max(bounds.max - bounds.min, 1);
    const y =
      point.valueUsd === null
        ? plotBottom
        : plotTop + ((bounds.max - point.valueUsd) / valueRange) * (plotBottom - plotTop);
    setSelection({
      point,
      coordinate: { x: plotLeft + ratio * (plotRight - plotLeft), y },
      pointIndex: null,
    });
    onScrub?.(point);
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
          setSelection({ point, coordinate, pointIndex: nextIndex });
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
      onScrub?.(null);
    }
  };

  const gridValues = [
    bounds.max,
    bounds.max - (bounds.max - bounds.min) / 4,
    bounds.min + (bounds.max - bounds.min) / 2,
    bounds.min + (bounds.max - bounds.min) / 4,
    bounds.min,
  ];
  const labelIndexes = [
    ...new Set(
      [0, 0.25, 0.5, 0.75, 1].map((ratio) => Math.max(0, Math.round((points.length - 1) * ratio))),
    ),
  ];

  return (
    <div className={`usage-chart ${reducedMotion ? 'reduced-motion' : ''}`}>
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
            <strong>{formatUsd(selected.valueUsd)}</strong>
          </div>
        )}
        {!points.length && <div className="chart-empty">Waiting for weekly observations</div>}
        <svg
          ref={svgRef}
          className="chart-canvas"
          viewBox={`0 0 ${chartWidth} ${chartHeight}`}
          role="img"
          aria-label="Estimated weekly API equivalent history chart. Use arrow keys to move between points."
          tabIndex={0}
          onPointerDown={(event) => {
            event.currentTarget.setPointerCapture?.(event.pointerId);
            isDragging.current = true;
            updateSelection(event.clientX);
          }}
          onPointerMove={(event) => {
            if (isDragging.current || event.pointerType === 'mouse') {
              const coalesced = event.nativeEvent.getCoalescedEvents?.();
              updateSelection(coalesced?.at(-1)?.clientX ?? event.clientX);
            }
          }}
          onPointerUp={(event) => {
            if (event.currentTarget.hasPointerCapture?.(event.pointerId)) {
              event.currentTarget.releasePointerCapture?.(event.pointerId);
            }
            isDragging.current = false;
          }}
          onPointerCancel={() => {
            isDragging.current = false;
          }}
          onDoubleClick={() => {
            setSelection(null);
            onScrub?.(null);
          }}
          onKeyDown={keyHandler}
        >
          <defs>
            <linearGradient id="usage-area" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0" stopColor="#5cf07a" stopOpacity="0.3" />
              <stop offset="0.72" stopColor="#5cf07a" stopOpacity="0.09" />
              <stop offset="1" stopColor="#5cf07a" stopOpacity="0" />
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
          {labelIndexes.slice(1, -1).map((pointIndex) => (
            <line
              key={`vertical-${pointIndex}`}
              className="chart-grid chart-grid-vertical"
              x1={coordinates[pointIndex]?.x ?? 0}
              x2={coordinates[pointIndex]?.x ?? 0}
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
          {annotations.map((annotation) => {
            const ratio =
              points.length <= 1
                ? 0
                : (annotation.timestamp - points[0].timestamp) /
                  (points[points.length - 1].timestamp - points[0].timestamp);
            if (ratio < 0 || ratio > 1) return null;
            const x = plotLeft + ratio * (plotRight - plotLeft);
            return (
              <g key={annotation.id} className="chart-annotation">
                <line x1={x} x2={x} y1={20} y2={plotBottom} />
                <circle cx={x} cy={20} r={4.4} />
                <g transform={`translate(${Math.max(4, Math.min(x - 50, plotRight - 112))}, -4)`}>
                  <rect width="112" height="30" rx="7" />
                  <text x="56" y="19" textAnchor="middle">
                    {annotation.label}
                  </text>
                </g>
              </g>
            );
          })}
          {selectedCoordinate && selected && (
            <g className="chart-crosshair">
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
                {Math.round(value)}
              </text>
            );
          })}
          {labelIndexes.map((pointIndex, index) => {
            const point = points[pointIndex];
            const x = coordinates[pointIndex]?.x ?? 0;
            return (
              <text
                key={`x-${index}`}
                className="chart-x-label"
                x={x}
                y="292"
                textAnchor={
                  index === 0 ? 'start' : index === labelIndexes.length - 1 ? 'end' : 'middle'
                }
              >
                {point ? formatDate(point.timestamp, range) : ''}
              </text>
            );
          })}
        </svg>
      </div>
    </div>
  );
}
