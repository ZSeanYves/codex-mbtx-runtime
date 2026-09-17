//! Scoped archives accompany the existing local spawning path. The task-local
//! only crosses the inline raw-exec boundary; spawned readers receive explicit
//! owned handles, so concurrent calls cannot inherit one another's resources.

use super::StreamOutput;
use super::append_capped;
use codex_tools::output_archive::OutputArchive;
use std::io;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;

tokio::task_local! {
    pub(super) static ARCHIVES: Vec<OutputArchive>;
}

struct IncompleteArchive(Option<OutputArchive>);
impl Drop for IncompleteArchive {
    fn drop(&mut self) {
        if let Some(archive) = &self.0 {
            archive.interrupted();
        }
    }
}

pub(super) async fn read<R: AsyncRead + Unpin>(
    mut reader: R,
    max_bytes: usize,
    archive: Option<OutputArchive>,
) -> io::Result<StreamOutput<Vec<u8>>> {
    let archive = IncompleteArchive(archive);
    let mut bytes = Vec::new();
    let mut chunk = [0; 8192];
    loop {
        let n = reader.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        if let Some(archive) = &archive.0 {
            archive.append(&chunk[..n]);
        }
        append_capped(&mut bytes, &chunk[..n], max_bytes);
    }
    if let Some(archive) = &archive.0 {
        archive.finish();
    }
    Ok(StreamOutput {
        text: bytes,
        truncated_after_lines: None,
    })
}
