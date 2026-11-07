import type { SVGProps } from "react";

export type IconName =
  | "archive"
  | "check"
  | "chevronDown"
  | "clipboard"
  | "close"
  | "code"
  | "copy"
  | "file"
  | "history"
  | "image"
  | "keyboard"
  | "moon"
  | "paste"
  | "pause"
  | "pin"
  | "play"
  | "refresh"
  | "search"
  | "settings"
  | "shield"
  | "sun"
  | "text"
  | "trash"
  | "warning";

interface IconProps extends SVGProps<SVGSVGElement> {
  name: IconName;
  size?: number;
}

export function Icon({ name, size = 18, ...props }: IconProps) {
  const paths: Record<IconName, React.ReactNode> = {
    archive: <><path d="M4 7h16"/><path d="M5 7v12h14V7"/><path d="M3.5 3h17v4h-17z"/><path d="M9 11h6"/></>,
    check: <path d="m5 12 4 4L19 6" />,
    chevronDown: <path d="m7 10 5 5 5-5" />,
    clipboard: <><rect x="5" y="4" width="14" height="17" rx="2"/><path d="M9 4.5V3h6v1.5A1.5 1.5 0 0 1 13.5 6h-3A1.5 1.5 0 0 1 9 4.5Z"/><path d="M9 11h6M9 15h4"/></>,
    close: <path d="m6 6 12 12M18 6 6 18" />,
    code: <path d="m8 9-3 3 3 3M16 9l3 3-3 3M14 6l-4 12" />,
    copy: <><rect x="8" y="8" width="11" height="11" rx="2"/><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2"/></>,
    file: <><path d="M6 3h8l4 4v14H6z"/><path d="M14 3v5h5"/></>,
    history: <><path d="M4 12a8 8 0 1 0 2.2-5.5L4 9"/><path d="M4 4v5h5M12 8v5l3 2"/></>,
    image: <><rect x="3" y="4" width="18" height="16" rx="2"/><circle cx="8.5" cy="9" r="1.5"/><path d="m4 17 5-5 3 3 2-2 6 5"/></>,
    keyboard: <><rect x="3" y="6" width="18" height="12" rx="2"/><path d="M7 10h.01M11 10h.01M15 10h.01M18 10h.01M7 14h.01M10 14h7"/></>,
    moon: <path d="M20 15.2A8.5 8.5 0 0 1 8.8 4 8.5 8.5 0 1 0 20 15.2Z" />,
    paste: <><path d="M8 4h8v3H8z"/><path d="M6 5H5a2 2 0 0 0-2 2v13h13v-2M13 10h8v8h-8z"/><path d="m16 14 2 2 3-4"/></>,
    pause: <><path d="M9 6v12M15 6v12"/></>,
    pin: <><path d="m9 4 6 0-1 5 3 3v2H7v-2l3-3z"/><path d="M12 14v7"/></>,
    play: <path d="m8 5 11 7-11 7z" />,
    refresh: <><path d="M20 11a8 8 0 0 0-14.5-4L3 10"/><path d="M3 5v5h5M4 13a8 8 0 0 0 14.5 4L21 14"/><path d="M21 19v-5h-5"/></>,
    search: <><circle cx="11" cy="11" r="7"/><path d="m16 16 4 4"/></>,
    settings: <><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1-2.8 2.8-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.6v.2h-4V21a1.7 1.7 0 0 0-1-1.6 1.7 1.7 0 0 0-1.9.3l-.1.1L4.2 17l.1-.1a1.7 1.7 0 0 0 .3-1.9A1.7 1.7 0 0 0 3 14H2.8v-4H3a1.7 1.7 0 0 0 1.6-1 1.7 1.7 0 0 0-.3-1.9L4.2 7 7 4.2l.1.1a1.7 1.7 0 0 0 1.9.3A1.7 1.7 0 0 0 10 3V2.8h4V3a1.7 1.7 0 0 0 1 1.6 1.7 1.7 0 0 0 1.9-.3l.1-.1L19.8 7l-.1.1a1.7 1.7 0 0 0-.3 1.9 1.7 1.7 0 0 0 1.6 1h.2v4H21a1.7 1.7 0 0 0-1.6 1Z"/></>,
    shield: <><path d="M12 3 5 6v5c0 4.8 2.8 8.2 7 10 4.2-1.8 7-5.2 7-10V6z"/><path d="m9 12 2 2 4-5"/></>,
    sun: <><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/></>,
    text: <><path d="M5 5h14M12 5v14M8 19h8"/></>,
    trash: <><path d="M4 7h16M9 7V4h6v3M6 7l1 14h10l1-14M10 11v6M14 11v6"/></>,
    warning: <><path d="M12 3 2.8 20h18.4z"/><path d="M12 9v4M12 17h.01"/></>,
  };

  return (
    <svg
      aria-hidden="true"
      fill="none"
      height={size}
      viewBox="0 0 24 24"
      width={size}
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth="1.8"
      {...props}
    >
      {paths[name]}
    </svg>
  );
}
