//! Authenticated webhook-to-submission boundary.
//!
//! HTTP adapters must enforce the same 2 MiB limit while reading the body (before
//! buffering it), and pass the original, unmodified bytes and signature header.
//! These adapters only produce submissions: the coordinator still applies its
//! repository/revision authorization policy when workers claim work. Administrator
//! configuration, never webhook data, supplies commands and repository locators.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{plan::Plan, protocol::Request};

/// Maximum accepted webhook body size, in bytes.
pub const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;

/// Administrator-owned trigger configuration, separate from server configuration.
///
/// The environment variable is read for each verification; its value is never
/// included in errors or stored in this configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerConfig {
    /// Environment variable containing the nonempty HMAC shared secret.
    pub secret_env: String,
    /// Coordinator repository ID to administrator-configured trigger template.
    pub repositories: BTreeMap<String, TriggerRepository>,
}

/// Trusted mapping from a provider repository identity to an execution template.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerRepository {
    /// Exact, case-sensitive provider repository identity (GitHub `owner/name`).
    pub provider_repository: String,
    /// Trusted template; its repository must equal this entry's configuration key.
    pub plan: Plan,
    /// Whether the submitted graph remains open for administrator expansion.
    pub dynamic: bool,
}

/// Source information accepted by the generic signed webhook.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebhookSource {
    /// Exact provider repository identity, matched against administrator configuration.
    pub repository: String,
    /// Full 40-character hexadecimal commit SHA; zero (deletion) is forbidden.
    pub revision: String,
}

/// Generic webhook body. No workflow or execution settings are accepted.
///
/// For example: `{"source":{"repository":"owner/repo","revision":"..."}}`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenericWebhook {
    /// Immutable source snapshot for the configured template.
    pub source: WebhookSource,
}

#[derive(Deserialize)]
struct GithubPush {
    repository: GithubRepository,
    after: String,
    deleted: bool,
}

#[derive(Deserialize)]
struct GithubRepository {
    full_name: String,
}

/// Verify a GitHub push webhook and construct an idempotent submission.
///
/// `signature` is the `X-Hub-Signature-256` value (`sha256=` followed by 64 hex
/// digits), `event` is `X-GitHub-Event`, and `delivery` is `X-GitHub-Delivery`.
/// Signature verification precedes JSON parsing. Additional GitHub payload fields
/// are ignored, including any purported workflow. Only the configured plan is used.
/// The returned request never approves the pushed revision.
///
/// # Errors
///
/// * The body exceeds 2 MiB, the configured secret is unavailable/empty, or the
///   signature is malformed or does not authenticate the exact body bytes.
/// * The event is not `push`, the delivery ID is invalid, or the payload is invalid.
/// * The push deletes a reference or has an invalid/zero 40-character commit SHA.
/// * The repository is unknown/ambiguous, its template repository does not match
///   its configuration key, or the resulting configured plan fails validation.
pub fn github(
    config: &TriggerConfig,
    delivery: &str,
    event: &str,
    signature: &str,
    body: &[u8],
) -> Result<Request, String> {
    verify(config, signature, body)?;
    github_verified(config, delivery, event, body)
}

fn github_verified(
    config: &TriggerConfig,
    delivery: &str,
    event: &str,
    body: &[u8],
) -> Result<Request, String> {
    if event != "push" {
        return Err("only GitHub push events are supported".into());
    }
    let payload: GithubPush =
        serde_json::from_slice(body).map_err(|_| "invalid GitHub push payload".to_owned())?;
    if payload.deleted {
        return Err("reference deletion cannot trigger a submission".into());
    }
    submission(
        config,
        "github",
        delivery,
        &WebhookSource {
            repository: payload.repository.full_name,
            revision: payload.after,
        },
    )
}

