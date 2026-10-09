pub mod active;
pub mod aliases;
pub mod assignments;
pub mod autostart;
pub mod backup;
pub mod buses;
pub mod channels;
pub mod eq;
pub mod eq_presets;
pub mod mic;
pub mod mic_presets;
pub mod outputs;
pub mod prefs;
pub mod profile_automation;
pub mod profiles;
pub mod seen;
pub mod window;
pub mod wireplumber;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

thread_local! {
    static CONFIG_QUIESCE_OWNER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static CONFIG_WRITE_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

struct ConfigWriteBarrier {
    lock: RwLock<()>,
    quiesced: AtomicBool,
}

impl ConfigWriteBarrier {
    fn shared(&self) -> std::io::Result<RwLockReadGuard<'_, ()>> {
        if self.quiesced.load(Ordering::Acquire) {
            return Err(std::io::Error::other(
                "configuration is quiesced while Mixweave restarts",
            ));
        }
        let guard = self
            .lock
            .read()
            .map_err(|_| std::io::Error::other("configuration write barrier is poisoned"))?;
        if self.quiesced.load(Ordering::Acquire) {
            return Err(std::io::Error::other(
                "configuration is quiesced while Mixweave restarts",
            ));
        }
        Ok(guard)
    }

    fn exclusive(&self) -> std::io::Result<RwLockWriteGuard<'_, ()>> {
        self.lock
            .write()
            .map_err(|_| std::io::Error::other("configuration write barrier is poisoned"))
    }
}

static CONFIG_WRITES: LazyLock<ConfigWriteBarrier> = LazyLock::new(|| ConfigWriteBarrier {
    lock: RwLock::new(()),
    quiesced: AtomicBool::new(false),
});

