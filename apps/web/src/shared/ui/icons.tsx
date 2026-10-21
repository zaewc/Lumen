import type { ReactNode } from "react";

// Small decorative icons, drawn inline (no icon library): the text next to
// them, or the control's label, carries the meaning.

interface IconProps {
  readonly className?: string;
}

/** A 20 × 20 outline icon in the current text colour. */
function Outline({
  className = "",
  children,
}: {
  readonly className?: string;
  readonly children: ReactNode;
}) {
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={`size-5 shrink-0 ${className}`}
    >
      {children}
    </svg>
  );
}

export function ChevronIcon({ className = "" }: IconProps) {
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 20 20"
      fill="currentColor"
      className={`size-4 shrink-0 ${className}`}
    >
      <path
        fillRule="evenodd"
        d="M7.2 14.8a.75.75 0 0 1 0-1.06L10.94 10 7.2 6.26a.75.75 0 1 1 1.06-1.06l4.27 4.27a.75.75 0 0 1 0 1.06l-4.27 4.27a.75.75 0 0 1-1.06 0Z"
        clipRule="evenodd"
      />
    </svg>
  );
}

export function CheckIcon({ className = "" }: IconProps) {
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 20 20"
      fill="currentColor"
      className={`size-5 shrink-0 ${className}`}
    >
      <path
        fillRule="evenodd"
        d="M16.7 5.3a1 1 0 0 1 0 1.4l-8 8a1 1 0 0 1-1.4 0l-4-4a1 1 0 1 1 1.4-1.4l3.3 3.29 7.3-7.3a1 1 0 0 1 1.4 0Z"
        clipRule="evenodd"
      />
    </svg>
  );
}

export function WarningIcon({ className = "" }: IconProps) {
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 20 20"
      fill="currentColor"
      className={`size-5 shrink-0 ${className}`}
    >
      <path
        fillRule="evenodd"
        d="M8.48 2.88a1.75 1.75 0 0 1 3.04 0l6.28 10.97A1.75 1.75 0 0 1 16.28 16.5H3.72a1.75 1.75 0 0 1-1.52-2.65L8.48 2.88ZM10 6.5a.75.75 0 0 1 .75.75v3.5a.75.75 0 0 1-1.5 0v-3.5A.75.75 0 0 1 10 6.5Zm0 7.75a1 1 0 1 0 0-2 1 1 0 0 0 0 2Z"
        clipRule="evenodd"
      />
    </svg>
  );
}

/** Overview: a gauge. */
export function GaugeIcon({ className = "" }: IconProps) {
  return (
    <Outline className={className}>
      <path d="M3.5 13.5a6.5 6.5 0 1 1 13 0" />
      <path d="M10 13.5 13 8.5" />
      <circle cx="10" cy="13.5" r="1" fill="currentColor" />
      <path d="M3 16.5h14" />
    </Outline>
  );
}

/** Clean up: a sparkle. */
export function SparkleIcon({ className = "" }: IconProps) {
  return (
    <Outline className={className}>
      <path d="M8.5 3c.6 3.2 1.8 4.4 5 5-3.2.6-4.4 1.8-5 5-.6-3.2-1.8-4.4-5-5 3.2-.6 4.4-1.8 5-5Z" />
      <path d="M15 11.5c.3 1.5.9 2.1 2.4 2.4-1.5.3-2.1.9-2.4 2.4-.3-1.5-.9-2.1-2.4-2.4 1.5-.3 2.1-.9 2.4-2.4Z" />
    </Outline>
  );
}

/** Quarantine: a storage box. */
export function ArchiveIcon({ className = "" }: IconProps) {
  return (
    <Outline className={className}>
      <rect x="2.75" y="3.5" width="14.5" height="4" rx="1" />
      <path d="M4 7.5v7.25c0 .97.78 1.75 1.75 1.75h8.5c.97 0 1.75-.78 1.75-1.75V7.5" />
      <path d="M8 10.75h4" />
    </Outline>
  );
}

