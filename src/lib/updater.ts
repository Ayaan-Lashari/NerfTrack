import { getVersion } from '@tauri-apps/api/app';
import { invoke } from '@tauri-apps/api/core';
import packageJson from '../../package.json';
import type {
  DownloadedUpdate,
  InstallUpdateResult,
  UpdateCheckResult,
  UpdateState,
} from '../domain';
import { isTauri } from './tauri';

export const CURRENT_APP_VERSION = packageJson.version;

export function initialUpdateState(): UpdateState {
  return {
    status: 'idle',
    currentVersion: CURRENT_APP_VERSION,
    latestVersion: null,
    releaseUrl: null,
    assetName: null,
    message: 'Update checks have not run yet.',
  };
}

export async function getCurrentAppVersion() {
  if (!isTauri()) return CURRENT_APP_VERSION;
  try {
    return await getVersion();
  } catch {
    return CURRENT_APP_VERSION;
  }
}

function noRepositoryResult(currentVersion: string): UpdateCheckResult {
  return {
    currentVersion,
    latestVersion: null,
    updateAvailable: false,
    releaseUrl: null,
    assetName: null,
    assetUrl: null,
    message: 'GitHub Releases updates are not configured yet.',
  };
}

export async function checkForUpdate(repositoryUrl: string): Promise<UpdateCheckResult> {
  const currentVersion = await getCurrentAppVersion();
  if (!repositoryUrl.trim()) return noRepositoryResult(currentVersion);
  if (!isTauri()) {
    return {
      ...noRepositoryResult(currentVersion),
      message: 'Update checks are available in the packaged NerfTrack desktop app.',
    };
  }
  return invoke<UpdateCheckResult>('check_for_update', { repositoryUrl });
}

export async function downloadUpdate(repositoryUrl: string) {
  if (!isTauri()) throw new Error('The desktop updater is not available in this preview.');
  return invoke<DownloadedUpdate>('download_update', { repositoryUrl });
}

export async function installUpdate(path: string) {
  if (!isTauri()) throw new Error('The desktop installer is not available in this preview.');
  return invoke<InstallUpdateResult>('install_update', { path });
}

export async function consumeUpdateFailure() {
  if (!isTauri()) return null;
  return invoke<string | null>('consume_update_failure');
}

export async function openExternalUrl(url: string) {
  if (!url.trim()) throw new Error('The NerfTrack GitHub URL has not been configured yet.');
  if (!isTauri()) {
    const opened = window.open(url, '_blank', 'noopener,noreferrer');
    if (!opened) throw new Error('The browser blocked the GitHub window.');
    return;
  }
  return invoke<void>('open_external_url', { url });
}