/// Verify a generic signed webhook and construct an idempotent submission.
///
/// The body is a [`GenericWebhook`]. The signature format and HMAC algorithm are
/// identical to [`github`]. Delivery IDs must be stable across retries and unique
/// per provider event. This adapter uses a separate `generic` deduplication domain.
/// Unknown body fields (including workflow definitions) are rejected.
///
/// # Errors
///
/// * The body exceeds 2 MiB, the configured secret is unavailable/empty, or the
///   signature is malformed or does not authenticate the exact body bytes.
/// * The delivery ID or typed JSON payload is invalid, or its commit SHA is not
///   exactly 40 hexadecimal characters or represents deletion (all zeros).
/// * The repository is unknown/ambiguous, its configured template has a mismatched
///   repository ID, or the resulting configured plan fails validation.
pub fn generic(
    config: &TriggerConfig,
    delivery: &str,
    signature: &str,
    body: &[u8],
) -> Result<Request, String> {
    verify(config, signature, body)?;
    let payload: GenericWebhook =
        serde_json::from_slice(body).map_err(|_| "invalid generic webhook payload".to_owned())?;
    submission(config, "generic", delivery, &payload.source)
}

fn submission(
    config: &TriggerConfig,
    provider: &str,
    delivery: &str,
    source: &WebhookSource,
) -> Result<Request, String> {
    if delivery.is_empty()
        || delivery.len() > 256
        || delivery
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace())
    {
        return Err("invalid webhook delivery ID".into());
    }
    if source.revision.len() != 40
        || !source.revision.bytes().all(|byte| byte.is_ascii_hexdigit())
        || source.revision.bytes().all(|byte| byte == b'0')
    {
        return Err("webhook revision must be a nonzero 40-character hexadecimal SHA".into());
    }
    if source.repository.is_empty() {
        return Err("webhook repository is not configured".into());
    }
    let mut matches = config
        .repositories
        .iter()
        .filter(|(_, repo)| repo.provider_repository == source.repository);
    let (repository, template) = matches
        .next()
        .ok_or_else(|| "webhook repository is not configured".to_owned())?;
    if matches.next().is_some() {
        return Err("ambiguous webhook repository configuration".into());
    }
    if template.plan.repository != *repository {
        return Err("trigger template repository does not match configuration key".into());
    }
    let mut plan = template.plan.clone();
    plan.revision = source.revision.to_ascii_lowercase();
    // Do not echo configured commands or other potentially sensitive metadata.
    plan.validate()
        .map_err(|_| "invalid configured trigger plan".to_owned())?;
    // Length-delimited JSON tuple avoids delimiter collisions. The coordinator
    // persists this key and rejects conflicting submissions; no local replay cache
    // or approval mutation is needed here. A changed template must not silently
    // replace a previously submitted event.
    let identity = serde_json::to_vec(&(
        provider,
        delivery,
        &source.repository,
        repository,
        &plan.revision,
    ))
    .map_err(|_| "could not encode webhook submission identity".to_owned())?;
    let key = format!("webhook:{provider}:{}", hex(&Sha256::digest(&identity)));
    Ok(Request::Submit {
        key,
        plan,
        dynamic: template.dynamic,
    })
}

fn verify(config: &TriggerConfig, signature: &str, body: &[u8]) -> Result<(), String> {
    if body.len() > MAX_BODY_BYTES {
        return Err("webhook body exceeds 2 MiB".into());
    }
    if config.secret_env.is_empty() || config.secret_env.contains(['=', '\0']) {
        return Err("webhook secret is unavailable".into());
    }
    let secret = std::env::var(&config.secret_env)
        .map_err(|_| "webhook secret is unavailable".to_owned())?;
    if secret.is_empty() {
        return Err("webhook secret is unavailable".into());
    }
    verify_signature(secret.as_bytes(), signature, body)
}

