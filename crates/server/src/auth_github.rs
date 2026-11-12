//! GitHub OAuth sign-in: authorization code flow with PKCE.
//!
//! This is the classic OAuth web-application flow, not a GitHub App. GitHub
//! does not issue OIDC tokens here, so identity comes from `GET /user`; the
//! login name is the stable key and the email address is deliberately ignored
//! because users can keep it private.

use crate::config::GitHubOAuth;
use base64::Engine;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::time::Duration;
use url::Url;

/// Callback path registered on the GitHub OAuth app.
pub const CALLBACK_PATH: &str = "/api/auth/github/callback";

const AUTHORIZE_URL: &str = "https://github.com/login/oauth/authorize";
const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const USER_URL: &str = "https://api.github.com/user";
const USER_AGENT: &str = "threadbase";

#[derive(Debug, Clone)]
pub struct Profile {
    pub login: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

/// Unguessable value that ties the callback to the browser that started it.
#[must_use]
pub fn new_state() -> String {
    random_b64url()
}

/// PKCE verifier: 43 characters from the unreserved set, as GitHub requires.
#[must_use]
pub fn new_verifier() -> String {
    random_b64url()
}

/// `BASE64URL(SHA256(verifier))` without padding.
#[must_use]
pub fn pkce_challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn random_b64url() -> String {
    let seed = format!("{}{}", ulid::Ulid::new(), ulid::Ulid::new());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(seed.as_bytes()))
}

/// GitHub redirect that asks the user to authorize this app.
#[must_use]
pub fn authorize_url(
    oauth: &GitHubOAuth,
    redirect_uri: &str,
    state: &str,
    verifier: &str,
) -> String {
    let mut url = Url::parse(AUTHORIZE_URL).expect("static authorize url");
    url.query_pairs_mut()
        .append_pair("client_id", &oauth.client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("state", state)
        .append_pair("code_challenge", &pkce_challenge(verifier))
        .append_pair("code_challenge_method", "S256")
        .append_pair("allow_signup", "false");
    url.into()
}

/// Exchanges the authorization code for an access token.
pub async fn exchange(
    oauth: &GitHubOAuth,
    redirect_uri: &str,
    code: &str,
    verifier: &str,
) -> anyhow::Result<String> {
    #[derive(Deserialize)]
    struct TokenResponse {
        access_token: Option<String>,
        error: Option<String>,
        error_description: Option<String>,
    }

    // reqwest's `form` helper is behind a feature this build does not enable,
    // and the body is four fields; encode it directly.
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("client_id", &oauth.client_id)
        .append_pair("client_secret", &oauth.client_secret)
        .append_pair("code", code)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("code_verifier", verifier)
        .finish();
    let response = client()?
        .post(TOKEN_URL)
        .header("accept", "application/json")
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        anyhow::bail!("github token endpoint returned {status}");
    }
    let parsed: TokenResponse = serde_json::from_str(&body)
        .map_err(|error| anyhow::anyhow!("github token response was not JSON: {error}"))?;
    match (parsed.access_token, parsed.error) {
        (Some(token), _) if !token.is_empty() => Ok(token),
        (_, Some(error)) => anyhow::bail!(
            "github rejected the code exchange: {error} {}",
            parsed.error_description.unwrap_or_default()
        ),
        _ => anyhow::bail!("github token response contained no access token"),
    }
}

/// Reads the authenticated user's profile.
pub async fn profile(access_token: &str) -> anyhow::Result<Profile> {
    #[derive(Deserialize)]
    struct UserResponse {
        login: String,
        name: Option<String>,
        avatar_url: Option<String>,
    }

    let response = client()?
        .get(USER_URL)
        .header("accept", "application/vnd.github+json")
        .bearer_auth(access_token)
        .send()
        .await?;
    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("github user endpoint returned {status}");
    }
    let user: UserResponse = response.json().await?;
    let login = user.login.trim().to_owned();
    if login.is_empty() || login.len() > 100 {
        anyhow::bail!("github returned an unusable login");
    }
    Ok(Profile {
        login,
        display_name: user
            .name
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty()),
        avatar_url: user
            .avatar_url
            .map(|url| url.trim().to_owned())
            .filter(|url| !url.is_empty()),
    })
}

fn client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(USER_AGENT)
        .build()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GitHubOAuth;

    fn oauth() -> GitHubOAuth {
        GitHubOAuth {
            client_id: "client".to_owned(),
            client_secret: "secret".to_owned(),
        }
    }

    #[test]
    fn state_and_verifier_are_random_and_url_safe() {
        assert_ne!(new_state(), new_state());
        let verifier = new_verifier();
        assert!((43..=128).contains(&verifier.len()));
        assert!(verifier
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~')));
    }

    #[test]
    fn challenge_matches_rfc7636_example() {
        // Verifier from RFC 7636 appendix B.
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn authorize_url_carries_state_and_pkce() {
        let url = authorize_url(
            &oauth(),
            "https://machine.example.ts.net/api/auth/github/callback",
            "st4te",
            "verifier-verifier-verifier-verifier-verifier",
        );
        let parsed = Url::parse(&url).unwrap();
        let params: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(parsed.host_str(), Some("github.com"));
        assert_eq!(params["client_id"], "client");
        assert_eq!(params["state"], "st4te");
        assert_eq!(params["code_challenge_method"], "S256");
        assert!(!params["code_challenge"].is_empty());
        assert_eq!(
            params["redirect_uri"],
            "https://machine.example.ts.net/api/auth/github/callback"
        );
    }
}
