use std::str::FromStr;

use axum::{
    async_trait,
    body::Bytes,
    extract::{FromRequestParts, Host, State},
    http::{request::Parts, HeaderMap, HeaderName, Method, StatusCode},
    response::IntoResponse,
};
use protocol::ContainerProxyRequest;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::AppState;

#[axum::debug_handler]
pub async fn proxy(
    State(app_state): State<AppState>,
    ExtractMethod(method): ExtractMethod,
    Host(host): Host,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, StatusCode> {
    let subdomain = host.split('.').next().ok_or(StatusCode::BAD_REQUEST)?;
    let parts = subdomain.split("--").collect::<Vec<_>>();

    let run_request_id: Uuid = parts
        .get(0)
        .ok_or(StatusCode::BAD_REQUEST)?
        .parse()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let port: u16 = parts
        .get(1)
        .ok_or(StatusCode::BAD_REQUEST)?
        .parse()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let uri = headers
        .get("X-Hydra-URI")
        .ok_or(StatusCode::BAD_REQUEST)?
        .to_str()
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    let proxy_requests = app_state
        .read()
        .await
        .proxy_requests
        .get(&run_request_id.into())
        .ok_or(StatusCode::BAD_REQUEST)?
        .clone();

    let request = ContainerProxyRequest {
        method: method.to_string(),
        uri: uri.to_string(),
        // FIXME: is there a way to not clone the underlying byte representation?
        body: body.to_vec(),
        headers: headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap().to_string()))
            .collect(),
        port: port.into(),
    };
    let (response_tx, response_rx) = oneshot::channel();

    proxy_requests.send((request, response_tx)).await.unwrap();

    let res = response_rx.await.unwrap();
    let mut headers = HeaderMap::new();
    for (k, v) in res.headers {
        headers.insert(
            HeaderName::from_str(&k).map_err(|_| StatusCode::BAD_GATEWAY)?,
            v.parse().map_err(|_| StatusCode::BAD_GATEWAY)?,
        );
    }

    let status_code = StatusCode::try_from(res.status_code).map_err(|_| StatusCode::BAD_GATEWAY)?;

    Ok((status_code, headers, res.body))
}

pub struct ExtractMethod(pub Method);

#[async_trait]
impl<S> FromRequestParts<S> for ExtractMethod {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(ExtractMethod(parts.method.clone()))
    }
}
