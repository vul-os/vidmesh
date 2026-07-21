/**
 * @vidmesh/ui — shared React components for Vidmesh gateways.
 *
 * This is the uniform reference UI's component layer (spec 009 §7): the
 * player, the verification badge, and generic record display. Every
 * gateway deploying the reference frontend imports from here so the
 * product stays identical across domains — differentiation happens in
 * catalog and branding accents (CSS custom properties), not in
 * reimplementing these components.
 */

export { Player } from "./player/Player.js";
export type { PlayerProps, PlayerCaption } from "./player/Player.js";
export {
  keyToAction,
  playerReducer,
  clampVolume,
  clampTime,
  bufferedRanges,
  sponsorSegmentStyle,
  formatClockTime,
  INITIAL_PLAYER_STATE,
} from "./player/playerLogic.js";
export type { PlayerAction, PlayerState, SponsorSegment } from "./player/playerLogic.js";

export { VerifiedBadge } from "./VerifiedBadge.js";
export type { VerifiedState, VerifiedBadgeProps } from "./VerifiedBadge.js";

export { RecordCard } from "./RecordCard.js";
export type { RecordCardProps } from "./RecordCard.js";

export { Avatar, initialsFor } from "./Avatar.js";
export type { AvatarProps } from "./Avatar.js";

export { TimeAgo, formatTimeAgo } from "./TimeAgo.js";
export type { TimeAgoProps } from "./TimeAgo.js";

export { cn } from "./cn.js";
export type { ClassValue } from "./cn.js";

export {
  PlayIcon,
  PauseIcon,
  VolumeIcon,
  MuteIcon,
  FullscreenIcon,
  FullscreenExitIcon,
  SunIcon,
  MoonIcon,
  CloseIcon,
  ChevronDownIcon,
  SearchIcon,
  CaptionsIcon,
  UploadCloudIcon,
} from "./Icon.js";
export type { IconProps } from "./Icon.js";
