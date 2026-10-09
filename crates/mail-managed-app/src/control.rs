//! Strict managed-app v1 control vocabulary.
//!
//! Every control payload is one flat JSON object carried in a stream-0
//! control frame. Decoding rejects duplicate keys and unknown fields before
//! any semantic value is produced. Wrong-direction variants are rejected:
//! an application endpoint decodes only host events, and the fake host used
//! in tests decodes only application requests.
//!
//! Required app→host messages: `hello`, `open`, `close`, `reset`.
//! Required host→app messages: `reply`, `capabilities`, `stream_closed`,
//! `stream_reset`, `health`. `permission_request` is deliberately absent; an
//! unknown `type` value fails closed so a future upstream addition cannot
//! silently become authority.

use std::collections::{HashMap, HashSet};

use thiserror::Error;

use crate::frame::MAX_CONTROL_JSON_LEN;
use crate::launch::{validate_app_id, validate_instance_decimal};

/// Maximum capability entries in one `capabilities` event.
pub const MAX_CAPABILITIES: usize = 16;
/// Maximum length of a single capability/service/status token.
pub const MAX_TOKEN_LEN: usize = 32;
/// Maximum length of a free-form reason/error/status detail.
pub const MAX_DETAIL_LEN: usize = 256;

/// Control decode/encode failures. Variants carry only static reasons and
/// bounded ids, never payload bytes or authority values.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ControlError {
    /// Payload exceeds the control JSON ceiling.
    #[error("control payload oversize")]
    Oversize,
    /// Payload is not valid UTF-8.
    #[error("control payload not utf-8")]
    NotUtf8,
    /// Payload is not valid JSON.
    #[error("control payload malformed")]
    Malformed,
    /// An object contains the same key twice.
    #[error("control payload duplicate key")]
    DuplicateKey,
    /// An object contains a field the frozen vocabulary does not define.
    #[error("control payload unknown field")]
    UnknownField,
    /// The `type` value is not part of the frozen vocabulary.
    #[error("control type unknown")]
    UnknownType,
    /// A message arrived on the wrong endpoint direction.
    #[error("control wrong direction")]
    WrongDirection,
    /// A required field is missing or has the wrong JSON shape.
    #[error("control field invalid")]
    Field,
    /// A bounded value violates its grammar.
    #[error("control value invalid")]
    Value,
}

/// Application→host request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppRequest {
    /// Exactly one per channel generation, carrying trusted launch identity.
    Hello {
        id: u64,
        app_id: String,
        instance: String,
    },
    /// Request a logical service stream. `service` is the frozen v1 name
    /// (`sam` for mail); the codec preserves any bounded token so tests can
    /// prove unsupported services never become usable.
    Open {
        id: u64,
        service: String,
        stream_id: u32,
    },
    /// Best-effort close notification; no reply is expected. The host answers
    /// with `stream_closed`.
    Close { stream_id: u32 },
    /// Best-effort reset notification; no reply is expected. The host answers
    /// with `stream_reset`.
    Reset { stream_id: u32 },
}

/// Host→application event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostEvent {
    /// Correlated reply to `hello` or `open`.
    Reply {
        id: u64,
        ok: bool,
        error: Option<String>,
    },
    /// One-shot effective capability set. The first such event freezes
    /// authority; a second is a fatal contract error handled by the client.
    Capabilities { granted: Vec<String> },
    /// Host confirms a logical stream ended cleanly.
    StreamClosed { stream_id: u32 },
    /// Host reports a logical stream reset with an optional bounded reason.
    StreamReset {
        stream_id: u32,
        reason: Option<String>,
    },
    /// Host health hint; mail does not act on it beyond validation.
    Health { status: String },
}

fn check_token(value: &str) -> Result<(), ControlError> {
    if value.is_empty() || value.len() > MAX_TOKEN_LEN {
        return Err(ControlError::Value);
    }
    for b in value.as_bytes() {
        let ok = b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_' || *b == b'-';
        if !ok {
            return Err(ControlError::Value);
        }
    }
    Ok(())
}

fn check_detail(value: &str) -> Result<(), ControlError> {
    if value.is_empty() || value.len() > MAX_DETAIL_LEN {
        return Err(ControlError::Value);
    }
    for b in value.as_bytes() {
        if *b == 0 || *b == b'\n' || *b == b'\r' {
            return Err(ControlError::Value);
        }
        if !b.is_ascii() {
            return Err(ControlError::Value);
        }
    }
    Ok(())
}

