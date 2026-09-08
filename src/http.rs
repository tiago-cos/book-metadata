use std::time::Duration;

use crate::error::{Error, Result};

pub fn build_client(
    provider: &'static str,
    user_agent: &str,
    timeout: Duration,
) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(user_agent)
        .build()
        .map_err(|e| transport(provider, &e))
}

pub struct Response {
    pub status: reqwest::StatusCode,
    pub body: String,
    pub retry_after: Option<Duration>,
}

pub async fn send_raw(
    provider: &'static str,
    request: reqwest::RequestBuilder,
) -> Result<Response> {
    let response = request.send().await.map_err(|e| transport(provider, &e))?;

    let status = response.status();
    let retry_after = retry_after(&response);
    let body = response.text().await.map_err(|e| transport(provider, &e))?;

    Ok(Response {
        status,
        body,
        retry_after,
    })
}

pub async fn send(
    provider: &'static str,
    request: reqwest::RequestBuilder,
) -> Result<Option<String>> {
    let response = send_raw(provider, request).await?;
    let status = response.status;

    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(Error::Unauthorized { provider });
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(Error::RateLimited {
            provider,
            retry_after: response.retry_after,
        });
    }
    if !status.is_success() {
        return Err(Error::Api {
            provider,
            status: Some(status.as_u16()),
            message: snippet(&response.body),
        });
    }

    Ok(Some(response.body))
}

pub fn decode<T: serde::de::DeserializeOwned>(provider: &'static str, body: &str) -> Result<T> {
    serde_json::from_str(body).map_err(|e| Error::UnexpectedResponse {
        provider,
        message: format!("{e}; body was: {}", snippet(body)),
    })
}

pub fn transport(provider: &'static str, error: &reqwest::Error) -> Error {
    let kind = if error.is_timeout() {
        "the request timed out"
    } else if error.is_connect() {
        "could not connect"
    } else if error.is_redirect() {
        "too many redirects"
    } else if error.is_body() || error.is_decode() {
        "could not read the response"
    } else {
        "the request failed"
    };

    let mut parts = vec![
        error
            .url()
            .map_or_else(|| kind.to_owned(), |url| format!("{kind} ({url})")),
    ];

    let mut cause: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(error);
    while let Some(error) = cause {
        parts.push(error.to_string());
        cause = error.source();
    }

    Error::Transport {
        provider,
        message: parts.join(": "),
    }
}

fn retry_after(response: &reqwest::Response) -> Option<Duration> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

pub fn snippet(body: &str) -> String {
    const MAX: usize = 300;
    let trimmed = body.trim();
    if trimmed.chars().count() <= MAX {
        return trimmed.to_owned();
    }
    let truncated: String = trimmed.chars().take(MAX).collect();
    format!("{truncated}...")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_long_error_bodies() {
        let body = "x".repeat(1000);
        assert!(snippet(&body).ends_with("..."));
        assert_eq!(snippet("short"), "short");
        assert_eq!(snippet("  padded  "), "padded");
    }
}
