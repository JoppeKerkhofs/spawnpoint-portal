pub mod bedrock;
pub mod discord;
pub mod download;
pub mod gamefiles;
pub mod instance;
pub mod join;
pub mod java;
pub mod launch;
pub mod loader;
pub mod mods;
pub mod pack;
pub mod panel;
pub mod settings;
pub mod sync;
pub mod error;
pub mod paths;
pub mod play;
pub mod secrets;
pub mod version;

pub use error::{Error, Result};
pub use paths::Paths;

// Hash-less downloads (legacy Forge libraries) must not be downgraded to plain http by a redirect.
fn downgrades(from: &str, to: &str) -> bool {
    from == "https" && to != "https"
}

pub fn http_client() -> reqwest::Client {
    let redirects = reqwest::redirect::Policy::custom(|attempt| {
        let from = attempt.previous().last().map_or("", |u| u.scheme());
        if downgrades(from, attempt.url().scheme()) {
            attempt.error("a redirect from https to plain http was refused")
        } else if attempt.previous().len() > 10 {
            attempt.error("too many redirects")
        } else {
            attempt.follow()
        }
    });
    reqwest::Client::builder()
        .redirect(redirects)
        .user_agent(concat!("spawnpoint-portal/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .expect("static client configuration is valid")
}

// Never follows redirects: a bearer token or pairing secret must not reach any host but the panel's.
pub fn panel_client() -> reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .user_agent(concat!("spawnpoint-portal/", env!("CARGO_PKG_VERSION")))
                .connect_timeout(std::time::Duration::from_secs(15))
                .read_timeout(std::time::Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("static client configuration is valid")
        })
        .clone()
}

#[cfg(test)]
mod redirect_tests {
    use super::downgrades;

    #[test]
    fn a_redirect_from_https_to_http_is_refused() {
        assert!(downgrades("https", "http"));
        assert!(!downgrades("https", "https"));
        assert!(!downgrades("http", "http"));
        assert!(!downgrades("http", "https"));
    }
}
