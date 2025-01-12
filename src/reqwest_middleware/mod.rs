#[cfg(any(feature = "reqwest_moka_cache", feature = "reqwest_cacache"))]
mod http_cache;

#[cfg(any(feature = "reqwest_moka_cache", feature = "reqwest_cacache"))]
pub use http_cache_reqwest::*;

/// A resolver that uses [`reqwest_middleware``] to fetch images.
///
/// This resolve can be used inside [`tokio`] rutime,
/// but it will block the current thread when resolving images.
/// And it *panic* if it is used with current_thread runtime.
pub struct ReqwestWithMiddlewareResolver {
    client: reqwest_middleware::ClientWithMiddleware,
}

impl ReqwestWithMiddlewareResolver {
    /// Create a new `ReqwestResolver` with the given [`ClientWithMiddleware`](`reqwest_middleware::ClientWithMiddleware`).
    pub fn new(client: reqwest_middleware::ClientWithMiddleware) -> Self {
        Self { client }
    }

    /// Get the underlying [`ClientWithMiddleware`](`reqwest_middleware::ClientWithMiddleware`) of this resolver.
    pub fn client(&self) -> &reqwest_middleware::ClientWithMiddleware {
        &self.client
    }
}

impl crate::HrefStringResolver<'_> for ReqwestWithMiddlewareResolver {
    fn is_target(&self, href: &str) -> bool {
        href.starts_with("https://") || href.starts_with("http://")
    }
    fn get_image_kind(&self, href: &str, options: &usvg::Options) -> Option<usvg::ImageKind> {
        let (sender, receiver) = tokio::sync::oneshot::channel();

        let client = self.client.clone();
        let href = href.to_string();
        tokio::spawn(async move {
            let resp = client.get(&href).send().await.ok()?;
            let content_type = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok());
            let image_type = crate::utils::ImageKindTypes::get_image_type(content_type, &href)?;
            let body = resp.bytes().await.ok()?.to_vec();
            sender.send((image_type, body)).ok();
            Some(())
        });
        tokio::task::block_in_place(|| {
            let (img_type, body) = receiver.blocking_recv().ok()?;
            return img_type.to_image_kind(body.into(), options);
        })
    }
}
