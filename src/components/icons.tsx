// Icônes dessinées à la main, au trait de la maquette (pas de bibliothèque d'icônes).

import type { SVGProps } from "react";

type IconProps = SVGProps<SVGSVGElement> & { size?: number };

function Svg({ size = 16, viewBox = "0 0 16 16", children, ...rest }: IconProps & { viewBox?: string }) {
  return (
    <svg width={size} height={size} viewBox={viewBox} aria-hidden="true" focusable="false" {...rest}>
      {children}
    </svg>
  );
}

export function LogoIcon({ size = 18 }: { size?: number }) {
  return (
    <Svg size={size} viewBox="0 0 18 18">
      <rect x="0.5" y="0.5" width="17" height="17" rx="5" fill="var(--text-1)" />
      <path d="M9 4.5a4.5 4.5 0 0 1 0 9z" fill="var(--bg-app)" />
      <circle cx="9" cy="9" r="4.5" fill="none" stroke="var(--bg-app)" strokeWidth="1" />
    </Svg>
  );
}

export function SearchIcon({ size = 13 }: { size?: number }) {
  return (
    <Svg size={size} fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round">
      <circle cx="7" cy="7" r="4.75" />
      <path d="M10.5 10.5 14 14" />
    </Svg>
  );
}

export function SunIcon({ size = 15 }: { size?: number }) {
  return (
    <Svg size={size} fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round">
      <circle cx="8" cy="8" r="3" />
      <path d="M8 1.5v1.5M8 13v1.5M1.5 8H3M13 8h1.5M3.4 3.4l1.06 1.06M11.54 11.54l1.06 1.06M3.4 12.6l1.06-1.06M11.54 4.46l1.06-1.06" />
    </Svg>
  );
}

export function MoonIcon({ size = 15 }: { size?: number }) {
  return (
    <Svg size={size} fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round">
      <path d="M13.2 9.9A5.6 5.6 0 0 1 6.1 2.8a5.6 5.6 0 1 0 7.1 7.1z" />
    </Svg>
  );
}

export function GearIcon({ size = 15 }: { size?: number }) {
  return (
    <Svg size={size} fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round">
      <path d="M6.7 1.5h2.6l.4 1.8 1.2.7 1.8-.6 1.3 2.3-1.4 1.2v1.4l1.4 1.2-1.3 2.3-1.8-.6-1.2.7-.4 1.8H6.7l-.4-1.8-1.2-.7-1.8.6-1.3-2.3 1.4-1.2V7.3L2 6.1l1.3-2.3 1.8.6 1.2-.7z" />
      <circle cx="8" cy="8" r="2.1" />
    </Svg>
  );
}

export function MinimizeIcon() {
  return (
    <Svg size={10} viewBox="0 0 10 10">
      <path d="M0 5h10" stroke="currentColor" strokeWidth="1" />
    </Svg>
  );
}

export function MaximizeIcon() {
  return (
    <Svg size={10} viewBox="0 0 10 10">
      <rect x="0.5" y="0.5" width="9" height="9" rx="1.5" fill="none" stroke="currentColor" strokeWidth="1" />
    </Svg>
  );
}

export function RestoreIcon() {
  return (
    <Svg size={10} viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1">
      <rect x="0.5" y="2.5" width="7" height="7" rx="1.5" />
      <path d="M2.5 2.5V2A1.5 1.5 0 0 1 4 .5h4A1.5 1.5 0 0 1 9.5 2v4A1.5 1.5 0 0 1 8 7.5h-.5" />
    </Svg>
  );
}

export function CloseIcon({ size = 10 }: { size?: number }) {
  return (
    <Svg size={size} viewBox="0 0 10 10">
      <path d="M0.5 0.5l9 9M9.5 0.5l-9 9" stroke="currentColor" strokeWidth="1" />
    </Svg>
  );
}

export function CloseSmallIcon() {
  return (
    <Svg size={10} viewBox="0 0 10 10">
      <path d="M1 1l8 8M9 1 1 9" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" />
    </Svg>
  );
}

export function ChevronLeftIcon() {
  return (
    <Svg size={12} viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M7.5 2.5 4 6l3.5 3.5" />
    </Svg>
  );
}

export function ChevronDownIcon() {
  return (
    <Svg size={10} viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round">
      <path d="M2.5 4 5 6.5 7.5 4" />
    </Svg>
  );
}

export function InfoIcon() {
  return (
    <Svg size={13} fill="none" stroke="currentColor" strokeWidth="1.4">
      <circle cx="8" cy="8" r="6.25" />
      <path d="M8 7.2v4M8 4.9v.1" strokeLinecap="round" />
    </Svg>
  );
}

export function DriveIcon() {
  return (
    <Svg size={13} fill="none" stroke="currentColor" strokeWidth="1.3">
      <rect x="1.75" y="4.25" width="12.5" height="7.5" rx="2" />
      <path d="M4.5 9h.01" strokeLinecap="round" strokeWidth="1.6" />
    </Svg>
  );
}

export function TrendIcon() {
  return (
    <Svg size={11} fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M2 11.5 6 7.5l2.5 2.5L14 4.5M10 4.5h4v4" />
    </Svg>
  );
}

export function ArrowLeftIcon() {
  return (
    <Svg size={13} fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M13 8H3.5M7.5 3.5 3 8l4.5 4.5" />
    </Svg>
  );
}

export function ArrowRightIcon() {
  return (
    <Svg size={13} fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M3 8h9.5M8.5 3.5 13 8l-4.5 4.5" />
    </Svg>
  );
}

/** « Étendre » : flèches en diagonale (zoom sur une tuile). */
export function ExpandIcon() {
  return (
    <Svg size={11} fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M9.5 2.5h4v4M13.5 2.5 9 7M6.5 13.5h-4v-4M2.5 13.5 7 9" />
    </Svg>
  );
}
