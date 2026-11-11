//! Fetching an image over HTTP(S), buffered whole in memory.
//!
//! TODO: investigate streaming the body to a temp file or an XDG cache directory
//! instead of buffering it, which would bound memory on huge responses and give
//! a disk cache for re-loads.

use std::io::Cursor;
use std::path::Path;
use std::sync::OnceLock;

use image::ImageReader;

use super::decode::decode;
use super::{Decoded, LoadError, LoadOptions};

/// One shared agent, so connections and TLS state survive between loads. The
/// options never change after startup, so the first load's timeout is the one.
fn agent(opts: &LoadOptions) -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .timeout_global(Some(opts.download_timeout))
            // NOTE: redirects are followed with ureq's defaults, so an https URL
            // may redirect to plain http. Intentionally not blocked.
            .build()
            .into()
    })
}

fn fetch_error(url: &str, source: ureq::Error) -> LoadError {
    LoadError::Http {
        path: url.to_string(),
        message: source.to_string(),
    }
}

pub fn load(path: &Path, opts: &LoadOptions) -> Result<Decoded, LoadError> {
    let url = path.to_string_lossy().into_owned();
    let body = agent(opts)
        .get(url.as_str())
        .call()
        .map_err(|source| fetch_error(&url, source))?
        .body_mut()
        .with_config()
        .limit(opts.max_download_bytes)
        .read_to_vec()
        .map_err(|source| fetch_error(&url, source))?;

    // The format is guessed from the bytes, so a wrong Content-Type or missing
    // extension costs nothing.
    decode(ImageReader::new(Cursor::new(body)), path, opts)
}
