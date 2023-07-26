use std::{collections::HashMap, str::FromStr};

use reqwest::{
    header::{HeaderMap, HeaderName},
    Method,
};
use shared::protocol::{ContainerProxyRequest, ContainerProxyResponse};

pub async fn make_proxy_request(
    req: ContainerProxyRequest,
) -> Result<ContainerProxyResponse, String> {
    let method = Method::try_from(req.method.as_str()).map_err(|e| e.to_string())?;
    let url = format!("http://localhost:{}", req.port);
    let mut headers = HeaderMap::new();

    for (k, v) in req.headers {
        headers.insert(
            HeaderName::from_str(&k).map_err(|_| "Error parsing header name")?,
            v.parse().map_err(|_| "Error parsing header value")?,
        );
    }

    log::debug!("Making request: {} {}", method, url);

    // FIXME: THE LINE BELOW SEGFAULTS
    let client = reqwest::Client::new();
    let response = client
        .request(method, url)
        .headers(headers)
        .body(req.body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let container_response = ContainerProxyResponse {
        status_code: response.status().as_u16(),
        headers: response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap().to_string()))
            .collect::<HashMap<_, _>>(),
        // TODO: no-copy
        body: response.bytes().await.map_err(|e| e.to_string())?.to_vec(),
    };

    Ok(container_response)
}
