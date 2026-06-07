use axum::http::{header, HeaderMap};
use rand::{distributions::Alphanumeric, Rng};

pub fn new_session_id() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(40)
        .map(char::from)
        .collect()
}

pub fn find_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie.split(';').find_map(|part| {
        let (key, value) = part.trim().split_once('=')?;
        (key == name && is_valid_session_id(value)).then(|| value.to_owned())
    })
}

pub fn cookie_header(name: &str, value: &str, max_age_seconds: u64) -> String {
    format!("{name}={value}; Max-Age={max_age_seconds}; Path=/; SameSite=Lax; HttpOnly")
}

fn is_valid_session_id(value: &str) -> bool {
    value.len() >= 32 && value.bytes().all(|b| b.is_ascii_alphanumeric())
}
