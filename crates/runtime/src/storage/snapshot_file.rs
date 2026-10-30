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
    pub(crate) path: PathBuf,
    temporary: bool,
}
impl Drop for SnapshotFile {
    fn drop(&mut self) {
        if self.temporary {
            let _ = std::fs::remove_file(&self.path);
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
            path: path.into(),
            temporary: true,
        })
    }
    pub async fn open(path: &Path) -> Result<Self> {
        Ok(Self {
            file: tokio::fs::File::open(path).await?,
            path: path.into(),
            temporary: false,
        })
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
