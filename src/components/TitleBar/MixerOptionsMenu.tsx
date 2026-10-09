import { useRef, useState, type CSSProperties } from "react";
import { useMixerStore } from "../../store/mixer";
import { useStreamerModeStore } from "../../store/streamerMode";
import { Ms } from "../Icons";
import { MenuItem } from "../MenuItem";
import { Modal } from "../Modal";
import { Popover } from "../Popover";
import { useI18n } from "../../i18n";

const SUBMENU_CLOSE_DELAY_MS = 150;

/**
 * Mixer options and Profiles submenu. Streamer Mode toggles both the two-lane
 * layout and the selectable backend mix node via set_streamer_mode_enabled.
 */
export function MixerOptionsMenu({
  onManageProfiles,
}: Readonly<{
  onManageProfiles: () => void;
}>) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const streamerMode = useStreamerModeStore((s) => s.enabled);
  const setStreamerMode = useStreamerModeStore((s) => s.setEnabled);
  const [profilesOpen, setProfilesOpen] = useState(false);
  // Measure available space on each open; orient the submenu and chevron together.
  const [submenuOpensRight, setSubmenuOpensRight] = useState(false);
  // Distance from the trigger row to the panel edge on the side the submenu
  // opens toward, measured so the gap stays right whatever the panel padding.
  const [panelInset, setPanelInset] = useState(0);
  const [creatingProfile, setCreatingProfile] = useState(false);
  const [newProfileName, setNewProfileName] = useState("");
  const closeSubmenuTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const profilesAnchorRef = useRef<HTMLDivElement>(null);
  // Wider than the panel's own 220-280px range, so a "fits" verdict is never
  // wrong even at the widest possible profile-name content.
  const SUBMENU_WIDTH_ESTIMATE = 290 + 16;

  const profiles = useMixerStore((s) => s.profiles);
  const activeProfile = useMixerStore((s) => s.activeProfile);
  const loadProfile = useMixerStore((s) => s.loadProfile);
  const createBlankProfile = useMixerStore((s) => s.createBlankProfile);

  const close = () => {
    setOpen(false);
    setProfilesOpen(false);
  };

  const openSubmenu = () => {
    if (closeSubmenuTimer.current) {
      clearTimeout(closeSubmenuTimer.current);
      closeSubmenuTimer.current = null;
    }
    const anchor = profilesAnchorRef.current;
    if (anchor) {
      const rect = anchor.getBoundingClientRect();
      const spaceRight = window.innerWidth - rect.right;
      const opensRight = spaceRight >= SUBMENU_WIDTH_ESTIMATE;
      setSubmenuOpensRight(opensRight);
      const panel = anchor.closest<HTMLElement>(".menu")?.getBoundingClientRect();
      if (panel) setPanelInset(Math.max(0, opensRight ? panel.right - rect.right : rect.left - panel.left));
    }
    setProfilesOpen(true);
  };
  const scheduleCloseSubmenu = () => {
    closeSubmenuTimer.current = setTimeout(() => setProfilesOpen(false), SUBMENU_CLOSE_DELAY_MS);
  };

  const closeCreateModal = () => {
    setCreatingProfile(false);
    setNewProfileName("");
  };
  const submitNewProfile = () => {
    const name = newProfileName.trim();
    if (!name) return;
    void createBlankProfile(name, true);
    closeCreateModal();
    close();
  };

  return (
    <div className="mode-menu">
      <button
        type="button"
        className={"mode-button" + (open ? " open" : "") + (streamerMode ? " enabled" : "")}
        aria-label={t("mixer.options.button")}
        aria-expanded={open}
        title={streamerMode ? t("mixer.options.titleStreamerOn") : t("mixer.options.button")}
        onClick={() => setOpen((o) => !o)}
      >
        <Ms name="tune" />
        <Ms name="expand_more" className="chevron" />
      </button>

      <Popover
        open={open}
        onClose={close}
        side="bottom"
        align="end"
        style={{
          width: "min(282px, calc(100vw - 42px))",
          padding: 15,
          background: "var(--bg-popover)",
          border: "1px solid var(--border-popover)",
          borderRadius: 13,
          boxShadow: "var(--shadow-popover)",
        }}
      >
        <div className="mode-panel-inner">
          <button
            type="button"
            className="streamer-switch"
            role="switch"
            aria-checked={streamerMode}
            onClick={() => {
              void setStreamerMode(!streamerMode).then(() => {
                const { error } = useStreamerModeStore.getState();
                if (error) useMixerStore.setState({ error });
              });
              close();
            }}
          >
            <Ms name="stream" />
            <span>{t("mixer.options.streamerMode")}</span>
            <span className="switch-track" aria-hidden="true" />
          </button>
          <p className="mode-panel-desc">{t("mixer.options.streamerModeDesc")}</p>

          <div className="menu-sep" />

          <div
            className="profiles-submenu-anchor"
            ref={profilesAnchorRef}
            onMouseEnter={openSubmenu}
            onMouseLeave={scheduleCloseSubmenu}
            onFocus={openSubmenu}
            onBlur={scheduleCloseSubmenu}
          >
            <MenuItem
              className="profiles-submenu-trigger"
              icon="bookmarks"
              onClick={() => {
                if (!profilesOpen) openSubmenu();
                else setProfilesOpen(false);
              }}
            >
              {t("profiles.title")}
              <Ms name={submenuOpensRight ? "chevron_right" : "chevron_left"} className="chevron" />
            </MenuItem>
            {profilesOpen && (
              <div
                className={"profiles-submenu" + (submenuOpensRight ? " opens-right" : "")}
                style={{ "--panel-inset": `${panelInset}px` } as CSSProperties}
                role="menu"
              >
                {profiles.map((profile) => (
                  <MenuItem
                    key={profile.name}
                    icon={profile.name === activeProfile ? "check" : "bookmark"}
                    selected={profile.name === activeProfile}
                    onClick={() => {
                      if (profile.name !== activeProfile) void loadProfile(profile.name);
                      close();
                    }}
                  >
                    {profile.name}
                  </MenuItem>
                ))}
                <div className="menu-sep" />
                <MenuItem icon="add" onClick={() => setCreatingProfile(true)}>
                  {t("profiles.new")}
                </MenuItem>
                <MenuItem
                  icon="settings"
                  onClick={() => {
                    close();
                    onManageProfiles();
                  }}
                >
                  {t("profiles.menu.manage")}
                </MenuItem>
              </div>
            )}
          </div>
        </div>
      </Popover>

      <Modal open={creatingProfile} onClose={closeCreateModal} title={t("profiles.new")}>
        <input
          className="menu-input"
          placeholder={t("profiles.create.placeholder")}
          value={newProfileName}
          autoFocus
          maxLength={32}
          onChange={(e) => setNewProfileName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") submitNewProfile();
          }}
        />
        <div className="modal-btns">
          <button type="button" className="modal-btn primary" onClick={submitNewProfile} disabled={!newProfileName.trim()}>
            {t("profiles.create.action")}
          </button>
          <button type="button" className="modal-btn" onClick={closeCreateModal}>
            {t("common.action.cancel")}
          </button>
        </div>
      </Modal>
    </div>
  );
}
