use crate::{parse_orgs, parse_usage, Org, ParseError, SessionCookies, Usage};
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, COOKIE, ORIGIN, REFERER, USER_AGENT as UA};
use std::time::Duration;
use url::Url;

/// Desktop Firefox UA: the session (and Cloudflare's cf_clearance) comes from Firefox.
pub const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:144.0) Gecko/20100101 Firefox/144.0";
const BODY_HEAD_MAX: usize = 2048;

#[derive(Debug, Clone, PartialEq)]
pub enum ApiError {
    Expired,
    Unavailable(String),
    Unexpected { status: u16, body_head: String },
}

#[async_trait]
pub trait ClaudeApi: Send + Sync {
    async fn orgs(&self, c: &SessionCookies) -> Result<Vec<Org>, ApiError>;
    async fn usage(&self, c: &SessionCookies, org_uuid: &str) -> Result<Usage, ApiError>;
}

pub struct HttpApi {
    base: Url,
    client: reqwest::Client,
}

impl HttpApi {
    pub fn new(base: Url) -> Self {
        Self::with_timeout(base, Duration::from_secs(15))
    }

    pub fn claude() -> Self {
        Self::new(Url::parse("https://claude.ai").expect("static url"))
    }

    pub fn with_timeout(base: Url, timeout: Duration) -> Self {
        let origin = base.origin().ascii_serialization();
        let mut h = HeaderMap::new();
        h.insert(ACCEPT, HeaderValue::from_static("application/json"));
        h.insert(UA, HeaderValue::from_static(USER_AGENT));
        if let Ok(v) = HeaderValue::from_str(&origin) {
            h.insert(ORIGIN, v);
        }
        if let Ok(v) = HeaderValue::from_str(&format!("{origin}/settings/usage")) {
            h.insert(REFERER, v);
        }
        // No cookie store: the Cookie header is set explicitly per request.
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .default_headers(h)
            .build()
            .expect("reqwest client");
        Self { base, client }
    }

    async fn get_body(&self, path: &str, c: &SessionCookies) -> Result<(u16, String), ApiError> {
        let url = self
            .base
            .join(path)
            .map_err(|e| ApiError::Unavailable(format!("bad url: {e}")))?;
        let cookie = HeaderValue::from_str(&c.header())
            .map_err(|_| ApiError::Unavailable("invalid cookie header".into()))?;
        let resp = self
            .client
            .get(url)
            .header(COOKIE, cookie)
            .send()
            .await
            .map_err(|e| ApiError::Unavailable(net_msg(e)))?;
        let status = resp.status().as_u16();
        let body = resp
            .text()
            .await
            .map_err(|e| ApiError::Unavailable(net_msg(e)))?;
        Ok((status, body))
    }

    /// Classify the response; `Ok(body)` only for a 200.
    async fn fetch(&self, path: &str, c: &SessionCookies) -> Result<(u16, String), ApiError> {
        let (status, body) = self.get_body(path, c).await?;
        match status {
            200 => Ok((status, body)),
            401 => Err(ApiError::Expired),
            403 if is_auth_error(&body) => Err(ApiError::Expired),
            _ => {
                // 403 without an auth error (Cloudflare HTML), other 4xx (404, 429 = rate
                // limit: retry later) and 5xx are all transient from our point of view.
                tracing::warn!(status, body_head = %head(&body), "claude.ai request failed");
                Err(ApiError::Unavailable(format!("HTTP {status}")))
            }
        }
    }
}

fn net_msg(e: reqwest::Error) -> String {
    // Without the URL: it may carry the org uuid, and nothing here needs it.
    let msg = if e.is_timeout() { "request timed out".to_string() } else { e.without_url().to_string() };
    tracing::warn!(error = %msg, "claude.ai network error");
    msg
}

fn is_auth_error(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.pointer("/error/type").and_then(|t| t.as_str()).map(str::to_owned))
        .is_some_and(|t| t == "permission_error" || t == "authentication_error")
}

/// First `BODY_HEAD_MAX` bytes, cut on a char boundary.
fn head(body: &str) -> String {
    let mut end = body.len().min(BODY_HEAD_MAX);
    while !body.is_char_boundary(end) {
        end -= 1;
    }
    body[..end].to_owned()
}

fn unexpected(status: u16, body: &str, e: &ParseError) -> ApiError {
    let body_head = head(body);
    tracing::warn!(status, body_head = %body_head, error = %e, "unexpected claude.ai response");
    ApiError::Unexpected { status, body_head }
}

#[async_trait]
impl ClaudeApi for HttpApi {
    async fn orgs(&self, c: &SessionCookies) -> Result<Vec<Org>, ApiError> {
        let (status, body) = self.fetch("/api/organizations", c).await?;
        parse_orgs(&body).map_err(|e| unexpected(status, &body, &e))
    }