fn get_u64_nonzero(
    map: &HashMap<String, serde_json::Value>,
    key: &str,
) -> Result<u64, ControlError> {
    let v = map.get(key).ok_or(ControlError::Field)?;
    let n = v.as_u64().ok_or(ControlError::Field)?;
    if n == 0 {
        return Err(ControlError::Value);
    }
    Ok(n)
}

fn get_u32_nonzero(
    map: &HashMap<String, serde_json::Value>,
    key: &str,
) -> Result<u32, ControlError> {
    let n = get_u64_nonzero(map, key)?;
    u32::try_from(n).map_err(|_| ControlError::Value)
}

fn get_string(map: &HashMap<String, serde_json::Value>, key: &str) -> Result<String, ControlError> {
    let v = map.get(key).ok_or(ControlError::Field)?;
    v.as_str().ok_or(ControlError::Field).map(str::to_string)
}

/// Reject JSON objects containing a duplicate key at any nesting level.
///
/// `serde_json` keeps the last duplicate silently, so this scan runs on the
/// raw payload before any semantic value is produced. It tracks object key
/// sets with a stack and understands string escapes; anything that is not a
/// strict object key (strings inside arrays, values) is ignored.
fn reject_duplicate_keys(raw: &[u8]) -> Result<(), ControlError> {
    let text = std::str::from_utf8(raw).map_err(|_| ControlError::NotUtf8)?;
    let bytes = text.as_bytes();
    let mut stack: Vec<Option<HashSet<String>>> = Vec::new();
    let mut in_string = false;
    let mut escaped = false;
    let mut current_string: Option<String> = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if escaped {
                escaped = false;
                if let Some(s) = current_string.as_mut() {
                    s.push(b as char);
                }
            } else if b == b'\\' {
                escaped = true;
                if let Some(s) = current_string.as_mut() {
                    s.push('\\');
                }
            } else if b == b'"' {
                in_string = false;
                // Completed string; check whether it is an object key by
                // looking ahead past whitespace for ':'.
                let completed = current_string.take().unwrap_or_default();
                let mut j = i + 1;
                while j < bytes.len() && (bytes[j] as char).is_whitespace() {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b':' {
                    if let Some(Some(set)) = stack.last_mut() {
                        if !set.insert(completed) {
                            return Err(ControlError::DuplicateKey);
                        }
                    }
                }
            } else if let Some(s) = current_string.as_mut() {
                // Record raw char; non-ASCII keys fail later at semantic
                // validation, but duplicates of ASCII keys are still caught.
                s.push(b as char);
            }
            i += 1;
            continue;
        }
        match b {
            b'"' => {
                in_string = true;
                escaped = false;
                current_string = Some(String::new());
            }
            b'{' => stack.push(Some(HashSet::new())),
            b'[' => stack.push(None),
            b'}' | b']' => {
                stack.pop();
            }
            _ => {}
        }
        i += 1;
    }
    if in_string {
        return Err(ControlError::Malformed);
    }
    Ok(())
}

fn parse_object(raw: &[u8]) -> Result<HashMap<String, serde_json::Value>, ControlError> {
    if raw.len() > MAX_CONTROL_JSON_LEN {
        return Err(ControlError::Oversize);
    }
    reject_duplicate_keys(raw)?;
    let text = std::str::from_utf8(raw).map_err(|_| ControlError::NotUtf8)?;
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|_| ControlError::Malformed)?;
    match value {
        serde_json::Value::Object(map) => Ok(map.into_iter().collect()),
        _ => Err(ControlError::Malformed),
    }
}

fn check_only_allowed(
    map: &HashMap<String, serde_json::Value>,
    allowed: &[&str],
) -> Result<(), ControlError> {
    let allowed_set: HashSet<&str> = allowed.iter().copied().collect();
    for key in map.keys() {
        if !allowed_set.contains(key.as_str()) {
            return Err(ControlError::UnknownField);
        }
    }
    Ok(())
}

