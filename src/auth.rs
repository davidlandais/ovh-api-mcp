use anyhow::{Context, Result};
use secrecy::{ExposeSecret, SecretString};
use sha1::{Digest, Sha1};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

enum AuthMode {
    ApiKey {
        app_key: String,
        app_secret: SecretString,
        consumer_key: SecretString,
        time_delta: i64,
    },
    OAuth2 {
        client_id: String,
        client_secret: SecretString,
        token: RwLock<SecretString>,
        token_endpoint: String,
    },
}

fn base_url_for(endpoint: &str) -> String {
    match endpoint {
        "ca" => "https://ca.api.ovh.com",
        "us" => "https://api.us.ovhcloud.com",
        _ => "https://eu.api.ovh.com",
    }
    .to_string()
}

pub struct OvhClient {
    auth: AuthMode,
    base_url: String,
    client: reqwest::Client,
}

fn build_http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("failed to build HTTP client")
}

impl OvhClient {
    fn well_known_url(endpoint: &str) -> String {
        match endpoint {
            "ca" => "https://www.ovh.ca/auth/.well-known/openid-configuration",
            "us" => "https://us.ovhcloud.com/auth/.well-known/openid-configuration",
            _ => "https://www.ovh.com/auth/.well-known/openid-configuration",
        }
        .to_string()
    }

