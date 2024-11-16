use std::sync::Arc;

pub enum ImageKindTypes {
    Jpeg,
    Png,
    Gif,
    Webp,
    Svg,
}

impl ImageKindTypes {
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

    pub fn to_image_kind(
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
