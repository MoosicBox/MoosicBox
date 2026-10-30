//! Local operator client for CI runs. Network operations remain explicitly requested.

use crate::protocol::{Envelope, PROTOCOL_VERSION, Request, Response};
use reqwest::blocking::Client;
use std::{io::Read, time::Duration};

/// Authenticated, bounded coordinator client; tokens are never serialized.
pub struct OperatorClient {
    client: Client,
    endpoint: reqwest::Url,
    token: String,
}

impl OperatorClient {
    /// Connect to an explicitly configured coordinator without making a request.
    ///
    /// # Errors
    /// * URL is invalid, contains credentials/query/fragment, or uses remote HTTP.
    /// * Token environment variable is missing or the client cannot initialize.
    pub fn new(url: &str, token_env: &str) -> Result<Self, String> {
        let mut endpoint = reqwest::Url::parse(url).map_err(|_| "invalid coordinator URL")?;
        let loopback = endpoint.host_str().is_some_and(|host| {
            host == "localhost"
                || host
                    .trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
        if endpoint.host().is_none()
            || (endpoint.scheme() != "https" && !(endpoint.scheme() == "http" && loopback))
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || !matches!(endpoint.path(), "" | "/" | "/v1/rpc")
        {
            return Err("coordinator requires HTTPS (HTTP only on loopback), no credentials/query/fragment/path prefix".into());
        }
        endpoint.set_path("/v1/rpc");
        let token =
            std::env::var(token_env).map_err(|_| "missing operator token environment variable")?;
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| "cannot initialize operator client")?;
        Ok(Self {
            client,
            endpoint,
            token,
        })
    }

    /// Send one operation; mutations are never automatically retried.
    ///
    /// # Errors
    /// * Transport, HTTP, protocol, authorization or size validation fails.
    /// * The coordinator rejects the operation or fences a stale completion.
    pub fn call(&self, request: Request) -> Result<Response, String> {
        let response = self
            .client
            .post(self.endpoint.clone())
            .bearer_auth(&self.token)
            .json(&Envelope {
                version: PROTOCOL_VERSION,
                request,
            })
            .send()
            .map_err(|_| "coordinator transport failed (mutation outcome may be unknown)")?;
        if !response.status().is_success() {
            return Err(format!("coordinator HTTP {}", response.status().as_u16()));
        }
        let mut bytes = Vec::new();
        response
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "cannot read coordinator response")?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err("coordinator response exceeds 4 MiB".into());
        }
        let response: Response =
            serde_json::from_slice(&bytes).map_err(|_| "invalid coordinator response")?;
        match response {
            Response::Error { message } => Err(message),
            Response::Fenced { .. } => Err("attempt completion was fenced".into()),
            response => Ok(response),
        }
    }

    /// Download and independently digest one completed attempt artifact.
    ///
    /// # Errors
    /// * Authorization, transport, chunk protocol, digest or aggregate size fails.
    pub fn artifact(
        &self,
        run_id: &str,
        task_id: &str,
        attempt_id: &str,
        name: &str,
    ) -> Result<Vec<u8>, String> {
        use sha2::{Digest, Sha256};
        let mut endpoint = self.endpoint.clone();
        endpoint.set_path("/v1/artifacts/inspect");
        let mut bytes = Vec::new();
        loop {
            let request = crate::server::ArtifactInspection {
                run_id: run_id.into(),
                task_id: task_id.into(),
                attempt_id: attempt_id.into(),
                name: name.into(),
                offset: bytes.len() as u64,
            };
            let response = self
                .client
                .post(endpoint.clone())
                .bearer_auth(&self.token)
                .json(&request)
                .send()
                .map_err(|_| "artifact transport failed")?;
            if !response.status().is_success() {
                return Err(format!("artifact HTTP {}", response.status().as_u16()));
            }
            let mut body = Vec::new();
            response
                .take(512 * 1024 + 1)
                .read_to_end(&mut body)
                .map_err(|_| "artifact read failed")?;
            if body.len() > 512 * 1024 {
                return Err("artifact chunk response oversized".into());
            }
            let response: crate::transfer::TransferResponse =
                serde_json::from_slice(&body).map_err(|_| "invalid artifact chunk")?;
            let crate::transfer::TransferResponse::Chunk { data, done, sha256 } = response else {
                return Err("unexpected artifact response".into());
            };
            if data.len() > 64 * 1024
                || (!done && data.is_empty())
                || bytes.len() + data.len() > 64 * 1024 * 1024
            {
                return Err("invalid or oversized artifact".into());
            }
            bytes.extend(data);
            if done {
                let digest = Sha256::digest(&bytes);
                let actual = digest.iter().fold(String::new(), |mut output, byte| {
                    use std::fmt::Write;
                    let _ = write!(output, "{byte:02x}");
                    output
                });
                if sha256.as_deref() != Some(actual.as_str()) {
                    return Err("artifact digest mismatch".into());
                }
                return Ok(bytes);
            }
            if sha256.is_some() {
                return Err("premature artifact digest".into());
            }
        }
    }

    /// Retrieve run-bound evidence for all successful suite attempts and verify it.
    ///
    /// # Errors
    /// * Run, task, attempt, artifacts or required stage evidence is missing or invalid.
    pub fn verify_ci(
        &self,
        prepared: &crate::moosicbox::PreparedRun,
        run_id: &str,
    ) -> Result<crate::moosicbox::VerifiedRun, String> {
        let mut inspection = self.inspect(run_id)?;
        let attempts = inspection["attempts"]
            .as_object()
            .ok_or("inspection lacks attempts")?;
        let mut evidence = Vec::new();
        let mut total = 0usize;
        for (attempt_id, attempt) in attempts {
            if attempt["status"] != "COMPLETED" || attempt["report"]["success"] != true {
                continue;
            }
            let task_id = attempt["task_id"]
                .as_str()
                .ok_or("attempt lacks task identity")?;
            let task = prepared
                .plan
                .tasks
                .get(task_id)
                .ok_or("unexpected suite task")?;
            let spec = crate::transfer::ArtifactSpec::from_steps(&task.steps)?;
            let mut artifacts = std::collections::BTreeMap::new();
            for output in spec.outputs {
                let bytes = self.artifact(run_id, task_id, attempt_id, &output.name)?;
                total = total
                    .checked_add(bytes.len())
                    .ok_or("artifact budget overflow")?;
                if total > 64 * 1024 * 1024 {
                    return Err("aggregate CI evidence exceeds 64 MiB".into());
                }
                artifacts.insert(output.name, bytes);
            }
            evidence.push(crate::moosicbox::CaseEvidence {
                run_id: run_id.into(),
                revision: prepared.manifest.revision.clone(),
                case_id: task_id.into(),
                attempt_id: attempt_id.into(),
                artifacts,
            });
        }
        inspection["run_id"] = run_id.into();
        inspection["ci_evidence"] =
            serde_json::to_value(evidence).map_err(|error| error.to_string())?;
        crate::moosicbox::verify(prepared, &inspection)
    }

    /// Read a run's durable state and recorded evidence.
    ///
    /// # Errors
    /// * Any RPC error, or an unexpected non-data response.
    pub fn inspect(&self, run_id: &str) -> Result<serde_json::Value, String> {
        match self.call(Request::Inspect {
            run_id: run_id.into(),
        })? {
            Response::Data { value } => Ok(value),
            _ => Err("unexpected inspection response".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unsafe_urls_before_token_lookup() {
        for url in [
            "http://example.com",
            "https://user:secret@example.com",
            "https://example.com?secret=x",
            "https://example.com/proxy",
        ] {
            assert!(OperatorClient::new(url, "GPIPE_TEST_MISSING_OPERATOR_TOKEN").is_err());
        }
    }
}
