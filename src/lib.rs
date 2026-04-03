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
use std::path::PathBuf;

use usvg::{ImageHrefStringResolverFn, ImageKind, Options};

#[cfg(feature = "reqwest")]
pub mod reqwest;
#[cfg(feature = "reqwest_blocking")]
pub mod reqwest_blocking;
#[cfg(feature = "reqwest_http_cache")]
pub mod reqwest_http_cache;
#[cfg(feature = "s3")]
pub mod s3;
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

/// Resolver for `file://` URLs.
///
/// Strips the `file://` scheme and delegates to [`default_string_resolver`](`usvg::ImageHrefResolver::default_string_resolver`)
/// for actual file loading and format detection.
///
/// Optionally restricts access to specific directories via [`allowed_dirs`](`FileResolver::allowed_dirs`).
/// When set, only paths that fall under one of the allowed directories (after canonicalization) will be resolved.
/// If no allowed directories are set, all `file://` paths are accepted.
#[derive(Debug, Default, Clone)]
pub struct FileResolver {
    /// List of directories that are allowed to be accessed.
    /// If empty, all paths are allowed.
    pub allowed_dirs: Vec<PathBuf>,
}

impl FileResolver {
    /// Create a new `FileResolver` with no path restrictions.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new `FileResolver` that only allows access to the given directories.
    pub fn with_allowed_dirs(allowed_dirs: Vec<PathBuf>) -> Self {
        Self { allowed_dirs }
    }

    fn strip_file_scheme(href: &str) -> Option<&str> {
        href.strip_prefix("file://")
    }

    fn is_path_allowed(&self, path: &str) -> bool {
        if self.allowed_dirs.is_empty() {
            return true;
        }
        let Ok(canonical) = std::path::Path::new(path).canonicalize() else {
            return false;
        };
        self.allowed_dirs.iter().any(|dir| {
            dir.canonicalize()
                .is_ok_and(|allowed| canonical.starts_with(&allowed))
        })
    }
}

impl<'a> HrefStringResolver<'a> for FileResolver {
    fn is_target(&self, href: &str) -> bool {
        Self::strip_file_scheme(href).is_some()
    }
    fn get_image_kind(&self, href: &str, options: &Options) -> Option<ImageKind> {
        let path = Self::strip_file_scheme(href)?;
        if !self.is_path_allowed(path) {
            return None;
        }
        usvg::ImageHrefResolver::default_string_resolver()(path, options)
    }
    fn into_fn(self) -> ImageHrefStringResolverFn<'a> {
        let default = usvg::ImageHrefResolver::default_string_resolver();
        Box::new(move |href, options| {
            let path = Self::strip_file_scheme(href)?;
            if !self.is_path_allowed(path) {
                return None;
            }
            default(path, options)
        })
    }
}

/// A resolver that tries the `primary` resolver first, and falls back to the `fallback` resolver
/// if the primary does not handle the `href` or fails to resolve it.
///
/// This can be created using [`HrefStringResolver::with_fallback`] or [`From`] tuple conversions.
///
/// ```
/// use usvg_remote_resolvers::{DefaultResolver, FallbackResolver};
///
/// // Using `with_fallback`
/// use usvg_remote_resolvers::HrefStringResolver;
/// let resolver = DefaultResolver.with_fallback(DefaultResolver);
///
/// // Using `From` tuple conversion
/// let resolver: FallbackResolver<_, _> = (DefaultResolver, DefaultResolver).into();
/// ```
#[derive(Debug, Default, Clone, Copy)]
pub struct FallbackResolver<T, U> {
    /// The resolver that is tried first.
    pub primary: T,
    /// The resolver that is used if the primary resolver does not handle the `href` or fails.
    pub fallback: U,
}

impl<T, U> FallbackResolver<T, U> {
    /// Create a new `FallbackResolver` with the given `primary` and `fallback` resolvers.
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
        let resolver = DefaultResolver;
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

    #[test]
    fn file_resolver() {
        let abs_path = std::path::Path::new("./test_data/gray.png")
            .canonicalize()
            .unwrap();
        let file_url = format!("file://{}", abs_path.display());

        let resolver = FileResolver::new().with_fallback(DefaultResolver);
        let mut options = Options::default();
        resolver.set_into_options(&mut options);

        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg">
                <image href="{file_url}" />
            </svg>"#,
        );
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
    }

    #[test]
    fn file_resolver_is_target() {
        let resolver = FileResolver::new();
        assert!(resolver.is_target("file:///path/to/image.png"));
        assert!(!resolver.is_target("./relative/path.png"));
        assert!(!resolver.is_target("https://example.com/image.png"));
    }

    #[test]
    fn file_resolver_allowed_dirs() {
        let test_data_dir = std::path::Path::new("./test_data")
            .canonicalize()
            .unwrap();
        let abs_path = std::path::Path::new("./test_data/gray.png")
            .canonicalize()
            .unwrap();
        let file_url = format!("file://{}", abs_path.display());

        // Allowed dir includes test_data -> should resolve
        let resolver =
            FileResolver::with_allowed_dirs(vec![test_data_dir.clone()]).with_fallback(DefaultResolver);
        let mut options = Options::default();
        resolver.set_into_options(&mut options);

        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg">
                <image href="{file_url}" />
            </svg>"#,
        );
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
    }

    #[test]
    fn file_resolver_blocked_by_allowed_dirs() {
        let abs_path = std::path::Path::new("./test_data/gray.png")
            .canonicalize()
            .unwrap();
        let file_url = format!("file://{}", abs_path.display());

        // Allowed dir is /tmp -> test_data path should be blocked
        let resolver =
            FileResolver::with_allowed_dirs(vec![PathBuf::from("/tmp")]).with_fallback(DefaultResolver);
        let mut options = Options::default();
        resolver.set_into_options(&mut options);

        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg">
                <image href="{file_url}" />
            </svg>"#,
        );
        let tree = usvg::Tree::from_str(&svg, &options).unwrap();
        // Image should not be loaded, so pixel stays at default (transparent)
        let mut pixmap = resvg::tiny_skia::Pixmap::new(200, 200).unwrap();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );
        assert_eq!(
            pixmap.pixel(0, 0).unwrap(),
            resvg::tiny_skia::PremultipliedColorU8::from_rgba(0, 0, 0, 0).unwrap()
        );
    }
}
