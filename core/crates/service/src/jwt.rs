//! EdDSA (Ed25519) JWTs for access tokens and service tokens (spec 3.2, 4.1), and the JWKS other
//! services verify with. Compact JWS by hand on ed25519-dalek: a few lines, and no key-format
//! mismatch between libraries.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ed25519_dalek::pkcs8::{DecodePrivateKey, EncodePrivateKey};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const ISSUER: &str = "crux-core";
pub const ACCESS_AUDIENCE: &str = "crux-api";
pub const SERVICE_TOKEN_TTL_SECS: i64 = 300;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccessClaims {
    pub iss: String,
    pub aud: String,
    /// User PublicId.
    pub sub: String,
    pub role: String,
    pub tier: String,
    pub iat: i64,
    pub exp: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceClaims {
    pub iss: String,
    /// The service being called.
    pub aud: String,
    /// The calling service.
    pub sub: String,
    pub typ: String,
    pub iat: i64,
    pub exp: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TokenError {
    #[error("malformed token")]
    Malformed,
    #[error("bad signature")]
    BadSignature,
    #[error("wrong issuer or audience")]
    WrongAudience,
    #[error("expired")]
    Expired,
}

pub struct JwtKeys {
    signing: SigningKey,
    verifying: VerifyingKey,
    kid: String,
}

#[derive(Serialize, Deserialize)]
struct Header {
    alg: String,
    typ: String,
    kid: String,
}

impl JwtKeys {
    pub fn from_pkcs8_pem(pem: &str) -> anyhow::Result<Self> {
        let signing = SigningKey::from_pkcs8_pem(pem).map_err(|e| anyhow::anyhow!("JWT private key: {e}"))?;
        Ok(Self::from_signing_key(signing))
    }

    pub fn from_signing_key(signing: SigningKey) -> Self {
        let verifying = signing.verifying_key();
        let kid = hex::encode(&Sha256::digest(verifying.as_bytes())[..8]);
        Self { signing, verifying, kid }
    }

    /// A fresh key as PKCS#8 PEM (`crux-core gen-keys`).
    pub fn generate_pem() -> String {
        let key = SigningKey::generate(&mut rand::rngs::OsRng);
        key.to_pkcs8_pem(ed25519_dalek::pkcs8::spki::der::pem::LineEnding::LF).expect("encode key").to_string()
    }

    pub fn kid(&self) -> &str {
        &self.kid
    }

    /// RFC 8037 OKP key set, served at `/internal/v1/jwks`.
    pub fn jwks(&self) -> serde_json::Value {
        serde_json::json!({
            "keys": [{
                "kty": "OKP",
                "crv": "Ed25519",
                "alg": "EdDSA",
                "use": "sig",
                "kid": self.kid,
                "x": URL_SAFE_NO_PAD.encode(self.verifying.as_bytes()),
            }]
        })
    }

    fn sign<C: Serialize>(&self, claims: &C) -> String {
        let header = Header { alg: "EdDSA".into(), typ: "JWT".into(), kid: self.kid.clone() };
        let h = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).expect("header"));
        let c = URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims).expect("claims"));
        let input = format!("{h}.{c}");
        let sig = self.signing.sign(input.as_bytes());
        format!("{input}.{}", URL_SAFE_NO_PAD.encode(sig.to_bytes()))
    }

    fn verify<C: DeserializeOwned>(&self, token: &str) -> Result<C, TokenError> {
        let mut parts = token.split('.');
        let (Some(h), Some(c), Some(s), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
            return Err(TokenError::Malformed);
        };
        let header: Header =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(h).map_err(|_| TokenError::Malformed)?).map_err(|_| TokenError::Malformed)?;
        if header.alg != "EdDSA" {
            return Err(TokenError::Malformed);
        }
        let sig_bytes: [u8; 64] =
            URL_SAFE_NO_PAD.decode(s).map_err(|_| TokenError::Malformed)?.try_into().map_err(|_| TokenError::Malformed)?;
        let input = &token[..h.len() + 1 + c.len()];
        self.verifying.verify(input.as_bytes(), &Signature::from_bytes(&sig_bytes)).map_err(|_| TokenError::BadSignature)?;
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(c).map_err(|_| TokenError::Malformed)?).map_err(|_| TokenError::Malformed)
    }

    pub fn sign_access(&self, claims: &AccessClaims) -> String {
        self.sign(claims)
    }

    pub fn verify_access(&self, token: &str, now_unix: i64) -> Result<AccessClaims, TokenError> {
        let claims: AccessClaims = self.verify(token)?;
        if claims.iss != ISSUER || claims.aud != ACCESS_AUDIENCE {
            return Err(TokenError::WrongAudience);
        }
        if claims.exp <= now_unix {
            return Err(TokenError::Expired);
        }
        Ok(claims)
    }

    pub fn sign_service(&self, caller: &str, audience: &str, now_unix: i64) -> String {
        self.sign(&ServiceClaims {
            iss: ISSUER.into(),
            aud: audience.into(),
            sub: caller.into(),
            typ: "service".into(),
            iat: now_unix,
            exp: now_unix + SERVICE_TOKEN_TTL_SECS,
        })
    }

    pub fn verify_service(&self, token: &str, audience: &str, now_unix: i64) -> Result<ServiceClaims, TokenError> {
        let claims: ServiceClaims = self.verify(token)?;
        if claims.iss != ISSUER || claims.aud != audience || claims.typ != "service" {
            return Err(TokenError::WrongAudience);
        }
        if claims.exp <= now_unix {
            return Err(TokenError::Expired);
        }
        Ok(claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys() -> JwtKeys {
        JwtKeys::from_pkcs8_pem(&JwtKeys::generate_pem()).unwrap()
    }

    fn claims(exp: i64) -> AccessClaims {
        AccessClaims {
            iss: ISSUER.into(),
            aud: ACCESS_AUDIENCE.into(),
            sub: "01TEST00000000000000000001".into(),
            role: "User".into(),
            tier: "free".into(),
            iat: 1000,
            exp,
        }
    }

    #[test]
    fn access_round_trip() {
        let k = keys();
        let token = k.sign_access(&claims(2000));
        assert_eq!(k.verify_access(&token, 1500).unwrap(), claims(2000));
    }

    #[test]
    fn access_expiry_and_tampering() {
        let k = keys();
        let token = k.sign_access(&claims(2000));
        assert_eq!(k.verify_access(&token, 2000), Err(TokenError::Expired));

        let mut parts: Vec<String> = token.split('.').map(String::from).collect();
        let mut c = claims(2000);
        c.role = "Admin".into();
        parts[1] = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&c).unwrap());
        assert_eq!(k.verify_access(&parts.join("."), 1500), Err(TokenError::BadSignature));

        assert_eq!(k.verify_access("a.b", 1500), Err(TokenError::Malformed));
        assert_eq!(keys().verify_access(&token, 1500), Err(TokenError::BadSignature), "other key");
    }

    #[test]
    fn rejects_alg_none() {
        let k = keys();
        let token = k.sign_access(&claims(2000));
        let parts: Vec<&str> = token.split('.').collect();
        let none = URL_SAFE_NO_PAD.encode(br#"{"alg":"none","typ":"JWT","kid":"x"}"#);
        assert_eq!(k.verify_access(&format!("{none}.{}.", parts[1]), 1500), Err(TokenError::Malformed));
    }

    #[test]
    fn service_tokens_are_bound_to_audience_and_not_access_tokens() {
        let k = keys();
        let svc = k.sign_service("files", "core", 1000);
        assert_eq!(k.verify_service(&svc, "core", 1100).unwrap().sub, "files");
        assert_eq!(k.verify_service(&svc, "ai", 1100), Err(TokenError::WrongAudience));
        assert_eq!(k.verify_service(&svc, "core", 1300), Err(TokenError::Expired));
        assert!(k.verify_access(&svc, 1100).is_err(), "a service token is not an access token");
        let access = k.sign_access(&claims(2000));
        assert!(k.verify_service(&access, ACCESS_AUDIENCE, 1500).is_err(), "and vice versa");
    }

    const INTEROP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../contracts/fixtures/jwt-interop.json");

    /// Regenerate with `cargo test -p crux-service write_interop_fixture -- --ignored`.
    /// The Python services verify the same fixture (services/common/tests/test_auth.py).
    #[test]
    #[ignore]
    fn write_interop_fixture() {
        let pem = JwtKeys::generate_pem();
        let k = JwtKeys::from_pkcs8_pem(&pem).unwrap();
        let fixture = serde_json::json!({
            "comment": "Test-only key. Tokens signed by crux-core; verified by the Python services.",
            "privateKeyPem": pem,
            "jwks": k.jwks(),
            "accessToken": k.sign_access(&claims(4_102_444_800)),
            "serviceToken": k.sign(&ServiceClaims {
                iss: ISSUER.into(), aud: "files".into(), sub: "core".into(), typ: "service".into(), iat: 1000, exp: 4_102_444_800,
            }),
        });
        std::fs::create_dir_all(std::path::Path::new(INTEROP).parent().unwrap()).unwrap();
        std::fs::write(INTEROP, serde_json::to_string_pretty(&fixture).unwrap() + "\n").unwrap();
    }

    #[test]
    fn interop_fixture_is_current() {
        let Ok(text) = std::fs::read_to_string(INTEROP) else { return };
        let f: serde_json::Value = serde_json::from_str(&text).unwrap();
        let k = JwtKeys::from_pkcs8_pem(f["privateKeyPem"].as_str().unwrap()).unwrap();
        assert_eq!(k.jwks(), f["jwks"]);
        assert_eq!(k.verify_access(f["accessToken"].as_str().unwrap(), 2000).unwrap(), claims(4_102_444_800));
        assert_eq!(k.verify_service(f["serviceToken"].as_str().unwrap(), "files", 2000).unwrap().sub, "core");
    }

    /// `openssl genpkey -algorithm ed25519` (PKCS#8 v1, no public key) is how `make keys` and CI
    /// make the signing key. Test-only key.
    #[test]
    fn loads_openssl_generated_keys() {
        let pem =
            "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEILaMKfsZspqulSZJver4+S54NGJL15TMRTrBYotv8Cw3\n-----END PRIVATE KEY-----\n";
        let k = JwtKeys::from_pkcs8_pem(pem).unwrap();
        let token = k.sign_access(&claims(2000));
        assert!(k.verify_access(&token, 1500).is_ok());
    }

    #[test]
    fn jwks_exposes_public_key_only() {
        let k = keys();
        let jwks = k.jwks();
        let key = &jwks["keys"][0];
        assert_eq!(key["kid"], k.kid());
        assert_eq!(key["crv"], "Ed25519");
        assert!(key.get("d").is_none());
        assert_eq!(URL_SAFE_NO_PAD.decode(key["x"].as_str().unwrap()).unwrap().len(), 32);
    }
}
