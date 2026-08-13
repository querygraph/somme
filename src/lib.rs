//! Product-neutral primitives for authenticated, rate-aware command-line clients.

use std::{
    collections::BTreeMap,
    fmt,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

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
    #[serde(flatten)]
    pub settings: BTreeMap<String, toml::Value>,
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
    #[serde(flatten)]
    pub settings: BTreeMap<String, toml::Value>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiResponse {
    pub body: Value,
    pub rate_limit: RateLimit,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

impl ApiResponse {
    pub fn new(body: Value, rate_limit: RateLimit) -> Self {
        Self {
            body,
            rate_limit,
            request_id: None,
        }
    }

    /// Replace a response body while preserving quota and request metadata.
    ///
    /// Derived CLIs that filter or reshape a successful response should use
    /// this instead of rebuilding `ApiResponse` and losing correlation data.
    pub fn with_body(self, body: Value) -> Self {
        Self { body, ..self }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct FairUseAlternative {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct FairUse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shortfall: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_at: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternatives: Vec<FairUseAlternative>,
}

/// A non-successful JSON response returned by an authenticated API.
///
/// `ApiClient` continues to return `anyhow::Result` so transport and local
/// configuration errors stay convenient for derived CLIs. API failures retain
/// this concrete type in the error chain and can be inspected with
/// [`as_api_error`].
#[derive(Debug, Clone, Serialize)]
pub struct ApiError {
    pub status: u16,
    pub message: String,
    pub body: Value,
    pub rate_limit: RateLimit,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fair_use: Option<FairUse>,
}

impl ApiError {
    pub fn is_rate_limited(&self) -> bool {
        self.status == 429
    }

    pub fn alternatives(&self) -> &[FairUseAlternative] {
        self.fair_use
            .as_ref()
            .map(|details| details.alternatives.as_slice())
            .unwrap_or_default()
    }

    pub fn metadata_lines(&self) -> Vec<String> {
        let mut lines = metadata_lines(&self.rate_limit, self.request_id.as_deref());
        if let Some(fair_use) = &self.fair_use {
            let mut details = Vec::new();
            if let Some(scope) = &fair_use.scope {
                details.push(format!("scope {scope}"));
            }
            if self.rate_limit.cost.is_none()
                && let Some(cost) = fair_use.cost
            {
                details.push(format!("cost {cost} units"));
            }
            if self.rate_limit.remaining.is_none()
                && let Some(remaining) = fair_use.remaining
            {
                details.push(format!("{remaining} remaining"));
            }
            if let Some(shortfall) = fair_use.shortfall {
                details.push(format!("shortfall {shortfall}"));
            }
            if !details.is_empty() {
                lines.push(format!("fair use: {}", details.join("; ")));
            }
            if let Some(retry_at) = &fair_use.retry_at {
                lines.push(format!("retry at: {retry_at}"));
            }
            lines.extend(fair_use.alternatives.iter().map(format_alternative));
        }
        lines
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = reqwest::StatusCode::from_u16(self.status)
            .map(|value| value.to_string())
            .unwrap_or_else(|_| self.status.to_string());
        write!(formatter, "{status}: {}", self.message)?;
        for line in self.metadata_lines() {
            write!(formatter, "\n{line}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ApiError {}

pub fn as_api_error(error: &anyhow::Error) -> Option<&ApiError> {
    error.downcast_ref::<ApiError>()
}

pub fn response_metadata_lines(response: &ApiResponse) -> Vec<String> {
    metadata_lines(&response.rate_limit, response.request_id.as_deref())
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
        let path = self.config_path()?;
        if path.exists() {
            return read_toml(&path);
        }
        let legacy = legacy_config_path(&self.slug)?;
        if legacy.exists() {
            return read_toml(&legacy);
        }
        Ok(Config::default())
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
            settings: BTreeMap::new(),
        })
    }
}

pub fn config_path(app: &str) -> Result<PathBuf> {
    if app.is_empty()
        || !app.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        bail!(
            "application config name must contain only ASCII letters, numbers, hyphens, or underscores"
        )
    }
    let home = dirs::home_dir().ok_or_else(|| anyhow!("could not find the user home directory"))?;
    Ok(home.join(format!(".{app}")))
}

pub fn legacy_config_path(app: &str) -> Result<PathBuf> {
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
        #[cfg(unix)]
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .with_context(|| format!("could not secure {}", parent.display()))?;
    }
    let serialized = toml::to_string(value)?;
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = path.with_extension(format!("toml.{}.{}.tmp", std::process::id(), suffix));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(&temporary)
        .with_context(|| format!("could not create {}", temporary.display()))?;
    if let Err(error) = file
        .write_all(serialized.as_bytes())
        .and_then(|_| file.sync_all())
    {
        let _ = fs::remove_file(&temporary);
        return Err(error).with_context(|| format!("could not write {}", path.display()));
    }
    drop(file);
    fs::rename(&temporary, path)
        .with_context(|| format!("could not replace {}", path.display()))?;
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("could not secure {}", path.display()))?;
    Ok(())
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
            (config.accounts.len() == 1)
                .then(|| {
                    config
                        .accounts
                        .iter()
                        .next()
                        .map(|(name, account)| (name.as_str(), account))
                })
                .flatten()
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
    let mut rate_limit = rate_limit_from_headers(response.headers());
    let request_id = response
        .headers()
        .get("x-request-id")
        .or_else(|| response.headers().get("request-id"))
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let bytes = response.bytes().context("could not read response body")?;
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice::<Value>(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    if !status.is_success() {
        let message = value
            .get("message")
            .and_then(Value::as_str)
            .or_else(|| value.get("error").and_then(Value::as_str))
            .or_else(|| value.get("title").and_then(Value::as_str))
            .or_else(|| value.as_str())
            .or_else(|| status.canonical_reason())
            .unwrap_or("request failed")
            .to_owned();
        let fair_use = fair_use_from_body(&value);
        if let Some(details) = &fair_use {
            rate_limit.cost = rate_limit.cost.or(details.cost);
            rate_limit.remaining = rate_limit.remaining.or(details.remaining);
        }
        let request_id = request_id.or_else(|| body_string(&value, &["requestId", "request_id"]));
        return Err(ApiError {
            status: status.as_u16(),
            message,
            body: value,
            rate_limit,
            request_id,
            fair_use,
        }
        .into());
    }
    Ok(ApiResponse {
        body: value,
        rate_limit,
        request_id,
    })
}

pub fn rate_limit_from_headers(headers: &reqwest::header::HeaderMap) -> RateLimit {
    let text = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
    let limit_text = text("x-ratelimit-limit");
    RateLimit {
        unlimited: limit_text.is_some_and(|value| value.eq_ignore_ascii_case("unlimited")),
        limit: limit_text.and_then(|value| value.parse().ok()),
        remaining: text("x-ratelimit-remaining").and_then(|value| value.parse().ok()),
        reset_epoch_seconds: text("x-ratelimit-reset").and_then(|value| value.parse().ok()),
        cost: text("x-ratelimit-cost").and_then(|value| value.parse().ok()),
        retry_after: text("retry-after")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        warning: text("x-ratelimit-warning")
            .or_else(|| text("x-fair-use-warning"))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        tier: text("x-ratelimit-tier")
            .or_else(|| text("x-fair-use-tier"))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
    }
}

fn metadata_lines(rate_limit: &RateLimit, request_id: Option<&str>) -> Vec<String> {
    let mut lines = Vec::new();
    let has_quota = rate_limit.unlimited
        || rate_limit.limit.is_some()
        || rate_limit.remaining.is_some()
        || rate_limit.cost.is_some()
        || rate_limit.reset_epoch_seconds.is_some()
        || rate_limit.retry_after.is_some()
        || rate_limit.tier.is_some();
    if has_quota {
        let mut details = Vec::new();
        if let Some(cost) = rate_limit.cost {
            details.push(format!("cost {cost} units"));
        }
        if rate_limit.unlimited {
            details.push("daily unlimited".to_owned());
        } else {
            match (rate_limit.remaining, rate_limit.limit) {
                (Some(remaining), Some(limit)) => {
                    details.push(format!("{remaining}/{limit} remaining"));
                }
                (Some(remaining), None) => details.push(format!("{remaining} remaining")),
                (None, Some(limit)) => details.push(format!("limit {limit}")),
                (None, None) => {}
            }
        }
        if let Some(reset) = rate_limit.reset_epoch_seconds {
            details.push(format!("reset epoch {reset}"));
        }
        if let Some(retry_after) = &rate_limit.retry_after {
            details.push(format!("retry after {retry_after}"));
        }
        if let Some(tier) = &rate_limit.tier {
            details.push(format!("tier {tier}"));
        }
        if let Some(request_id) = request_id {
            details.push(format!("request {request_id}"));
        }
        lines.push(format!("quota: {}", details.join("; ")));
    } else if let Some(request_id) = request_id {
        lines.push(format!("request: {request_id}"));
    }
    if let Some(warning) = &rate_limit.warning {
        lines.push(format!("warning: {warning}"));
    }
    lines
}

fn format_alternative(alternative: &FairUseAlternative) -> String {
    let label = alternative
        .label
        .as_deref()
        .or(alternative.kind.as_deref())
        .unwrap_or("lower-cost request");
    let mut details = Vec::new();
    if let Some(cost) = alternative.cost {
        details.push(format!("cost {cost} units"));
    }
    if let Some(command) = &alternative.command {
        details.push(format!("command: {command}"));
    }
    if details.is_empty() {
        format!("alternative: {label}")
    } else {
        format!("alternative: {label} ({})", details.join("; "))
    }
}

fn fair_use_from_body(body: &Value) -> Option<FairUse> {
    let root = body.as_object()?;
    let fields = root
        .get("fair_use")
        .or_else(|| root.get("fairUse"))
        .and_then(Value::as_object)
        .unwrap_or(root);
    let alternatives = fields
        .get("alternatives")
        .and_then(Value::as_array)
        .map(|values| values.iter().cloned().map(alternative_from_value).collect())
        .unwrap_or_default();
    let details = FairUse {
        code: object_string(fields, &["code"]),
        scope: object_string(fields, &["scope"]),
        cost: object_u64(fields, &["cost"]),
        remaining: object_u64(fields, &["remaining"]),
        shortfall: object_u64(fields, &["shortfall"]),
        retry_at: object_string(fields, &["retryAt", "retry_at"]),
        alternatives,
    };
    let present = details.code.is_some()
        || details.scope.is_some()
        || details.cost.is_some()
        || details.remaining.is_some()
        || details.shortfall.is_some()
        || details.retry_at.is_some()
        || !details.alternatives.is_empty();
    present.then_some(details)
}

fn alternative_from_value(value: Value) -> FairUseAlternative {
    let Some(mut fields) = value.as_object().cloned() else {
        return FairUseAlternative {
            label: value.as_str().map(str::to_owned),
            ..FairUseAlternative::default()
        };
    };
    let kind = remove_string(&mut fields, &["kind", "action"]);
    let label = remove_string(&mut fields, &["label"]);
    let cost = remove_u64(&mut fields, &["cost"]);
    let command = remove_string(&mut fields, &["command", "commandHint", "command_hint"]);
    let parameters = fields
        .remove("parameters")
        .or_else(|| fields.remove("params"));
    FairUseAlternative {
        kind,
        label,
        cost,
        command,
        parameters,
        extra: fields.into_iter().collect(),
    }
}

fn body_string(body: &Value, names: &[&str]) -> Option<String> {
    body.as_object()
        .and_then(|fields| object_string(fields, names))
}

fn object_string(fields: &serde_json::Map<String, Value>, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        fields
            .get(*name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

fn object_u64(fields: &serde_json::Map<String, Value>, names: &[&str]) -> Option<u64> {
    names
        .iter()
        .find_map(|name| fields.get(*name).and_then(Value::as_u64))
}

fn remove_string(fields: &mut serde_json::Map<String, Value>, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        fields
            .remove(*name)
            .and_then(|value| value.as_str().map(str::to_owned))
    })
}

fn remove_u64(fields: &mut serde_json::Map<String, Value>, names: &[&str]) -> Option<u64> {
    names
        .iter()
        .find_map(|name| fields.remove(*name).and_then(|value| value.as_u64()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Read, net::TcpListener, thread};

    fn response(status: &str, headers: &[(&str, &str)], body: &str) -> Response {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let status = status.to_owned();
        let headers = headers
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect::<Vec<_>>();
        let body = body.to_owned();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 1024];
            let bytes_read = stream.read(&mut request).unwrap();
            assert!(bytes_read > 0);
            let mut wire = format!(
                "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n",
                body.len()
            );
            for (name, value) in headers {
                wire.push_str(&format!("{name}: {value}\r\n"));
            }
            wire.push_str("\r\n");
            wire.push_str(&body);
            stream.write_all(wire.as_bytes()).unwrap();
        });
        let response = Client::new()
            .get(format!("http://{address}"))
            .send()
            .unwrap();
        server.join().unwrap();
        response
    }

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
                settings: BTreeMap::new(),
            },
        );
        config.active_account = Some("work".into());
        assert_eq!(select_account(&config, None).unwrap().0, "work");
    }
    #[test]
    fn selects_the_only_account_but_requires_a_choice_for_multiple_accounts() {
        let account = Account {
            api_base: "https://example.test".into(),
            token: Some("secret".into()),
            email: None,
            tier: None,
            updated_at: None,
            settings: BTreeMap::new(),
        };
        let mut config = Config::default();
        config.accounts.insert("one".into(), account.clone());
        assert_eq!(active_account(&config).unwrap().0, "one");
        config.accounts.insert("two".into(), account);
        assert!(active_account(&config).is_none());
        assert!(select_account(&config, None).is_err());
    }
    #[test]
    fn product_configs_are_home_dotfiles_and_names_are_bounded() {
        assert!(config_path("bay").unwrap().ends_with(".bay"));
        assert!(config_path("suffix").unwrap().ends_with(".suffix"));
        assert!(config_path("../unsafe").is_err());
    }
    #[test]
    fn parses_unlimited_admin_headers() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ratelimit-limit", "unlimited".parse().unwrap());
        headers.insert("x-ratelimit-remaining", "unlimited".parse().unwrap());
        assert!(rate_limit_from_headers(&headers).unlimited);
    }
    #[test]
    fn parses_weighted_response_metadata() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-ratelimit-limit", "2000".parse().unwrap());
        headers.insert("x-ratelimit-remaining", "375".parse().unwrap());
        headers.insert("x-ratelimit-reset", "1786233600".parse().unwrap());
        headers.insert("x-ratelimit-cost", "145".parse().unwrap());
        headers.insert("retry-after", "30".parse().unwrap());
        headers.insert("x-ratelimit-warning", "Quota is low".parse().unwrap());
        headers.insert("x-ratelimit-tier", "claimed".parse().unwrap());
        assert_eq!(
            rate_limit_from_headers(&headers),
            RateLimit {
                limit: Some(2000),
                remaining: Some(375),
                reset_epoch_seconds: Some(1786233600),
                unlimited: false,
                cost: Some(145),
                retry_after: Some("30".into()),
                warning: Some("Quota is low".into()),
                tier: Some("claimed".into()),
            }
        );
    }
    #[test]
    fn successful_response_exposes_request_and_quota_metadata() {
        let response = response_json(response(
            "200 OK",
            &[
                ("content-type", "application/json"),
                ("x-ratelimit-limit", "1000"),
                ("x-ratelimit-remaining", "975"),
                ("x-ratelimit-cost", "25"),
                ("x-ratelimit-tier", "member"),
                ("x-ratelimit-warning", "Daily allowance is below 20 percent"),
                ("x-request-id", "req_success"),
            ],
            r#"{"ok":true}"#,
        ))
        .unwrap();
        assert_eq!(response.body, serde_json::json!({ "ok": true }));
        assert_eq!(response.request_id.as_deref(), Some("req_success"));
        assert_eq!(
            response_metadata_lines(&response),
            vec![
                "quota: cost 25 units; 975/1000 remaining; tier member; request req_success",
                "warning: Daily allowance is below 20 percent",
            ]
        );
    }
    #[test]
    fn retains_structured_rate_limit_error_body_and_alternatives() {
        let body = serde_json::json!({
            "error": "This request needs 40 units; 20 remain.",
            "code": "quota_exceeded",
            "scope": "daily",
            "cost": 40,
            "remaining": 20,
            "shortfall": 20,
            "retryAt": "2026-08-09T00:00:00Z",
            "requestId": "req_body",
            "alternatives": [{
                "kind": "reduce_limit",
                "label": "Return 25 rows",
                "cost": 10,
                "command": "somme request /entities?limit=25",
                "parameters": { "limit": 25 }
            }]
        });
        let error = response_json(response(
            "429 Too Many Requests",
            &[
                ("content-type", "application/json"),
                ("x-ratelimit-limit", "1000"),
                ("x-ratelimit-remaining", "20"),
                ("x-ratelimit-cost", "40"),
                ("x-request-id", "req_header"),
                ("retry-after", "60"),
            ],
            &body.to_string(),
        ))
        .unwrap_err();
        let api = as_api_error(&error).expect("typed API error");
        assert_eq!(api.status, 429);
        assert_eq!(api.body, body);
        assert_eq!(api.request_id.as_deref(), Some("req_header"));
        assert_eq!(api.rate_limit.cost, Some(40));
        assert_eq!(api.rate_limit.retry_after.as_deref(), Some("60"));
        assert_eq!(
            api.fair_use.as_ref().unwrap().scope.as_deref(),
            Some("daily")
        );
        assert_eq!(api.alternatives()[0].cost, Some(10));
        assert_eq!(
            api.alternatives()[0].parameters,
            Some(serde_json::json!({ "limit": 25 }))
        );
        let rendered = api.to_string();
        assert!(rendered.contains("429 Too Many Requests"));
        assert!(rendered.contains("cost 40 units"));
        assert!(rendered.contains("fair use: scope daily; shortfall 20"));
        assert!(rendered.contains("retry after 60"));
        assert!(rendered.contains("alternative: Return 25 rows (cost 10 units"));
    }
    #[test]
    fn preserves_non_json_error_bodies() {
        let error = response_json(response(
            "502 Bad Gateway",
            &[("content-type", "text/plain")],
            "upstream unavailable",
        ))
        .unwrap_err();
        let api = as_api_error(&error).expect("typed API error");
        assert_eq!(api.body, Value::String("upstream unavailable".into()));
        assert_eq!(api.message, "upstream unavailable");
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
    #[cfg(unix)]
    #[test]
    fn writes_config_with_private_permissions() {
        let directory =
            std::env::temp_dir().join(format!("somme-config-test-{}", std::process::id()));
        let path = directory.join("config.toml");
        write_toml(&path, &Config::default()).unwrap();
        assert_eq!(
            fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::remove_file(path).unwrap();
        fs::remove_dir(directory).unwrap();
    }
}
