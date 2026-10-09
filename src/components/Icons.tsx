import type { CSSProperties } from "react";
import appIcon from "../../src-tauri/icons/128x128.png";

/** Material Symbol glyph (self-hosted subset - see globals.css). */
export function Ms({
  name,
  className,
  style,
}: Readonly<{
  name: string;
  className?: string;
  style?: CSSProperties;
}>) {
  return (
    <span
      className={"ms material-symbols-outlined" + (className ? " " + className : "")}
      style={style}
      aria-hidden="true"
    >
      {name}
    </span>
  );
}

/** The same Mixweave application icon used by the desktop launcher. */
export function SinkMark() {
  return <img src={appIcon} alt="" aria-hidden="true" />;
}

/** Legacy fallback icons for channels created before icons existed. */
export const CHANNEL_ICONS: Record<string, string> = {
  sink_game: "sports_esports",
  sink_chat: "forum",
  sink_media: "music_note",
  sink_aux: "cable",
};

export function channelIcon(channel: { name: string; icon?: string | null }): string {
  return channel.icon ?? CHANNEL_ICONS[channel.name] ?? "graphic_eq";
}

/** Shared channel colour used by mixer strips, workspace tabs and headings. */
export function channelAccentClass(channel: { name: string }): string {
  switch (channel.name) {
    case "sink_game": return "strip-accent-game";
    case "sink_chat": return "strip-accent-chat";
    case "sink_media": return "strip-accent-media";
    case "sink_aux": return "strip-accent-aux";
    default: return "strip-accent-default";
  }
}

/** Curated icon choices for the channel icon picker. */
export const ICON_CHOICES: string[] = [
  "sports_esports",
  "forum",
  "music_note",
  "desktop_windows",
  "headphones",
  "mic",
  "movie",
  "tv",
  "videogame_asset",
  "campaign",
  "record_voice_over",
  "radio",
  "podcasts",
  "terminal",
  "public",
  "star",
];
