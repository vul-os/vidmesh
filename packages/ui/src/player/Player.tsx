import Hls from "hls.js";
import { useEffect, useMemo, useReducer, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { cn } from "../cn.js";
import { CaptionsMenu } from "./CaptionsMenu.js";
import {
  bufferedRanges,
  formatClockTime,
  INITIAL_PLAYER_STATE,
  keyToAction,
  playerReducer,
  sponsorSegmentStyle,
  type SponsorSegment,
} from "./playerLogic.js";

export interface PlayerCaption {
  language: string;
  url: string;
  label?: string;
}

export interface PlayerProps {
  hls?: string | null;
  mp4?: string | null;
  captions?: PlayerCaption[];
  poster?: string;
  /** Sponsorship-segment markers rendered on the scrubber. */
  sponsorSegments?: SponsorSegment[];
  className?: string;
}

/**
 * The one video player every gateway ships (spec 009 §7). Uses hls.js
 * when MSE is available and an HLS URL was given; falls back to native
 * `<video src>` (Safari's built-in HLS, or a plain mp4) otherwise. Fully
 * keyboard-operable: space/k play-pause, ←/→ seek ±5s, ↑/↓ volume, f
 * fullscreen, m mute, c captions — active whenever the player has focus.
 */
export function Player({ hls, mp4, captions = [], poster, sponsorSegments = [], className }: PlayerProps): JSX.Element {
  const videoRef = useRef<HTMLVideoElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [state, dispatch] = useReducer(playerReducer, INITIAL_PLAYER_STATE);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [buffered, setBuffered] = useState<[number, number][]>([]);
  const [activeCaption, setActiveCaption] = useState<string | null>(null);

  const selectCaption = (language: string | null) => {
    if (language === null) {
      if (state.captionsOn) dispatch({ type: "toggle-captions" });
      return;
    }
    setActiveCaption(language);
    if (!state.captionsOn) dispatch({ type: "toggle-captions" });
  };

  // Source selection: hls.js -> native HLS -> mp4.
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return undefined;

    if (hls && Hls.isSupported()) {
      const player = new Hls();
      player.loadSource(hls);
      player.attachMedia(video);
      return () => player.destroy();
    }
    if (hls && video.canPlayType("application/vnd.apple.mpegurl")) {
      video.src = hls;
      return undefined;
    }
    if (mp4) {
      video.src = mp4;
    }
    return undefined;
  }, [hls, mp4]);

  // Sync intent (state) -> DOM.
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;
    if (state.playing && video.paused) void video.play().catch(() => dispatch({ type: "sync-playing", playing: false }));
    if (!state.playing && !video.paused) video.pause();
  }, [state.playing]);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;
    video.volume = state.volume;
    video.muted = state.muted;
  }, [state.volume, state.muted]);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;
    if (Math.abs(video.currentTime - state.currentTime) > 0.25) {
      video.currentTime = state.currentTime;
    }
  }, [state.currentTime]);

  useEffect(() => {
    const tracks = videoRef.current?.textTracks;
    if (!tracks) return;
    for (let i = 0; i < tracks.length; i++) {
      const track = tracks[i];
      if (!track) continue;
      const isActive = state.captionsOn && (activeCaption === null || track.language === activeCaption);
      track.mode = isActive ? "showing" : "hidden";
    }
  }, [state.captionsOn, activeCaption]);

  // DOM -> state sync (native events, other tabs' scrubbing, etc).
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return undefined;
    const onTime = () => dispatch({ type: "sync-time", time: video.currentTime });
    const onDuration = () => dispatch({ type: "sync-duration", duration: video.duration });
    const onPlay = () => dispatch({ type: "sync-playing", playing: true });
    const onPause = () => dispatch({ type: "sync-playing", playing: false });
    const onProgress = () => setBuffered(bufferedRanges(video.buffered));
    video.addEventListener("timeupdate", onTime);
    video.addEventListener("durationchange", onDuration);
    video.addEventListener("loadedmetadata", onDuration);
    video.addEventListener("play", onPlay);
    video.addEventListener("pause", onPause);
    video.addEventListener("progress", onProgress);
    return () => {
      video.removeEventListener("timeupdate", onTime);
      video.removeEventListener("durationchange", onDuration);
      video.removeEventListener("loadedmetadata", onDuration);
      video.removeEventListener("play", onPlay);
      video.removeEventListener("pause", onPause);
      video.removeEventListener("progress", onProgress);
    };
  }, []);

  useEffect(() => {
    const onFsChange = () => setIsFullscreen(document.fullscreenElement === containerRef.current);
    document.addEventListener("fullscreenchange", onFsChange);
    return () => document.removeEventListener("fullscreenchange", onFsChange);
  }, []);

  const handleKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    const action = keyToAction(event.key);
    if (!action) return;
    event.preventDefault();
    if (action.type === "toggle-fullscreen") {
      toggleFullscreen();
      return;
    }
    dispatch(action);
  };

  const toggleFullscreen = () => {
    const el = containerRef.current;
    if (!el) return;
    if (document.fullscreenElement) void document.exitFullscreen();
    else void el.requestFullscreen();
  };

  const sponsorMarks = useMemo(
    () => sponsorSegments.map((seg) => ({ ...seg, style: sponsorSegmentStyle(seg, state.duration) })),
    [sponsorSegments, state.duration],
  );

  return (
    <div
      ref={containerRef}
      className={cn("group relative w-full overflow-hidden rounded-lg bg-black outline-none", className)}
      tabIndex={0}
      role="group"
      aria-label="Video player"
      onKeyDown={handleKeyDown}
    >
      <video
        ref={videoRef}
        poster={poster}
        className="aspect-video w-full bg-black"
        onClick={() => dispatch({ type: "toggle-play" })}
      >
        {captions.map((cap) => (
          <track key={cap.language} kind="captions" srcLang={cap.language} label={cap.label ?? cap.language} src={cap.url} />
        ))}
      </video>

      {sponsorSegments.length > 0 && (
        <ul className="sr-only">
          {sponsorSegments.map((seg, i) => (
            <li key={i}>
              Sponsored segment: {formatClockTime(seg.startMs / 1000)} to {formatClockTime(seg.endMs / 1000)}, {seg.label}
            </li>
          ))}
        </ul>
      )}

      <div className="absolute inset-x-0 bottom-0 flex flex-col gap-1 bg-gradient-to-t from-black/80 to-transparent p-2 text-white">
        <div className="relative h-2 w-full">
          <div className="absolute inset-0 rounded bg-white/20" />
          {buffered.map(([start, end], i) => (
            <div
              key={i}
              className="absolute inset-y-0 rounded bg-white/40"
              style={{ left: `${(start / (state.duration || 1)) * 100}%`, width: `${((end - start) / (state.duration || 1)) * 100}%` }}
            />
          ))}
          {sponsorMarks.map((seg, i) => (
            <div
              key={i}
              title={`Sponsored: ${seg.label}`}
              className="absolute inset-y-0 rounded bg-brand-400"
              style={{ left: `${seg.style.leftPct}%`, width: `${Math.max(seg.style.widthPct, 0.5)}%` }}
            />
          ))}
          <input
            type="range"
            aria-label="Seek"
            min={0}
            max={state.duration || 0}
            step={0.1}
            value={state.currentTime}
            onChange={(e) => dispatch({ type: "seek-to", time: Number(e.target.value) })}
            className="absolute inset-0 h-2 w-full cursor-pointer appearance-none bg-transparent accent-brand-400"
          />
        </div>

        <div className="flex items-center gap-2">
          <button
            type="button"
            aria-label={state.playing ? "Pause" : "Play"}
            onClick={() => dispatch({ type: "toggle-play" })}
            className="rounded p-1.5 focus-visible:outline focus-visible:outline-[3px] focus-visible:outline-brand-300"
          >
            {state.playing ? "⏸" : "▶"}
          </button>

          <span className="min-w-[5.5rem] font-mono text-xs tabular-nums">
            {formatClockTime(state.currentTime)} / {formatClockTime(state.duration)}
          </span>

          <button
            type="button"
            aria-label={state.muted || state.volume === 0 ? "Unmute" : "Mute"}
            aria-pressed={state.muted}
            onClick={() => dispatch({ type: "toggle-mute" })}
            className="rounded p-1.5 focus-visible:outline focus-visible:outline-[3px] focus-visible:outline-brand-300"
          >
            {state.muted || state.volume === 0 ? "🔇" : "🔊"}
          </button>
          <input
            type="range"
            aria-label="Volume"
            min={0}
            max={1}
            step={0.05}
            value={state.muted ? 0 : state.volume}
            onChange={(e) => dispatch({ type: "set-volume", volume: Number(e.target.value) })}
            className="h-2 w-16 cursor-pointer accent-brand-400"
          />

          <div className="relative ml-auto flex items-center gap-2">
            <CaptionsMenu captions={captions} captionsOn={state.captionsOn} activeLanguage={activeCaption} onSelect={selectCaption} />

            <button
              type="button"
              aria-label={isFullscreen ? "Exit fullscreen" : "Enter fullscreen"}
              aria-pressed={isFullscreen}
              onClick={toggleFullscreen}
              className="rounded p-1.5 focus-visible:outline focus-visible:outline-[3px] focus-visible:outline-brand-300"
            >
              {isFullscreen ? "⤢" : "⤡"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
