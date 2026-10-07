use flate2::read::GzDecoder;
use serde_json::Value;
use std::io::Read;
use ui::i18n::LocalizedText;

pub(crate) fn decode(raw: &[u8]) -> Result<Value, LocalizedText> {
    if raw.len() < 12 || raw[0] >> 4 != 1 {
        return Err(LocalizedText::new("doubao.asr_header_invalid"));
    }
    let kind = raw[1] >> 4;
    let serialization = raw[2] >> 4;
    if serialization != 1 && !(kind == 15 && serialization == 0) {
        return Err(LocalizedText::new("doubao.asr_header_invalid"));
    }
    if kind != 9 && kind != 15 {
        return Err(LocalizedText::new("doubao.asr_type_unknown"));
    }
    let flags = raw[1] & 15;
    let mut offset = usize::from(raw[0] & 15) * 4;
    if flags & 1 != 0 {
        offset += 4;
    }
    if flags & 4 != 0 {
        offset += 4;
    }
    let error_code = if kind == 15 {
        let code = raw
            .get(offset..offset + 4)
            .and_then(|part| <[u8; 4]>::try_from(part).ok())
            .map(u32::from_be_bytes)
            .ok_or_else(|| LocalizedText::new("doubao.asr_code_missing"))?;
        offset += 4;
        Some(code)
    } else {
        None
    };
    let len = raw
        .get(offset..offset + 4)
        .and_then(|part| <[u8; 4]>::try_from(part).ok())
        .map(u32::from_be_bytes)
        .ok_or_else(|| LocalizedText::new("doubao.asr_length_missing"))? as usize;
    offset += 4;
    if len > 2 * 1024 * 1024 || raw.len() != offset + len {
        return Err(LocalizedText::new("doubao.asr_length_invalid"));
    }
    let mut payload = Vec::new();
    match raw[2] & 15 {
        0 => payload.extend_from_slice(&raw[offset..]),
        1 => {
            GzDecoder::new(&raw[offset..])
                .take(2 * 1024 * 1024 + 1)
                .read_to_end(&mut payload)
                .map_err(|_| LocalizedText::new("doubao.asr_decompress"))?;
        }
        _ => return Err(LocalizedText::new("doubao.asr_compression_invalid")),
    }
    if payload.len() > 2 * 1024 * 1024 {
        return Err(LocalizedText::new("doubao.asr_response_large"));
    }
    if let Some(code) = error_code {
        return Err(LocalizedText::new("doubao.asr_rejected").with_arg("code", code));
    }
    let value: Value = serde_json::from_slice(&payload)
        .map_err(|_| LocalizedText::new("doubao.asr_json_invalid"))?;
    if value["code"]
        .as_i64()
        .is_some_and(|code| code != 0 && code != 1000 && code != 20_000_000)
    {
        return Err(LocalizedText::new("doubao.asr_rejected")
            .with_arg("code", value["code"].as_i64().unwrap_or_default()));
    }
    Ok(value)
}
