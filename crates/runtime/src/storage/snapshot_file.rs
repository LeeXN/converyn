//! OpenRaft streams disk snapshots instead of retaining the complete history in RAM.
use std::{
    io::Result,
    path::{Path, PathBuf},
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite, ReadBuf, SeekFrom};
#[derive(Debug)]
pub struct SnapshotFile {
    pub(crate) file: tokio::fs::File,
    temporary_path: Option<PathBuf>,
}
impl Drop for SnapshotFile {
    fn drop(&mut self) {
        let Some(path) = self.temporary_path.take() else {
            return;
        };
        let remove = move || {
            let _ = std::fs::remove_file(path);
        };
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn_blocking(remove);
        } else {
            remove();
        }
    }
}
impl SnapshotFile {
    pub async fn create(path: &Path) -> Result<Self> {
        let file = tokio::fs::OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(path)
            .await?;
        Ok(Self {
            file,
            temporary_path: Some(path.into()),
        })
    }
    pub async fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            file: tokio::fs::File::open(path).await?,
            temporary_path: None,
        })
    }

    pub(crate) async fn close(mut self) {
        if let Some(path) = &self.temporary_path {
            let _ = tokio::fs::remove_file(path).await;
            self.temporary_path = None;
        }
    }
}
impl AsyncRead for SnapshotFile {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        b: &mut ReadBuf<'_>,
    ) -> Poll<Result<()>> {
        Pin::new(&mut self.get_mut().file).poll_read(cx, b)
    }
}
impl AsyncWrite for SnapshotFile {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, b: &[u8]) -> Poll<Result<usize>> {
        Pin::new(&mut self.get_mut().file).poll_write(cx, b)
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        Pin::new(&mut self.get_mut().file).poll_flush(cx)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        Pin::new(&mut self.get_mut().file).poll_shutdown(cx)
    }
}
impl AsyncSeek for SnapshotFile {
    fn start_seek(self: Pin<&mut Self>, p: SeekFrom) -> Result<()> {
        Pin::new(&mut self.get_mut().file).start_seek(p)
    }
    fn poll_complete(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<u64>> {
        Pin::new(&mut self.get_mut().file).poll_complete(cx)
    }
}
