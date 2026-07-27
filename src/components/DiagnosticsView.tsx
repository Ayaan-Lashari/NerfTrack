import type { DiagnosticsSummary } from '../domain';
import { Icon } from './Icons';

export function DiagnosticsView({ diagnostics }: { diagnostics: DiagnosticsSummary }) {
  const rows = [
    ['Events observed', diagnostics.totalEvents.toLocaleString(), 'activity'],
    ['Priced token events', diagnostics.pricedEvents.toLocaleString(), 'check'],
    ['Pricing pending', diagnostics.pendingEvents.toLocaleString(), 'clock'],
    ['Rejected observations', diagnostics.rejectedEvents.toLocaleString(), 'alert'],
    ['Partial-line retries', diagnostics.partialLineRetries.toLocaleString(), 'refresh'],
    ['Monitoring gaps', diagnostics.monitoringGaps.toLocaleString(), 'history'],
  ] as const;
  return (
    <section className="page-shell diagnostics-page">
      <header className="page-heading">
        <h1>Diagnostics</h1>
        <p>Aggregate health signals for local collection and estimation.</p>
      </header>
      <div className="diagnostics-summary-grid">
        {rows.map(([label, value, icon]) => (
          <div className="diagnostic-stat" key={label}>
            <Icon name={icon} size={21} />
            <span>{label}</span>
            <strong>{value}</strong>
          </div>
        ))}
      </div>
      <div className="diagnostics-columns">
        <div className="panel diagnostics-list">
          <div className="panel-heading">
            <Icon name="alert" size={23} />
            <h2>Quality reasons</h2>
          </div>
          {diagnostics.reasons.map((item) => (
            <div className="reason-row" key={item.reason}>
              <span>{item.reason}</span>
              <strong>{item.count}</strong>
            </div>
          ))}
        </div>
        <div className="panel diagnostics-list">
          <div className="panel-heading">
            <Icon name="chart" size={23} />
            <h2>Models observed</h2>
          </div>
          {diagnostics.modelIds.map((model) => (
            <div className="model-row" key={model}>
              <span className="model-dot" />
              <code>{model}</code>
              <span className="model-status">eligible evidence</span>
            </div>
          ))}
          <div className="privacy-note">
            <Icon name="lock" size={17} />
            {diagnostics.privacy}
          </div>
        </div>
      </div>
      <div className="panel diagnostic-callout">
        <Icon name="info" size={20} />
        <div>
          <strong>
            Diagnostics never include prompts, account identifiers, or full local paths.
          </strong>
          <span>
            Use this page to identify unpriced models, reset boundaries, and
            data-quality interruptions before relying on an estimate.
          </span>
        </div>
      </div>
    </section>
  );
}
