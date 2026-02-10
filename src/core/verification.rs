use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckConfig {
    pub name: String,
    pub command: String,
    #[serde(default = "default_required")]
    pub required: bool,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

fn default_required() -> bool {
    true
}

impl<'de> Deserialize<'de> for CheckConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            String(String),
            Object {
                name: String,
                command: String,
                #[serde(default = "default_required")]
                required: bool,
                #[serde(default)]
                timeout_ms: Option<u64>,
            },
        }

        match Wire::deserialize(deserializer)? {
            Wire::String(command) => Ok(Self {
                name: command.clone(),
                command,
                required: true,
                timeout_ms: None,
            }),
            Wire::Object {
                name,
                command,
                required,
                timeout_ms,
            } => Ok(Self {
                name,
                command,
                required,
                timeout_ms,
            }),
        }
    }
}

impl From<&str> for CheckConfig {
    fn from(command: &str) -> Self {
        Self {
            name: command.to_string(),
            command: command.to_string(),
            required: true,
            timeout_ms: None,
        }
    }
}

impl From<String> for CheckConfig {
    fn from(command: String) -> Self {
        Self {
            name: command.clone(),
            command,
            required: true,
            timeout_ms: None,
        }
    }
}

impl CheckConfig {
    #[must_use]
    pub fn new(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            command: command.into(),
            required: true,
            timeout_ms: None,
        }
    }

    #[must_use]
    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    #[must_use]
    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckResult {
    pub name: String,
    pub passed: bool,
    pub exit_code: i32,
    pub output: String,
    pub duration_ms: u64,
    pub required: bool,
}
