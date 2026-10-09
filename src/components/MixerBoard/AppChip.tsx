import { useState } from "react";
import type { AppIdentity } from "../../types";
import { AppIcon } from "../AppList/AppIcon";
import { Ms } from "../Icons";
import { MenuItem } from "../MenuItem";
import { Popover } from "../Popover";
import { useI18n } from "../../i18n";
import type { DragAppPayload } from "./useAppCardDrag";

export interface AppChipData {
  key: string;
  name: string;
  iconPath: string | null;
  active: boolean;
  streamIndexes: number[];
  identities: AppIdentity[];
  desktopId: string | null;
}

interface AppChipProps {
  app: AppChipData;
  originChannel: string;
  beginDrag: (
    event: React.PointerEvent<HTMLElement>,
    payload: DragAppPayload,
    onTap: (payload: DragAppPayload) => void,
  ) => void;
  isDraggingSource: boolean;
  /** Reassignment choices for the keyboard-accessible alternative to dragging. */
  destinations: ReadonlyArray<{ name: string; label: string; icon: string }>;
  onMove: (destination: string) => void;
}

/** A routed application: draggable onto another channel's well, or tap/Enter
 * to reassign it through a small picker instead. */
export function AppChip({
  app,
  originChannel,
  beginDrag,
  isDraggingSource,
  destinations,
  onMove,
}: Readonly<AppChipProps>) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const payload: DragAppPayload = {
    key: app.key,
    name: app.name,
    iconPath: app.iconPath,
    streamIndexes: app.streamIndexes,
    identities: app.identities,
    desktopId: app.desktopId,
    originChannel,
  };

  return (
    <div style={{ position: "relative" }}>
      <div
        className={"strip-app-chip" + (app.active ? " active" : "") + (isDraggingSource ? " dragging-source" : "")}
        role="button"
        tabIndex={0}
        title={t("mixer.channel.dragApp", { application: app.name })}
        onPointerDown={(event) => {
          if (event.button !== 0) return;
          beginDrag(event, payload, () => setOpen(true));
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            setOpen(true);
          }
        }}
      >
        <span className="strip-app-icon">
          <AppIcon iconPath={app.iconPath} />
        </span>
        <span className="strip-app-name">{app.name}</span>
        {app.active && <span className="strip-app-live" title={t("mixer.running")} />}
        <Ms name="drag_indicator" className="strip-app-grip" />
      </div>
      <Popover open={open} onClose={() => setOpen(false)} side="bottom" align="start" style={{ minWidth: 200 }}>
        {destinations.map((destination) => (
          <MenuItem
            key={destination.name}
            icon={destination.icon}
            onClick={() => {
              onMove(destination.name);
              setOpen(false);
            }}
          >
            {destination.label}
          </MenuItem>
        ))}
      </Popover>
    </div>
  );
}
