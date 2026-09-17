use super::*;
use pretty_assertions::assert_eq;

#[test]
fn complete_bytes_and_utf8_pages_survive_preview_size_and_cannot_escape() -> io::Result<()> {
    let root = tempfile::tempdir()?;
    let archive = OutputArchive::create(root.path(), "call", "build", "stderr")?;
    let text = "first 雪\n".repeat(5000) + "last diagnostic\n";
    for chunk in text.as_bytes().chunks(19) {
        archive.append(chunk);
    }
    let receipt = archive.finish();
    assert!(receipt.complete && receipt.eof);
    assert_eq!(receipt.bytes, text.len() as u64);
    assert_eq!(
        receipt.sha256,
        Some(format!("{:x}", Sha256::digest(text.as_bytes())))
    );
    let mut reconstructed = String::new();
    let mut offset = 0;
    loop {
        let page = read_output(root.path(), &receipt.resource_id, offset, 17)?;
        reconstructed.push_str(&page.text);
        offset = page.next_offset;
        if page.eof {
            break;
        }
    }
    assert_eq!(reconstructed, text);
    assert!(read_output(root.path(), "../secret", 0, 16).is_err());
    let other = tempfile::tempdir()?;
    assert!(read_output(other.path(), &receipt.resource_id, 0, 16).is_err());
    Ok(())
}

#[test]
fn interrupted_stream_is_not_complete_and_finish_does_not_rewrite_it() -> io::Result<()> {
    let root = tempfile::tempdir()?;
    let archive = OutputArchive::create(root.path(), "call", "run", "stdout")?;
    archive.append(b"prefix");
    let receipt = archive.interrupted();
    assert!(!receipt.complete && !receipt.eof);
    assert_eq!(archive.finish(), receipt);
    assert_eq!(
        read_output(root.path(), &receipt.resource_id, 0, 16)?.text,
        "prefix"
    );
    Ok(())
}
#[test]
fn archive_failure_retains_observed_bytes_and_marks_incomplete() {
    let directory = tempfile::tempdir().unwrap();
    let blocked = directory.path().join("not-a-directory");
    std::fs::write(&blocked, b"occupied").unwrap();
    let archive = super::OutputArchive::capture(&blocked, "call", "run", "stderr");
    archive.append("diagnostic tail 雪".as_bytes());
    let receipt = archive.finish();
    assert_eq!(receipt.bytes, "diagnostic tail 雪".len() as u64);
    assert!(receipt.eof);
    assert!(!receipt.complete);
    assert!(receipt.error.is_some());
    assert_eq!(receipt.sha256, None);
}
