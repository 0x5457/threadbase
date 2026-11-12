//! GitHub OAuth sign-in: authorization code flow with PKCE.
//!
//! This is the classic OAuth web-application flow, not a GitHub App. GitHub
//! does not issue OIDC tokens here, so identity comes from `GET /user`; the
//! numeric account id is the stable key, while the login name is only a mutable
//! display handle (GitHub allows renames and re-registration of freed logins).
//! The email address is deliberately ignored because users can keep it private.

use crate::config::GitHubOAuth;
use oauth2::{
    basic::BasicClient, AuthType, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken,
    EndpointNotSet, EndpointSet, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, TokenResponse,
    TokenUrl,
};
use serde::Deserialize;
use std::borrow::Cow;
use std::time::Duration;

/// Callback path registered on the GitHub OAuth app.
pub const CALLBACK_PATH: &str = "/api/auth/github/callback";

const AUTHORIZE_URL: &str = "https://github.com/login/oauth/authorize";
const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const USER_URL: &str = "https://api.github.com/user";
const USER_AGENT: &str = "threadbase";

#[derive(Debug, Clone)]
pub struct Profile {
    /// Immutable GitHub account id; survives login renames.
    pub id: i64,
    pub login: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

/// OAuth client configured for GitHub's authorize and token endpoints.
type GitHubClient =
    BasicClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>;

fn github_client(oauth: &GitHubOAuth) -> GitHubClient {
    BasicClient::new(ClientId::new(oauth.client_id.clone()))
        .set_client_secret(ClientSecret::new(oauth.client_secret.clone()))
        // GitHub expects client credentials in the form body, not HTTP Basic.
        .set_auth_type(AuthType::RequestBody)
        .set_auth_uri(AuthUrl::new(AUTHORIZE_URL.to_owned()).expect("valid github authorize url"))
        .set_token_uri(TokenUrl::new(TOKEN_URL.to_owned()).expect("valid github token url"))
}

fn validated_redirect(uri: &str) -> anyhow::Result<RedirectUrl> {
    RedirectUrl::new(uri.to_owned())
        .map_err(|error| anyhow::anyhow!("invalid github callback url: {error}"))
}

fn pkce(verifier: &str) -> PkceCodeChallenge {
    PkceCodeChallenge::from_code_verifier_sha256(&PkceCodeVerifier::new(verifier.to_owned()))
}

/// Unguessable value that ties the callback to the browser that started it.
#[must_use]
pub fn new_state() -> String {
    CsrfToken::new_random().secret().to_owned()
}

/// PKCE verifier: 43 characters from the unreserved set, as GitHub requires.
#[must_use]
pub fn new_verifier() -> String {
    let (_, verifier) = PkceCodeChallenge::new_random_sha256();
    verifier.secret().to_owned()
}

/// `BASE64URL(SHA256(verifier))` without padding.
#[must_use]
pub fn pkce_challenge(verifier: &str) -> String {
    pkce(verifier).as_str().to_owned()
}

/// GitHub redirect that asks the user to authorize this app.
pub fn authorize_url(
    oauth: &GitHubOAuth,
    redirect_uri: &str,
    state: &str,
    verifier: &str,
) -> anyhow::Result<String> {
    let (url, _) = github_client(oauth)
        .authorize_url(|| CsrfToken::new(state.to_owned()))
        .set_redirect_uri(Cow::Owned(validated_redirect(redirect_uri)?))
        .set_pkce_challenge(pkce(verifier))
        .add_extra_param("allow_signup", "false")
        .url();
    Ok(url.to_string())
}

/// Exchanges the authorization code for an access token.
pub async fn exchange(
    oauth: &GitHubOAuth,
    redirect_uri: &str,
    code: &str,
    verifier: &str,
) -> anyhow::Result<String> {
    let http = token_client()?;
    let token = github_client(oauth)
        .exchange_code(AuthorizationCode::new(code.to_owned()))
        .set_redirect_uri(Cow::Owned(validated_redirect(redirect_uri)?))
        .set_pkce_verifier(PkceCodeVerifier::new(verifier.to_owned()))
        .request_async(&http)
        .await
        .map_err(|error| anyhow::anyhow!("github token exchange failed: {error}"))?;
    Ok(token.access_token().secret().to_owned())
}

/// Reads the authenticated user's profile.
pub async fn profile(access_token: &str) -> anyhow::Result<Profile> {
    #[derive(Deserialize)]
    struct UserResponse {
        id: i64,
        login: String,
        name: Option<String>,
        avatar_url: Option<String>,
    }

    let response = api_client()?
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
    if user.id <= 0 {
        anyhow::bail!("github returned an unusable account id");
    }
    let login = user.login.trim().to_owned();
    if login.is_empty() || login.len() > 100 {
        anyhow::bail!("github returned an unusable login");
    }
    Ok(Profile {
        id: user.id,
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

fn api_client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(USER_AGENT)
        .build()?)
}

fn token_client() -> anyhow::Result<oauth2::reqwest::Client> {
    Ok(oauth2::reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(USER_AGENT)
        .redirect(oauth2::reqwest::redirect::Policy::none())
        .build()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GitHubOAuth;
    use url::Url;

    fn oauth() -> GitHubOAuth {
        GitHubOAuth {
            client_id: "client".to_owned(),
            client_secret: "secret".to_owned(),
        }
    }

    #[test]
    fn state_and_verifier_are_random_and_url_safe() {
        let state = new_state();
        assert_ne!(state, new_state());
        assert!(state
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_')));
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
        )
        .unwrap();
        let parsed = Url::parse(&url).unwrap();
        let params: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert_eq!(parsed.host_str(), Some("github.com"));
        assert_eq!(params["client_id"], "client");
        assert_eq!(params["state"], "st4te");
        assert_eq!(params["code_challenge_method"], "S256");
        assert_eq!(params["allow_signup"], "false");
        assert!(!params["code_challenge"].is_empty());
        assert_eq!(
            params["redirect_uri"],
            "https://machine.example.ts.net/api/auth/github/callback"
        );
    }

    #[test]
    fn authorize_url_rejects_an_invalid_callback() {
        assert!(authorize_url(
            &oauth(),
            "machine.example.ts.net/api/auth/github/callback",
            "st4te",
            "verifier-verifier-verifier-verifier-verifier",
        )
        .is_err());
    }
}
