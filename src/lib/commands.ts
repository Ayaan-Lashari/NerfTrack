import type {
  AdvancedSettings,
  Annotation,
  AppSettings,
  AppStatus,
  CurrentQuote,
  DiagnosticsSummary,
  HistoryResponse,
  Range,
  RedactedSelection,
} from '../domain';
import {
  defaultAdvancedSettings,
  demoAnnotations,
  demoDiagnostics,
  demoQuote,
  demoSettings,
  demoStatus,
  getDemoHistory,
} from './fixtures';
import { invokeOr } from './tauri';

export const getCurrentQuote = () => invokeOr<CurrentQuote | null>('get_current_quote', demoQuote);

export const getCurrentStatus = () => invokeOr<AppStatus>('get_current_status', demoStatus);

export const getHistory = (range: Range) =>
  invokeOr<HistoryResponse>('get_history', getDemoHistory(range), { range });

export const getAnnotations = () => invokeOr<Annotation[]>('get_annotations', demoAnnotations);

export const resetAnnotations = () => invokeOr<void>('reset_annotations', undefined);

export const getDiagnosticsSummary = () =>
  invokeOr<DiagnosticsSummary>('get_diagnostics_summary', demoDiagnostics);

export const retryDetection = () => invokeOr<AppStatus>('retry_detection', demoStatus);

const missingSelection: RedactedSelection = {
  selected: false,
  status: {
    state: 'missing',
    redactedLocation: null,
    message: 'No selection made',
  },
};

export const selectCodexHome = () =>
  invokeOr<RedactedSelection>('select_codex_home', missingSelection);

export const selectCodexExecutable = () =>
  invokeOr<RedactedSelection>('select_codex_executable', missingSelection);

export const getSettings = () => invokeOr<AppSettings>('get_settings', demoSettings);

export const updateSettings = (settings: AdvancedSettings) =>
  invokeOr<AppSettings>('update_settings', { ...demoSettings, ...settings }, { settings });

export { defaultAdvancedSettings };
