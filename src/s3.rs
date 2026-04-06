use std::sync::Arc;

use crate::HrefStringResolver;

/// Check if the `href` is an S3 URL (`s3://`).
pub fn is_s3_url(href: &str) -> bool {
    href.starts_with("s3://")
}

/// Parse an `s3://bucket/key` URL into `(bucket, key)`.
fn parse_s3_url(href: &str) -> Option<(&str, &str)> {
    let rest = href.strip_prefix("s3://")?;
    let (bucket, key) = rest.split_once('/')?;
    if bucket.is_empty() || key.is_empty() {
        return None;
    }
    Some((bucket, key))
}

/// Cached entry stored by [`S3CacheStore`] implementations.
#[derive(Debug, Clone)]
pub struct S3CacheEntry {
    /// The ETag returned by S3 for this object.
    pub etag: String,
    /// The content type of the object (e.g. `"image/png"`).
    pub content_type: Option<String>,
    /// The raw body bytes of the object.
    pub body: Arc<Vec<u8>>,
}

/// Trait for storing and retrieving cached S3 objects.
///
/// Implement this trait to provide a custom cache backend (in-memory, disk, Redis, etc.).
pub trait S3CacheStore: Send + Sync {
    /// Look up a cached entry by its S3 URL (`s3://bucket/key`).
    fn get(&self, href: &str) -> Option<S3CacheEntry>;
    /// Store a cache entry for the given S3 URL.
    fn put(&self, href: &str, entry: S3CacheEntry);
}

/// A no-op cache store that never caches anything.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopS3CacheStore;

/// An in-memory cache store backed by [`moka::sync::Cache`].
#[cfg(feature = "s3_cache_moka")]
#[derive(Debug, Clone)]
pub struct MokaS3CacheStore {
    cache: moka::sync::Cache<String, S3CacheEntry>,
}

#[cfg(feature = "s3_cache_moka")]
impl MokaS3CacheStore {
    /// Create a new `MokaS3CacheStore` with the given max capacity.
    pub fn new(max_capacity: u64) -> Self {
        Self {
            cache: moka::sync::Cache::new(max_capacity),
        }
    }

    /// Create a new `MokaS3CacheStore` from an existing [`moka::sync::Cache`].
    pub fn from_cache(cache: moka::sync::Cache<String, S3CacheEntry>) -> Self {
        Self { cache }
    }
}

#[cfg(feature = "s3_cache_moka")]
impl From<moka::sync::Cache<String, S3CacheEntry>> for MokaS3CacheStore {
    fn from(cache: moka::sync::Cache<String, S3CacheEntry>) -> Self {
        Self { cache }
    }
}

#[cfg(feature = "s3_cache_moka")]
impl S3CacheStore for MokaS3CacheStore {
    fn get(&self, href: &str) -> Option<S3CacheEntry> {
        self.cache.get(href)
    }
    fn put(&self, href: &str, entry: S3CacheEntry) {
        self.cache.insert(href.to_string(), entry);
    }
}

impl S3CacheStore for NoopS3CacheStore {
    fn get(&self, _href: &str) -> Option<S3CacheEntry> {
        None
    }
    fn put(&self, _href: &str, _entry: S3CacheEntry) {}
}

/// A resolver that fetches images from S3 without caching.
///
/// This is an alias for [`CachedS3Resolver`] with [`NoopS3CacheStore`].
pub type S3Resolver = CachedS3Resolver<NoopS3CacheStore>;

/// A resolver that fetches images from S3 with ETag-based caching.
///
/// On the first request for a given `s3://` URL, the object is fetched and stored in the
/// provided [`S3CacheStore`]. On subsequent requests, a conditional GET is issued using
/// `If-None-Match` with the cached ETag. If S3 returns 304 Not Modified, the cached
/// body is reused without re-downloading.
///
/// When used with [`NoopS3CacheStore`] (via the [`S3Resolver`] type alias), no caching
/// is performed and every request fetches the object from S3.
///
/// This resolver handles `s3://bucket/key` URLs.
/// It can be used inside a [`tokio`] runtime,
/// but it will block the current thread when resolving images.
/// And it *panics* if it is used with a current_thread runtime.
#[derive(Debug, Clone)]
pub struct CachedS3Resolver<C> {
    client: aws_sdk_s3::Client,
    cache: C,
}

