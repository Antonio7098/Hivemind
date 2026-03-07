use super::*;

pub(super) fn generate_master_key() -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(Uuid::new_v4().as_bytes());
    hasher.update(now_ms().to_le_bytes());
    hasher.update(std::process::id().to_le_bytes());
    let digest = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest[..32]);
    key
}
pub(super) fn generate_nonce() -> [u8; 24] {
    let mut hasher = Sha256::new();
    hasher.update(Uuid::new_v4().as_bytes());
    hasher.update(now_ms().to_le_bytes());
    hasher.update(std::process::id().to_le_bytes());
    let digest = hasher.finalize();
    let mut nonce = [0u8; 24];
    nonce.copy_from_slice(&digest[..24]);
    nonce
}
pub(super) fn xor_stream_cipher(key: &[u8; 32], nonce: &[u8], input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut offset = 0usize;
    let mut counter = 0u64;
    while offset < input.len() {
        let mut hasher = Sha256::new();
        hasher.update(key);
        hasher.update(nonce);
        hasher.update(counter.to_le_bytes());
        let block = hasher.finalize();
        let block_len = (input.len() - offset).min(block.len());
        for idx in 0..block_len {
            output.push(input[offset + idx] ^ block[idx]);
        }
        offset += block_len;
        counter = counter.saturating_add(1);
    }
    output
}
pub(super) fn compute_cipher_tag(key: &[u8; 32], nonce: &[u8], ciphertext: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(key);
    hasher.update(nonce);
    hasher.update(ciphertext);
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest[..32]);
    out
}
pub(super) fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}
pub(super) fn decode_hex(raw: &str, code: &str) -> Result<Vec<u8>> {
    if raw.len() % 2 != 0 {
        return Err(RuntimeHardeningError::new(
            code,
            format!("Invalid hex length {}", raw.len()),
        ));
    }
    let mut out = Vec::with_capacity(raw.len() / 2);
    let bytes = raw.as_bytes();
    for pair in bytes.chunks_exact(2) {
        let high = decode_hex_nibble(pair[0]).ok_or_else(|| {
            RuntimeHardeningError::new(code, format!("Invalid hex digit '{}'", pair[0] as char))
        })?;
        let low = decode_hex_nibble(pair[1]).ok_or_else(|| {
            RuntimeHardeningError::new(code, format!("Invalid hex digit '{}'", pair[1] as char))
        })?;
        out.push((high << 4) | low);
    }
    Ok(out)
}
pub(super) fn decode_hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
pub(super) fn wipe_buffer(buf: &mut [u8]) {
    for byte in buf.iter_mut() {
        *byte = 0;
    }
}