/** Settings: a gear. */
export function GearIcon({ className = "" }: IconProps) {
  return (
    <Outline className={className}>
      <circle cx="10" cy="10" r="2.5" />
      <path d="M10 2.75v1.5M10 15.75v1.5M17.25 10h-1.5M4.25 10h-1.5M15.13 4.87l-1.06 1.06M5.93 14.07l-1.06 1.06M15.13 15.13l-1.06-1.06M5.93 5.93 4.87 4.87" />
      <circle cx="10" cy="10" r="5.25" />
    </Outline>
  );
}

/** A disk drive. */
export function DriveIcon({ className = "" }: IconProps) {
  return (
    <Outline className={className}>
      <rect x="2.75" y="5" width="14.5" height="10" rx="2" />
      <path d="M2.75 11.25h14.5" />
      <circle cx="14" cy="13.1" r=".6" fill="currentColor" />
    </Outline>
  );
}

/** Developer tools: angle brackets. */
export function CodeIcon({ className = "" }: IconProps) {
  return (
    <Outline className={className}>
      <path d="m7 6-4 4 4 4M13 6l4 4-4 4M11.25 4.5l-2.5 11" />
    </Outline>
  );
}

/** Caches: stacked layers. */
export function LayersIcon({ className = "" }: IconProps) {
  return (
    <Outline className={className}>
      <path d="m10 3 7 3.75L10 10.5 3 6.75 10 3Z" />
      <path d="m3 10 7 3.75L17 10" />
      <path d="m3 13.25 7 3.75 7-3.75" />
    </Outline>
  );
}

/** Application data: an app window. */
export function AppIcon({ className = "" }: IconProps) {
  return (
    <Outline className={className}>
      <rect x="3" y="3" width="14" height="14" rx="3.5" />
      <path d="M3 7h14" />
      <circle cx="5.6" cy="5" r=".55" fill="currentColor" />
      <circle cx="7.6" cy="5" r=".55" fill="currentColor" />
    </Outline>
  );
}

/** Anything else: a folder. */
export function FolderIcon({ className = "" }: IconProps) {
  return (
    <Outline className={className}>
      <path d="M2.75 6.25c0-.97.78-1.75 1.75-1.75h3.1c.46 0 .9.18 1.23.51l1.17 1.16c.33.33.77.51 1.24.51h4.26c.97 0 1.75.78 1.75 1.75v6.32c0 .97-.78 1.75-1.75 1.75h-11c-.97 0-1.75-.78-1.75-1.75V6.25Z" />
    </Outline>
  );
}

/** Close. */
export function CloseIcon({ className = "" }: IconProps) {
  return (
    <Outline className={className}>
      <path d="m5.5 5.5 9 9M14.5 5.5l-9 9" />
    </Outline>
  );
}

/** A shield, for what keeps data safe. */
export function ShieldIcon({ className = "" }: IconProps) {
  return (
    <Outline className={className}>
      <path d="M10 2.75 4 5v4.6c0 3.6 2.5 6.4 6 7.65 3.5-1.25 6-4.05 6-7.65V5l-6-2.25Z" />
      <path d="m7.5 10 1.75 1.75L12.75 8.25" />
    </Outline>
  );
}

/** Language: a globe. */
export function GlobeIcon({ className = "" }: IconProps) {
  return (
    <Outline className={className}>
      <circle cx="10" cy="10" r="7.25" />
      <path d="M2.75 10h14.5M10 2.75c2 2 3 4.4 3 7.25s-1 5.25-3 7.25c-2-2-3-4.4-3-7.25s1-5.25 3-7.25Z" />
    </Outline>
  );
}

/** Information. */
export function InfoIcon({ className = "" }: IconProps) {
  return (
    <Outline className={className}>
      <circle cx="10" cy="10" r="7.25" />
      <path d="M10 9v4.5" />
      <circle cx="10" cy="6.5" r=".6" fill="currentColor" />
    </Outline>
  );
}

/** A magnifier, for scanning. */
export function SearchIcon({ className = "" }: IconProps) {
  return (
    <Outline className={className}>
      <circle cx="8.75" cy="8.75" r="5.25" />
      <path d="m12.75 12.75 4 4" />
    </Outline>
  );
}

/** Undo, for restoring. */
export function RestoreIcon({ className = "" }: IconProps) {
  return (
    <Outline className={className}>
      <path d="M5.5 8.5h7a4 4 0 0 1 0 8H9" />
      <path d="m8.5 5.5-3 3 3 3" />
    </Outline>
  );
}
