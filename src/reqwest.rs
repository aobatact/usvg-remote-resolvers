use crate::HrefStringResolver;

/// A resolver that uses reqwest to fetch images.
///
/// This resolve can be used inside [`tokio`] rutime,
/// but it will block the current thread when resolving images.
/// And it *panic* if it is used with current_thread runtime.
#[derive(Debug, Clone)]
pub struct ReqwestResolver {
    client: reqwest::Client,
}

impl ReqwestResolver {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl Default for ReqwestResolver {
    fn default() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl From<reqwest::Client> for ReqwestResolver {
    fn from(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl HrefStringResolver<'_> for ReqwestResolver {
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