pub(crate) enum ConfigWritePermit {
    Shared(#[allow(dead_code)] RwLockReadGuard<'static, ()>),
    Nested,
    QuiesceOwner,
}

impl Drop for ConfigWritePermit {
    fn drop(&mut self) {
        if matches!(self, Self::Shared(_) | Self::Nested) {
            CONFIG_WRITE_DEPTH.set(CONFIG_WRITE_DEPTH.get().saturating_sub(1));
        }
    }
}

pub(crate) fn begin_config_write() -> std::io::Result<ConfigWritePermit> {
    if CONFIG_QUIESCE_OWNER.get() {
        return Ok(ConfigWritePermit::QuiesceOwner);
    }
    if CONFIG_WRITE_DEPTH.get() > 0 {
        CONFIG_WRITE_DEPTH.set(CONFIG_WRITE_DEPTH.get() + 1);
        return Ok(ConfigWritePermit::Nested);
    }
    let guard = CONFIG_WRITES.shared()?;
    CONFIG_WRITE_DEPTH.set(1);
    Ok(ConfigWritePermit::Shared(guard))
}

pub struct ConfigQuiesceGuard {
    guard: Option<RwLockWriteGuard<'static, ()>>,
    committed: bool,
}

pub struct ConfigSnapshotGuard {
    #[allow(dead_code)]
    guard: RwLockWriteGuard<'static, ()>,
}

/// Wait for in-flight configuration writers and exclude new ones for the
/// lifetime of the guard, without changing the permanent quiescence flag.
pub fn lock_config_snapshot() -> std::io::Result<ConfigSnapshotGuard> {
    Ok(ConfigSnapshotGuard {
        guard: CONFIG_WRITES.exclusive()?,
    })
}

impl ConfigQuiesceGuard {
    pub fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for ConfigQuiesceGuard {
    fn drop(&mut self) {
        if !self.committed {
            CONFIG_WRITES.quiesced.store(false, Ordering::Release);
        }
        self.guard.take();
        CONFIG_QUIESCE_OWNER.set(false);
    }
}

/// Wait for every in-flight configuration writer, then reject all future
/// writes until the current process exits. Call `commit` once destructive
/// replacement has begun; dropping an uncommitted guard reopens writes.
pub fn quiesce_config_writes() -> std::io::Result<ConfigQuiesceGuard> {
    let guard = CONFIG_WRITES.exclusive()?;
    CONFIG_QUIESCE_OWNER.set(true);
    CONFIG_WRITES.quiesced.store(true, Ordering::Release);
    Ok(ConfigQuiesceGuard {
        guard: Some(guard),
        committed: false,
    })
}

pub fn config_writes_quiesced() -> bool {
    CONFIG_WRITES.quiesced.load(Ordering::Acquire)
}

/// Seconds since the Unix epoch, or 0 if the clock predates it.
pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Move settings written under an older app name into the current config
/// directory. Never merge into or overwrite an existing current directory -
/// each past rename (Sink -> Sonux -> Mixweave) just adds one more name to
/// try, checked oldest-safe-first since only one can ever exist at a time.
fn migrate_legacy_config_dir_from(base: &std::path::Path) -> std::io::Result<()> {
    let current = base.join("mixweave");
    if current.exists() {
        return Ok(());
    }
    for legacy_name in ["sonux", "sink"] {
        let legacy = base.join(legacy_name);
        if legacy.exists() {
            std::fs::rename(legacy, &current)?;
            break;
        }
    }
    Ok(())
}

/// Same idea as [`migrate_legacy_config_dir_from`], for the separate
/// `$XDG_DATA_HOME` tree (currently just backups - see
/// [`backup::backups_dir`]).
fn migrate_legacy_data_dir_from(base: &std::path::Path) -> std::io::Result<()> {
    let current = base.join("mixweave");
    let legacy = base.join("sonux");
    if legacy.exists() && !current.exists() {
        std::fs::rename(legacy, current)?;
    }
    Ok(())
}

pub fn migrate_legacy_config_dir() -> std::io::Result<()> {
    if let Some(base) = dirs::config_dir() {
        migrate_legacy_config_dir_from(&base)?;
    }
    if let Some(base) = dirs::data_local_dir() {
        migrate_legacy_data_dir_from(&base)?;
    }
    Ok(())
}

/// Create Mixweave's config directory (and parents) with owner-only access -
/// routing rules and app history are nobody else's business. Used by every
/// save path that writes under `$XDG_CONFIG_HOME/mixweave`.
pub fn ensure_private_dir(path: &std::path::Path) -> std::io::Result<()> {
    let _write = begin_config_write()?;
    std::fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Write `contents` to `path` atomically: write a sibling temp file, fsync it,
/// then rename it over the target. A crash or power loss mid-write then leaves
/// either the old file or the complete new one - never a truncated file that
/// load paths silently discard (resetting the user's config). The parent
/// directory is created if missing; callers needing 0700 call
/// [`ensure_private_dir`] first, which this preserves.
pub fn write_atomic(path: &std::path::Path, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let _write = begin_config_write()?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Temp file in the same directory so the rename stays on one filesystem
    // (a cross-device rename is not atomic). Each writer needs its own file:
    // several command threads may persist independent settings concurrently.
    let (tmp, mut file) = loop {
        let mut candidate = path.as_os_str().to_owned();
        candidate.push(format!(
            ".{}.{}.tmp",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let candidate = std::path::PathBuf::from(candidate);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => break (candidate, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    let result = (|| {
        file.write_all(contents.as_ref())?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)?;
        // fsyncing the file makes its contents durable; fsyncing the parent
        // makes the rename itself durable across sudden power loss.
        #[cfg(unix)]
        if let Some(parent) = path.parent() {
            std::fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

pub fn remove_file(path: &std::path::Path) -> std::io::Result<()> {
    let _write = begin_config_write()?;
    std::fs::remove_file(path)
}

/// Factory reset: delete everything Mixweave or its legacy namespaces ever saved - the whole config
/// directory (channels, mixes, profiles, assignments, history, prefs)
/// and the WirePlumber routing rules.
pub fn wipe_all() -> Result<(), crate::error::SinkError> {
    if let Some(dir) = dirs::config_dir() {
        for name in ["mixweave", "sonux", "sink"] {
            let app_dir = dir.join(name);
            if app_dir.exists() {
                std::fs::remove_dir_all(&app_dir)?;
            }
        }
    }
    if let Some(dir) = dirs::data_local_dir() {
        for name in ["mixweave", "sonux"] {
            let app_dir = dir.join(name);
            if app_dir.exists() {
                std::fs::remove_dir_all(&app_dir)?;
            }
        }
    }
    wireplumber::remove_installation()?;
    Ok(())
}
