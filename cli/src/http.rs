//! One way out to the network. Keys travel in headers only, never in a URL, so errors cannot leak them.
use crate::util::R;
use std::time::Duration;

pub struct Resp {
    pub status: u16,
    pub body: String,
}

const MAX_RESPONSE: u64 = 64 * 1024 * 1024;

fn agent(timeout_secs: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(timeout_secs)))
        .http_status_as_error(false)
        .build()
        .into()
}

fn read(mut r: ureq::http::Response<ureq::Body>) -> R<Resp> {
    let status = r.status().as_u16();
    let body = r.body_mut().with_config().limit(MAX_RESPONSE).read_to_string().map_err(|e| e.to_string())?;
    Ok(Resp { status, body })
}

pub fn post_json(url: &str, headers: &[(&str, String)], body: &str, timeout_secs: u64) -> R<Resp> {
    let mut req = agent(timeout_secs).post(url).header("Content-Type", "application/json");
    for (k, v) in headers {
        req = req.header(*k, v.as_str());
    }
    read(req.send(body).map_err(|e| e.to_string())?)
}

pub fn get(url: &str, timeout_secs: u64) -> R<Resp> {
    read(agent(timeout_secs).get(url).call().map_err(|e| e.to_string())?)
}
