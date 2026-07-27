import type { AppSettings, AppStatus } from '../domain';
import { Icon, type IconName } from './Icons';

interface SetupViewProps {
  status: AppStatus;
  settings: AppSettings;
  onChooseHome: () => void;
  onChooseExecutable: () => void;
  onRetry: () => void;
  onStart: () => void;
  onSettingChange: (key: keyof AppSettings, value: number | boolean) => void;
}

const discoveryCards: Array<{
  key: 'codexHome' | 'codexExecutable' | 'appServer';
  title: string;
  icon: IconName;
  action?: string;
}> = [
  { key: 'codexHome', title: 'Codex data folder', icon: 'folder', action: 'Choose folder' },
  {
    key: 'codexExecutable',
    title: 'Codex executable',
    icon: 'terminal',
    action: 'Choose executable',
  },
  { key: 'appServer', title: 'App Server', icon: 'server', action: 'Test connection' },
];

const settingsRows: Array<{
  key: keyof AppSettings;
  icon: IconName;
  title: string;
  description: string;
  options: number[];
  suffix: string;
}> = [
  {
    key: 'refreshIntervalSeconds',
    icon: 'clock',
    title: 'Refresh interval',
    description: 'How often Nerfify checks for new Codex usage.',
    options: [10, 20, 30],
    suffix: ' seconds',
  },
];

export function SetupView({
  status,
  settings,
  onChooseHome,
  onChooseExecutable,
  onRetry,
  onStart,
  onSettingChange,
}: SetupViewProps) {
  const guiMode = status.integrationMode === 'gui';

  return (
    <section className="setup-page page-shell">
      <header className="page-heading">
        <h1>Set up Nerfify</h1>
        <p>
          {guiMode
            ? 'Connect local Codex desktop data to estimate weekly API-equivalent value from tokens.'
            : 'Connect local Codex data from the desktop app or CLI to estimate weekly API-equivalent value from tokens.'}
        </p>
      </header>
      <div className="discovery-grid">
        {discoveryCards.map((card) => {
          const discovery = status[card.key];
          const title =
            card.key === 'codexExecutable' && guiMode
              ? 'Codex CLI executable'
              : card.key === 'appServer' && guiMode
                ? 'App Server (CLI only)'
                : card.title;
          const label =
            card.key === 'codexExecutable' && guiMode
              ? 'Choose CLI executable'
              : card.key === 'appServer' && guiMode
                ? 'Use CLI instead'
                : card.action;
          const action =
            card.key === 'codexHome'
              ? onChooseHome
              : card.key === 'codexExecutable'
                ? onChooseExecutable
                : guiMode
                  ? onChooseExecutable
                  : onRetry;
          const isAlert = discovery.state === 'missing' || discovery.state === 'unsupported';
          const discoveryPath =
            discovery.redactedLocation ??
            (discovery.state === 'not_required'
              ? 'Not required in desktop mode'
              : 'Not discovered yet');
          return (
            <article className="discovery-card" key={card.key}>
              <div className="discovery-title-row">
                <div className="discovery-icon">
                  <Icon name={card.icon} size={28} />
                </div>
                <div>
                  <h2>{title}</h2>
                  <p className={`discovery-state ${isAlert ? 'missing' : ''}`}>
                    <Icon name={isAlert ? 'alert' : 'check'} size={17} />
                    {discovery.message}
                  </p>
                </div>
              </div>
              <span className="discovery-path">{discoveryPath}</span>
              {label && (
                <button className="quiet-button discovery-action" onClick={action}>
                  {label}
                  <Icon name="chevron" size={15} />
                </button>
              )}
            </article>
          );
        })}
      </div>
      <div className="panel monitoring-panel">
        <div className="panel-heading">
          <Icon name="settings" size={23} />
          <h2>Monitoring settings</h2>
        </div>
        <div className="setting-rows">
          {settingsRows.map((row) => (
            <div className="setting-row" key={row.key}>
              <div className="setting-row-icon">
                <Icon name={row.icon} size={25} />
              </div>
              <div className="setting-copy">
                <strong>{row.title}</strong>
                <span>{row.description}</span>
              </div>
              <label className="select-wrap">
                <span className="sr-only">{row.title}</span>
                <select
                  value={settings[row.key] as number}
                  onChange={(event) => onSettingChange(row.key, Number(event.target.value))}
                >
                  {row.options.map((option) => (
                    <option key={option} value={option}>
                      {option}
                      {row.suffix}
                    </option>
                  ))}
                </select>
                <Icon name="chevron" size={16} />
              </label>
            </div>
          ))}
        </div>
      </div>
      <div className="privacy-panel panel">
        <div className="privacy-icon">
          <Icon name="shield" size={36} strokeWidth={1.5} />
        </div>
        <div>
          <h2>Local-only</h2>
          <p>
            All processing and data storage happen only on this machine.
            <br />
            No data leaves your device.
          </p>
        </div>
        <span className="local-badge">
          <Icon name="lock" size={17} />
          100% Local
        </span>
      </div>
      <div className="setup-actions">
        <button className="primary-button" onClick={onStart}>
          <Icon name="play" size={21} />
          Start monitoring
        </button>
        <button className="secondary-button" onClick={onRetry}>
          <Icon name="refresh" size={21} />
          Retry detection
        </button>
        <button className="help-button">
          <span className="help-circle">?</span>
          <span>
            <strong>Need help?</strong>
            <small>View troubleshooting guide</small>
          </span>
          <Icon name="external" size={17} />
        </button>
      </div>
    </section>
  );
}
