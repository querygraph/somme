//! Product-neutral primitives for authenticated, rate-aware command-line clients.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use reqwest::blocking::{Client, RequestBuilder, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub active_account: Option<String>,
    #[serde(default)]
    pub accounts: BTreeMap<String, Account>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Account {
    pub api_base: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

impl Account {
    pub fn logged_in(&self) -> bool {
        self.token.is_some()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RateLimit {
    pub limit: Option<u64>,
    pub remaining: Option<u64>,
    pub reset_epoch_seconds: Option<u64>,
    pub unlimited: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiResponse {
    pub body: Value,
    pub rate_limit: RateLimit,
}

#[derive(Debug, Clone)]
pub struct Product {
    pub slug: String,
    pub env_prefix: String,
    pub default_api_base: String,
}

impl Product {
    pub fn new(
        slug: impl Into<String>,
        env_prefix: impl Into<String>,
        default_api_base: impl Into<String>,
    ) -> Self {
        Self {
            slug: slug.into(),
            env_prefix: env_prefix.into(),
            default_api_base: default_api_base.into(),
        }
    }
    pub fn config_path(&self) -> Result<PathBuf> {
        config_path(&self.slug)
    }
    pub fn load_config(&self) -> Result<Config> {
        read_toml(&self.config_path()?)
    }
    pub fn save_config(&self, config: &Config) -> Result<()> {
        write_toml(&self.config_path()?, config)
    }
    pub fn environment_account(&self) -> Option<Account> {
        let key = std::env::var(format!("{}_API_KEY", self.env_prefix)).ok()?;
        let api_base = std::env::var(format!("{}_API_BASE", self.env_prefix))
            .unwrap_or_else(|_| self.default_api_base.clone());
        Some(Account {
            api_base,
            token: Some(key),
            email: None,
            tier: None,
            updated_at: None,
        })
    }
}

pub fn config_path(app: &str) -> Result<PathBuf> {
    let base =
        dirs::config_dir().ok_or_else(|| anyhow!("could not find the user config directory"))?;
    Ok(base.join(app).join("config.toml"))
}

pub fn read_toml<T>(path: &PathBuf) -> Result<T>
where
    T: Default + for<'de> Deserialize<'de>,
{
    if !path.exists() {
        return Ok(T::default());
    }
    let input =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    toml::from_str(&input).with_context(|| format!("could not parse {}", path.display()))
}

pub fn write_toml<T: Serialize>(path: &PathBuf, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    fs::write(path, toml::to_string(value)?)
        .with_context(|| format!("could not write {}", path.display()))
}

/// Install a checked-in section-1 manual into an explicit or standard directory.
pub fn install_manpage(
    name: &str,
    content: &str,
    requested_directory: Option<PathBuf>,
) -> Result<PathBuf> {
    let directories = requested_directory
        .map(|directory| vec![directory])
        .unwrap_or_else(default_manpage_directories);
    let mut failures = Vec::new();
    for directory in directories {
        match install_manpage_in(name, content, &directory) {
            Ok(destination) => return Ok(destination),
            Err(error) => failures.push(format!("{}: {error:#}", directory.display())),
        }
    }
    bail!(
        "could not install the {name} manual; try --dir ~/.local/share/man/man1 or use the required system privileges\n{}",
        failures.join("\n")
    )
}

pub fn install_manpage_in(name: &str, content: &str, directory: &Path) -> Result<PathBuf> {
    fs::create_dir_all(directory)
        .with_context(|| format!("could not create {}", directory.display()))?;
    let destination = directory.join(format!("{name}.1"));
    fs::write(&destination, content)
        .with_context(|| format!("could not write {}", destination.display()))?;
    Ok(destination)
}

pub fn default_manpage_directories() -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let user = home.join(".local/share/man/man1");
    if std::env::consts::OS == "macos" {
        vec![
            PathBuf::from("/opt/homebrew/share/man/man1"),
            PathBuf::from("/usr/local/share/man/man1"),
            user,
        ]
    } else {
        vec![
            PathBuf::from("/usr/local/share/man/man1"),
            user,
            PathBuf::from("/usr/share/man/man1"),
        ]
    }
}

pub fn active_account(config: &Config) -> Option<(&str, &Account)> {
    config
        .active_account
        .as_deref()
        .and_then(|name| config.accounts.get(name).map(|account| (name, account)))
        .or_else(|| {
            config
                .accounts
                .iter()
                .next()
                .map(|(name, account)| (name.as_str(), account))
        })
}

pub fn select_account<'a>(
    config: &'a Config,
    name: Option<&str>,
) -> Result<(&'a str, &'a Account)> {
    match name {
        Some(name) => config
            .accounts
            .get_key_value(name)
            .map(|(name, account)| (name.as_str(), account))
            .ok_or_else(|| anyhow!("no stored account named {name}")),
        None => active_account(config)
            .ok_or_else(|| anyhow!("no saved account; run the product login command")),
    }
}

pub struct ApiClient {
    client: Client,
    base: String,
    token: String,
}

impl ApiClient {
    pub fn from_account(account: &Account) -> Result<Self> {
        let token = account
            .token
            .clone()
            .ok_or_else(|| anyhow!("account is logged out; sign in or provide an API key"))?;
        Ok(Self {
            client: Client::new(),
            base: account.api_base.trim_end_matches('/').to_string(),
            token,
        })
    }
    pub fn get_json(&self, path: &str) -> Result<Value> {
        Ok(self.get(path)?.body)
    }
    pub fn get(&self, path: &str) -> Result<ApiResponse> {
        self.send(self.client.get(self.url(path)))
    }
    pub fn post(&self, path: &str, body: &Value) -> Result<ApiResponse> {
        self.send(self.client.post(self.url(path)).json(body))
    }
    pub fn patch(&self, path: &str, body: &Value) -> Result<ApiResponse> {
        self.send(self.client.patch(self.url(path)).json(body))
    }
    pub fn delete(&self, path: &str) -> Result<ApiResponse> {
        self.send(self.client.delete(self.url(path)))
    }
    pub fn request_json(&self, request: RequestBuilder) -> Result<Value> {
        authenticated_json(request, &self.token)
    }
    fn url(&self, path: &str) -> String {
        format!(
            "{}{}{}",
            self.base,
            if path.starts_with('/') { "" } else { "/" },
            path
        )
    }
    fn send(&self, request: RequestBuilder) -> Result<ApiResponse> {
        response_json(
            request
                .bearer_auth(&self.token)
                .send()
                .context("request failed")?,
        )
    }
}

pub fn authenticated_json(request: RequestBuilder, token: &str) -> Result<Value> {
    Ok(response_json(
        request
            .bearer_auth(token)
            .send()
            .context("request failed")?,
    )?
    .body)
}

pub fn response_json(response: Response) -> Result<ApiResponse> {
    let status = response.status();
    let rate_limit = rate_limit_from_headers(response.headers());
    let value = response.json::<Value>().unwrap_or(Value::Null);
    if !status.is_success() {
        let message = value
            .get("message")
            .and_then(Value::as_str)
            .or_else(|| value.get("error").and_then(Value::as_str))
            .unwrap_or("request failed");
        if status.as_u16() == 429 {
            let reset = rate_limit
                .reset_epoch_seconds
                .map(|value| format!("; resets at epoch {value}"))
                .unwrap_or_default();
            bail!("{status}: {message}{reset}");
        }
        bail!("{status}: {message}");
    }
    Ok(ApiResponse {
        body: value,
        rate_limit,
    })
}

pub fn rate_limit_from_headers(headers: &reqwest::header::HeaderMap) -> RateLimit {
    let text = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
    let limit_text = text("x-ratelimit-limit");
    RateLimit {
        unlimited: limit_text == Some("unlimited"),
        limit: limit_text.and_then(|value| value.parse().ok()),
        remaining: text("x-ratelimit-remaining").and_then(|value| value.parse().ok()),
        reset_epoch_seconds: text("x-ratelimit-reset").and_then(|value| value.parse().ok()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selects_active_account() {
        let mut config = Config::default();
        config.accounts.insert(
            "work".into(),
            Account {
                api_base: "https://example.test".into(),
                token: Some("secret".into()),
                email: None,
                tier: None,
                updated_at: None,
            },
        );
        config.active_account = Some("work".into());
        assert_eq!(select_account(&config, None).unwrap().0, "work");
    }
    #[test]
    fn parses_unlimited_admin_headers() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ratelimit-limit", "unlimited".parse().unwrap());
        headers.insert("x-ratelimit-remaining", "unlimited".parse().unwrap());
        assert!(rate_limit_from_headers(&headers).unlimited);
    }
    #[test]
    fn installs_a_manual_in_an_explicit_directory() {
        let directory = std::env::temp_dir().join(format!("somme-man-test-{}", std::process::id()));
        let destination =
            install_manpage("sample", ".TH SAMPLE 1", Some(directory.clone())).unwrap();
        assert_eq!(fs::read_to_string(&destination).unwrap(), ".TH SAMPLE 1");
        fs::remove_file(destination).unwrap();
        fs::remove_dir(directory).unwrap();
    }
}
