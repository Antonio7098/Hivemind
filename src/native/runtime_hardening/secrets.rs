use super::*;
mod types;
use types::*;
mod crypto;
use crypto::*;
mod io;
use io::*;

#[derive(Debug, Clone)]
pub(super) struct NativeSecretsManager {
    store_path: PathBuf,
    keyring_path: PathBuf,
    env_master_key_b64: Option<String>,
}
impl NativeSecretsManager {
    pub(super) fn open(
        config: &RuntimeHardeningConfig,
        env: &HashMap<String, String>,
    ) -> Result<Self> {
        let env_master_key_b64 = env
            .get(SECRETS_MASTER_KEY_ENV)
            .filter(|value| !value.trim().is_empty())
            .cloned();
        let manager = Self {
            store_path: config.secrets_store_path.clone(),
            keyring_path: config.secrets_keyring_path.clone(),
            env_master_key_b64,
        };

        if let Some(parent) = manager.store_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                RuntimeHardeningError::new(
                    "native_secrets_store_dir_create_failed",
                    format!(
                        "Failed to create native secrets store dir '{}': {error}",
                        parent.display()
                    ),
                )
            })?;
        }
        if let Some(parent) = manager.keyring_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                RuntimeHardeningError::new(
                    "native_secrets_keyring_dir_create_failed",
                    format!(
                        "Failed to create native secrets keyring dir '{}': {error}",
                        parent.display()
                    ),
                )
            })?;
        }
        Ok(manager)
    }

    pub(super) fn set_secret(&self, name: &str, value: &str) -> Result<()> {
        if name.trim().is_empty() {
            return Err(RuntimeHardeningError::new(
                "native_secret_invalid_name",
                "Secret name must not be empty",
            ));
        }
        if value.trim().is_empty() {
            return Err(RuntimeHardeningError::new(
                "native_secret_invalid_value",
                "Secret value must not be empty",
            ));
        }

        let mut file = self.load_plain_file()?;
        file.secrets.insert(name.to_string(), value.to_string());
        self.persist_plain_file(&file)
    }

    pub(super) fn get_secret(&self, name: &str) -> Result<Option<String>> {
        let file = self.load_plain_file()?;
        Ok(file.secrets.get(name).cloned())
    }

    fn load_plain_file(&self) -> Result<PlainSecretsFile> {
        if !self.store_path.exists() {
            return Ok(PlainSecretsFile::default());
        }
        let encrypted_raw = fs::read(&self.store_path).map_err(|error| {
            RuntimeHardeningError::new(
                "native_secrets_store_read_failed",
                format!(
                    "Failed to read native secrets store '{}': {error}",
                    self.store_path.display()
                ),
            )
        })?;
        let envelope =
            serde_json::from_slice::<EncryptedSecretsFile>(&encrypted_raw).map_err(|error| {
                RuntimeHardeningError::new(
                    "native_secrets_store_decode_failed",
                    format!(
                        "Failed to decode encrypted native secrets store '{}': {error}",
                        self.store_path.display()
                    ),
                )
            })?;

        let key = self.load_or_create_master_key()?;
        let nonce = decode_hex(&envelope.nonce_hex, "native_secrets_nonce_decode_failed")?;
        let ciphertext = decode_hex(
            &envelope.ciphertext_hex,
            "native_secrets_ciphertext_decode_failed",
        )?;
        let expected_tag = decode_hex(&envelope.tag_hex, "native_secrets_tag_decode_failed")?;
        let actual_tag = compute_cipher_tag(&key, &nonce, &ciphertext);
        if expected_tag != actual_tag.as_slice() {
            return Err(RuntimeHardeningError::new(
                "native_secrets_decrypt_failed",
                "Native secrets ciphertext failed integrity verification",
            ));
        }
        let mut plaintext = xor_stream_cipher(&key, &nonce, &ciphertext);

        let parsed = serde_json::from_slice::<PlainSecretsFile>(&plaintext).map_err(|error| {
            RuntimeHardeningError::new(
                "native_secrets_plaintext_decode_failed",
                format!("Failed to decode native secrets plaintext: {error}"),
            )
        })?;
        wipe_buffer(&mut plaintext);
        Ok(parsed)
    }

    fn persist_plain_file(&self, file: &PlainSecretsFile) -> Result<()> {
        let mut plaintext = serde_json::to_vec(file).map_err(|error| {
            RuntimeHardeningError::new(
                "native_secrets_plaintext_encode_failed",
                format!("Failed to encode native secrets plaintext: {error}"),
            )
        })?;

        let key = self.load_or_create_master_key()?;
        let nonce_bytes = generate_nonce();
        let ciphertext = xor_stream_cipher(&key, &nonce_bytes, &plaintext);
        let tag = compute_cipher_tag(&key, &nonce_bytes, &ciphertext);

        let envelope = EncryptedSecretsFile {
            version: 1,
            nonce_hex: encode_hex(&nonce_bytes),
            ciphertext_hex: encode_hex(&ciphertext),
            tag_hex: encode_hex(&tag),
        };
        let serialized = serde_json::to_vec_pretty(&envelope).map_err(|error| {
            RuntimeHardeningError::new(
                "native_secrets_store_encode_failed",
                format!("Failed to encode encrypted native secrets store: {error}"),
            )
        })?;
        atomic_write(&self.store_path, &serialized)?;
        wipe_buffer(&mut plaintext);
        Ok(())
    }

    fn load_or_create_master_key(&self) -> Result<[u8; 32]> {
        if let Some(raw) = self.env_master_key_b64.as_deref() {
            let decoded = decode_hex(raw, "native_secrets_master_key_decode_failed")?;
            if decoded.len() != 32 {
                return Err(RuntimeHardeningError::new(
                    "native_secrets_master_key_invalid",
                    "HIVEMIND_NATIVE_SECRETS_MASTER_KEY must decode to 32 hex bytes",
                ));
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&decoded);
            return Ok(key);
        }

        if self.keyring_path.exists() {
            let encoded = fs::read_to_string(&self.keyring_path).map_err(|error| {
                RuntimeHardeningError::new(
                    "native_secrets_keyring_read_failed",
                    format!(
                        "Failed to read native secrets keyring '{}': {error}",
                        self.keyring_path.display()
                    ),
                )
            })?;
            let decoded = decode_hex(encoded.trim(), "native_secrets_master_key_decode_failed")?;
            if decoded.len() != 32 {
                return Err(RuntimeHardeningError::new(
                    "native_secrets_master_key_invalid",
                    format!(
                        "Native secrets keyring '{}' contains invalid key length",
                        self.keyring_path.display()
                    ),
                ));
            }
            let mut key = [0u8; 32];
            key.copy_from_slice(&decoded);
            return Ok(key);
        }

        let key = generate_master_key();
        let encoded = encode_hex(&key);
        atomic_write(&self.keyring_path, encoded.as_bytes())?;
        Ok(key)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn secrets_manager_encrypts_with_atomic_store() {
        let dir = tempdir().expect("temp dir");
        let mut env = HashMap::new();
        env.insert(
            STATE_DB_PATH_ENV.to_string(),
            dir.path()
                .join("state.sqlite")
                .to_string_lossy()
                .to_string(),
        );
        env.insert(
            SECRETS_STORE_PATH_ENV.to_string(),
            dir.path()
                .join("secrets.enc.json")
                .to_string_lossy()
                .to_string(),
        );
        env.insert(
            SECRETS_KEYRING_PATH_ENV.to_string(),
            dir.path()
                .join("secrets.keyring")
                .to_string_lossy()
                .to_string(),
        );

        let support = NativeRuntimeSupport::bootstrap(&env).expect("bootstrap should pass");
        let mut runtime_env = HashMap::new();
        runtime_env.insert("OPENROUTER_API_KEY".to_string(), "s3cr3t".to_string());
        support
            .ensure_secret_from_or_to_env(&mut runtime_env, "OPENROUTER_API_KEY")
            .expect("secret import");
        support.shutdown().expect("shutdown");

        let raw = fs::read_to_string(dir.path().join("secrets.enc.json")).expect("encrypted store");
        assert!(
            !raw.contains("s3cr3t"),
            "encrypted store must not contain plaintext secret"
        );

        let support_restart = NativeRuntimeSupport::bootstrap(&env).expect("bootstrap restart");
        let mut runtime_env2 = HashMap::new();
        support_restart
            .ensure_secret_from_or_to_env(&mut runtime_env2, "OPENROUTER_API_KEY")
            .expect("secret export");
        assert_eq!(
            runtime_env2.get("OPENROUTER_API_KEY"),
            Some(&"s3cr3t".to_string())
        );
        support_restart.shutdown().expect("shutdown restart");
    }

    #[test]
    #[cfg(unix)]
    fn atomic_write_restricts_secret_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().expect("temp dir");
        let path = dir.path().join("secret.keyring");
        atomic_write(&path, b"secret").expect("atomic write should pass");

        let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
