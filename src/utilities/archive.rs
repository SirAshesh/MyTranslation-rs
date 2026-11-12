use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn extract_archive(archive_path: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination)
        .map_err(|e| format!("Failed to create destination {}: {}", destination.display(), e))?;

    let name = archive_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        let status = Command::new("tar")
            .arg("-xzf")
            .arg(archive_path)
            .arg("-C")
            .arg(destination)
            .status()
            .map_err(|e| format!("Failed to run tar command: {}", e))?;

        if !status.success() {
            return Err(format!("tar extraction failed with code {:?}", status.code()));
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(exec) = find_file_recursive(destination, "llama-server") {
                let _ = fs::set_permissions(&exec, fs::Permissions::from_mode(0o755));
            }
        }

        return Ok(());
    }

    extract_zip(archive_path, destination)
}

pub fn extract_zip(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let archive_file = fs::File::open(archive_path)
        .map_err(|e| format!("Failed to open {}: {}", archive_path.display(), e))?;
    let mut archive = zip::ZipArchive::new(archive_file)
        .map_err(|e| format!("Failed to read {}: {}", archive_path.display(), e))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("Failed to read archive entry {}: {}", index, e))?;
        let Some(relative_path) = entry.enclosed_name() else {
            continue;
        };
        let target_path = destination.join(relative_path);

        if entry.is_dir() {
            fs::create_dir_all(&target_path)
                .map_err(|e| format!("Failed to create {}: {}", target_path.display(), e))?;
            continue;
        }

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
        }
        let mut output = fs::File::create(&target_path)
            .map_err(|e| format!("Failed to create {}: {}", target_path.display(), e))?;
        io::copy(&mut entry, &mut output)
            .map_err(|e| format!("Failed to extract {}: {}", target_path.display(), e))?;
    }
    Ok(())
}

pub fn find_file_recursive(directory: &Path, file_name: &str) -> Option<PathBuf> {
    for entry in fs::read_dir(directory).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file_recursive(&path, file_name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|name| name.eq_ignore_ascii_case(file_name)) {
            return Some(path);
        }
    }
    None
}
