# Mixweave AppImage updates

Implemented design (supersedes the previous opt-in/manual-install plan):

- Public releases: `iishkas/Mixweave`, stable x86_64 AppImage only.
- Enabled by default for existing and new preferences. Rust checks after 30
  seconds, then every six hours while running. Enabling wakes the scheduler.
- Desktop notification and persistent in-app status precede download. Signed
  updates install automatically; restart remains explicit or next launch.
- Disabling cancels checking/downloading. Once file replacement holds the install
  lock it finishes before the toggle is saved. Manual updates remain available.
- Updater plugin verifies both artifact signature and signed version. The signing
  helper adds version metadata because the installed Tauri CLI's generic signer
  does not add it. No unsigned production manifests or fake signatures ship.
- `update_install.rs` stages and syncs in the destination directory, keeps a
  `.previous` recovery copy, then renames atomically and syncs the directory.
  Existing open descriptors remain on the old inode. No automatic rollback or
  configuration rollback is claimed.
- AppImage must be a type-2 regular file owned by the current user; symlinks are
  rejected. deb/rpm/development installations cannot self-update.
- No webview networking/updater permission. All downloads run in Rust over HTTPS.
- The download URL must be a versioned asset in the configured GitHub repository.
- Same updater for X11 and Wayland. Restart cleans AppImage runtime variables,
  forwards display-session variables to systemd-run, and waits for old-process exit.
- CI creates `Mixweave.AppImage`, `.sig`, `latest.json`, and `SHA256SUMS`, uploads
  into a draft, then publishes it as Latest. Private keys are GitHub Secrets;
  public key is a repository Variable and embedded in the binary.

Maintainer instructions and release tools: [Arabic guide](../قيتهب%20ابديت/ابديت%20قيتهب.md).

## Validation required before first public rollout

Automated: frontend build/tests, Rust tests including replacement/recovery and
invalid-file rejection, release-manifest tests, cryptographic signer verification.

Manual: real two-release update, offline/timeout, disk-full, unwritable directory,
Wayland and X11 restart during active audio, autostart, Gear Lever integration.
An automated test passing is not evidence that the graphical/audio matrix passed.
