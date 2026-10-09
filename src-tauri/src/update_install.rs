//! AppImage-only atomic replacement. Call only with signature-verified bytes.
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::{
    fs,
    io::{self, Write},
    path::Path,
};

pub fn is_appimage(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x7fELF") && bytes.get(8..11) == Some(b"AI\x02")
}

pub fn validate_path(path: &Path) -> io::Result<fs::Metadata> {
    use std::io::Read;
    let meta = fs::symlink_metadata(path)?;
    if !path.is_absolute() || !meta.is_file() || meta.uid() != unsafe { libc::geteuid() } {
        return Err(io::Error::other(
            "AppImage must be a regular file owned by the current user (not a symlink)",
        ));
    }
    let mut header = [0; 11];
    fs::File::open(path)?.read_exact(&mut header)?;
    if !is_appimage(&header) {
        return Err(io::Error::other("Not a type-2 AppImage"));
    }
    Ok(meta)
}

pub fn install(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if !is_appimage(bytes) {
        return Err(io::Error::other(
            "The signed download is not a type-2 AppImage",
        ));
    }
    let original = validate_path(path)?;
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::other("Missing AppImage directory"))?;
    // Reserve space for both staged image and durable recovery copy.
    use std::os::unix::ffi::OsStrExt;
    let name = std::ffi::CString::new(dir.as_os_str().as_bytes())?;
    let mut space = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(name.as_ptr(), space.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let space = unsafe { space.assume_init() };
    let available = u128::from(space.f_bavail) * u128::from(space.f_frsize);
    if available < bytes.len() as u128 + u128::from(original.len()) + 10 * 1024 * 1024 {
        return Err(io::Error::other(
            "Not enough space for the update and recovery copy",
        ));
    }
    let mut staged = tempfile::NamedTempFile::new_in(dir)?;
    staged.write_all(bytes)?;
    staged
        .as_file()
        .set_permissions(fs::Permissions::from_mode(
            (original.mode() & 0o777) | 0o100,
        ))?;
    staged.as_file().sync_all()?;
    let mut backup = tempfile::NamedTempFile::new_in(dir)?;
    io::copy(&mut fs::File::open(path)?, backup.as_file_mut())?;
    backup
        .as_file()
        .set_permissions(fs::Permissions::from_mode(
            (original.mode() & 0o777) | 0o100,
        ))?;
    backup.as_file().sync_all()?;
    let current = validate_path(path)?;
    if (
        original.dev(),
        original.ino(),
        original.len(),
        original.mtime(),
        original.mtime_nsec(),
    ) != (
        current.dev(),
        current.ino(),
        current.len(),
        current.mtime(),
        current.mtime_nsec(),
    ) {
        return Err(io::Error::other(
            "AppImage changed during update; retry after stopping other update managers",
        ));
    }
    let mut backup_name = path.as_os_str().to_os_string();
    backup_name.push(".previous");
    backup
        .persist(Path::new(&backup_name))
        .map_err(|e| e.error)?;
    fs::File::open(dir)?.sync_all()?;
    staged.persist(path).map_err(|e| e.error)?;
    fs::File::open(dir)?.sync_all()?;
    Ok(())
}
