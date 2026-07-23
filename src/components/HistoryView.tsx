import type { HistoryResponse, Range } from '../domain';
import { Icon } from './Icons';

interface HistoryViewProps {
  history: HistoryResponse;
  range: Range;
  onRangeChange: (range: Range) => void;
}

const ranges: Range[] = ['1D', '1W', '1M', '3M', '6M'];

export function HistoryView({ history, range, onRangeChange }: HistoryViewProps) {
  const recent = history.points.slice(-8).reverse();
  return (
    <section className="page-shell history-page">
      <header className="page-heading history-heading">
        <div>
          <h1>History</h1>
          <p>Finalized quotes and workload-comparable observations.</p>
        </div>
        <div className="range-tabs compact-tabs">
          {ranges.map((item) => (
            <button
              className={item === range ? 'selected' : ''}
              key={item}
              onClick={() => onRangeChange(item)}
            >
              {item}
            </button>
          ))}
        </div>
      </header>
      <div className="history-stats">
        <div>
          <span>Current</span>
          <strong>
            {history.statistics.currentValueUsd === null
              ? '—'
              : `$${history.statistics.currentValueUsd.toFixed(2)}`}
          </strong>
        </div>
        <div>
          <span>Range change</span>
          <strong
            className={
              history.statistics.deltaUsd !== null && history.statistics.deltaUsd < 0
                ? 'negative'
                : ''
            }
          >
            {history.statistics.deltaUsd === null
              ? '—'
              : `${history.statistics.deltaUsd < 0 ? '−' : '+'}$${Math.abs(history.statistics.deltaUsd).toFixed(2)}`}
          </strong>
        </div>
        <div>
          <span>Observations</span>
          <strong>{history.statistics.pointCount}</strong>
        </div>
        <div>
          <span>Bucket</span>
          <strong>{history.bucket}</strong>
        </div>
      </div>
      <div className="panel history-table-panel">
        <div className="panel-heading">
          <Icon name="history" size={23} />
          <h2>Recent observations</h2>
          <span className="table-note">
            {history.statistics.partial ? 'Partial range' : 'Complete range'}
          </span>
        </div>
        <div className="history-table" role="table" aria-label="Recent observations">
          <div className="history-table-row history-table-header" role="row">
            <span>Date</span>
            <span>Estimated value</span>
            <span>Weekly usage</span>
            <span>Model</span>
            <span>Status</span>
          </div>
          {recent.map((point) => (
            <div className="history-table-row" role="row" key={point.timestamp}>
              <span>
                {new Date(point.timestamp).toLocaleString('en-US', {
                  month: 'short',
                  day: 'numeric',
                  hour: 'numeric',
                  minute: '2-digit',
                })}
              </span>
              <strong>{point.valueUsd === null ? '—' : `$${point.valueUsd.toFixed(2)}`}</strong>
              <span>
                {point.weeklyUsedPercent === null ? '—' : `${Math.round(point.weeklyUsedPercent)}%`}
              </span>
              <code>{point.dominantModel ?? 'unknown'}</code>
              <span className={`table-status ${point.isFinalized ? 'finalized' : 'settling'}`}>
                {point.isFinalized ? 'Finalized' : 'Settling'}
              </span>
            </div>
          ))}
        </div>
      </div>
      <div className="pre-nerfify-note">
        <Icon name="info" size={18} />
        <span>
          <strong>Pre-Nerfify usage history</strong> may be available for cost totals, but
          weekly-value quotes begin only with a reliable paired cost and quota measurement.
        </span>
      </div>
    </section>
  );
}
