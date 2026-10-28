use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result, bail, ensure};
use rustix::fs::{self as sys, AtFlags, FileType, Mode, OFlags, ResolveFlags};
use rustix::io::Errno;

pub const MARKER: &str = ".oci-kit-staging";
const MARKER_CONTENT: &str = "oci-kit offline staging v1\n";
static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const CHILD_RESOLVE: ResolveFlags = ResolveFlags::BENEATH
    .union(ResolveFlags::NO_SYMLINKS)
    .union(ResolveFlags::NO_MAGICLINKS)
    .union(ResolveFlags::NO_XDEV);

pub fn relative_path(path: &Path) -> Result<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => normalized.push(name),
            Component::CurDir => {}
            _ => bail!(
                "expected a relative path without parent traversal: {}",
                path.display()
            ),
        }
    }
    ensure!(
        !normalized.as_os_str().is_empty(),
        "the staging root itself cannot be selected"
    );
    ensure!(
        !normalized.starts_with(MARKER),
        "the staging marker is protected"
    );
    Ok(normalized)
}

fn open_root(root: &Path) -> Result<(PathBuf, File)> {
    let directory = File::from(
        sys::openat2(
            sys::CWD,
            root,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
            ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_MAGICLINKS,
        )
        .context("opening staging root without symlinks requires Linux openat2 (5.6+)")?,
    );
    let metadata = directory.metadata()?;
    let live_root = fs::metadata("/")?;
    ensure!(
        metadata.dev() != live_root.dev() || metadata.ino() != live_root.ino(),
        "refusing the running filesystem root"
    );
    let path = fs::read_link(proc_fd_path(&directory))
        .context("Linux procfs is required to identify the opened staging directory")?;
    reject_mounts(&path)?;
    Ok((path, directory))
}

fn proc_fd_path(file: &File) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd()))
}

fn open_child(directory: &File, path: &Path, flags: OFlags) -> rustix::io::Result<File> {
    sys::openat2(
        directory,
        path,
        flags | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
        CHILD_RESOLVE,
    )
    .map(File::from)
}

fn open_directory(directory: &File, path: &Path) -> rustix::io::Result<File> {
    open_child(directory, path, OFlags::RDONLY | OFlags::DIRECTORY)
}

