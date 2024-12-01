use super::HrefStringResolver;
use crate::utils::ImageKindTypes;

/// Blocking Reqwest resolver.
///
/// This resolver uses `reqwest::blocking::Client` to fetch images.
/// Cannot be used inside async context.
/// If you want to use it inside async context, use [`ReqwestResolver`](`crate::reqwest::ReqwestResolver`) instead.
#[derive(Debug, Default, Clone)]
pub struct BlockingReqwestResolver {
    client: reqwest::blocking::Client,
}

impl From<reqwest::blocking::Client> for BlockingReqwestResolver {
    fn from(client: reqwest::blocking::Client) -> Self {
        Self { client }
    }
}

impl HrefStringResolver<'_> for BlockingReqwestResolver {
    fn is_target(&self, href: &str) -> bool {
        href.starts_with("https://") || href.starts_with("http://")
    }
    fn get_image_kind(&self, href: &str, options: &usvg::Options) -> Option<usvg::ImageKind> {
        let resp = self.client.get(href).send().ok()?;
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok());
        let image_type = ImageKindTypes::get_image_type(content_type, href)?;
        let body = resp.bytes().ok()?.to_vec();
        image_type.to_image_kind(body.into(), options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use usvg::Options;

    #[test]
    fn reqwest_resolver() {
        let resolver = BlockingReqwestResolver::default();
        let mut options = Options::default();
        options.image_href_resolver.resolve_string = resolver.into_fn();

        let mut s = mockito::Server::new();
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
}
