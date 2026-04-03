use std::sync::Arc;

/// Check if the `href` is a remote URL (http or https).
pub fn is_remote_url(href: &str) -> bool {
    href.starts_with("https://") || href.starts_with("http://")
}

/// Represents the image format types supported by usvg.
pub enum ImageKindTypes {
    /// JPEG image format.
    Jpeg,
    /// PNG image format.
    Png,
    /// GIF image format.
    Gif,
    /// WebP image format.
    Webp,
    /// SVG image format (parsed into a [`usvg::Tree`]).
    Svg,
}

impl ImageKindTypes {
    /// Detect the image type from the HTTP `Content-Type` header or the file extension in the `href`.
    ///
    /// The `content_type` is checked first. If it is `None` or not recognized,
    /// the file extension of the `href` is used as a fallback.
    pub fn get_image_type(content_type: Option<&str>, href: &str) -> Option<Self> {
        let kind = match content_type.unwrap_or_default() {
            "image/png" => Self::Png,
            "image/jpeg" => Self::Jpeg,
            "image/webp" => Self::Webp,
            "image/gif" => Self::Gif,
            "image/svg+xml" => Self::Svg,
            _ => match href.rsplit_once('.')?.1 {
                "png" => Self::Png,
                "jpg" | "jpeg" => Self::Jpeg,
                "webp" => Self::Webp,
                "gif" => Self::Gif,
                "svg" => Self::Svg,
                _ => return None,
            },
        };
        Some(kind)
    }

    /// Convert image data into a [`usvg::ImageKind`] based on this image type.
    ///
    /// For SVG images, the data is parsed into a [`usvg::Tree`] using the given `options`.
    /// Returns `None` if the SVG parsing fails.
    pub fn into_image_kind(
        self,
        vec: Arc<Vec<u8>>,
        options: &usvg::Options,
    ) -> Option<usvg::ImageKind> {
        let ik = match self {
            Self::Jpeg => usvg::ImageKind::JPEG(vec),
            Self::Png => usvg::ImageKind::PNG(vec),
            Self::Gif => usvg::ImageKind::GIF(vec),
            Self::Webp => usvg::ImageKind::WEBP(vec),
            Self::Svg => {
                let tree = usvg::Tree::from_data(&vec, options).ok()?;
                usvg::ImageKind::SVG(tree)
            }
        };
        Some(ik)
    }
}
