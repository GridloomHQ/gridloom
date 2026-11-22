//! Optimistic workbook revision shared by every explicit writer. Workbook
//! packages are replaced atomically, so locking the old file inode is unsafe.
use crate::CellError;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

pub(crate) type Fingerprint = [u8; 32];

#[derive(Debug)]
pub(crate) struct Baseline {
    pub path: PathBuf,
    pub digest: Fingerprint,
}

pub(crate) fn normalized_path(path: &Path) -> Result<PathBuf, CellError> {
    let file = path
        .file_name()
        .ok_or_else(|| detail("Target must name a workbook file"))?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let canonical = parent
        .canonicalize()
        .map_err(|e| detail(format!("Can't resolve workbook directory: {e}")))?;
    Ok(canonical.join(file))
}

pub(crate) fn detail(message: impl Into<String>) -> CellError {
    CellError::Sql(message.into())
}

/// Refuse workbook links before opening/importing a workbook by path.
pub fn check_workbook_file(_path: &Path, metadata: &fs::Metadata) -> Result<(), CellError> {
    if !metadata.is_file() {
        return Err(detail("Expected a regular workbook file, not a link."));
    }
    #[cfg(windows)]
    {
        use std::os::windows::{fs::MetadataExt, io::AsRawHandle};
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_REPARSE_POINT,
        };
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(detail(
                "Expected a regular workbook file, not a reparse point.",
            ));
        }
        let file =
            File::open(_path).map_err(|e| detail(format!("Can't inspect workbook links: {e}")))?;
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // The handle is borrowed from the live file, and info is a writable struct.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(detail(format!(
                "Can't inspect workbook links: {}",
                io::Error::last_os_error()
            )));
        }
        if info.nNumberOfLinks != 1 {
            return Err(detail("Do not use a hard-linked workbook."));
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(detail("Do not use a hard-linked workbook."));
        }
    }
    Ok(())
}

pub(crate) fn fingerprint(
    path: &Path,
    limit: Option<u64>,
) -> Result<Option<Fingerprint>, CellError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(detail(format!("Can't inspect workbook: {e}"))),
    };
    check_workbook_file(path, &metadata)?;
    if limit.is_some_and(|n| metadata.len() > n) {
        return Err(detail("Workbook exceeds the headless 64 MiB limit."));
    }
    // Supported workbook containers use rollback-journal mode; never hash
    // just the main image if SQLite may be reconstructing it from a sidecar.
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        match fs::symlink_metadata(sidecar) {
            Ok(_) => {
                return Err(detail(
                    "Workbook has a SQLite sidecar. Close other editors and try again.",
                ))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(detail(format!("Can't inspect workbook sidecars: {e}"))),
        }
    }
    let mut file = File::open(path).map_err(|e| detail(format!("Can't read workbook: {e}")))?;
    let mut hash = Sha256::new();
    let mut buf = [0; 32 * 1024];
    let mut total = 0;
    loop {
        let count = file
            .read(&mut buf)
            .map_err(|e| detail(format!("Can't fingerprint workbook: {e}")))?;
        if count == 0 {
            break;
        }
        if total == 0
            && count >= 20
            && &buf[..16] == b"SQLite format 3\0"
            && (buf[18] != 1 || buf[19] != 1)
        {
            return Err(detail("Unsupported journal mode in the workbook file."));
        }
        total += count as u64;
        if limit.is_some_and(|n| total > n) {
            return Err(detail("Workbook exceeds the headless 64 MiB limit."));
        }
        hash.update(&buf[..count]);
    }
    Ok(Some(hash.finalize().into()))
}

pub(crate) fn lock_parent(path: &Path) -> Result<File, CellError> {
    let parent = path.parent().expect("normalized workbook path");
    // No lock file to clean up and no inode replacement race on Unix.
    #[cfg(unix)]
    let lock = File::open(parent);
    // Where opening a directory as a plain file is unsupported, never
    // unlink the stable per-directory lock file, even after an error.
    #[cfg(not(unix))]
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(parent.join(".gridloom-workbook.lock"));
    let file = lock.map_err(|e| detail(format!("Can't open workbook lock: {e}")))?;
    file.try_lock().map_err(|e| match e {
        fs::TryLockError::WouldBlock => CellError::SaveConflict(
            "Another Gridloom session is saving in this folder. Try again when it finishes.".into(),
        ),
        fs::TryLockError::Error(e) => detail(format!("Can't lock workbook directory: {e}")),
    })?;
    Ok(file)
}

pub(crate) fn verify(
    path: &Path,
    expected: Option<Fingerprint>,
    limit: Option<u64>,
) -> Result<(), CellError> {
    if fingerprint(path, limit)? != expected {
        let message = if expected.is_some() {
            "Workbook changed on disk. Your edits are still open. Use Save As with a new filename to keep them, or reopen the original to see the latest version."
        } else {
            "Save destination already exists. Use Save As with a new filename."
        };
        Err(CellError::SaveConflict(message.into()))
    } else {
        Ok(())
    }
}