    async fn usage(&self, c: &SessionCookies, org_uuid: &str) -> Result<Usage, ApiError> {
        let (status, body) = self.fetch(&format!("/api/organizations/{org_uuid}/usage"), c).await?;
        parse_usage(&body).map_err(|e| unexpected(status, &body, &e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const REAL: &str = include_str!("../tests/fixtures/usage_real_2026-11-02.json");
    const ORGS: &str = include_str!("../tests/fixtures/orgs_two.json");

    fn cookies() -> SessionCookies {
        SessionCookies { cookies: vec![("sessionKey".into(), "sk-test".into()), ("a".into(), "1".into())] }
    }
    fn api(s: &MockServer) -> HttpApi {
        HttpApi::with_timeout(Url::parse(&s.uri()).unwrap(), Duration::from_secs(1))
    }
    async fn usage_with(resp: ResponseTemplate) -> Result<Usage, ApiError> {
        let s = MockServer::start().await;
        Mock::given(method("GET")).and(path("/api/organizations/org1/usage")).respond_with(resp).mount(&s).await;
        api(&s).usage(&cookies(), "org1").await
    }

    #[tokio::test]
    async fn ok_200_real_fixture() {
        assert!(usage_with(ResponseTemplate::new(200).set_body_string(REAL)).await.is_ok());
    }
    #[tokio::test]
    async fn orgs_ok() {
        let s = MockServer::start().await;
        Mock::given(method("GET")).and(path("/api/organizations"))
            .respond_with(ResponseTemplate::new(200).set_body_string(ORGS)).mount(&s).await;
        assert!(!api(&s).orgs(&cookies()).await.unwrap().is_empty());
    }
    #[tokio::test]
    async fn status_401_is_expired() {
        assert_eq!(usage_with(ResponseTemplate::new(401)).await.unwrap_err(), ApiError::Expired);
    }
    #[tokio::test]
    async fn status_403_permission_error_is_expired() {
        let r = ResponseTemplate::new(403)
            .insert_header("content-type", "application/json")
            .set_body_string(r#"{"type":"error","error":{"type":"permission_error"}}"#);
        assert_eq!(usage_with(r).await.unwrap_err(), ApiError::Expired);
    }
    #[tokio::test]
    async fn status_403_authentication_error_is_expired() {
        let r = ResponseTemplate::new(403).set_body_string(r#"{"error":{"type":"authentication_error"}}"#);
        assert_eq!(usage_with(r).await.unwrap_err(), ApiError::Expired);
    }
    #[tokio::test]
    async fn status_403_html_is_unavailable() {
        let r = ResponseTemplate::new(403)
            .insert_header("content-type", "text/html")
            .set_body_string("<html><title>Just a moment...</title>cf-chl</html>");
        assert!(matches!(usage_with(r).await.unwrap_err(), ApiError::Unavailable(_)));
    }
    #[tokio::test]
    async fn status_503_and_429_are_unavailable() {
        assert!(matches!(usage_with(ResponseTemplate::new(503)).await.unwrap_err(), ApiError::Unavailable(_)));
        assert!(matches!(usage_with(ResponseTemplate::new(429)).await.unwrap_err(), ApiError::Unavailable(_)));
        assert!(matches!(usage_with(ResponseTemplate::new(404)).await.unwrap_err(), ApiError::Unavailable(_)));
    }
    #[tokio::test]
    async fn unexpected_shape_keeps_body_head() {
        match usage_with(ResponseTemplate::new(200).set_body_string(r#"{"hello":"world"}"#)).await.unwrap_err() {
            ApiError::Unexpected { status, body_head } => {
                assert_eq!(status, 200);
                assert!(body_head.starts_with(r#"{"hello""#));
            }
            e => panic!("{e:?}"),
        }
    }
    #[tokio::test]
    async fn non_json_200_is_unexpected() {
        assert!(matches!(
            usage_with(ResponseTemplate::new(200).set_body_string("<html>")).await.unwrap_err(),
            ApiError::Unexpected { status: 200, .. }
        ));
    }
    #[tokio::test]
    async fn body_head_cut_on_char_boundary() {
        let body = "é".repeat(2000); // 4000 bytes; 2048 is a boundary, 2047 would not be odd-split
        let body = format!("a{body}"); // shifts boundaries: byte 2048 falls mid-char
        match usage_with(ResponseTemplate::new(200).set_body_string(body)).await.unwrap_err() {
            ApiError::Unexpected { body_head, .. } => {
                assert!(body_head.len() <= 2048 && body_head.len() >= 2046);
            }
            e => panic!("{e:?}"),
        }
    }
    #[tokio::test]
    async fn sends_cookie_and_browser_headers() {
        let s = MockServer::start().await;
        Mock::given(method("GET")).and(path("/api/organizations/org1/usage"))
            .and(header("cookie", cookies().header().as_str()))
            .and(header("accept", "application/json"))
            .and(header("user-agent", USER_AGENT))
            .and(header("origin", s.uri().as_str()))
            .and(header("referer", format!("{}/settings/usage", s.uri()).as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_string(REAL)).expect(1).mount(&s).await;
        api(&s).usage(&cookies(), "org1").await.unwrap();
    }
    #[tokio::test]
    async fn timeout_is_unavailable() {
        let r = ResponseTemplate::new(200).set_body_string(REAL).set_delay(Duration::from_secs(16));
        assert!(matches!(usage_with(r).await.unwrap_err(), ApiError::Unavailable(_)));
    }
    #[tokio::test]
    async fn connection_refused_is_unavailable() {
        let a = HttpApi::with_timeout(Url::parse("http://127.0.0.1:1").unwrap(), Duration::from_secs(1));
        assert!(matches!(a.orgs(&cookies()).await.unwrap_err(), ApiError::Unavailable(_)));
    }
}
