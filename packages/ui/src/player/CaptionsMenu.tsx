import { useId, useState } from "react";
import { cn } from "../cn.js";
import { CaptionsIcon } from "../Icon.js";
import type { PlayerCaption } from "./Player.js";

export interface CaptionsMenuProps {
  captions: PlayerCaption[];
  captionsOn: boolean;
  activeLanguage: string | null;
  onSelect: (language: string | null) => void;
}

/**
 * The captions ("CC") button + track picker, factored out of Player.tsx
 * to keep that file's DOM-sync effects the main focus. `onSelect(null)`
 * means "off"; any other value both selects that track and turns
 * captions on.
 */
export function CaptionsMenu({ captions, captionsOn, activeLanguage, onSelect }: CaptionsMenuProps): JSX.Element | null {
  const [open, setOpen] = useState(false);
  const menuId = useId();

  if (captions.length === 0) return null;

  return (
    <>
      <button
        type="button"
        aria-haspopup="true"
        aria-expanded={open}
        aria-controls={menuId}
        aria-pressed={captionsOn}
        onClick={() => setOpen((v) => !v)}
        className={cn(
          "rounded-full p-1.5 transition-colors hover:bg-white/15 focus-visible:outline focus-visible:outline-[3px] focus-visible:outline-brand-300",
          captionsOn && "text-brand-300",
        )}
      >
        <CaptionsIcon />
      </button>
      {open && (
        <ul
          id={menuId}
          role="menu"
          className="absolute bottom-9 right-0 min-w-[9rem] rounded-control border border-white/10 bg-slate-900/95 p-1 text-sm text-white shadow-elevated backdrop-blur"
        >
          <li role="none">
            <button
              role="menuitemradio"
              aria-checked={!captionsOn}
              onClick={() => {
                onSelect(null);
                setOpen(false);
              }}
              className="block w-full rounded px-2 py-1.5 text-left transition-colors hover:bg-white/10"
            >
              Off
            </button>
          </li>
          {captions.map((cap) => (
            <li key={cap.language} role="none">
              <button
                role="menuitemradio"
                aria-checked={captionsOn && activeLanguage === cap.language}
                onClick={() => {
                  onSelect(cap.language);
                  setOpen(false);
                }}
                className="block w-full rounded px-2 py-1.5 text-left transition-colors hover:bg-white/10"
              >
                {cap.label ?? cap.language}
              </button>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}
