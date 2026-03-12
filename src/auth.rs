use anyhow::{Context, Result};
use secrecy::{ExposeSecret, SecretString};
use sha1::{Digest, Sha1};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct OvhClient {
    app_key: String,
    app_secret: SecretString,
    consumer_key: SecretString,
    base_url: String,
    client: reqwest::Client,
    time_delta: i64,
}

impl OvhClient {
    /// Create a new OVH API client.
    ///
    /// `endpoint` maps to a base URL: "eu", "ca", "us" (defaults to "eu").
    /// Calls GET /auth/time to synchronize the clock.
    pub async fn new(
        app_key: String,
        app_secret: SecretString,
        consumer_key: SecretString,
        endpoint: &str,
    ) -> Result<Self> {
        let base_url = match endpoint {
            "ca" => "https://ca.api.ovh.com/1.0",
            "us" => "https://api.us.ovhcloud.com/1.0",
            _ => "https://eu.api.ovh.com/1.0",
        }
        .to_string();

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;

        // Fetch server time to compute delta
        let server_time: i64 = client
            .get(format!("{}/auth/time", base_url))
            .send()
            .await
            .context("failed to fetch OVH server time")?
            .text()
            .await
            .context("failed to read OVH server time response")?
            .trim()
            .parse()
            .context("failed to parse OVH server time")?;

        let local_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let time_delta = server_time - local_time;
        tracing::info!(time_delta, "OVH time delta synchronized");

        Ok(Self {
            app_key,
            app_secret,
            consumer_key,
            base_url,
            client,
            time_delta,
        })
    }

    /// Compute the OVH API signature.
    ///
    /// Format: `$1$` + SHA1 hex of `APP_SECRET+CONSUMER_KEY+METHOD+URL+BODY+TIMESTAMP`
    fn sign(&self, method: &str, url: &str, body: &str, timestamp: &str) -> String {
        let to_sign = format!(
            "{}+{}+{}+{}+{}+{}",
            self.app_secret.expose_secret(),
            self.consumer_key.expose_secret(),
            method,
            url,
            body,
            timestamp
        );
        let mut hasher = Sha1::new();
        hasher.update(to_sign.as_bytes());
        let hash = hex::encode(hasher.finalize());
        format!("$1${}", hash)
    }

    /// Send an authenticated request to the OVH API.
    ///
    /// - `method`: HTTP method ("GET", "POST", "PUT", "DELETE")
    /// - `path`: API path (e.g. "/domain/zone")
    /// - `query`: optional JSON object whose entries become query parameters
    /// - `body`: optional JSON value sent as request body
    pub async fn request(
        &self,
        method: &str,
        path: &str,
        query: Option<&serde_json::Value>,
        body: Option<&serde_json::Value>,
    ) -> Result<serde_json::Value> {
        // Build full URL with query params
        let mut url = format!("{}{}", self.base_url, path);

        if let Some(serde_json::Value::Object(params)) = query {
            let mut first = true;
            for (key, value) in params {
                let separator = if first { '?' } else { '&' };
                first = false;
                let value_str = match value {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                url = format!(
                    "{}{}{}={}",
                    url,
                    separator,
                    urlencoding::encode(key),
                    urlencoding::encode(&value_str)
                );
            }
        }

        // Body string for signature and request
        let body_str = match body {
            Some(b) => serde_json::to_string(b)?,
            None => String::new(),
        };

        // Compute timestamp
        let local_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let timestamp = (local_time + self.time_delta).to_string();

        // Sign
        let signature = self.sign(method, &url, &body_str, &timestamp);

        // Parse HTTP method
        let http_method: reqwest::Method = method
            .parse()
            .with_context(|| format!("invalid HTTP method: {}", method))?;

        // Build and send request
        let mut req = self
            .client
            .request(http_method, &url)
            .header("X-Ovh-Application", &self.app_key)
            .header("X-Ovh-Timestamp", &timestamp)
            .header("X-Ovh-Consumer", self.consumer_key.expose_secret())
            .header("X-Ovh-Signature", &signature)
            .header("Content-Type", "application/json");

        if !body_str.is_empty() {
            req = req.body(body_str);
        }

        let response = req.send().await.context("OVH API request failed")?;
        let status = response.status();
        let text = response
            .text()
            .await
            .context("failed to read OVH response")?;

        if !status.is_success() {
            anyhow::bail!("OVH API error {}: {}", status, text);
        }

        // Parse response — fallback to Value::String if not valid JSON
        let value = serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text));

        Ok(value)
    }

    /// Returns the base URL for this client.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}
