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
