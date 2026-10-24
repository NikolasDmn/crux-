//! OpenAPI 3.1 (spec 3.1). Core serves its own at `/openapi.json` and the merged document for the
//! whole API at `/api/v1/openapi.json` (the gateway forwards that path unstripped).

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::openapi::{InfoBuilder, OpenApi, ServerBuilder};

use crate::AppState;

pub fn finish(mut api: OpenApi) -> OpenApi {
    api.info = InfoBuilder::new().title("Crux core").version(env!("CARGO_PKG_VERSION")).build();
    api.servers = Some(vec![ServerBuilder::new().url("/api/v1").build()]);
    let components = api.components.get_or_insert_with(Default::default);
    components.add_security_scheme(
        "bearer",
        SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).bearer_format("JWT").build()),
    );
    api
}

pub fn router(api: OpenApi) -> Router<AppState> {
    let own = api.clone();
    Router::new()
        .route("/openapi.json", get(move || async move { Json(own) }))
        .route("/api/v1/openapi.json", get(move |state: State<AppState>| merged(state, api)))
}

/// Core's paths plus each peer's paths under its prefix. A peer that is down is left out (the ai
/// service may not be deployed at all; spec 1). Merged as plain JSON: the Python services' FastAPI
/// documents use 3.1 shapes the typed model doesn't read. A peer schema whose name is already taken
/// by a different schema is renamed `<Peer><Name>` and that peer's `$ref`s follow.
async fn merged(State(state): State<AppState>, mut api: OpenApi) -> Json<serde_json::Value> {
    api.info.title = "Crux API".into();
    let mut doc = serde_json::to_value(&api).unwrap_or_default();
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(2)).build().unwrap_or_default();
    for (prefix, base) in &state.peers.openapi_sources {
        let Ok(resp) = client.get(format!("{base}/openapi.json")).send().await else { continue };
        let Ok(peer) = resp.json::<serde_json::Value>().await else { continue };
        merge_peer(&mut doc, prefix, peer);
    }
    Json(doc)
}

/// Adds one peer document to the merged one (see [`merged`]).
pub fn merge_peer(doc: &mut serde_json::Value, prefix: &str, peer: serde_json::Value) {
    use serde_json::{Map, Value};
    let tag: String = prefix
        .trim_matches('/')
        .split('/')
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_ascii_uppercase().to_string() + c.as_str()).unwrap_or_default()
        })
        .collect();
    let taken: Map<String, Value> = doc.pointer("/components/schemas").and_then(Value::as_object).cloned().unwrap_or_default();
    let theirs = peer.pointer("/components/schemas").and_then(Value::as_object).cloned().unwrap_or_default();
    let renames: Vec<(String, String)> = theirs
        .iter()
        .filter(|(name, schema)| taken.get(*name).is_some_and(|mine| mine != *schema))
        .map(|(name, _)| (name.clone(), format!("{tag}{name}")))
        .collect();
    let mut text = peer.to_string();
    for (from, to) in &renames {
        text = text.replace(&format!("\"#/components/schemas/{from}\""), &format!("\"#/components/schemas/{to}\""));
    }
    let peer: Value = serde_json::from_str(&text).unwrap_or_default();
    let renamed: std::collections::HashMap<&str, &str> = renames.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    if let Some(paths) = peer.get("paths").and_then(Value::as_object) {
        let mine = doc.as_object_mut().and_then(|d| d.entry("paths").or_insert_with(|| Value::Object(Map::new())).as_object_mut());
        if let Some(mine) = mine {
            // Internal routes are blocked at the gateway: never part of the public document.
            for (path, item) in paths.iter().filter(|(p, _)| !p.starts_with("/internal")) {
                mine.insert(format!("{prefix}{path}"), item.clone());
            }
        }
    }
    if let Some(schemas) = peer.pointer("/components/schemas").and_then(Value::as_object) {
        let components =
            doc.as_object_mut().and_then(|d| d.entry("components").or_insert_with(|| Value::Object(Map::new())).as_object_mut());
        if let Some(mine) = components.and_then(|c| c.entry("schemas").or_insert_with(|| Value::Object(Map::new())).as_object_mut()) {
            for (name, schema) in schemas {
                let name = renamed.get(name.as_str()).map(|n| n.to_string()).unwrap_or_else(|| name.clone());
                mine.insert(name, schema.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn peers_merge_under_their_prefix_and_clashing_schemas_are_renamed() {
        let mut doc = json!({"paths": {"/me": {}}, "components": {"schemas": {"Same": {"type": "string"}, "Clash": {"type": "string"}}}});
        let peer = json!({
            "paths": {"/internal/v1/exports": {}, "/storage": {"get": {"responses": {"200": {"content": {"application/json": {"schema": {"$ref": "#/components/schemas/Clash"}}}}}}}},
            "components": {"schemas": {"Same": {"type": "string"}, "Clash": {"type": "object"}, "Own": {"type": "integer"}}}
        });
        super::merge_peer(&mut doc, "/files", peer);
        assert!(doc["paths"]["/files/storage"].is_object() && doc["paths"]["/me"].is_object());
        assert!(doc["paths"].get("/files/internal/v1/exports").is_none(), "internal routes stay out");
        let s = &doc["components"]["schemas"];
        assert_eq!(
            (s["Clash"]["type"].as_str(), s["FilesClash"]["type"].as_str(), s["Own"]["type"].as_str()),
            (Some("string"), Some("object"), Some("integer"))
        );
        let r = &doc["paths"]["/files/storage"]["get"]["responses"]["200"]["content"]["application/json"]["schema"]["$ref"];
        assert_eq!(r, "#/components/schemas/FilesClash");
    }
}
