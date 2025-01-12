//! Provides a way to resolve the `href` attribute of the `<image>` tag in the SVG for [`usvg`](`usvg`).
//!
//! # Example
//!
//! ```rust
//! use usvg::Options;
//! use usvg_remote_resolvers::{HrefStringResolver, reqwest_blocking::BlockingReqwestResolver};
//!
//! let resolver = BlockingReqwestResolver::default();
//! let mut options = Options::default();
//! resolver.set_into_options(&mut options);
//!
//! let tree = usvg::Tree::from_str(
//!     r#"<svg xmlns="http://www.w3.org/2000/svg">
//!         <image href="https://example.com/sample.png" />
//!     </svg>"#,
//!     &options,
//! ).unwrap();
//!
//! let mut pixmap = resvg::tiny_skia::Pixmap::new(200, 200).unwrap();
//! resvg::render(
//!     &tree,
//!     resvg::tiny_skia::Transform::identity(),
//!     &mut pixmap.as_mut(),
//! );
//! ```
//!
//! # Feature Flags
//!
//! - `reqwest`: Enable the `reqwest` resolver.
//! - `reqwest_blocking`: Enable the `reqwest_blocking` resolver.
//!
use usvg::{ImageHrefStringResolverFn, ImageKind, Options};

#[cfg(feature = "reqwest")]
pub mod reqwest;
#[cfg(feature = "reqwest_blocking")]
pub mod reqwest_blocking;
#[cfg(feature = "reqwest_middleware")]
pub mod reqwest_middleware;

mod utils;

/// HrefStringResolver is a trait that is used to resolve the `href` attribute of the `<image>` tag.
/// It will be converted to [`ImageHrefResolver`](`usvg::ImageHrefResolver`) to be set in the [`Options`](`usvg::Options`).
pub trait HrefStringResolver<'a>: Send + Sync {
    /// Check if the `href` is the target of this resolver.
    /// If it return false, it will resolve to `None`.
    fn is_target(&self, href: &str) -> bool;
    /// This is where the logic for resolving the `href` is implemented.
    fn get_image_kind(&self, href: &str, options: &Options) -> Option<ImageKind>;
    /// Convert this resolver to put into [`ImageHrefResolver`](`usvg::ImageHrefResolver`).
    fn into_fn(self) -> ImageHrefStringResolverFn<'a>
    where
        Self: Sized + 'a,
    {
        Box::new(move |href, options| {
            if self.is_target(href) {
                self.get_image_kind(href, options)
            } else {
                None
            }
        })
    }
    /// Set this resolver into the [`Options`](`usvg::Options`).
    ///
    /// ```
    /// use usvg::Options;
    /// use usvg_remote_resolvers::{HrefStringResolver, reqwest_blocking::BlockingReqwestResolver};
    ///
    /// let resolver = BlockingReqwestResolver::default();
    /// let mut options = Options::default();
    /// resolver.set_into_options(&mut options);
    /// ```
    fn set_into_options(self, options: &mut Options<'a>)
    where
        Self: Sized + 'a,
    {
        options.image_href_resolver.resolve_string = self.into_fn();
    }
    /// Add a fallback to this resolver in case if the url is not the target of this resolver, or if
    /// it fails to resolve.
    fn with_fallback<T>(self, fallback: T) -> FallbackResolver<Self, T>
    where
        Self: Sized,
        T: HrefStringResolver<'a>,
    {
        FallbackResolver::new(self, fallback)
    }
}

/// Resolver using [`default_string_resolver`](`usvg::ImageHrefResolver::default_string_resolver`)
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultResolver;

impl<'a> HrefStringResolver<'a> for DefaultResolver {
    fn is_target(&self, _: &str) -> bool {
        true
    }
    fn get_image_kind(&self, href: &str, options: &Options) -> Option<ImageKind> {
        usvg::ImageHrefResolver::default_string_resolver()(href, options)
    }
    fn into_fn(self) -> ImageHrefStringResolverFn<'a> {
        usvg::ImageHrefResolver::default_string_resolver()
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FallbackResolver<T, U> {
    pub primary: T,
    pub fallback: U,
}

impl<T, U> FallbackResolver<T, U> {
    pub fn new(primary: T, fallback: U) -> Self {
        Self { primary, fallback }
    }
}

impl<'a, T, U> HrefStringResolver<'a> for FallbackResolver<T, U>
where
    T: HrefStringResolver<'a>,
    U: HrefStringResolver<'a>,
{
    fn is_target(&self, href: &str) -> bool {
        self.primary.is_target(href) || self.fallback.is_target(href)
    }
    fn get_image_kind(&self, href: &str, options: &Options) -> Option<ImageKind> {
        self.primary
            .is_target(href)
            .then(|| self.primary.get_image_kind(href, options))
            .flatten()
            .or_else(|| {
                self.fallback
                    .is_target(href)
                    .then(|| self.fallback.get_image_kind(href, options))
                    .flatten()
            })
    }
}

impl<T, U> From<(T, U)> for FallbackResolver<T, U> {
    fn from((primary, fallback): (T, U)) -> Self {
        Self { primary, fallback }
    }
}
impl<T, U, V> From<(T, U, V)> for FallbackResolver<T, FallbackResolver<U, V>> {
    fn from((primary, secondary, tertiary): (T, U, V)) -> Self {
        Self {
            primary,
            fallback: (secondary, tertiary).into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_resolver() {
        let resolver = DefaultResolver::default();
        let mut options = Options::default();
        resolver.set_into_options(&mut options);

        let tree = usvg::Tree::from_str(
            r#"<svg xmlns="http://www.w3.org/2000/svg">
                <image href="./test_data/gray.png" />
            </svg>"#,
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
