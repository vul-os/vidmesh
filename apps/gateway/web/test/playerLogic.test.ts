import {
  bufferedRanges,
  clampTime,
  clampVolume,
  formatClockTime,
  INITIAL_PLAYER_STATE,
  keyToAction,
  playerReducer,
  sponsorSegmentStyle,
} from "@evermesh/ui";
import { describe, expect, it } from "vitest";

describe("keyToAction (the player's keyboard map)", () => {
  it("maps space and k to toggle-play", () => {
    expect(keyToAction(" ")).toEqual({ type: "toggle-play" });
    expect(keyToAction("k")).toEqual({ type: "toggle-play" });
    expect(keyToAction("K")).toEqual({ type: "toggle-play" });
  });

  it("maps arrow keys to seek ±5s and volume ±10%", () => {
    expect(keyToAction("ArrowLeft")).toEqual({ type: "seek", deltaSec: -5 });
    expect(keyToAction("ArrowRight")).toEqual({ type: "seek", deltaSec: 5 });
    expect(keyToAction("ArrowUp")).toEqual({ type: "volume", deltaPct: 0.1 });
    expect(keyToAction("ArrowDown")).toEqual({ type: "volume", deltaPct: -0.1 });
  });

  it("maps f/m/c to fullscreen/mute/captions", () => {
    expect(keyToAction("f")).toEqual({ type: "toggle-fullscreen" });
    expect(keyToAction("m")).toEqual({ type: "toggle-mute" });
    expect(keyToAction("c")).toEqual({ type: "toggle-captions" });
  });

  it("returns null for keys the player doesn't handle", () => {
    expect(keyToAction("q")).toBeNull();
    expect(keyToAction("Tab")).toBeNull();
  });
});

describe("playerReducer", () => {
  it("toggles playing", () => {
    expect(playerReducer(INITIAL_PLAYER_STATE, { type: "toggle-play" }).playing).toBe(true);
  });

  it("clamps seeks to [0, duration]", () => {
    const state = { ...INITIAL_PLAYER_STATE, duration: 10, currentTime: 8 };
    expect(playerReducer(state, { type: "seek", deltaSec: 5 }).currentTime).toBe(10);
    expect(playerReducer(state, { type: "seek", deltaSec: -20 }).currentTime).toBe(0);
  });

  it("adjusts volume and unmutes on a nonzero volume change", () => {
    const muted = { ...INITIAL_PLAYER_STATE, muted: true, volume: 0.5 };
    const state = playerReducer(muted, { type: "volume", deltaPct: 0.2 });
    expect(state.volume).toBeCloseTo(0.7);
    expect(state.muted).toBe(false);
  });

  it("toggles captions and mute", () => {
    expect(playerReducer(INITIAL_PLAYER_STATE, { type: "toggle-captions" }).captionsOn).toBe(true);
    expect(playerReducer(INITIAL_PLAYER_STATE, { type: "toggle-mute" }).muted).toBe(true);
  });

  it("leaves state untouched for toggle-fullscreen (owned by the DOM, not this reducer)", () => {
    expect(playerReducer(INITIAL_PLAYER_STATE, { type: "toggle-fullscreen" })).toEqual(INITIAL_PLAYER_STATE);
  });

  it("syncs duration/time/playing from native video events", () => {
    let state = playerReducer(INITIAL_PLAYER_STATE, { type: "sync-duration", duration: 120 });
    state = playerReducer(state, { type: "sync-time", time: 42 });
    state = playerReducer(state, { type: "sync-playing", playing: true });
    expect(state).toMatchObject({ duration: 120, currentTime: 42, playing: true });
  });
});

describe("clampVolume / clampTime", () => {
  it("clamps volume to [0,1]", () => {
    expect(clampVolume(-1)).toBe(0);
    expect(clampVolume(2)).toBe(1);
    expect(clampVolume(0.4)).toBeCloseTo(0.4);
  });

  it("clamps time to a known duration", () => {
    expect(clampTime(-5, 10)).toBe(0);
    expect(clampTime(15, 10)).toBe(10);
  });

  it("only floors at zero when duration is unknown (0 or non-finite)", () => {
    expect(clampTime(-5, 0)).toBe(0);
    expect(clampTime(999, 0)).toBe(999);
  });
});

describe("bufferedRanges", () => {
  it("reads a TimeRanges-shaped object into plain [start,end] tuples", () => {
    const starts = [0, 20];
    const ends = [10, 30];
    const fakeTimeRanges = { length: 2, start: (i: number) => starts[i]!, end: (i: number) => ends[i]! };
    expect(bufferedRanges(fakeTimeRanges)).toEqual([
      [0, 10],
      [20, 30],
    ]);
  });

  it("returns an empty array for no buffered ranges", () => {
    expect(bufferedRanges({ length: 0, start: () => 0, end: () => 0 })).toEqual([]);
  });
});

describe("sponsorSegmentStyle", () => {
  it("computes left/width percentages from ms offsets against the duration", () => {
    const style = sponsorSegmentStyle({ startMs: 30_000, endMs: 45_000, label: "x" }, 300);
    expect(style.leftPct).toBeCloseTo(10);
    expect(style.widthPct).toBeCloseTo(5);
  });

  it("returns zero-width/zero-offset when duration is unknown", () => {
    expect(sponsorSegmentStyle({ startMs: 0, endMs: 10, label: "x" }, 0)).toEqual({ leftPct: 0, widthPct: 0 });
  });
});

describe("formatClockTime", () => {
  it("formats under an hour as m:ss", () => {
    expect(formatClockTime(65)).toBe("1:05");
  });

  it("formats an hour or more as h:mm:ss", () => {
    expect(formatClockTime(3665)).toBe("1:01:05");
  });

  it("formats non-finite/negative input as 0:00", () => {
    expect(formatClockTime(Number.NaN)).toBe("0:00");
    expect(formatClockTime(-5)).toBe("0:00");
  });
});
