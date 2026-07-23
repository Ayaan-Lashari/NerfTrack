import type { ReactNode } from 'react';

export type IconName =
  | 'activity'
  | 'alert'
  | 'calendar'
  | 'chart'
  | 'check'
  | 'chevron'
  | 'clock'
  | 'external'
  | 'folder'
  | 'heart'
  | 'history'
  | 'home'
  | 'info'
  | 'lock'
  | 'more'
  | 'play'
  | 'refresh'
  | 'server'
  | 'settings'
  | 'shield'
  | 'terminal'
  | 'trend';

interface IconProps {
  name: IconName;
  size?: number;
  strokeWidth?: number;
  className?: string;
}

const base = {
  fill: 'none',
  stroke: 'currentColor',
  strokeLinecap: 'round' as const,
  strokeLinejoin: 'round' as const,
};

export function Icon({ name, size = 24, strokeWidth = 1.8, className }: IconProps) {
  const props = {
    ...base,
    width: size,
    height: size,
    viewBox: '0 0 24 24',
    strokeWidth,
    className,
  };
  let content: ReactNode;

  switch (name) {
    case 'activity':
      content = <path d="M2.5 12h4l2.2-7 4.1 14 2.5-7h6.2" />;
      break;
    case 'alert':
      content = (
        <>
          <path d="M12 3.2 21 19a1.3 1.3 0 0 1-1.1 1.9H4.1A1.3 1.3 0 0 1 3 19L12 3.2Z" />
          <path d="M12 8v5.2M12 17.1v.2" />
        </>
      );
      break;
    case 'calendar':
      content = (
        <>
          <rect x="3.5" y="5.3" width="17" height="15" rx="2" />
          <path d="M7.5 3.3v4M16.5 3.3v4M3.5 9.4h17" />
        </>
      );
      break;
    case 'chart':
      content = (
        <>
          <path d="M4 19.5V4.5M4 19.5h16" />
          <path d="m7 15 3.2-4 3 2.2 4.8-6" />
          <path d="M15.8 7.2H18v2.2" />
        </>
      );
      break;
    case 'check':
      content = <path d="m5.1 12.4 4.2 4.2 9.6-9.2" />;
      break;
    case 'chevron':
      content = <path d="m7.5 9.5 4.5 4.5 4.5-4.5" />;
      break;
    case 'clock':
      content = (
        <>
          <circle cx="12" cy="12" r="8.7" />
          <path d="M12 7v5.2l3.6 2.1" />
        </>
      );
      break;
    case 'external':
      content = (
        <>
          <path d="M14 5h5v5M19 5l-8 8" />
          <path d="M18 13.5v4a1.5 1.5 0 0 1-1.5 1.5h-10A1.5 1.5 0 0 1 5 17.5v-10A1.5 1.5 0 0 1 6.5 6h4" />
        </>
      );
      break;
    case 'folder':
      content = (
        <path d="M3.5 7.5a2 2 0 0 1 2-2h4l2 2h7a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2v-9Z" />
      );
      break;
    case 'heart':
      content = (
        <path d="M20.8 8.8c0 4.7-8.8 10-8.8 10s-8.8-5.3-8.8-10A4.6 4.6 0 0 1 12 6a4.6 4.6 0 0 1 8.8 2.8Z" />
      );
      break;
    case 'history':
      content = (
        <>
          <path d="M4.6 8.6A8.2 8.2 0 1 1 4 13" />
          <path d="M4.7 4.7v4h4M12 7.5v4.9l3 1.8" />
        </>
      );
      break;
    case 'home':
      content = (
        <>
          <path d="m3.7 10.7 8.3-6.6 8.3 6.6v8.1a1.7 1.7 0 0 1-1.7 1.7H5.4a1.7 1.7 0 0 1-1.7-1.7v-8.1Z" />
          <path d="M9.2 20.2v-5.8h5.6v5.8" />
        </>
      );
      break;
    case 'info':
      content = (
        <>
          <circle cx="12" cy="12" r="9" />
          <path d="M12 10.7v5.2M12 7.7v.2" />
        </>
      );
      break;
    case 'lock':
      content = (
        <>
          <rect x="4.2" y="10" width="15.6" height="10" rx="2" />
          <path d="M7.5 10V7.8a4.5 4.5 0 0 1 9 0V10" />
          <path d="M12 14v2.2" />
        </>
      );
      break;
    case 'more':
      content = (
        <>
          <circle cx="5" cy="12" r="1" fill="currentColor" stroke="none" />
          <circle cx="12" cy="12" r="1" fill="currentColor" stroke="none" />
          <circle cx="19" cy="12" r="1" fill="currentColor" stroke="none" />
        </>
      );
      break;
    case 'play':
      content = <path d="m8 5.5 10 6.5-10 6.5v-13Z" />;
      break;
    case 'refresh':
      content = (
        <>
          <path d="M19.1 8.8A7.4 7.4 0 0 0 6.2 7L4.4 8.8" />
          <path d="M4.4 4.8v4h4M4.9 15.2A7.4 7.4 0 0 0 17.8 17l1.8-1.8" />
          <path d="M19.6 19.2v-4h-4" />
        </>
      );
      break;
    case 'server':
      content = (
        <>
          <rect x="4" y="4" width="16" height="6" rx="1.8" />
          <rect x="4" y="14" width="16" height="6" rx="1.8" />
          <path d="M7.3 7h.1M7.3 17h.1M10.5 7h6M10.5 17h6" />
        </>
      );
      break;
    case 'settings':
      content = (
        <>
          <circle cx="12" cy="12" r="3.1" />
          <path d="m19.4 15 .1.1-1.3 2.2-.2-.1a2 2 0 0 0-3 .9v.3h-2.6v-.3a2 2 0 0 0-3-1l-.2.2-1.3-2.2.2-.1a2 2 0 0 0-1-3v-.1a2 2 0 0 0 1-3l-.2-.1 1.3-2.2.2.1a2 2 0 0 0 3-.9v-.3H15v.3a2 2 0 0 0 3 .9l.2-.1 1.3 2.2-.1.1a2 2 0 0 0 1 3v.1a2 2 0 0 0-1 3Z" />
        </>
      );
      break;
    case 'shield':
      content = (
        <path d="M12 3.5 19.5 6v5.3c0 4.4-3 7.8-7.5 9.2-4.5-1.4-7.5-4.8-7.5-9.2V6L12 3.5Z M8.3 12.3 10.7 15l5-5" />
      );
      break;
    case 'terminal':
      content = (
        <>
          <path d="m5.2 7.5 4.2 4.5-4.2 4.5M12 16.5h6.8" />
        </>
      );
      break;
    case 'trend':
      content = (
        <>
          <path d="M4 17.5 9.4 12l3.2 3.2L20 7.8" />
          <path d="M15.8 7.8H20v4.2" />
        </>
      );
      break;
  }

  return (
    <svg {...props} aria-hidden="true">
      {content}
    </svg>
  );
}

export function LogoMark({ size = 28 }: { size?: number }) {
  return (
    <svg width={size * 1.7} height={size} viewBox="0 0 48 28" fill="none" aria-hidden="true">
      <path
        d="M1.7 15.4h5.2l3.3-10.9 5.7 19.8 5.6-17.7 4 13.2 4.1-8.5h5.1"
        stroke="currentColor"
        strokeWidth="2.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <path
        d="M36 8.8v10.5M42 8.8v10.5"
        stroke="currentColor"
        strokeWidth="2.6"
        strokeLinecap="round"
      />
    </svg>
  );
}
