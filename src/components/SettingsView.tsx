import type { AppSettings } from '../domain';
import { Icon } from './Icons';

interface SettingsViewProps {
  settings: AppSettings;
  onChange: (key: keyof AppSettings, value: number | boolean) => void;
}

const advancedRows: Array<{
  key: keyof AppSettings;
  label: string;
  description: string;
  type: 'number' | 'toggle';
  min?: number;
  max?: number;
  step?: number;
  suffix?: string;
}> = [
  {
    key: 'reconciliationIntervalHours',
    label: 'Reconciliation interval',
    description: 'Re-scan known files and recover missed notifications.',
    type: 'number',
    min: 1,
    max: 24,
    suffix: ' h',
  },
  {
    key: 'monitoringGapMinutes',
    label: 'Monitoring gap threshold',
    description: 'Start a new estimator epoch after this interruption.',
    type: 'number',
    min: 1,
    max: 30,
    suffix: ' min',
  },
  {
    key: 'minimumEligibleCostUsd',
    label: 'Minimum eligible cost',
    description: 'Required priced cost before a quote can settle.',
    type: 'number',
    min: 0.25,
    max: 20,
    step: 0.25,
    suffix: ' USD',
  },
  {
    key: 'minimumEvents',
    label: 'Minimum events',
    description: 'Required eligible events in a settled interval.',
    type: 'number',
    min: 1,
    max: 10,
    suffix: ' events',
  },
  {
    key: 'reducedMotion',
    label: 'Reduced motion',
    description: 'Disable quote-finalization animations and motion cues.',
    type: 'toggle',
  },
];

export function SettingsView({ settings, onChange }: SettingsViewProps) {
  return (
    <section className="page-shell settings-page">
      <header className="page-heading">
        <h1>Settings</h1>
        <p>Local defaults for monitoring, privacy, and presentation.</p>
      </header>
      <div className="settings-layout">
        <div className="panel settings-panel">
          <div className="panel-heading">
            <Icon name="settings" size={23} />
            <h2>Advanced monitoring</h2>
          </div>
          {advancedRows.map((row) => (
            <div className="advanced-row" key={row.key}>
              <div className="advanced-copy">
                <strong>{row.label}</strong>
                <span>{row.description}</span>
              </div>
              {row.type === 'toggle' ? (
                <button
                  className={`toggle ${settings[row.key] ? 'on' : ''}`}
                  role="switch"
                  aria-checked={Boolean(settings[row.key])}
                  onClick={() => onChange(row.key, !settings[row.key])}
                >
                  <span />
                </button>
              ) : (
                <label className="number-input">
                  <span className="sr-only">{row.label}</span>
                  <input
                    type="number"
                    min={row.min}
                    max={row.max}
                    step={row.step ?? 1}
                    value={settings[row.key] as number}
                    onChange={(event) => onChange(row.key, Number(event.target.value))}
                  />
                  <em>{row.suffix}</em>
                </label>
              )}
            </div>
          ))}
        </div>
        <div className="panel privacy-settings-panel">
          <div className="privacy-large-icon">
            <Icon name="lock" size={29} />
          </div>
          <h2>Privacy first</h2>
          <p>
            Nerfify runs locally. No prompts, code, raw account identifiers, or telemetry leave this
            device.
          </p>
          <div className="privacy-check">
            <Icon name="check" size={17} />
            Local-only storage
          </div>
          <div className="privacy-check">
            <Icon name="check" size={17} />
            No auto-updater in V1
          </div>
          <div className="privacy-check">
            <Icon name="check" size={17} />
            No historical repricing
          </div>
        </div>
      </div>
      <div className="panel defaults-panel">
        <div>
          <span className="settings-kicker">V1 defaults</span>
          <strong>English · USD · dark appearance</strong>
        </div>
        <span className="algorithm-badge">Algorithm defaults v1</span>
      </div>
    </section>
  );
}
