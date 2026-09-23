use super::is_peer_close;
use super::write_ack;
use pretty_assertions::assert_eq;
use tokio::io::AsyncReadExt;

#[tokio::test]
async fn closed_ack_peer_is_classified_as_nonfatal() {
    let (mut writer, reader) = tokio::io::duplex(32);
    drop(reader);

    let error = write_ack(&mut writer).await.expect_err("peer is closed");
    assert!(is_peer_close(&error));
}

#[tokio::test]
async fn successful_ack_is_written() -> anyhow::Result<()> {
    let (mut writer, mut reader) = tokio::io::duplex(32);
    let write = write_ack(&mut writer);
    let read = async {
        let mut bytes = [0; 9];
        reader.read_exact(&mut bytes).await?;
        Ok::<_, std::io::Error>(bytes)
    };
    let (write_result, bytes) = tokio::join!(write, read);
    write_result?;
    let bytes = bytes?;
    assert_eq!(&bytes, b"recorded\n");
    Ok(())
}

#[test]
fn unrelated_ack_errors_remain_fatal() {
    let error = std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid ack");
    assert!(!is_peer_close(&error));
}