fn kind_at(directory: &File, name: &OsStr) -> Result<Option<&'static str>> {
    match open_child(directory, Path::new(name), OFlags::PATH) {
        Ok(file) => Ok(Some(
            match FileType::from_raw_mode(sys::fstat(&file)?.st_mode) {
                FileType::Symlink => "symlink",
                FileType::Directory => "directory",
                FileType::RegularFile => "file",
                _ => "special",
            },
        )),
        Err(Errno::NOENT) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub fn mark(root: &Path) -> Result<PathBuf> {
    let (root, directory) = open_root(root)?;
    match sys::openat2(
        &directory,
        MARKER,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::from_bits_truncate(0o600),
        CHILD_RESOLVE,
    ) {
        Ok(descriptor) => {
            let mut file = File::from(descriptor);
            file.write_all(MARKER_CONTENT.as_bytes())?;
            file.sync_all()?;
            directory.sync_all()?;
        }
        Err(Errno::EXIST) => {
            validate_marker(&directory)?;
        }
        Err(error) => return Err(error.into()),
    }
    Ok(root)
}

fn validate_marker(directory: &File) -> Result<()> {
    let marker = open_child(directory, Path::new(MARKER), OFlags::PATH)?;
    ensure!(
        marker.metadata()?.is_file(),
        "staging marker must be a regular file"
    );
    let file = File::open(proc_fd_path(&marker))?;
    let mut content = String::new();
    file.take(MARKER_CONTENT.len() as u64 + 1)
        .read_to_string(&mut content)?;
    ensure!(content == MARKER_CONTENT, "invalid staging marker");
    Ok(())
}

pub struct StagingRoot {
    path: PathBuf,
    directory: File,
}

impl StagingRoot {
    pub fn open(root: &Path) -> Result<Self> {
        let (path, directory) = open_root(root)?;
        validate_marker(&directory)
            .context("not a marked offline rootfs; use rootfs-cleaner mark first")?;
        Ok(Self { path, directory })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn check_path(&self, relative: &Path) -> Result<()> {
        self.kind(relative).map(|_| ())
    }

    fn parent(&self, relative: &Path, create: bool) -> Result<Option<(File, OsString)>> {
        let relative = relative_path(relative)?;
        let name = relative
            .file_name()
            .context("missing leaf name")?
            .to_owned();
        let mut directory = self.directory.try_clone()?;
        for component in relative
            .parent()
            .context("missing parent path")?
            .components()
        {
            let path = Path::new(component.as_os_str());
            directory = match open_directory(&directory, path) {
                Ok(child) => child,
                Err(Errno::NOENT) if create => {
                    match sys::mkdirat(&directory, path, Mode::from_bits_truncate(0o755)) {
                        Ok(()) | Err(Errno::EXIST) => {}
                        Err(error) => return Err(error.into()),
                    }
                    open_directory(&directory, path)?
                }
                Err(Errno::NOENT) => return Ok(None),
                Err(error) => return Err(error.into()),
            };
        }
        Ok(Some((directory, name)))
    }

    pub fn kind(&self, relative: &Path) -> Result<Option<&'static str>> {
        let Some((directory, name)) = self.parent(relative, false)? else {
            return Ok(None);
        };
        kind_at(&directory, &name)
    }

    pub fn read_file(&self, relative: &Path, limit: usize) -> Result<Option<Vec<u8>>> {
        let Some((directory, name)) = self.parent(relative, false)? else {
            return Ok(None);
        };
        let file = match open_child(&directory, Path::new(&name), OFlags::PATH) {
            Ok(file) => file,
            Err(Errno::NOENT) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let metadata = file.metadata()?;
        ensure!(metadata.is_file(), "staged content must be a regular file");
        let maximum = u64::try_from(limit)?;
        ensure!(
            metadata.len() <= maximum,
            "staged content exceeds read limit"
        );
        let mut content = Vec::new();
        File::open(proc_fd_path(&file))?
            .take(maximum.checked_add(1).context("read limit overflow")?)
            .read_to_end(&mut content)?;
        ensure!(content.len() <= limit, "staged content exceeds read limit");
        Ok(Some(content))
    }

    pub fn remove(&self, relative: &Path) -> Result<()> {
        if let Some((directory, name)) = self.parent(relative, false)? {
            remove_entry(&directory, &name)?;
        }
        Ok(())
    }

    pub fn write_file(&self, relative: &Path, data: &[u8], mode: u32) -> Result<()> {
        let (directory, name) = self.parent(relative, true)?.context("missing parent")?;
        ensure!(
            !matches!(
                kind_at(&directory, &name)?,
                Some("symlink" | "directory" | "special")
            ),
            "refusing to replace a non-regular file"
        );
        let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = format!(".oci-kit-{}-{sequence}.tmp", std::process::id());
        let mut created = false;
        let result = (|| {
            let mut file = File::from(sys::openat2(
                &directory,
                temporary.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::CLOEXEC | OFlags::NOFOLLOW,
                Mode::from_bits_truncate(0o600),
                CHILD_RESOLVE,
            )?);
            created = true;
            file.set_permissions(fs::Permissions::from_mode(mode))?;
            file.write_all(data)?;
            file.sync_all()?;
            sys::renameat(&directory, temporary.as_str(), &directory, &name)?;
            directory.sync_all()?;
            Ok(())
        })();
        if result.is_err() && created {
            let _ = sys::unlinkat(&directory, temporary.as_str(), AtFlags::empty());
        }
        result
    }

    pub fn symlink(&self, relative: &Path, target: &Path) -> Result<()> {
        let (directory, name) = self.parent(relative, true)?.context("missing parent")?;
        match sys::readlinkat(&directory, &name, Vec::new()) {
            Ok(existing) if OsStr::from_bytes(existing.as_bytes()) == target.as_os_str() => {
                return Ok(());
            }
            Ok(_) => bail!("refusing to replace a different symlink"),
            Err(Errno::NOENT) => {}
            Err(error) => return Err(error.into()),
        }
        sys::symlinkat(target, &directory, &name)?;
        Ok(())
    }
}

fn remove_entry(directory: &File, name: &OsStr) -> Result<()> {
    match kind_at(directory, name)? {
        Some("directory") => {
            let child = open_directory(directory, Path::new(name))?;
            for entry in sys::Dir::read_from(&child)? {
                let entry = entry?;
                let name = entry.file_name().to_bytes();
                if name != b"." && name != b".." {
                    remove_entry(&child, OsStr::from_bytes(name))?;
                }
            }
            sys::unlinkat(directory, name, AtFlags::REMOVEDIR)?;
        }
        Some(_) => sys::unlinkat(directory, name, AtFlags::empty())?,
        None => {}
    }
    Ok(())
}

fn unescape_mount_path(value: &str) -> Result<PathBuf> {
    let mut decoded = Vec::new();
    let mut remaining = value.as_bytes();
    while let Some((&byte, tail)) = remaining.split_first() {
        if byte == b'\\' {
            ensure!(
                tail.len() >= 3 && tail[..3].iter().all(|digit| (b'0'..=b'7').contains(digit)),
                "invalid mountinfo escape"
            );
            let decoded_byte = (tail[0] - b'0') as u16 * 64
                + (tail[1] - b'0') as u16 * 8
                + (tail[2] - b'0') as u16;
            decoded.push(u8::try_from(decoded_byte).context("invalid mountinfo byte")?);
            remaining = &tail[3..];
        } else {
            decoded.push(byte);
            remaining = tail;
        }
    }
    use std::os::unix::ffi::OsStringExt;
    Ok(std::ffi::OsString::from_vec(decoded).into())
}

pub fn reject_mounts_in(path: &Path, mountinfo: &str) -> Result<()> {
    for line in mountinfo.lines() {
        let mount = line
            .split_whitespace()
            .nth(4)
            .context("malformed mountinfo")?;
        let mount = unescape_mount_path(mount)?;
        ensure!(
            !mount.starts_with(path),
            "refusing mounted path or subtree: {}",
            mount.display()
        );
    }
    Ok(())
}

fn reject_mounts(path: &Path) -> Result<()> {
    let mountinfo = fs::read_to_string("/proc/self/mountinfo")
        .context("Linux mountinfo is required for safe offline operations")?;
    reject_mounts_in(path, &mountinfo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_boundary_is_enforced_without_a_matching_mountinfo_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let root = StagingRoot {
            path: directory.path().into(),
            directory: File::open("/").unwrap(),
        };
        let error = root.kind(Path::new("proc/version")).unwrap_err();
        assert_eq!(error.downcast_ref::<Errno>(), Some(&Errno::XDEV));
    }
}
