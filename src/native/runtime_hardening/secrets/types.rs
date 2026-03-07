use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct EncryptedSecretsFile {
    pub(super) version: u32,
    pub(super) nonce_hex: String,
    pub(super) ciphertext_hex: String,
    pub(super) tag_hex: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PlainSecretsFile {
    pub(super) version: u32,
    pub(super) secrets: BTreeMap<String, String>,
}
impl Default for PlainSecretsFile {
    fn default() -> Self {
        Self {
            version: 1,
            secrets: BTreeMap::new(),
        }
    }
}
