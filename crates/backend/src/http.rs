//! Blocking HTTP helpers shared by the HTTP backends and model discovery.

use std::time::Duration;

use crate::error::BackendError;

/// Build a `ureq::Agent` with the given timeout applied globally.
pub(crate) fn build_agent(timeout: Duration) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .build();
    ureq::Agent::new_with_config(config)
}

/// Map a `ureq::Error` (transport-level) to a [`BackendError`].
///
/// `send_json` raises `ureq::Error::StatusCode` for non-2xx responses that
/// come back with a body; all other variants are transport failures.
pub(crate) fn map_ureq_error(e: ureq::Error, backend: &str) -> BackendError {
    match &e {
        ureq::Error::StatusCode(code) => {
            let code = *code;
            match code {
                401 | 403 => BackendError::Auth {
                    backend: backend.to_owned(),
                    message: format!("HTTP {code}"),
                },
                404 => BackendError::Configuration {
                    backend: backend.to_owned(),
                    message: format!("HTTP 404 — model not found or endpoint wrong: {e}"),
                },
                _ => BackendError::Network {
                    backend: backend.to_owned(),
                    message: format!("HTTP {code}: {e}"),
                },
            }
        }
        _ => BackendError::Network {
            backend: backend.to_owned(),
            message: e.to_string(),
        },
    }
}

/// `GET url` (or `POST url` with `body`) and parse the response as JSON.
pub(crate) fn request_json(
    agent: &ureq::Agent,
    url: &str,
    body: Option<&serde_json::Value>,
    api_key: Option<&str>,
    backend: &str,
) -> Result<serde_json::Value, BackendError> {
    let response = match body {
        Some(body) => {
            let mut req = agent.post(url);
            if let Some(key) = api_key {
                req = req.header("Authorization", format!("Bearer {key}"));
            }
            req.send_json(body)
        }
        None => {
            let mut req = agent.get(url);
            if let Some(key) = api_key {
                req = req.header("Authorization", format!("Bearer {key}"));
            }
            req.call()
        }
    }
    .map_err(|e| map_ureq_error(e, backend))?;

    let text = response
        .into_body()
        .read_to_string()
        .map_err(|e| BackendError::Protocol {
            backend: backend.to_owned(),
            message: format!("failed to read response body: {e}"),
        })?;
    serde_json::from_str(&text).map_err(|e| BackendError::Protocol {
        backend: backend.to_owned(),
        message: format!("{backend} returned non-JSON response: {e}"),
    })
}
