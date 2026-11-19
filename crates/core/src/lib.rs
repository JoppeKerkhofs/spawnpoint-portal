pub mod download;
pub mod gamefiles;
pub mod instance;
pub mod java;
pub mod launch;
pub mod loader;
pub mod pack;
pub mod panel;
pub mod sync;
pub mod error;
pub mod paths;
pub mod play;
pub mod secrets;
pub mod version;

pub use error::{Error, Result};
pub use paths::Paths;

pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("spawnpoint-portal/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(15))
        .build()
        .expect("static client configuration is valid")
}
