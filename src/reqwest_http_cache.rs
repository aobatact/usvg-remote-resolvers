use crate::HrefStringResolver;

/// A resolver that uses reqwest with HTTP cache middleware to fetch images.
///
/// This resolver wraps a [`reqwest_middleware::ClientWithMiddleware`] configured
/// with [`http_cache_reqwest`] to cache HTTP responses according to standard
/// HTTP caching semantics.
///
/// Like [`ReqwestResolver`](`crate::reqwest::ReqwestResolver`), this resolver
/// can be used inside a [`tokio`] runtime but will block the current thread
/// when resolving images. It *panics* if used with a current_thread runtime.
#[derive(Debug, Clone)]
pub struct HttpCacheReqwestResolver {
    client: reqwest_middleware::ClientWithMiddleware,
}

impl HttpCacheReqwestResolver {
    /// Create a new `HttpCacheReqwestResolver` with the given middleware client.
    pub fn new(client: reqwest_middleware::ClientWithMiddleware) -> Self {
        Self { client }
    }

    /// Create a new `HttpCacheReqwestResolver` from cache configuration.
    ///
    /// This builds a default [`reqwest::Client`] with the given cache manager, mode, and options.
    /// Use [`new`](`Self::new`) if you need to customize the reqwest client or add other middleware.
    pub fn from_cache_options(
        manager: impl http_cache_reqwest::CacheManager + 'static,
        mode: http_cache_reqwest::CacheMode,
        options: http_cache_reqwest::HttpCacheOptions,
    ) -> Self {
        Self::from_cache_options_with_client(reqwest::Client::new(), manager, mode, options)
    }

    /// Create a new `HttpCacheReqwestResolver` from a custom [`reqwest::Client`] and cache configuration.
    ///
    /// Use this if you need to customize the reqwest client (e.g., set timeouts, headers, TLS settings).
    pub fn from_cache_options_with_client(
        client: reqwest::Client,
        manager: impl http_cache_reqwest::CacheManager + 'static,
        mode: http_cache_reqwest::CacheMode,
        options: http_cache_reqwest::HttpCacheOptions,
    ) -> Self {
        let client = reqwest_middleware::ClientBuilder::new(client)
            .with(http_cache_reqwest::Cache(http_cache_reqwest::HttpCache {
                mode,
                manager,
                options,
            }))
            .build();
        Self { client }
    }

    /// Get the underlying [`ClientWithMiddleware`](`reqwest_middleware::ClientWithMiddleware`) of this resolver.
    pub fn client(&self) -> &reqwest_middleware::ClientWithMiddleware {
        &self.client
    }
}

impl From<reqwest_middleware::ClientWithMiddleware> for HttpCacheReqwestResolver {
    fn from(client: reqwest_middleware::ClientWithMiddleware) -> Self {
        Self { client }
    }
}

impl HrefStringResolver<'_> for HttpCacheReqwestResolver {
    fn is_target(&self, href: &str) -> bool {
        crate::utils::is_remote_url(href)
    }
    fn get_image_kind(&self, href: &str, options: &usvg::Options) -> Option<usvg::ImageKind> {
        let client = self.client.clone();
        let href = href.to_string();
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            crate::utils::log_warn!(
                "no tokio runtime found; cannot resolve '{}'",
                href
            );
            return None;
        };
        let (image_type, body) = tokio::task::block_in_place(|| {
            handle.block_on(async {
                let resp = match client.get(&href).send().await {
                    Ok(resp) => resp,
                    Err(e) => {
                        crate::utils::log_warn!("failed to fetch '{}': {}", href, e);
                        return None;
                    }
                };
                let content_type = resp
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok());
                let image_type = match crate::utils::ImageKindTypes::get_image_type(content_type, &href) {
                    Some(t) => t,
                    None => {
                        crate::utils::log_warn!(
                            "unsupported image type for '{}' (content-type: {:?})",
                            href,
                            content_type
                        );
                        return None;
                    }
                };
                let body = match resp.bytes().await {
                    Ok(b) => b.to_vec(),
                    Err(e) => {
                        crate::utils::log_warn!("failed to read response body for '{}': {}", href, e);
                        return None;
                    }
                };
                Some((image_type, body))
            })
        })?;

        image_type.into_image_kind(body.into(), options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_cache_reqwest::{Cache, CacheMode, HttpCache, HttpCacheOptions};
    use usvg::Options;

    fn build_cached_client(
        cache: impl http_cache_reqwest::CacheManager + 'static,
    ) -> reqwest_middleware::ClientWithMiddleware {
        reqwest_middleware::ClientBuilder::new(reqwest::Client::new())
            .with(Cache(HttpCache {
                mode: CacheMode::Default,
                manager: cache,
                options: HttpCacheOptions::default(),
            }))
            .build()
    }

    #[cfg(feature = "reqwest_http_cache_manager_moka")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn http_cache_moka_resolver() {
        let client = build_cached_client(http_cache_reqwest::MokaManager::default());
        let resolver = HttpCacheReqwestResolver::new(client);
        let mut options = Options::default();
        options.image_href_resolver.resolve_string = resolver.into_fn();

        let mut s = mockito::Server::new_async().await;
        let mock = s
            .mock("GET", "/gray.png")
            .with_status(200)
            .with_header("content-type", "image/png")
            .with_header("cache-control", "max-age=3600")
            .with_body(include_bytes!("../test_data/gray.png"))
            .expect_at_most(1)
            .create();

        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg">
                <image href="{}/gray.png" />
            </svg>"#,
            s.url()
        );

        // First request - should hit the server
        let tree = usvg::Tree::from_str(&svg, &options).unwrap();
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

        // Second request - should be served from cache (mock expects at most 1 hit)
        let tree = usvg::Tree::from_str(&svg, &options).unwrap();
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

        mock.assert();
    }
}
