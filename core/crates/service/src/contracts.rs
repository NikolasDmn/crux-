//! Event payload contracts (spec 3.3): the producer validates before writing the outbox row.
//! Schemas are compiled in from `contracts/events/` at the repo root.

use std::collections::HashMap;
use std::sync::OnceLock;

macro_rules! schema {
    ($topic:literal, $version:literal) => {
        (
            ($topic, $version),
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../contracts/events/", $topic, "@", $version, ".json")),
        )
    };
}

const SCHEMAS: &[((&str, &str), &str)] = &[
    schema!("user.created", "1"),
    schema!("user.updated", "1"),
    schema!("user.deleted", "1"),
    schema!("user.tier_changed", "1"),
    schema!("notify.requested", "1"),
    schema!("course.created", "1"),
    schema!("course.updated", "1"),
    schema!("course.archived", "1"),
    schema!("course.deleted", "1"),
    schema!("course.member_changed", "1"),
    schema!("material.created", "1"),
    schema!("material.updated", "1"),
    schema!("material.purge_requested", "1"),
    schema!("material.purge_cancelled", "1"),
    schema!("file.purge_scheduled", "1"),
    schema!("file.purged", "1"),
    schema!("file.restored", "1"),
    schema!("mock.submitted", "1"),
    schema!("mock.ai_grade_decided", "1"),
    schema!("ai.grade_suggested", "1"),
    schema!("user.purged", "1"),
];

pub const ENVELOPE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../contracts/events/envelope.json"));

fn validators() -> &'static HashMap<(String, u16), jsonschema::Validator> {
    static V: OnceLock<HashMap<(String, u16), jsonschema::Validator>> = OnceLock::new();
    V.get_or_init(|| {
        SCHEMAS
            .iter()
            .map(|((topic, version), text)| {
                let schema: serde_json::Value = serde_json::from_str(text).expect("contract JSON");
                let validator = jsonschema::validator_for(&schema).expect("contract schema compiles");
                ((topic.to_string(), version.parse().expect("version")), validator)
            })
            .collect()
    })
}

#[derive(Debug, thiserror::Error)]
pub enum ContractError {
    #[error("no contract for {0}@{1}")]
    Unknown(String, u16),
    #[error("{topic}@{version} payload invalid: {errors:?}")]
    Invalid { topic: String, version: u16, errors: Vec<String> },
}

pub fn validate(topic: &str, version: u16, payload: &serde_json::Value) -> Result<(), ContractError> {
    let v = validators().get(&(topic.to_string(), version)).ok_or_else(|| ContractError::Unknown(topic.into(), version))?;
    let errors: Vec<String> = v.iter_errors(payload).map(|e| format!("{}: {}", e.instance_path, e)).collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ContractError::Invalid { topic: topic.into(), version, errors })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_produced_topic_has_a_contract() {
        for topic in crux_domain::topics::PRODUCED {
            assert!(validators().contains_key(&(topic.to_string(), 1)), "missing contract for {topic}");
        }
    }

    #[test]
    fn user_created_fixture_validates() {
        let ok = json!({
            "userRef": "01TEST00000000000000000001", "email": "a@b.nl", "emailVerified": false,
            "timeZone": "Europe/Amsterdam", "locale": "en", "tierCode": "free", "storageQuotaBytes": 1073741824
        });
        validate("user.created", 1, &ok).unwrap();
        let mut bad = ok.clone();
        bad["storageQuotaBytes"] = json!(-1);
        bad["extra"] = json!(1);
        let Err(ContractError::Invalid { errors, .. }) = validate("user.created", 1, &bad) else { panic!() };
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn unknown_topic_is_rejected() {
        assert!(matches!(validate("nope", 1, &json!({})), Err(ContractError::Unknown(..))));
    }
}