    async fn fetch_token(
        client: &reqwest::Client,
        token_endpoint: &str,
        client_id: &str,
        client_secret: &SecretString,
    ) -> Result<SecretString> {
        let response = client
            .post(token_endpoint)
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", client_id),
                ("client_secret", client_secret.expose_secret()),
                ("scope", "all"),
            ])
            .send()
            .await
            .context("failed to request OAuth2 token")?;

        let status = response.status();
        let body: serde_json::Value = response
            .json()
            .await
            .context("failed to parse OAuth2 token response")?;

        if !status.is_success() {
            anyhow::bail!("OAuth2 token error {}: {}", status, body);
        }

        let access_token = body["access_token"]
            .as_str()
            .context("missing access_token in OAuth2 response")?;

        Ok(SecretString::from(access_token.to_string()))
    }

    /// Create a new OVH API client using OAuth2 client credentials.
    ///
    /// `endpoint` maps to the OIDC discovery URL: "eu", "ca", "us" (defaults to "eu").
    /// Fetches the well-known OIDC configuration to discover the token endpoint,
    /// then acquires an initial access token.
    pub async fn new_oauth2(
        client_id: String,
        client_secret: SecretString,
        endpoint: &str,
    ) -> Result<Self> {
        let base_url = base_url_for(endpoint);
        let client = build_http_client()?;

        let wk_url = Self::well_known_url(endpoint);
        let discovery: serde_json::Value = client
            .get(&wk_url)
            .send()
            .await
            .with_context(|| format!("failed to fetch OIDC discovery from {}", wk_url))?
            .json()
            .await
            .context("failed to parse OIDC discovery response")?;

        let token_endpoint = discovery["token_endpoint"]
            .as_str()
            .context("missing token_endpoint in OIDC discovery")?
            .to_string();

        tracing::info!(token_endpoint, "OAuth2 discovery complete");

        let token = Self::fetch_token(&client, &token_endpoint, &client_id, &client_secret).await?;
        tracing::info!("OAuth2 initial token acquired");

        Ok(Self {
            auth: AuthMode::OAuth2 {
                client_id,
                client_secret,
                token: RwLock::new(token),
                token_endpoint,
            },
            base_url,
            client,
        })
    }

    /// Create a new OVH API client using API key authentication.
    ///
    /// `endpoint` maps to a base URL: "eu", "ca", "us" (defaults to "eu").
    /// Calls GET /v1/auth/time to synchronize the clock.
    pub async fn new_apikey(
        app_key: String,
        app_secret: SecretString,
        consumer_key: SecretString,
        endpoint: &str,
    ) -> Result<Self> {
        let base_url = base_url_for(endpoint);
        let client = build_http_client()?;

        let server_time: i64 = client
            .get(format!("{}/v1/auth/time", base_url))
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
            auth: AuthMode::ApiKey {
                app_key,
                app_secret,
                consumer_key,
                time_delta,
            },
            base_url,
            client,
        })
    }

    /// Compute the OVH API signature.
    ///
    /// Format: `$1$` + SHA1 hex of `APP_SECRET+CONSUMER_KEY+METHOD+URL+BODY+TIMESTAMP`
    fn sign(
        app_secret: &SecretString,
        consumer_key: &SecretString,
        method: &str,
        url: &str,
        body: &str,
        timestamp: &str,
    ) -> String {
        let to_sign = format!(
            "{}+{}+{}+{}+{}+{}",
            app_secret.expose_secret(),
            consumer_key.expose_secret(),
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

    /// Check if an HTTP status code should trigger an OAuth2 token refresh and retry.
    fn is_retryable_status(status: reqwest::StatusCode) -> bool {
        status == reqwest::StatusCode::UNAUTHORIZED // 401 only, NOT 403
    }

    /// Refresh the OAuth2 access token using client credentials.
    async fn refresh_token(&self) -> Result<()> {
        if let AuthMode::OAuth2 {
            client_id,
            client_secret,
            token,
            token_endpoint,
        } = &self.auth
        {
            let new_token =
                Self::fetch_token(&self.client, token_endpoint, client_id, client_secret).await?;
            let mut guard = token.write().await;
            *guard = new_token;
            tracing::info!("OAuth2 token refreshed");
            Ok(())
        } else {
            anyhow::bail!("refresh_token called on non-OAuth2 client")
        }
    }

    /// Build the full URL from a path and optional query parameters.
    fn build_url(&self, path: &str, query: Option<&serde_json::Value>) -> String {
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

        url
    }

    /// Check response status and parse the body as JSON.
    async fn parse_response(&self, response: reqwest::Response) -> Result<serde_json::Value> {
        let status = response.status();
        let text = response
            .text()
            .await
            .context("failed to read OVH response")?;

        if !status.is_success() {
            anyhow::bail!("OVH API error {}: {}", status, text);
        }

        let value = serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text));

        Ok(value)
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
        let url = self.build_url(path, query);

        let body_str = match body {
            Some(b) => serde_json::to_string(b)?,
            None => String::new(),
        };

        let http_method: reqwest::Method = method
            .parse()
            .with_context(|| format!("invalid HTTP method: {}", method))?;

        let response = match &self.auth {
            AuthMode::ApiKey {
                app_key,
                app_secret,
                consumer_key,
                time_delta,
            } => {
                let local_time = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;
                let timestamp = (local_time + time_delta).to_string();
                let signature = Self::sign(
                    app_secret,
                    consumer_key,
                    method,
                    &url,
                    &body_str,
                    &timestamp,
                );

                let mut req = self
                    .client
                    .request(http_method, &url)
                    .header("X-Ovh-Application", app_key)
                    .header("X-Ovh-Timestamp", &timestamp)
                    .header("X-Ovh-Consumer", consumer_key.expose_secret())
                    .header("X-Ovh-Signature", &signature)
                    .header("Content-Type", "application/json");

                if !body_str.is_empty() {
                    req = req.body(body_str);
                }

                req.send().await.context("OVH API request failed")?
            }
            AuthMode::OAuth2 { token, .. } => {
                let bearer = token.read().await.expose_secret().to_string();

                let mut req = self
                    .client
                    .request(http_method.clone(), &url)
                    .header("Authorization", format!("Bearer {}", bearer))
                    .header("Content-Type", "application/json");
                if !body_str.is_empty() {
                    req = req.body(body_str.clone());
                }
                let response = req.send().await.context("OVH API request failed")?;

                if Self::is_retryable_status(response.status()) {
                    tracing::info!("OAuth2 token expired (401), refreshing");
                    self.refresh_token().await?;

                    let bearer = token.read().await.expose_secret().to_string();
                    let mut req = self
                        .client
                        .request(http_method, &url)
                        .header("Authorization", format!("Bearer {}", bearer))
                        .header("Content-Type", "application/json");
                    if !body_str.is_empty() {
                        req = req.body(body_str);
                    }
                    req.send().await.context("OVH API request failed (retry)")?
                } else {
                    response
                }
            }
        };

        self.parse_response(response).await
    }

    /// Returns the base URL for this client.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_known_url_by_region() {
        assert_eq!(
            OvhClient::well_known_url("eu"),
            "https://www.ovh.com/auth/.well-known/openid-configuration"
        );
        assert_eq!(
            OvhClient::well_known_url("ca"),
            "https://www.ovh.ca/auth/.well-known/openid-configuration"
        );
        assert_eq!(
            OvhClient::well_known_url("us"),
            "https://us.ovhcloud.com/auth/.well-known/openid-configuration"
        );
    }

    #[test]
    fn base_url_by_region() {
        assert_eq!(base_url_for("eu"), "https://eu.api.ovh.com");
        assert_eq!(base_url_for("ca"), "https://ca.api.ovh.com");
        assert_eq!(base_url_for("us"), "https://api.us.ovhcloud.com");
    }

    #[test]
    fn status_401_should_trigger_retry() {
        assert!(OvhClient::is_retryable_status(
            reqwest::StatusCode::UNAUTHORIZED
        ));
    }

    #[test]
    fn status_403_should_not_trigger_retry() {
        assert!(!OvhClient::is_retryable_status(
            reqwest::StatusCode::FORBIDDEN
        ));
    }

    #[test]
    fn status_200_should_not_trigger_retry() {
        assert!(!OvhClient::is_retryable_status(reqwest::StatusCode::OK));
    }
}
