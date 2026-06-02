use anyhow::Context;
use fs4::FileExt;
use std::{
    fs::OpenOptions,
    future::Future,
    path::{Path, PathBuf},
};
use tokio::fs;

struct FileLock {
    file: std::fs::File,
}

impl FileLock {
    async fn acquire(lock_path: PathBuf) -> anyhow::Result<Self> {
        if let Some(parent) = lock_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        tokio::task::spawn_blocking(move || {
            let file = OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .open(&lock_path)
                .with_context(|| format!("open lock file {}", lock_path.display()))?;
            file.lock_exclusive()
                .with_context(|| format!("lock {}", lock_path.display()))?;
            Ok(Self { file })
        })
        .await
        .context("join lock acquisition")?
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

pub async fn with_lock<F, Fut, T>(path: &Path, f: F) -> anyhow::Result<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    let lock_path = PathBuf::from(format!("{}.lock", path.display()));
    let _guard = FileLock::acquire(lock_path).await?;
    f().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::fs::File;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_increments_are_serialized() {
        let temp = tempfile::tempdir().expect("tempdir");
        let counter_path = temp.path().join("tasks.json.counter");
        fs::write(&counter_path, b"0").await.expect("seed counter");

        let mut handles = Vec::new();
        for _ in 0..2 {
            let path = counter_path.clone();
            handles.push(tokio::spawn(async move {
                for _ in 0..500 {
                    with_lock(&path, || {
                        let path = path.clone();
                        async move {
                            let mut content = String::new();
                            File::open(&path)
                                .await?
                                .read_to_string(&mut content)
                                .await?;
                            let next = content.trim().parse::<u32>()? + 1;
                            let mut file = File::create(&path).await?;
                            file.write_all(next.to_string().as_bytes()).await?;
                            file.flush().await?;
                            Ok(())
                        }
                    })
                    .await?;
                }
                anyhow::Ok(())
            }));
        }

        for handle in handles {
            handle
                .await
                .expect("counter task panicked")
                .expect("increment counter");
        }

        let content = fs::read_to_string(counter_path)
            .await
            .expect("read counter");
        assert_eq!(content.trim(), "1000");
    }
}
