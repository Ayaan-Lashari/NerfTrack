import { useState } from 'react';
import type { AppSettings, CustomPriceOverride } from '../domain';
import { Icon } from './Icons';

interface SettingsViewProps {
  settings: AppSettings;
  detectedModelIds: string[];
  onChange: (key: keyof AppSettings, value: number | boolean) => void;
  onCustomPricingChange: (prices: CustomPriceOverride[]) => Promise<void>;
  onResetAllData: () => Promise<void>;
  onRestoreLastCheckpoint: () => Promise<void>;
  onImportAllData: () => Promise<void>;
  onOpenStarterPage: () => void;
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
    description: 'Record a collection interruption for diagnostics.',
    type: 'number',
    min: 1,
    max: 30,
    suffix: ' min',
  },
  {
    key: 'reducedMotion',
    label: 'Reduced motion',
    description: 'Disable estimate-finalization animations and motion cues.',
    type: 'toggle',
  },
];

const createPricingOverride = (modelId = ''): CustomPriceOverride => ({
  modelId,
  alias: null,
  inputUsdPerMillion: 0,
  cachedInputUsdPerMillion: 0,
  outputUsdPerMillion: 0,
});

export function SettingsView({
  settings,
  detectedModelIds,
  onChange,
  onCustomPricingChange,
  onResetAllData,
  onRestoreLastCheckpoint,
  onImportAllData,
  onOpenStarterPage,
}: SettingsViewProps) {
  const [dataAction, setDataAction] = useState<
    'idle' | 'resetting' | 'restoring-checkpoint' | 'importing-all'
  >('idle');
  const [confirmReset, setConfirmReset] = useState(false);
  const [dataMessage, setDataMessage] = useState<string | null>(null);
  const [pricingDraft, setPricingDraft] = useState<CustomPriceOverride[]>(settings.customPricing);
  const [pricingError, setPricingError] = useState<string | null>(null);
  const [pricingSaving, setPricingSaving] = useState(false);
  const draftModelIds = new Set(pricingDraft.map((price) => price.modelId.trim().toLowerCase()));
  const availableDetectedModelIds = detectedModelIds.filter(
    (modelId) => modelId.trim() && !draftModelIds.has(modelId.trim().toLowerCase()),
  );

  const autofillDetectedModels = () => {
    setPricingDraft((current) => {
      const existing = new Set(current.map((price) => price.modelId.trim().toLowerCase()));
      const additions = detectedModelIds
        .filter((modelId) => modelId.trim() && !existing.has(modelId.trim().toLowerCase()))
        .map((modelId) => createPricingOverride(modelId));
      return additions.length ? [...current, ...additions] : current;
    });
    setPricingError(null);
  };

  const savePricing = async () => {
    for (const price of pricingDraft) {
      if (!price.modelId.trim()) return setPricingError('Each override needs a model ID.');
      if (
        [price.inputUsdPerMillion, price.cachedInputUsdPerMillion, price.outputUsdPerMillion].some(
          (value) => !Number.isFinite(value) || value < 0,
        )
      ) {
        return setPricingError('Prices must be finite, non-negative USD amounts.');
      }
    }
    setPricingSaving(true);
    setPricingError(null);
    try {
      await onCustomPricingChange(pricingDraft);
    } catch {
      setPricingError('Could not save local pricing overrides.');
    } finally {
      setPricingSaving(false);
    }
  };

  const runDataAction = async (action: 'resetting' | 'restoring-checkpoint' | 'importing-all') => {
    if (dataAction !== 'idle') return;
    setDataAction(action);
    setDataMessage(null);
    try {
      if (action === 'resetting') await onResetAllData();
      else if (action === 'restoring-checkpoint') await onRestoreLastCheckpoint();
      else await onImportAllData();
      setDataMessage(
        action === 'resetting'
          ? 'Local data reset. Current weekly allowance synced; monitoring new Codex activity.'
          : action === 'restoring-checkpoint'
            ? 'Pre-reset graph restored and updated with activity since the reset.'
            : 'Full import complete. All available Codex logs were indexed.',
      );
    } catch (error) {
      const actionLabel =
        action === 'resetting'
          ? 'reset local data'
          : action === 'restoring-checkpoint'
            ? 'restore the last checkpoint'
            : 'import all log data';
      setDataMessage(`Unable to ${actionLabel}: ${String(error)}`);
    } finally {
      setDataAction('idle');
    }
  };

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
            NerfTrack runs locally. No prompts, code, raw account identifiers, or telemetry leave
            this device; only public pricing metadata is refreshed at launch.
          </p>
          <div className="privacy-check">
            <Icon name="check" size={17} />
            Local-only storage
          </div>
          <div className="privacy-check">
            <Icon name="check" size={17} />
            GitHub Releases checks stay opt-in to configuration
          </div>
          <div className="privacy-check">
            <Icon name="check" size={17} />
            Token prices stay local
          </div>
        </div>
      </div>
      <section className="panel custom-pricing-panel" aria-labelledby="custom-pricing-heading">
        <div className="panel-heading">
          <Icon name="settings" size={23} />
          <h2 id="custom-pricing-heading">Custom API pricing</h2>
        </div>
        <p>
          Overrides are local only and take precedence over models.dev and NerfTrack’s embedded
          fallback rates. Use them for an unpriced model or a local alias; prices are USD per 1M
          tokens.
        </p>
        <p className="settings-note">
          NerfTrack refreshes the public models.dev OpenAI catalog at each launch and caches the
          last valid response. If it is unavailable, embedded OpenAI fallback rates are used.
          Unknown models remain pending until a catalog price or override is available.
        </p>
        {detectedModelIds.length > 0 && (
          <div className="detected-models-callout">
            <div className="detected-models-copy">
              <Icon name={availableDetectedModelIds.length > 0 ? 'activity' : 'check'} size={18} />
              <div>
                <strong>
                  {availableDetectedModelIds.length > 0
                    ? `${availableDetectedModelIds.length} unpriced ${
                        availableDetectedModelIds.length === 1 ? 'model' : 'models'
                      } detected`
                    : 'All detected models are in this draft'}
                </strong>
                <span>
                  {availableDetectedModelIds.length > 0
                    ? availableDetectedModelIds.join(' · ')
                    : 'Review the prices below, then save when ready.'}
                </span>
              </div>
            </div>
            {availableDetectedModelIds.length > 0 && (
              <button
                type="button"
                className="detected-models-button"
                onClick={autofillDetectedModels}
              >
                Autofill detected {availableDetectedModelIds.length === 1 ? 'model' : 'models'}
              </button>
            )}
          </div>
        )}
        <div className="custom-price-grid" role="group" aria-label="Custom API pricing overrides">
          {pricingDraft.map((price, index) => (
            <div className="custom-price-row" key={`${price.modelId}-${index}`}>
              <label>
                Model ID
                <input
                  aria-label={`Model ID ${index + 1}`}
                  value={price.modelId}
                  onChange={(event) =>
                    setPricingDraft((current) =>
                      current.map((item, itemIndex) =>
                        itemIndex === index ? { ...item, modelId: event.target.value } : item,
                      ),
                    )
                  }
                />
              </label>
              <label>
                Alias
                <input
                  aria-label={`Alias ${index + 1}`}
                  value={price.alias ?? ''}
                  onChange={(event) =>
                    setPricingDraft((current) =>
                      current.map((item, itemIndex) =>
                        itemIndex === index ? { ...item, alias: event.target.value || null } : item,
                      ),
                    )
                  }
                />
              </label>
              {(
                ['inputUsdPerMillion', 'cachedInputUsdPerMillion', 'outputUsdPerMillion'] as const
              ).map((field) => (
                <label key={field}>
                  {field === 'inputUsdPerMillion'
                    ? 'Input'
                    : field === 'cachedInputUsdPerMillion'
                      ? 'Cached input'
                      : 'Output'}
                  <input
                    aria-label={`${field} ${index + 1}`}
                    type="number"
                    min="0"
                    step="any"
                    value={price[field]}
                    onChange={(event) =>
                      setPricingDraft((current) =>
                        current.map((item, itemIndex) =>
                          itemIndex === index
                            ? { ...item, [field]: Number(event.target.value) }
                            : item,
                        ),
                      )
                    }
                  />
                </label>
              ))}
              <button
                type="button"
                className="text-button"
                onClick={() =>
                  setPricingDraft((current) =>
                    current.filter((_, itemIndex) => itemIndex !== index),
                  )
                }
              >
                Remove
              </button>
            </div>
          ))}
        </div>
        {pricingError && (
          <p className="settings-error" role="alert">
            {pricingError}
          </p>
        )}
        <div className="custom-price-actions">
          <button
            type="button"
            onClick={() => setPricingDraft((current) => [...current, createPricingOverride()])}
          >
            Add override
          </button>
          <button
            type="button"
            className="data-action-button"
            disabled={pricingSaving}
            onClick={() => void savePricing()}
          >
            {pricingSaving ? 'Saving…' : 'Save pricing'}
          </button>
        </div>
      </section>
      <div className="panel defaults-panel">
        <div>
          <span className="settings-kicker">V1 defaults</span>
          <strong>English · USD · dark appearance</strong>
        </div>
        <span className="algorithm-badge">Token estimator v3 · local API-equivalent pricing</span>
      </div>
      <section className="panel starter-settings-panel" aria-labelledby="starter-settings-heading">
        <div>
          <span className="settings-kicker">Community</span>
          <h2 id="starter-settings-heading">Starter page</h2>
          <p>
            Reopen the GitHub support page any time. NerfTrack will remember that you have seen it
            again after you continue.
          </p>
        </div>
        <button type="button" className="data-action-button" onClick={onOpenStarterPage}>
          <Icon name="refresh" size={15} />
          Open starter page again
        </button>
      </section>
      <section className="panel data-management-panel" aria-labelledby="data-management-heading">
        <div className="panel-heading">
          <Icon name="history" size={23} />
          <h2 id="data-management-heading">Data management</h2>
        </div>
        <p className="data-management-intro">
          Reset local history without touching Codex logs. A fast checkpoint is saved immediately
          before every reset, while a full import can rebuild the index from every readable log.
        </p>
        <div className="data-action-grid">
          <div className="data-action-copy">
            <strong>Reset all data</strong>
            <span>
              Clear imported usage, quota observations, graphs, diagnostics, and annotations. New
              Codex activity is monitored immediately after reset.
            </span>
            <button
              className="data-action-button danger"
              disabled={dataAction !== 'idle'}
              onClick={() => {
                setDataMessage(null);
                setConfirmReset(true);
              }}
            >
              Reset all data
            </button>
          </div>
          <div className="data-action-copy">
            <div className="data-action-heading">
              <Icon name="history" size={17} />
              <strong>Restore from last checkpoint</strong>
            </div>
            <span>
              Restore the graph captured immediately before the latest reset, then index only newer
              activity. This is the fastest recovery option.
            </span>
            <button
              className="data-action-button"
              disabled={dataAction !== 'idle'}
              onClick={() => void runDataAction('restoring-checkpoint')}
            >
              <Icon name="refresh" size={15} />
              {dataAction === 'restoring-checkpoint'
                ? 'Restoring checkpoint…'
                : 'Restore last checkpoint'}
            </button>
          </div>
          <div className="data-action-copy">
            <div className="data-action-heading">
              <Icon name="refresh" size={17} />
              <strong>Import all data</strong>
            </div>
            <span>
              Clear the import index and re-read every available Codex log from the beginning. Best
              for a complete rebuild; may take longer.
            </span>
            <button
              className="data-action-button secondary"
              disabled={dataAction !== 'idle'}
              onClick={() => void runDataAction('importing-all')}
            >
              <Icon name="refresh" size={15} />
              {dataAction === 'importing-all' ? 'Importing all data…' : 'Import all data'}
            </button>
          </div>
        </div>
        {confirmReset && dataAction === 'idle' && (
          <div
            className="data-confirmation"
            role="alertdialog"
            aria-labelledby="reset-confirm-heading"
          >
            <div>
              <strong id="reset-confirm-heading">Reset all local data?</strong>
              <span>
                This clears NerfTrack’s imported usage, quota observations, graph history,
                diagnostics, annotations, and scan checkpoints. Codex source logs are not deleted;
                monitoring resumes from the current end of those logs.
              </span>
            </div>
            <div className="data-confirmation-actions">
              <button className="data-action-button quiet" onClick={() => setConfirmReset(false)}>
                Cancel
              </button>
              <button
                className="data-action-button danger"
                onClick={() => {
                  setConfirmReset(false);
                  void runDataAction('resetting');
                }}
              >
                Confirm reset
              </button>
            </div>
          </div>
        )}
        {dataMessage && (
          <p className="data-action-message" role="status">
            {dataMessage}
          </p>
        )}
      </section>
    </section>
  );
}
