import { useState } from 'react';
import starterPortrait from '../assets/starter-portrait.png';
import { GITHUB_REPOSITORY_URL } from '../lib/config';
import { openExternalUrl } from '../lib/updater';
import { Icon, LogoMark } from './Icons';

interface StarterPageProps {
  version: string;
  onComplete: () => Promise<void>;
}

export function StarterPage({ version, onComplete }: StarterPageProps) {
  const [githubOpened, setGithubOpened] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const openGithub = async () => {
    setError(null);
    if (!GITHUB_REPOSITORY_URL) {
      setError('The NerfTrack GitHub repository URL has not been configured yet.');
      return;
    }
    setBusy(true);
    try {
      await openExternalUrl(GITHUB_REPOSITORY_URL);
      setGithubOpened(true);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  };

  const continueToApp = async () => {
    if (busy) return;
    setError(null);
    setBusy(true);
    try {
      await onComplete();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      setBusy(false);
    }
  };

  return (
    <main className="starter-page" aria-labelledby="starter-heading">
      <header className="starter-top">
        <div className="starter-brand">
          <LogoMark size={28} />
          <span>NerfTrack</span>
        </div>
        <span className="starter-version">v{version}</span>
      </header>

      <section className="starter-content">
        <div className="starter-copy">
          <span className="starter-kicker">BEFORE YOU BEGIN · 01</span>
          <h1 id="starter-heading">Help NerfTrack keep going.</h1>
          <p>
            NerfTrack is built in the open. If it helps you understand your weekly usage, star the
            repository on GitHub so the project can keep growing.
          </p>

          <div className="starter-actions">
            <button
              type="button"
              className={`starter-github-button ${githubOpened ? 'is-complete' : ''}`}
              disabled={busy || !GITHUB_REPOSITORY_URL}
              onClick={() => void openGithub()}
            >
              <span className="starter-action-icon">
                <Icon name="github" size={19} strokeWidth={1.5} />
              </span>
              <span>
                <strong>{githubOpened ? 'GitHub opened' : 'Star NerfTrack on GitHub'}</strong>
                <small>
                  {GITHUB_REPOSITORY_URL
                    ? githubOpened
                      ? 'Thanks for supporting the project.'
                      : 'Open the repository and leave a star.'
                    : 'The repository link will appear here once configured.'}
                </small>
              </span>
              <Icon name={githubOpened ? 'check' : 'external'} size={17} />
            </button>
            {githubOpened && (
              <p className="starter-confirmation" role="status">
                <Icon name="check" size={16} /> Ready when you are.
              </p>
            )}
            {error && (
              <p className="starter-error" role="alert">
                {error}
              </p>
            )}
          </div>

          <div className="starter-skip">
            <span className="starter-skip-face" aria-hidden="true">
              ☹
            </span>
            <span className="starter-skip-copy">
              <strong>Not ready to star?</strong>
              <small>The little star will be sad, but you can still continue.</small>
            </span>
            <button
              type="button"
              className="starter-skip-button"
              disabled={busy}
              onClick={() => void continueToApp()}
            >
              Continue without starring
            </button>
          </div>
        </div>

        <div className="starter-visual" aria-hidden="true">
          <div className="starter-glow" />
          <img src={starterPortrait} alt="" />
          <span className="starter-visual-line" />
        </div>
      </section>

      <footer className="starter-footer">
        <div className="starter-tagline-wrap">
          <span className="starter-tagline-accent" />
          <strong>Let the resets continue</strong>
          <span className="starter-tagline-caption">Keep a clear eye on the week ahead.</span>
        </div>
        <button
          type="button"
          className="starter-continue-button"
          disabled={!githubOpened || busy}
          onClick={() => void continueToApp()}
        >
          {busy ? 'Saving…' : 'Continue to NerfTrack'}
          <Icon name="chevron-right" size={17} />
        </button>
      </footer>
    </main>
  );
}
