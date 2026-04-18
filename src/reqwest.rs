use crate::HrefStringResolver;

/// A resolver that uses reqwest to fetch images.
///
/// This resolver can be used inside a [`tokio`] runtime,
/// but it will block the current thread when resolving images.
/// And it *panics* if it is used with a current_thread runtime.
#[derive(Debug, Default, Clone)]
pub struct ReqwestResolver {
    client: reqwest::Client,
}

impl ReqwestResolver {
    /// Create a new `ReqwestResolver` with the given [`Client`](`reqwest::Client`).
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    /// Get the underlying [`Client`](`reqwest::Client`) of this resolver.
    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }
}

impl From<reqwest::Client> for ReqwestResolver {
    fn from(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl HrefStringResolver<'_> for ReqwestResolver {
    fn is_target(&self, href: &str) -> bool {
        crate::utils::is_remote_url(href)
    }
    fn get_image_kind(&self, href: &str, options: &usvg::Options) -> Option<usvg::ImageKind> {
        let client = self.client.clone();
        let href = href.to_string();
        // Check if we're already in a tokio runtime
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            crate::utils::log_warn!(
                "no tokio runtime found; cannot resolve '{}'",
                href
            );
            return None;
        };
        // We're in an async context, use block_in_place
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
    use usvg::Options;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn reqwest_resolver() {
        let resolver = ReqwestResolver::default();
        let mut options = Options::default();
        options.image_href_resolver.resolve_string = resolver.into_fn();

        let mut s = mockito::Server::new_async().await;
        s.mock("GET", "/gray.png")
            .with_status(200)
            .with_header("content-type", "image/png")
            .with_body(include_bytes!("../test_data/gray.png"))
            .create();

        let tree = usvg::Tree::from_str(
            &format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg">
                <image href="{}/gray.png" />
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
        assert_eq!(
            pixmap.pixel(199, 0).unwrap(),
            resvg::tiny_skia::PremultipliedColorU8::from_rgba(255, 127, 0, 255).unwrap()
        );
        assert_eq!(
            pixmap.pixel(0, 199).unwrap(),
            resvg::tiny_skia::PremultipliedColorU8::from_rgba(255, 0, 127, 255).unwrap()
        );
        assert_eq!(
            pixmap.pixel(199, 199).unwrap(),
            resvg::tiny_skia::PremultipliedColorU8::from_rgba(0, 127, 255, 255).unwrap()
        );
    }

    #[tokio::test]
    #[should_panic]
    async fn reqwest_resolve_current() {
        let resolver = ReqwestResolver::default();
        let mut options = Options::default();
        options.image_href_resolver.resolve_string = resolver.into_fn();

        let mut s = mockito::Server::new_async().await;
        s.mock("GET", "/gray.png")
            .with_status(200)
            .with_header("content-type", "image/png")
            .with_body(include_bytes!("../test_data/gray.png"))
            .create();

        let _tree = usvg::Tree::from_str(
            &format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg">
                <image href="{}/gray.png" />
            </svg>"#,
                s.url()
            ),
            &options,
        );
    }
}
