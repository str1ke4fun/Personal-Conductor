use anyhow::Context;
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};
use walkdir::{DirEntry, WalkDir};

pub async fn recently_modified(cwd: &Path, within: Duration) -> anyhow::Result<Vec<PathBuf>> {
    let cwd = cwd.to_path_buf();
    tokio::task::spawn_blocking(move || scan_recently_modified(&cwd, within))
        .await
        .context("join filewatch scan")?
}

fn scan_recently_modified(cwd: &Path, within: Duration) -> anyhow::Result<Vec<PathBuf>> {
    let cutoff = SystemTime::now()
        .checked_sub(within)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let mut files = Vec::new();
    for entry in WalkDir::new(cwd)
        .into_iter()
        .filter_entry(|entry| !is_ignored(entry))
    {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let metadata = entry.metadata()?;
        let modified = metadata.modified()?;
        if modified >= cutoff {
            files.push((modified, entry.path().to_path_buf()));
        }
    }
    files.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(files.into_iter().map(|(_, path)| path).collect())
}

fn is_ignored(entry: &DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|name| {
            matches!(
                name,
                "node_modules" | ".git" | "state" | "dist" | "build" | "target"
            )
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use filetime::{set_file_mtime, FileTime};

    #[tokio::test]
    async fn recently_modified_filters_by_mtime_and_ignored_dirs() {
        let temp = tempfile::tempdir().expect("tempdir");
        let now = SystemTime::now();
        let recent_a = temp.path().join("recent-a.md");
        let recent_b = temp.path().join("recent-b.md");
        let old = temp.path().join("old.md");
        let ignored_dir = temp.path().join("state");
        let ignored = ignored_dir.join("ignored.md");

        tokio::fs::write(&recent_a, b"a")
            .await
            .expect("write recent a");
        tokio::fs::write(&recent_b, b"b")
            .await
            .expect("write recent b");
        tokio::fs::write(&old, b"old").await.expect("write old");
        tokio::fs::create_dir(&ignored_dir)
            .await
            .expect("create ignored dir");
        tokio::fs::write(&ignored, b"ignored")
            .await
            .expect("write ignored");

        set_file_mtime(
            &recent_a,
            FileTime::from_system_time(now - Duration::from_secs(60)),
        )
        .expect("set recent a mtime");
        set_file_mtime(
            &recent_b,
            FileTime::from_system_time(now - Duration::from_secs(120)),
        )
        .expect("set recent b mtime");
        set_file_mtime(
            &old,
            FileTime::from_system_time(now - Duration::from_secs(10 * 60)),
        )
        .expect("set old mtime");
        set_file_mtime(&ignored, FileTime::from_system_time(now)).expect("set ignored mtime");

        let files = recently_modified(temp.path(), Duration::from_secs(5 * 60))
            .await
            .expect("scan recent files");

        assert_eq!(files, vec![recent_a, recent_b]);
    }
}
