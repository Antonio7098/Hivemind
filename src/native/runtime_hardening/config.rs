use super::*;

impl RuntimeHardeningConfig {
    #[must_use]
    pub fn from_env(env: &HashMap<String, String>) -> Self {
        let state_dir = env
            .get(STATE_DIR_ENV)
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                env.get("HIVEMIND_DATA_DIR")
                    .filter(|value| !value.trim().is_empty())
                    .map(PathBuf::from)
            })
            .or_else(default_hivemind_data_dir)
            .unwrap_or_else(|| PathBuf::from(".hivemind"));

        let state_db_path = env
            .get(STATE_DB_PATH_ENV)
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| state_dir.join("native").join("runtime-state.sqlite"));
        let secrets_store_path = env
            .get(SECRETS_STORE_PATH_ENV)
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| state_dir.join("native").join("secrets.enc.json"));
        let secrets_keyring_path = env
            .get(SECRETS_KEYRING_PATH_ENV)
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| state_dir.join("native").join("secrets.keyring"));

        Self {
            state_db_path,
            busy_timeout_ms: parse_u64_env(
                env,
                BUSY_TIMEOUT_ENV,
                DEFAULT_BUSY_TIMEOUT_MS,
                1,
                60_000,
            ),
            log_batch_size: parse_usize_env(
                env,
                LOG_BATCH_SIZE_ENV,
                DEFAULT_LOG_BATCH_SIZE,
                1,
                1024,
            ),
            log_flush_interval_ms: parse_u64_env(
                env,
                LOG_FLUSH_INTERVAL_ENV,
                DEFAULT_LOG_FLUSH_INTERVAL_MS,
                10,
                10_000,
            ),
            log_retention_days: parse_u64_env(
                env,
                LOG_RETENTION_DAYS_ENV,
                DEFAULT_LOG_RETENTION_DAYS,
                1,
                365,
            ),
            lease_ttl_ms: parse_u64_env(env, LEASE_TTL_ENV, DEFAULT_LEASE_TTL_MS, 1_000, 300_000),
            retention_sweep_interval_secs: parse_u64_env(
                env,
                RETENTION_SWEEP_INTERVAL_ENV,
                DEFAULT_RETENTION_SWEEP_INTERVAL_SECS,
                5,
                3600,
            ),
            secrets_store_path,
            secrets_keyring_path,
        }
    }
}

fn parse_u64_env(
    env: &HashMap<String, String>,
    key: &str,
    default: u64,
    min: u64,
    max: u64,
) -> u64 {
    env.get(key)
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .map_or(default, |value| value.clamp(min, max))
}

fn parse_usize_env(
    env: &HashMap<String, String>,
    key: &str,
    default: usize,
    min: usize,
    max: usize,
) -> usize {
    env.get(key)
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .map_or(default, |value| value.clamp(min, max))
}

fn default_hivemind_data_dir() -> Option<PathBuf> {
    std::env::var("HIVEMIND_DATA_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".hivemind")))
}