impl AppRequest {
    /// Encode an application request as control JSON bytes.
    pub fn encode(&self) -> Vec<u8> {
        match self {
            AppRequest::Hello {
                id,
                app_id,
                instance,
            } => format!(
                "{{\"type\":\"hello\",\"id\":{id},\"app_id\":{},\"instance\":{}}}",
                json_string(app_id),
                json_string(instance)
            )
            .into_bytes(),
            AppRequest::Open {
                id,
                service,
                stream_id,
            } => format!(
                "{{\"type\":\"open\",\"id\":{id},\"service\":{},\"stream_id\":{stream_id}}}",
                json_string(service)
            )
            .into_bytes(),
            AppRequest::Close { stream_id } => {
                format!("{{\"type\":\"close\",\"stream_id\":{stream_id}}}").into_bytes()
            }
            AppRequest::Reset { stream_id } => {
                format!("{{\"type\":\"reset\",\"stream_id\":{stream_id}}}").into_bytes()
            }
        }
    }

    /// Strictly decode an application request (fake-host side). Host events
    /// are rejected as wrong-direction.
    pub fn decode(raw: &[u8]) -> Result<Self, ControlError> {
        let map = parse_object(raw)?;
        let type_value = map
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or(ControlError::Field)?;
        match type_value {
            "hello" => {
                check_only_allowed(&map, &["type", "id", "app_id", "instance"])?;
                let id = get_u64_nonzero(&map, "id")?;
                let app_id = get_string(&map, "app_id")?;
                let instance = get_string(&map, "instance")?;
                validate_app_id(&app_id).map_err(|_| ControlError::Value)?;
                validate_instance_decimal(&instance).map_err(|_| ControlError::Value)?;
                Ok(AppRequest::Hello {
                    id,
                    app_id,
                    instance,
                })
            }
            "open" => {
                check_only_allowed(&map, &["type", "id", "service", "stream_id"])?;
                let id = get_u64_nonzero(&map, "id")?;
                let service = get_string(&map, "service")?;
                if service.is_empty() || service.len() > MAX_TOKEN_LEN {
                    return Err(ControlError::Value);
                }
                check_token(&service.to_lowercase()).map_err(|_| ControlError::Value)?;
                let stream_id = get_u32_nonzero(&map, "stream_id")?;
                Ok(AppRequest::Open {
                    id,
                    service,
                    stream_id,
                })
            }
            "close" => {
                check_only_allowed(&map, &["type", "stream_id"])?;
                let stream_id = get_u32_nonzero(&map, "stream_id")?;
                Ok(AppRequest::Close { stream_id })
            }
            "reset" => {
                check_only_allowed(&map, &["type", "stream_id"])?;
                let stream_id = get_u32_nonzero(&map, "stream_id")?;
                Ok(AppRequest::Reset { stream_id })
            }
            "reply" | "capabilities" | "stream_closed" | "stream_reset" | "health" => {
                Err(ControlError::WrongDirection)
            }
            _ => Err(ControlError::UnknownType),
        }
    }
}

impl HostEvent {
    /// Encode a host event as control JSON bytes (fake-host side).
    pub fn encode(&self) -> Vec<u8> {
        match self {
            HostEvent::Reply { id, ok, error } => {
                if *ok {
                    format!("{{\"type\":\"reply\",\"id\":{id},\"ok\":true}}").into_bytes()
                } else {
                    let detail = error.clone().unwrap_or_else(|| "denied".to_string());
                    format!(
                        "{{\"type\":\"reply\",\"id\":{id},\"ok\":false,\"error\":{}}}",
                        json_string(&detail)
                    )
                    .into_bytes()
                }
            }
            HostEvent::Capabilities { granted } => {
                let items: Vec<String> = granted.iter().map(|g| json_string(g)).collect();
                format!(
                    "{{\"type\":\"capabilities\",\"granted\":[{}]}}",
                    items.join(",")
                )
                .into_bytes()
            }
            HostEvent::StreamClosed { stream_id } => {
                format!("{{\"type\":\"stream_closed\",\"stream_id\":{stream_id}}}").into_bytes()
            }
            HostEvent::StreamReset { stream_id, reason } => match reason {
                Some(r) => format!(
                    "{{\"type\":\"stream_reset\",\"stream_id\":{stream_id},\"reason\":{}}}",
                    json_string(r)
                )
                .into_bytes(),
                None => {
                    format!("{{\"type\":\"stream_reset\",\"stream_id\":{stream_id}}}").into_bytes()
                }
            },
            HostEvent::Health { status } => {
                format!("{{\"type\":\"health\",\"status\":{}}}", json_string(status)).into_bytes()
            }
        }
    }

