/**
 * The interface's icon set: plain stroke glyphs, round caps, 1.75px at a
 * 20px grid — the same hand-drawn construction as the brand mark
 * (assets/mark.svg), so a player transport bar or a theme toggle reads as
 * part of the same object as the logo instead of a pile of emoji standing
 * in for icons. `currentColor` throughout; size via the `size` prop.
 */
import type { SVGProps } from "react";

export interface IconProps extends Omit<SVGProps<SVGSVGElement>, "viewBox" | "width" | "height"> {
  size?: number;
}

function base(size: number, props: IconProps) {
  const { className, ...rest } = props;
  return {
    width: size,
    height: size,
    viewBox: "0 0 20 20",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.75,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    className,
    "aria-hidden": true,
    ...rest,
  };
}

export function PlayIcon({ size = 20, ...props }: IconProps): JSX.Element {
  return (
    <svg {...base(size, props)}>
      <path d="M6.5 4.5v11l8-5.5-8-5.5Z" fill="currentColor" stroke="none" />
    </svg>
  );
}

export function PauseIcon({ size = 20, ...props }: IconProps): JSX.Element {
  return (
    <svg {...base(size, props)}>
      <path d="M6.5 4.5v11M13.5 4.5v11" />
    </svg>
  );
}

export function VolumeIcon({ size = 20, ...props }: IconProps): JSX.Element {
  return (
    <svg {...base(size, props)}>
      <path d="M3.5 7.5h2.8L10.5 4v12l-4.2-3.5H3.5v-5Z" />
      <path d="M13.2 7.4a4 4 0 0 1 0 5.2M15.4 5.3a7 7 0 0 1 0 9.4" />
    </svg>
  );
}

export function MuteIcon({ size = 20, ...props }: IconProps): JSX.Element {
  return (
    <svg {...base(size, props)}>
      <path d="M3.5 7.5h2.8L10.5 4v12l-4.2-3.5H3.5v-5Z" />
      <path d="M13.5 7.5 17 11M17 7.5l-3.5 3.5" />
    </svg>
  );
}

export function FullscreenIcon({ size = 20, ...props }: IconProps): JSX.Element {
  return (
    <svg {...base(size, props)}>
      <path d="M7.5 3.5H4a.5.5 0 0 0-.5.5v3.5M12.5 3.5H16a.5.5 0 0 1 .5.5v3.5M16.5 12.5V16a.5.5 0 0 1-.5.5h-3.5M3.5 12.5V16a.5.5 0 0 0 .5.5h3.5" />
    </svg>
  );
}

export function FullscreenExitIcon({ size = 20, ...props }: IconProps): JSX.Element {
  return (
    <svg {...base(size, props)}>
      <path d="M4 7.5h3a.5.5 0 0 0 .5-.5V4M16 7.5h-3a.5.5 0 0 1-.5-.5V4M12.5 16v-3a.5.5 0 0 1 .5-.5h3M7.5 16v-3a.5.5 0 0 0-.5-.5H4" />
    </svg>
  );
}

export function SunIcon({ size = 20, ...props }: IconProps): JSX.Element {
  return (
    <svg {...base(size, props)}>
      <circle cx="10" cy="10" r="3.4" />
      <path d="M10 2.5v2M10 15.5v2M17.5 10h-2M4.5 10h-2M15.3 4.7l-1.4 1.4M6.1 13.9l-1.4 1.4M15.3 15.3l-1.4-1.4M6.1 6.1 4.7 4.7" />
    </svg>
  );
}

export function MoonIcon({ size = 20, ...props }: IconProps): JSX.Element {
  return (
    <svg {...base(size, props)}>
      <path d="M16.5 12.3A6.7 6.7 0 0 1 7.7 3.5a6.9 6.9 0 1 0 8.8 8.8Z" />
    </svg>
  );
}

export function CloseIcon({ size = 20, ...props }: IconProps): JSX.Element {
  return (
    <svg {...base(size, props)}>
      <path d="M5.5 5.5 14.5 14.5M14.5 5.5 5.5 14.5" />
    </svg>
  );
}

export function ChevronDownIcon({ size = 20, ...props }: IconProps): JSX.Element {
  return (
    <svg {...base(size, props)}>
      <path d="M5 7.5 10 12.5 15 7.5" />
    </svg>
  );
}

export function SearchIcon({ size = 20, ...props }: IconProps): JSX.Element {
  return (
    <svg {...base(size, props)}>
      <circle cx="8.7" cy="8.7" r="5.2" />
      <path d="m16 16-3.6-3.6" />
    </svg>
  );
}

export function CaptionsIcon({ size = 20, ...props }: IconProps): JSX.Element {
  return (
    <svg {...base(size, props)}>
      <rect x="3" y="5.5" width="14" height="9" rx="2" />
      <path d="M7.8 9a1.6 1.6 0 0 0-2.6 1.2 1.6 1.6 0 0 0 2.6 1.2M13.8 9a1.6 1.6 0 0 0-2.6 1.2 1.6 1.6 0 0 0 2.6 1.2" />
    </svg>
  );
}

export function UploadCloudIcon({ size = 20, ...props }: IconProps): JSX.Element {
  return (
    <svg {...base(size, props)}>
      <path d="M6 14.5A3.5 3.5 0 0 1 5.5 7.6 4.5 4.5 0 0 1 14 6.7 3.25 3.25 0 0 1 14 14.5H6Z" />
      <path d="M10 8.5v6M7.5 10.5 10 8l2.5 2.5" />
    </svg>
  );
}
