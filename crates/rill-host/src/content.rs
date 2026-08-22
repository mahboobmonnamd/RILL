use crate::Error;
use rill_attach::cold_content_socket_path;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

const RESPONSE_MAX: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentItem {
    pub kind: u8,
    pub sequence: u64,
    pub event_id: String,
    pub text: String,
}

pub fn content_submit(
    attach: &Path,
    execution: u64,
    text: &[u8],
) -> Result<Vec<ContentItem>, Error> {
    if text.len() > 64 * 1024 {
        return Err(Error::InvalidContent);
    }
    let mut stream = UnixStream::connect(cold_content_socket_path(attach))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut request = Vec::with_capacity(17 + text.len());
    request.extend_from_slice(b"RLC1");
    request.push(2);
    request.extend_from_slice(&execution.to_be_bytes());
    request.extend_from_slice(&(text.len() as u32).to_be_bytes());
    request.extend_from_slice(text);
    stream.write_all(&request)?;
    let mut response = Vec::new();
    stream
        .take((RESPONSE_MAX + 1) as u64)
        .read_to_end(&mut response)?;
    if response.len() > RESPONSE_MAX {
        return Err(Error::InvalidContent);
    }
    parse_content_snapshot(&response)
}

pub fn parse_content_snapshot(bytes: &[u8]) -> Result<Vec<ContentItem>, Error> {
    if bytes.len() < 9 || &bytes[..4] != b"RLC1" || bytes[4] != 0 {
        return Err(Error::InvalidContent);
    }
    let count =
        u32::from_be_bytes(bytes[5..9].try_into().map_err(|_| Error::InvalidContent)?) as usize;
    if count > 64 {
        return Err(Error::InvalidContent);
    }
    let mut cursor = 9usize;
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        let header_end = cursor.checked_add(15).ok_or(Error::InvalidContent)?;
        if header_end > bytes.len() {
            return Err(Error::InvalidContent);
        }
        let kind = bytes[cursor];
        let sequence = u64::from_be_bytes(
            bytes[cursor + 1..cursor + 9]
                .try_into()
                .map_err(|_| Error::InvalidContent)?,
        );
        let id_len = u16::from_be_bytes(
            bytes[cursor + 9..cursor + 11]
                .try_into()
                .map_err(|_| Error::InvalidContent)?,
        ) as usize;
        let text_len = u32::from_be_bytes(
            bytes[cursor + 11..header_end]
                .try_into()
                .map_err(|_| Error::InvalidContent)?,
        ) as usize;
        let id_end = header_end
            .checked_add(id_len)
            .ok_or(Error::InvalidContent)?;
        let text_end = id_end.checked_add(text_len).ok_or(Error::InvalidContent)?;
        if text_end > bytes.len() {
            return Err(Error::InvalidContent);
        }
        let event_id = std::str::from_utf8(&bytes[header_end..id_end])
            .map_err(|_| Error::InvalidContent)?
            .to_owned();
        let text = std::str::from_utf8(&bytes[id_end..text_end])
            .map_err(|_| Error::InvalidContent)?
            .to_owned();
        if event_id.is_empty() {
            return Err(Error::InvalidContent);
        }
        items.push(ContentItem {
            kind,
            sequence,
            event_id,
            text,
        });
        cursor = text_end;
    }
    if cursor != bytes.len() {
        return Err(Error::InvalidContent);
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::parse_content_snapshot;

    fn snapshot(include_text: bool) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RLC1");
        bytes.push(0);
        bytes.extend_from_slice(&1u32.to_be_bytes());
        bytes.push(1);
        bytes.extend_from_slice(&1u64.to_be_bytes());
        bytes.extend_from_slice(&3u16.to_be_bytes());
        bytes.extend_from_slice(&4u32.to_be_bytes());
        bytes.extend_from_slice(b"id1");
        if include_text {
            bytes.extend_from_slice(b"test");
        }
        bytes
    }

    #[test]
    fn content_snapshot_parses_authoritative_item() {
        let bytes = snapshot(true);
        let items = parse_content_snapshot(&bytes).expect("snapshot");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].event_id, "id1");
        assert_eq!(items[0].text, "test");
    }

    #[test]
    fn content_snapshot_rejects_truncated_item() {
        assert!(parse_content_snapshot(&snapshot(false)).is_err());
    }
}