    /// Strictly decode a host event (application side). Application requests
    /// are rejected as wrong-direction.
    pub fn decode(raw: &[u8]) -> Result<Self, ControlError> {
        let map = parse_object(raw)?;
        let type_value = map
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or(ControlError::Field)?;
        match type_value {
            "reply" => {
                check_only_allowed(&map, &["type", "id", "ok", "error"])?;
                let id = get_u64_nonzero(&map, "id")?;
                let ok = map
                    .get("ok")
                    .and_then(|v| v.as_bool())
                    .ok_or(ControlError::Field)?;
                let error = match map.get("error") {
                    Some(v) => {
                        let s = v.as_str().ok_or(ControlError::Field)?;
                        check_detail(s).map_err(|_| ControlError::Value)?;
                        Some(s.to_string())
                    }
                    None => None,
                };
                if ok && error.is_some() {
                    return Err(ControlError::Field);
                }
                Ok(HostEvent::Reply { id, ok, error })
            }
            "capabilities" => {
                check_only_allowed(&map, &["type", "granted"])?;
                let granted_value = map.get("granted").ok_or(ControlError::Field)?;
                let arr = granted_value.as_array().ok_or(ControlError::Field)?;
                if arr.len() > MAX_CAPABILITIES {
                    return Err(ControlError::Value);
                }
                let mut granted = Vec::with_capacity(arr.len());
                for item in arr {
                    let s = item.as_str().ok_or(ControlError::Field)?;
                    check_token(&s.to_lowercase()).map_err(|_| ControlError::Value)?;
                    granted.push(s.to_string());
                }
                Ok(HostEvent::Capabilities { granted })
            }
            "stream_closed" => {
                check_only_allowed(&map, &["type", "stream_id"])?;
                let stream_id = get_u32_nonzero(&map, "stream_id")?;
                Ok(HostEvent::StreamClosed { stream_id })
            }
            "stream_reset" => {
                check_only_allowed(&map, &["type", "stream_id", "reason"])?;
                let stream_id = get_u32_nonzero(&map, "stream_id")?;
                let reason = match map.get("reason") {
                    Some(v) => {
                        let s = v.as_str().ok_or(ControlError::Field)?;
                        check_detail(s).map_err(|_| ControlError::Value)?;
                        Some(s.to_string())
                    }
                    None => None,
                };
                Ok(HostEvent::StreamReset { stream_id, reason })
            }
            "health" => {
                check_only_allowed(&map, &["type", "status"])?;
                let status = get_string(&map, "status")?;
                check_detail(&status).map_err(|_| ControlError::Value)?;
                Ok(HostEvent::Health { status })
            }
            "hello" | "open" | "close" | "reset" => Err(ControlError::WrongDirection),
            _ => Err(ControlError::UnknownType),
        }
    }
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_required_variant_round_trips() {
        let hello = AppRequest::Hello {
            id: 1,
            app_id: "mail/client".to_string(),
            instance: "42".to_string(),
        };
        assert_eq!(AppRequest::decode(&hello.encode()).expect("hello"), hello);
        let open = AppRequest::Open {
            id: 2,
            service: "sam".to_string(),
            stream_id: 7,
        };
        assert_eq!(AppRequest::decode(&open.encode()).expect("open"), open);
        let close = AppRequest::Close { stream_id: 7 };
        assert_eq!(AppRequest::decode(&close.encode()).expect("close"), close);
        let reset = AppRequest::Reset { stream_id: 9 };
        assert_eq!(AppRequest::decode(&reset.encode()).expect("reset"), reset);

        let reply = HostEvent::Reply {
            id: 1,
            ok: true,
            error: None,
        };
        assert_eq!(HostEvent::decode(&reply.encode()).expect("reply"), reply);
        let reply_fail = HostEvent::Reply {
            id: 2,
            ok: false,
            error: Some("denied".to_string()),
        };
        assert_eq!(
            HostEvent::decode(&reply_fail.encode()).expect("reply fail"),
            reply_fail
        );
        let caps = HostEvent::Capabilities {
            granted: vec!["sam".to_string()],
        };
        assert_eq!(HostEvent::decode(&caps.encode()).expect("caps"), caps);
        let closed = HostEvent::StreamClosed { stream_id: 7 };
        assert_eq!(HostEvent::decode(&closed.encode()).expect("closed"), closed);
        let reset_ev = HostEvent::StreamReset {
            stream_id: 7,
            reason: Some("idle".to_string()),
        };
        assert_eq!(
            HostEvent::decode(&reset_ev.encode()).expect("reset"),
            reset_ev
        );
        let health = HostEvent::Health {
            status: "ok".to_string(),
        };
        assert_eq!(HostEvent::decode(&health.encode()).expect("health"), health);
    }

