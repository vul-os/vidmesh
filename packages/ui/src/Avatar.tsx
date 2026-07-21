import { cn } from "./cn.js";

export interface AvatarProps {
  name: string;
  src?: string | null;
  size?: "sm" | "md" | "lg";
  className?: string;
}

const SIZE_CLASSES: Record<NonNullable<AvatarProps["size"]>, string> = {
  sm: "h-6 w-6 text-xs",
  md: "h-10 w-10 text-sm",
  lg: "h-16 w-16 text-xl",
};

/** First letters of up to two words, uppercased — fallback when no image. */
export function initialsFor(name: string): string {
  const words = name.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return "?";
  const first = words[0]?.[0] ?? "";
  const second = words.length > 1 ? (words[words.length - 1]?.[0] ?? "") : "";
  return (first + second).toUpperCase();
}

/** Author/channel avatar: image when available, deterministic initials otherwise. */
export function Avatar({ name, src, size = "md", className }: AvatarProps): JSX.Element {
  const base = cn(
    "inline-flex shrink-0 items-center justify-center rounded-full font-semibold ring-1 ring-inset ring-brand-700/10 dark:ring-brand-300/15",
    "bg-brand-100 text-brand-800 dark:bg-brand-900 dark:text-brand-100",
    SIZE_CLASSES[size],
    className,
  );

  if (src) {
    return (
      <img
        src={src}
        alt={name}
        className={cn(base, "object-cover")}
        loading="lazy"
      />
    );
  }

  return (
    <span className={base} role="img" aria-label={name}>
      {initialsFor(name)}
    </span>
  );
}