fn verify_signature(secret: &[u8], signature: &str, body: &[u8]) -> Result<(), String> {
    let invalid = || "invalid webhook signature".to_owned();
    let encoded = signature.strip_prefix("sha256=").ok_or_else(invalid)?;
    if encoded.len() != 64 {
        return Err(invalid());
    }
    let mut supplied = [0_u8; 32];
    for (slot, pair) in supplied
        .iter_mut()
        .zip(encoded.as_bytes().as_chunks::<2>().0)
    {
        let high = hex_digit(pair[0]).ok_or_else(invalid)?;
        let low = hex_digit(pair[1]).ok_or_else(invalid)?;
        *slot = high << 4 | low;
    }
    let expected = hmac_sha256(secret, body);
    // Fixed-size XOR accumulation: never short-circuit on a differing MAC byte.
    let difference = expected
        .iter()
        .zip(supplied)
        .fold(0_u8, |diff, (left, right)| diff | (left ^ right));
    if difference != 0 {
        return Err(invalid());
    }
    Ok(())
}

// RFC 2104 HMAC, with SHA-256's 64-byte block size. No extra crate required.
fn hmac_sha256(secret: &[u8], body: &[u8]) -> [u8; 32] {
    let mut key = [0_u8; 64];
    if secret.len() > key.len() {
        key[..32].copy_from_slice(&Sha256::digest(secret));
    } else {
        key[..secret.len()].copy_from_slice(secret);
    }
    let mut inner_pad = [0x36_u8; 64];
    let mut outer_pad = [0x5c_u8; 64];
    for ((inner, outer), byte) in inner_pad.iter_mut().zip(&mut outer_pad).zip(key) {
        *inner ^= byte;
        *outer ^= byte;
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(body);
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner.finalize());
    outer.finalize().into()
}

const fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{CommandStep, Task};

    fn config() -> TriggerConfig {
        TriggerConfig {
            secret_env: String::new(),
            repositories: BTreeMap::from([(
                "local".into(),
                TriggerRepository {
                    provider_repository: "owner/repo".into(),
                    plan: Plan {
                        repository: "local".into(),
                        revision: "a".repeat(40),
                        tasks: BTreeMap::from([(
                            "test".into(),
                            Task {
                                when: crate::plan::TaskWhen::default(),
                                needs: vec![],
                                steps: vec![CommandStep {
                                    shell: "sh".into(),
                                    run: "echo trusted".into(),
                                    timeout_secs: 30,
                                    env: BTreeMap::new(),
                                }],
                                labels: BTreeMap::new(),
                                cpu: 1,
                                memory_mb: 0,
                                max_attempts: 1,
                            },
                        )]),
                    },
                    dynamic: false,
                },
            )]),
        }
    }

    fn push(repository: &str, revision: &str, deleted: bool) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "repository": {"full_name": repository}, "after": revision,
            "deleted": deleted, "workflow": {"run": "untrusted"},
        }))
        .unwrap()
    }

    #[test]
    fn rfc_4231_hmac_vectors() {
        assert_eq!(
            hex(&hmac_sha256(&[0x0b; 20], b"Hi There")),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
        assert_eq!(
            hex(&hmac_sha256(
                &[0xaa; 131],
                b"Test Using Larger Than Block-Size Key - Hash Key First"
            )),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn signatures_authenticate_exact_bytes() {
        let signature = format!("sha256={}", hex(&hmac_sha256(b"secret", b"payload")));
        assert!(verify_signature(b"secret", &signature, b"payload").is_ok());
        assert!(verify_signature(b"secret", &signature, b"payload ").is_err());
        assert!(verify_signature(b"wrong", &signature, b"payload").is_err());
        for signature in [
            "",
            "sha1=abc",
            "sha256=00",
            &format!("sha256={}", "z".repeat(64)),
        ] {
            assert!(verify_signature(b"secret", signature, b"payload").is_err());
        }
        for index in 0..32 {
            let mut mac = hmac_sha256(b"secret", b"payload");
            mac[index] ^= 1;
            assert!(
                verify_signature(b"secret", &format!("sha256={}", hex(&mac)), b"payload").is_err()
            );
        }
    }

    #[test]
    fn replaces_only_revision_and_replay_is_stable() {
        let config = config();
        let body = push("owner/repo", &"B".repeat(40), false);
        let first = github_verified(&config, "delivery", "push", &body).unwrap();
        let replay = github_verified(&config, "delivery", "push", &body).unwrap();
        assert_eq!(
            serde_json::to_value(&first).unwrap(),
            serde_json::to_value(replay).unwrap()
        );
        let Request::Submit { key, plan, dynamic } = first else {
            panic!("expected submission")
        };
        let mut expected = config.repositories["local"].plan.clone();
        expected.revision = "b".repeat(40);
        assert_eq!(plan, expected);
        assert!(!dynamic);
        assert_eq!(config.repositories["local"].plan.revision, "a".repeat(40));
        for (delivery, revision) in [("other", "b".repeat(40)), ("delivery", "c".repeat(40))] {
            let Request::Submit { key: other, .. } = github_verified(
                &config,
                delivery,
                "push",
                &push("owner/repo", &revision, false),
            )
            .unwrap() else {
                panic!("expected submission")
            };
            assert_ne!(key, other);
        }
        let Request::Submit { key: other, .. } = submission(
            &config,
            "generic",
            "delivery",
            &WebhookSource {
                repository: "owner/repo".into(),
                revision: "b".repeat(40),
            },
        )
        .unwrap() else {
            panic!("expected submission")
        };
        assert_ne!(key, other);
    }

    #[test]
    fn rejects_untrusted_event_repository_and_source() {
        let config = config();
        let valid = push("owner/repo", &"b".repeat(40), false);
        assert!(github_verified(&config, "id", "pull_request", &valid).is_err());
        assert!(github_verified(&config, "", "push", &valid).is_err());
        assert!(github_verified(&config, "id", "push", b"{}").is_err());
        for repo in ["other/repo", "Owner/repo", ""] {
            assert!(
                github_verified(&config, "id", "push", &push(repo, &"b".repeat(40), false))
                    .is_err()
            );
        }
        for revision in [
            "main".into(),
            "0".repeat(40),
            "g".repeat(40),
            "b".repeat(64),
        ] {
            assert!(
                github_verified(&config, "id", "push", &push("owner/repo", &revision, false))
                    .is_err()
            );
        }
        assert!(
            github_verified(
                &config,
                "id",
                "push",
                &push("owner/repo", &"b".repeat(40), true)
            )
            .is_err()
        );
    }

    #[test]
    fn configuration_and_generic_payload_fail_closed() {
        let mut config = config();
        let body = push("owner/repo", &"b".repeat(40), false);
        let duplicate = config.repositories["local"].clone();
        config.repositories.insert("other".into(), duplicate);
        assert!(github_verified(&config, "id", "push", &body).is_err());
        config.repositories.remove("other");
        config
            .repositories
            .get_mut("local")
            .unwrap()
            .plan
            .repository = "other".into();
        assert!(github_verified(&config, "id", "push", &body).is_err());
        let body =
            serde_json::json!({"source": {"repository": "owner/repo", "revision": "b".repeat(40)}});
        assert!(serde_json::from_value::<GenericWebhook>(body.clone()).is_ok());
        let mut untrusted = body;
        untrusted["plan"] = serde_json::json!({"run": "untrusted"});
        assert!(serde_json::from_value::<GenericWebhook>(untrusted).is_err());
    }

    #[test]
    fn size_and_secret_errors_do_not_parse_or_disclose() {
        let config = config();
        assert_eq!(
            github(&config, "id", "push", "", &vec![b'x'; MAX_BODY_BYTES + 1]).unwrap_err(),
            "webhook body exceeds 2 MiB"
        );
        assert_eq!(
            github(&config, "id", "push", "", b"not json").unwrap_err(),
            "webhook secret is unavailable"
        );
        assert!(generic(&config, "id", "", b"not json").is_err());
    }
}