impl S3Resolver {
    /// Create a new `S3Resolver` with no caching.
    pub fn new(client: aws_sdk_s3::Client) -> Self {
        CachedS3Resolver {
            client,
            cache: NoopS3CacheStore,
        }
    }

    /// Add caching to this resolver using the given cache store.
    pub fn with_cache<C: S3CacheStore>(self, cache: C) -> CachedS3Resolver<C> {
        CachedS3Resolver { client: self.client, cache }
    }
}

impl<C: S3CacheStore> CachedS3Resolver<C> {
    /// Create a new `CachedS3Resolver` with the given S3 client and cache store.
    pub fn from_cache(client: aws_sdk_s3::Client, cache: C) -> Self {
        Self { client, cache }
    }

    /// Get the underlying [`Client`](`aws_sdk_s3::Client`) of this resolver.
    pub fn client(&self) -> &aws_sdk_s3::Client {
        &self.client
    }

    /// Get a reference to the cache store.
    pub fn cache(&self) -> &C {
        &self.cache
    }
}

impl From<aws_sdk_s3::Client> for S3Resolver {
    fn from(client: aws_sdk_s3::Client) -> Self {
        Self::new(client)
    }
}

impl<C: S3CacheStore> HrefStringResolver<'_> for CachedS3Resolver<C> {
    fn is_target(&self, href: &str) -> bool {
        is_s3_url(href)
    }
    fn get_image_kind(&self, href: &str, options: &usvg::Options) -> Option<usvg::ImageKind> {
        let (bucket, key) = parse_s3_url(href)?;
        let client = self.client.clone();
        let bucket = bucket.to_string();
        let key = key.to_string();
        let cached = self.cache.get(href);

        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return None;
        };

        let (image_type, body) = tokio::task::block_in_place(|| {
            handle.block_on(async {
                let mut req = client.get_object().bucket(&bucket).key(&key);
                if let Some(ref cached) = cached {
                    req = req.if_none_match(&cached.etag);
                }

                match req.send().await {
                    Ok(resp) => {
                        let etag = resp.e_tag().unwrap_or_default().to_string();
                        let content_type = resp.content_type().map(|s| s.to_string());
                        let image_type = crate::utils::ImageKindTypes::get_image_type(
                            content_type.as_deref(),
                            &key,
                        )?;
                        let body: Arc<Vec<u8>> =
                            Arc::new(resp.body.collect().await.ok()?.to_vec());

                        if !etag.is_empty() {
                            self.cache.put(
                                href,
                                S3CacheEntry {
                                    etag,
                                    content_type,
                                    body: Arc::clone(&body),
                                },
                            );
                        }

                        Some((image_type, body))
                    }
                    Err(err) => {
                        // 304 Not Modified — use cached entry
                        let raw = err.raw_response()?;
                        if raw.status().as_u16() == 304 {
                            let cached = cached?;
                            let image_type = crate::utils::ImageKindTypes::get_image_type(
                                cached.content_type.as_deref(),
                                &key,
                            )?;
                            Some((image_type, cached.body))
                        } else {
                            None
                        }
                    }
                }
            })
        })?;

        image_type.into_image_kind(body, options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_s3_url() {
        assert_eq!(
            parse_s3_url("s3://my-bucket/path/to/image.png"),
            Some(("my-bucket", "path/to/image.png"))
        );
        assert_eq!(parse_s3_url("s3://bucket/key"), Some(("bucket", "key")));
        assert_eq!(parse_s3_url("s3://bucket/"), None);
        assert_eq!(parse_s3_url("s3:///key"), None);
        assert_eq!(parse_s3_url("https://example.com"), None);
    }

    #[test]
    fn test_is_s3_url() {
        assert!(is_s3_url("s3://bucket/key"));
        assert!(!is_s3_url("https://example.com"));
        assert!(!is_s3_url("file:///path"));
    }
}