    #[test]
    fn duplicate_keys_rejected_before_semantics() {
        let raw = br#"{"type":"reply","id":1,"id":2,"ok":true}"#;
        assert_eq!(HostEvent::decode(raw), Err(ControlError::DuplicateKey));
        let raw = br#"{"type":"hello","id":1,"app_id":"a","app_id":"b","instance":"7"}"#;
        assert_eq!(AppRequest::decode(raw), Err(ControlError::DuplicateKey));
    }

    #[test]
    fn unknown_fields_rejected() {
        let raw = br#"{"type":"reply","id":1,"ok":true,"extra":1}"#;
        assert_eq!(HostEvent::decode(raw), Err(ControlError::UnknownField));
        let raw = br#"{"type":"open","id":1,"service":"sam","stream_id":3,"extra":0}"#;
        assert_eq!(AppRequest::decode(raw), Err(ControlError::UnknownField));
    }

    #[test]
    fn unknown_types_fail_closed() {
        let raw = br#"{"type":"permission_request","id":1}"#;
        assert_eq!(HostEvent::decode(raw), Err(ControlError::UnknownType));
        assert_eq!(AppRequest::decode(raw), Err(ControlError::UnknownType));
    }

    #[test]
    fn wrong_direction_rejected() {
        let hello = AppRequest::Hello {
            id: 1,
            app_id: "a".to_string(),
            instance: "1".to_string(),
        };
        assert_eq!(
            HostEvent::decode(&hello.encode()),
            Err(ControlError::WrongDirection)
        );
        let reply = HostEvent::Reply {
            id: 1,
            ok: true,
            error: None,
        };
        assert_eq!(
            AppRequest::decode(&reply.encode()),
            Err(ControlError::WrongDirection)
        );
        let caps = HostEvent::Capabilities { granted: vec![] };
        assert_eq!(
            AppRequest::decode(&caps.encode()),
            Err(ControlError::WrongDirection)
        );
    }

    #[test]
    fn malformed_ids_and_values_rejected() {
        for raw in [
            "{\"type\":\"reply\",\"id\":0,\"ok\":true}",
            "{\"type\":\"reply\",\"id\":-1,\"ok\":true}",
            "{\"type\":\"reply\",\"id\":1.5,\"ok\":true}",
            "{\"type\":\"reply\",\"id\":\"1\",\"ok\":true}",
            "{\"type\":\"stream_closed\",\"stream_id\":0}",
            "{\"type\":\"open\",\"id\":1,\"service\":\"sam\",\"stream_id\":0}",
            "{\"type\":\"hello\",\"id\":1,\"app_id\":\"bad id\",\"instance\":\"1\"}",
            "{\"type\":\"hello\",\"id\":1,\"app_id\":\"a\",\"instance\":\"01\"}",
        ] {
            let bytes = raw.as_bytes();
            let host = HostEvent::decode(bytes);
            let app = AppRequest::decode(bytes);
            assert!(
                host.is_err() && app.is_err()
                    || host == Err(ControlError::WrongDirection)
                    || app == Err(ControlError::WrongDirection),
                "must reject {raw}"
            );
        }
    }

    #[test]
    fn error_rendering_carries_no_payload_sentinel() {
        let sentinel = "SECRET-CREDENTIAL-PAYLOAD-BODY";
        let raw = format!("{{\"type\":\"reply\",\"id\":1,\"ok\":true,\"extra\":\"{sentinel}\"}}");
        let err = HostEvent::decode(raw.as_bytes()).unwrap_err();
        assert!(!format!("{err}").contains(sentinel));
        assert!(!format!("{err:?}").contains(sentinel));
    }
}
