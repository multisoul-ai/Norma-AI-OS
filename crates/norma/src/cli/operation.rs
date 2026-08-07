use std::io::Read;

pub(super) fn new_operation_id() -> String {
    let mut bytes = [0_u8; 16];
    if std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .is_err()
    {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        bytes.copy_from_slice(&now.to_le_bytes());
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

pub(super) fn non_blank_task(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        Err("task must not be blank".to_string())
    } else {
        Ok(value.to_string())
    }
}

pub(super) fn non_blank_operation_id(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        Err("operation ID must not be blank".to_string())
    } else {
        Ok(value.to_string())
    }
}

pub(super) fn non_blank_message(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        Err("message must not be blank".to_string())
    } else {
        Ok(value.to_string())
    }
}
