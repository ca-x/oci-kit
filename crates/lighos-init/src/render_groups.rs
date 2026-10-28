use std::collections::BTreeSet;
use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use rustix::fs::{self as sys, AtFlags, Mode, OFlags, RenameFlags, ResolveFlags};
use rustix::io::Errno;

use crate::{MAX_PROFILE_BYTES, Profile, trusted_metadata};

const DROP_IN: &str = "30-oci-kit-render-groups.conf";
const TEMPORARY: &str = ".30-oci-kit-render-groups.tmp";

pub fn generate_systemd(arguments: &[PathBuf]) -> Result<()> {
    ensure!(
        matches!(arguments.len(), 1 | 3),
        "render node generator expects one or three output directories"
    );
    let Some(profile) = read_profile(Path::new("/etc/oci-kit/lightos.toml"))? else {
        return Ok(());
    };
    if profile.render_node_services.is_empty() {
        return Ok(());
    }
    generate(&arguments[0], &profile).context("render node generator failed")
}

fn read_profile(path: &Path) -> Result<Option<Profile>> {
    let descriptor = match sys::openat2(
        sys::CWD,
        path,
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
        ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_MAGICLINKS,
    ) {
        Ok(descriptor) => descriptor,
        Err(Errno::NOENT) => return Ok(None),
        Err(error) => return Err(error).context("opening render node generator profile"),
    };
    let file = File::from(descriptor);
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file(),
        "render node profile must be a regular file"
    );
    trusted_metadata(&metadata)?;
    let mut input = String::new();
    file.take((MAX_PROFILE_BYTES + 1) as u64)
        .read_to_string(&mut input)?;
    ensure!(
        input.len() <= MAX_PROFILE_BYTES,
        "render node profile exceeds 64 KiB"
    );
    Profile::parse(&input)
        .context("invalid render node profile")
        .map(Some)
}

fn render_groups() -> Result<BTreeSet<u32>> {
    let descriptor = match sys::openat2(
        sys::CWD,
        "/dev/dri",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
        ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_MAGICLINKS,
    ) {
        Ok(descriptor) => descriptor,
        Err(Errno::NOENT) => return Ok(BTreeSet::new()),
        Err(error) => return Err(error).context("opening render node directory"),
    };
    let directory = File::from(descriptor);
    trusted_metadata(&directory.metadata()?)?;
    let mut groups = BTreeSet::new();
    let mut count = 0;
    for entry in sys::Dir::read_from(&directory)? {
        let entry = entry?;
        let name = entry.file_name();
        if !name.to_bytes().starts_with(b"renderD") {
            continue;
        }
        count += 1;
        ensure!(count <= 128, "too many render nodes");
        let suffix = &name.to_bytes()[7..];
        ensure!(
            !suffix.is_empty() && suffix.iter().all(u8::is_ascii_digit),
            "invalid render node name"
        );
        let minor: u32 = std::str::from_utf8(suffix)?.parse()?;
        let device = File::from(sys::openat(
            &directory,
            name,
            OFlags::PATH | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
        let metadata = device.metadata()?;
        ensure!(
            metadata.file_type().is_char_device()
                && metadata.uid() == 0
                && sys::major(metadata.rdev()) == 226
                && sys::minor(metadata.rdev()) == minor
                && minor >= 128,
            "render node must be a root-owned DRM render character device"
        );
        let group = metadata.gid();
        ensure!(
            !matches!(group, 0 | 65535 | u32::MAX),
            "render node has a privileged or invalid group"
        );
        groups.insert(group);
    }
    Ok(groups)
}

fn generate(output: &Path, profile: &Profile) -> Result<()> {
    let groups = render_groups()?;
    if groups.is_empty() {
        return Ok(());
    }
    ensure!(
        output.is_absolute() && output != Path::new("/"),
        "invalid generator output directory"
    );
    let directory = File::from(sys::openat2(
        sys::CWD,
        output,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
        ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_MAGICLINKS,
    )?);
    trusted_metadata(&directory.metadata()?)?;
    let content = format!(
        "[Service]\nSupplementaryGroups={}\n",
        groups
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(" ")
    );
    let mut targets = Vec::new();
    for service in &profile.render_node_services {
        let target = service_directory(&directory, service)?;
        let present = existing_drop_in(&target, &content)?;
        targets.push((target, present));
    }
    for (target, present) in targets {
        if !present {
            write_drop_in(&target, content.as_bytes())?;
        }
    }
    Ok(())
}

fn service_directory(output: &File, service: &str) -> Result<File> {
    let name = format!("{service}.d");
    match sys::mkdirat(output, &name, Mode::from_bits_truncate(0o755)) {
        Ok(()) | Err(Errno::EXIST) => {}
        Err(error) => return Err(error.into()),
    }
    let directory = File::from(sys::openat(
        output,
        &name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?);
    trusted_metadata(&directory.metadata()?)?;
    Ok(directory)
}

fn existing_drop_in(directory: &File, content: &str) -> Result<bool> {
    let descriptor = match sys::openat(
        directory,
        DROP_IN,
        OFlags::PATH | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(descriptor) => descriptor,
        Err(Errno::NOENT) => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let metadata = File::from(descriptor).metadata()?;
    ensure!(
        metadata.is_file(),
        "generator drop-in must be a regular file"
    );
    trusted_metadata(&metadata)?;
    let mut existing = String::new();
    File::from(sys::openat(
        directory,
        DROP_IN,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?)
    .take(4096)
    .read_to_string(&mut existing)?;
    ensure!(
        existing == content,
        "generator output was not cleared or was modified"
    );
    Ok(true)
}

fn write_drop_in(directory: &File, content: &[u8]) -> Result<()> {
    let mode = Mode::from_bits_truncate(0o644);
    let mut file = File::from(sys::openat(
        directory,
        TEMPORARY,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        mode,
    )?);
    let result = (|| -> Result<()> {
        file.write_all(content)?;
        sys::fchmod(&file, mode)?;
        file.sync_all()?;
        sys::renameat_with(
            directory,
            TEMPORARY,
            directory,
            DROP_IN,
            RenameFlags::NOREPLACE,
        )?;
        Ok(())
    })();
    if result.is_err() {
        let _ = sys::unlinkat(directory, TEMPORARY, AtFlags::empty());
    }
    result
}
