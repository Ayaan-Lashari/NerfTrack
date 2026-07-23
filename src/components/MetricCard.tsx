import type { ReactNode } from 'react';
import { Icon, type IconName } from './Icons';

interface MetricCardProps {
  icon: IconName;
  iconTone: 'green' | 'blue' | 'purple' | 'lime';
  label: string;
  value: string;
  detail: string;
  children?: ReactNode;
}

export function MetricCard({ icon, iconTone, label, value, detail, children }: MetricCardProps) {
  return (
    <article className="metric-card">
      <div className={`metric-icon metric-icon-${iconTone}`}>
        <Icon name={icon} size={27} strokeWidth={1.7} />
      </div>
      <div className="metric-copy">
        <span className="metric-label">{label}</span>
        <strong>{value}</strong>
        <span className="metric-detail">{detail}</span>
      </div>
      {children}
    </article>
  );
}

export function UsageRing({ value }: { value: number | null }) {
  const safe = value ?? 0;
  return (
    <div
      className="usage-ring"
      style={{ '--ring-value': `${safe * 3.6}deg` } as React.CSSProperties}
    >
      <div className="usage-ring-inner">{value === null ? '—' : `${Math.round(value)}%`}</div>
    </div>
  );
}
