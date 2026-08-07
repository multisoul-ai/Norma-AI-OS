//! Terminal paste 协议编码：统一逻辑换行，并按应用声明包装 bracketed paste。

const BRACKETED_PASTE_BEGIN: &[u8] = b"\x1b[200~";
const BRACKETED_PASTE_END: &[u8] = b"\x1b[201~";

pub(crate) fn encode_paste(text: &str, bracketed: bool) -> Vec<u8> {
    let line_break = if bracketed {
        b"\n".as_slice()
    } else {
        b"\r\n".as_slice()
    };
    let normalized = normalize_line_endings(text, line_break);
    if !bracketed {
        return normalized;
    }

    let mut bytes = Vec::with_capacity(
        BRACKETED_PASTE_BEGIN.len() + normalized.len() + BRACKETED_PASTE_END.len(),
    );
    bytes.extend_from_slice(BRACKETED_PASTE_BEGIN);
    bytes.extend_from_slice(&normalized);
    bytes.extend_from_slice(BRACKETED_PASTE_END);
    bytes
}

fn normalize_line_endings(text: &str, line_break: &[u8]) -> Vec<u8> {
    if !text.contains(['\r', '\n']) {
        return text.as_bytes().to_vec();
    }

    let bytes = text.as_bytes();
    let mut normalized = Vec::with_capacity(bytes.len().saturating_add(1));
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                normalized.extend_from_slice(line_break);
                index += usize::from(bytes.get(index + 1) == Some(&b'\n'));
            }
            b'\n' => normalized.extend_from_slice(line_break),
            byte => normalized.push(byte),
        }
        index += 1;
    }
    normalized
}
