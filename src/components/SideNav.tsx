import type { AppStatus, NavKey } from '../domain';
import { Icon, LogoMark, type IconName } from './Icons';

interface SideNavProps {
  active: NavKey;
  status: AppStatus;
  onNavigate: (key: NavKey) => void;
}

const navItems: Array<{ key: NavKey; label: string; icon: IconName }> = [
  { key: 'home', label: 'Home', icon: 'home' },
  { key: 'setup', label: 'Setup', icon: 'settings' },
  { key: 'diagnostics', label: 'Diagnostics', icon: 'activity' },
  { key: 'history', label: 'History', icon: 'history' },
  { key: 'settings', label: 'Settings', icon: 'settings' },
];

export function SideNav({ active, status, onNavigate }: SideNavProps) {
  const isConnected = status.state === 'connected';
  return (
    <aside className="side-nav">
      <div className="brand" aria-label="NerfTrack">
        <LogoMark size={28} />
        <span>NerfTrack</span>
      </div>
      <nav aria-label="Primary">
        {navItems.map((item) => (
          <button
            key={item.key}
            className={`nav-item ${active === item.key ? 'active' : ''}`}
            onClick={() => onNavigate(item.key)}
            aria-current={active === item.key ? 'page' : undefined}
          >
            <Icon name={item.icon} size={23} />
            <span>{item.label}</span>
          </button>
        ))}
      </nav>
      <div className="side-nav-bottom">
        <button className="connection-card" onClick={() => onNavigate('setup')}>
          <span className={`status-dot ${isConnected ? 'good' : 'warn'}`} />
          <span className="connection-copy">
            <strong>{status.label}</strong>
            <span>{status.detail}</span>
          </span>
          <Icon name="chevron" size={17} />
        </button>
        <span className="app-version">v0.1.0</span>
      </div>
    </aside>
  );
}
