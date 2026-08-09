import type { AppStatus, NavKey, UpdateState } from '../domain';
import { Icon, LogoMark, type IconName } from './Icons';

interface SideNavProps {
  active: NavKey;
  status: AppStatus;
  onNavigate: (key: NavKey) => void;
  updateState: UpdateState;
  onUpdate: () => void;
}

const navItems: Array<{ key: NavKey; label: string; icon: IconName }> = [
  { key: 'home', label: 'Home', icon: 'home' },
  { key: 'setup', label: 'Setup', icon: 'settings' },
  { key: 'diagnostics', label: 'Diagnostics', icon: 'activity' },
  { key: 'history', label: 'History', icon: 'history' },
  { key: 'settings', label: 'Settings', icon: 'settings' },
];

function updateLabel(updateState: UpdateState) {
  switch (updateState.status) {
    case 'checking':
      return 'Checking for updates';
    case 'available':
      return 'Update Available';
    case 'downloading':
      return 'Downloading…';
    case 'installing':
      return 'Installing…';
    case 'up-to-date':
      return 'Up to date';
    case 'failed':
      return 'Update failed';
    case 'not-configured':
      return 'Updates not configured';
    default:
      return 'Check for updates';
  }
}

export function SideNav({ active, status, onNavigate, updateState, onUpdate }: SideNavProps) {
  const isConnected = status.state === 'connected';
  const isBusy = ['checking', 'downloading', 'installing'].includes(updateState.status);
  const updateMessage = updateState.latestVersion
    ? `Installed v${updateState.currentVersion} · latest v${updateState.latestVersion}`
    : updateState.message;
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
        <div className="update-control">
          <button
            type="button"
            className={`update-button update-${updateState.status}`}
            disabled={isBusy}
            onClick={onUpdate}
            aria-label={updateLabel(updateState)}
            title={updateMessage}
          >
            <span className="update-button-label">
              <Icon name="refresh" size={17} />
              {updateLabel(updateState)}
            </span>
            {updateState.status === 'available' && updateState.latestVersion && (
              <span className="update-badge">v{updateState.latestVersion}</span>
            )}
          </button>
          <span className="update-message" aria-live="polite">
            {updateMessage}
          </span>
        </div>
        <span className="app-version">v{updateState.currentVersion}</span>
      </div>
    </aside>
  );
}
