import type { ReactNode } from "react";
import { Ms } from "./Icons";
import { Toggle } from "./Toggle";

/** Heading of a processing card: icon tile and title (with its info button),
 * any extra controls, and the card's on/off switch at the far end. */
export function ProcessingHead({
  icon,
  title,
  subtitle,
  tone,
  info,
  actions,
  on,
  onToggle,
}: Readonly<{
  icon: string;
  title: string;
  /** One line under the title. */
  subtitle?: string;
  /** "accent" tints the icon tile with the app colour (for a card's main feature). */
  tone?: "accent";
  info?: ReactNode;
  actions?: ReactNode;
  on?: boolean;
  onToggle?: () => void;
}>) {
  return (
    <div className="processing-card-head">
      <div className="processing-heading">
        <span className={"setting-icon" + (tone === "accent" ? " accent" : "")}><Ms name={icon} /></span>
        <div className="processing-heading-copy">
          <div className="processing-heading-title">
            <div className="rtitle">{title}</div>
            {info}
          </div>
          {subtitle && <div className="processing-subtitle">{subtitle}</div>}
        </div>
      </div>
      <div className="processing-card-head-actions">
        {actions}
        {onToggle && on !== undefined && <Toggle on={on} onClick={onToggle} />}
      </div>
    </div>
  );
}
