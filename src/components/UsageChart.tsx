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

function nearestPoint(points: HistoryPoint[], ratio: number) {
  if (!points.length) return null;
  const index = Math.max(0, Math.min(points.length - 1, Math.round(ratio * (points.length - 1))));
  return { point: points[index], index };
}

export function UsageChart({
  points,
  annotations,
  range,
  reducedMotion,
  onScrub,
}: UsageChartProps) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [selectedIndex, setSelectedIndex] = useState<number | null>(null);
  const [isDragging, setIsDragging] = useState(false);

  const values = useMemo(
    () => points.map((point) => point.valueUsd).filter((value): value is number => value !== null),
    [points],
  );
  const bounds = useMemo(() => {
    if (!values.length) return { min: 0, max: 1 };
    const min = Math.floor(Math.min(...values) / 10) * 10 - 10;
    const max = Math.ceil(Math.max(...values) / 10) * 10 + 10;
    return { min, max: Math.max(min + 10, max) };
  }, [values]);

  const coordinates = useMemo(() => {
    const divisor = Math.max(points.length - 1, 1);
    const valueRange = Math.max(bounds.max - bounds.min, 1);
    return points.map((point, index) => ({
      x: plotLeft + (index / divisor) * (plotRight - plotLeft),
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
  const areaPath = `${linePath} L ${plotRight} ${plotBottom} L ${plotLeft} ${plotBottom} Z`;
  const selected = selectedIndex === null ? null : points[selectedIndex];
  const selectedCoordinate = selectedIndex === null ? null : coordinates[selectedIndex];

  const updateSelection = (clientX: number) => {
    const svg = svgRef.current;
    if (!svg || points.length === 0) return;
    const rect = svg.getBoundingClientRect();
    const ratio = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
    const next = nearestPoint(points, ratio);
    if (!next) return;
    setSelectedIndex(next.index);
    onScrub?.(next.point);
  };

  useEffect(() => {
    if (!isDragging) return;
    const move = (event: PointerEvent) => updateSelection(event.clientX);
    const up = () => setIsDragging(false);
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up, { once: true });
    return () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
  });

  const keyHandler = (event: React.KeyboardEvent<SVGSVGElement>) => {
    if (!points.length) return;
    const current = selectedIndex ?? points.length - 1;
    if (event.key === 'ArrowLeft' || event.key === 'ArrowDown') {
      event.preventDefault();
      const next = Math.max(0, current - 1);
      setSelectedIndex(next);
      onScrub?.(points[next]);
    }
    if (event.key === 'ArrowRight' || event.key === 'ArrowUp') {
      event.preventDefault();
      const next = Math.min(points.length - 1, current + 1);
      setSelectedIndex(next);
      onScrub?.(points[next]);
    }
    if (event.key === 'Escape') {
      setSelectedIndex(null);
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
    0,
    Math.floor(points.length * 0.25),
    Math.floor(points.length * 0.5),
    Math.floor(points.length * 0.75),
    points.length - 1,
  ];

  return (
    <div className={`usage-chart ${reducedMotion ? 'reduced-motion' : ''}`}>
      <div className="chart-toolbar">
        <div className="chart-legend">
          <span className="legend-swatch" />
          Estimated API equivalent
        </div>
        {selected && (
          <div className="scrub-readout">
            <Icon name="calendar" size={14} />
            {formatDate(selected.timestamp, range)} <strong>{formatUsd(selected.valueUsd)}</strong>
          </div>
        )}
      </div>
      <div className="chart-canvas-wrap">
        <svg
          ref={svgRef}
          className="chart-canvas"
          viewBox={`0 0 ${chartWidth} ${chartHeight}`}
          role="img"
          aria-label="Estimated weekly API equivalent history chart. Use arrow keys to move between points."
          tabIndex={0}
          onPointerDown={(event) => {
            setIsDragging(true);
            updateSelection(event.clientX);
          }}
          onPointerMove={(event) => {
            if (!isDragging) updateSelection(event.clientX);
          }}
          onDoubleClick={() => {
            setSelectedIndex(null);
            onScrub?.(null);
          }}
          onKeyDown={keyHandler}
        >
          <defs>
            <linearGradient id="usage-area" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0" stopColor="#5cf07a" stopOpacity="0.38" />
              <stop offset="0.82" stopColor="#5cf07a" stopOpacity="0.08" />
              <stop offset="1" stopColor="#5cf07a" stopOpacity="0" />
            </linearGradient>
            <filter id="soft-line-glow" x="-20%" y="-20%" width="140%" height="140%">
              <feGaussianBlur stdDeviation="1.3" result="blur" />
              <feMerge>
                <feMergeNode in="blur" />
                <feMergeNode in="SourceGraphic" />
              </feMerge>
            </filter>
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
          <path className="chart-area" d={areaPath} />
          <path className="chart-line" d={linePath} filter="url(#soft-line-glow)" />
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
              <circle cx={selectedCoordinate.x} cy={selectedCoordinate.y} r={5.2} />
              <circle cx={selectedCoordinate.x} cy={selectedCoordinate.y} r={2.1} />
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
