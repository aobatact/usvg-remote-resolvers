use super::ReqwestWithMiddlewareResolver;
use http_cache_reqwest::CacheManager;

impl ReqwestWithMiddlewareResolver {
    /// Create a new `ReqwestResolver` with the given [`Client`](`reqwest::Client`) and [`Cache`](`moka::Cache`).
    pub fn with_http_cache<T: CacheManager>(
        client: reqwest::Client,
        cache: http_cache_reqwest::HttpCache<T>,
    ) -> Self {
        let client = reqwest_middleware::ClientBuilder::new(client)
            .with(http_cache_reqwest::Cache(cache))
            .build();
        Self { client }
    }
}

#[cfg(feature = "reqwest_cacache")]
mod cacache {
    use crate::reqwest_middleware::ReqwestWithMiddlewareResolver;
    use std::path::PathBuf;

    impl ReqwestWithMiddlewareResolver {
        /// Create a new `ReqwestResolver` with the given [`Client`](`reqwest::Client`) and [`CACacheManager`](`http_cache_reqwest::CACacheManager`).
        pub fn cacahe(path: PathBuf) -> Self {
            let client = reqwest_middleware::ClientBuilder::new(reqwest::Client::new())
                .with(http_cache_reqwest::Cache(http_cache_reqwest::HttpCache {
                    mode: http_cache_reqwest::CacheMode::Default,
                    manager: http_cache_reqwest::CACacheManager { path },
                    options: http_cache_reqwest::HttpCacheOptions::default(),
                }))
                .build();
            Self { client }
        }
    }
}

#[cfg(feature = "reqwest_moka_cache")]
mod moka_cache {
    use crate::reqwest_middleware::ReqwestWithMiddlewareResolver;
    use http_cache_reqwest::MokaCache;
    use std::sync::Arc;

    impl ReqwestWithMiddlewareResolver {
        /// Create a new `ReqwestResolver` with the given [`Client`](`reqwest::Client`) and [`MokaCache`](`http_cache_reqwest::MokaCache`).
        pub fn moka_cache(cache: impl Into<Arc<MokaCache<String, Arc<Vec<u8>>>>>) -> Self {
            let client = reqwest_middleware::ClientBuilder::new(reqwest::Client::new())
                .with(http_cache_reqwest::Cache(http_cache_reqwest::HttpCache {
                    mode: http_cache_reqwest::CacheMode::Default,
                    manager: http_cache_reqwest::MokaManager {
                        cache: cache.into(),
                    },
                    options: http_cache_reqwest::HttpCacheOptions::default(),
                }))
                .build();
            Self { client }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::HrefStringResolver;

    use super::*;
    use usvg::Options;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cacache() {
        let resolver = ReqwestWithMiddlewareResolver::cacahe("./cacache".into());
        let mut options = Options::default();
        resolver.set_into_options(&mut options);

        let mut s = mockito::Server::new_async().await;
        s.mock("GET", "/gray.png")
            .with_status(200)
            .with_header("content-type", "image/png")
            .with_body(include_bytes!("../../test_data/gray.png"))
            .create();

        let _tree = usvg::Tree::from_str(
            &format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
                <image href="{}/gray.png" width="100" height="100"/>
            </svg>"#,
                s.url()
            ),
            &options,
        )
        .unwrap();

        s.reset();

        let tree = usvg::Tree::from_str(
            &format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
                <image href="{}/gray.png" width="100" height="100"/>
            </svg>"#,
                s.url()
            ),
            &options,
        )
        .unwrap();

        let mut pixmap = resvg::tiny_skia::Pixmap::new(200, 200).unwrap();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );
        assert_eq!(
            pixmap.pixel(0, 0).unwrap(),
            resvg::tiny_skia::PremultipliedColorU8::from_rgba(127, 127, 127, 255).unwrap()
        );
    }
}
