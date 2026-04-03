use crate::HrefStringResolver;

/// A resolver that fetches images from S3 using `aws-sdk-s3`.
///
/// This resolver handles `s3://bucket/key` URLs.
/// It can be used inside a [`tokio`] runtime,
/// but it will block the current thread when resolving images.
/// And it *panics* if it is used with a current_thread runtime.
#[derive(Debug, Clone)]
pub struct S3Resolver {
    client: aws_sdk_s3::Client,
}

impl S3Resolver {
    /// Create a new `S3Resolver` with the given [`Client`](`aws_sdk_s3::Client`).
    pub fn new(client: aws_sdk_s3::Client) -> Self {
        Self { client }
    }

    /// Get the underlying [`Client`](`aws_sdk_s3::Client`) of this resolver.
    pub fn client(&self) -> &aws_sdk_s3::Client {
        &self.client
    }

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
}

impl From<aws_sdk_s3::Client> for S3Resolver {
    fn from(client: aws_sdk_s3::Client) -> Self {
        Self { client }
    }
}

impl HrefStringResolver<'_> for S3Resolver {
    fn is_target(&self, href: &str) -> bool {
        Self::is_s3_url(href)
    }
    fn get_image_kind(&self, href: &str, options: &usvg::Options) -> Option<usvg::ImageKind> {
        let (bucket, key) = Self::parse_s3_url(href)?;
        let client = self.client.clone();
        let bucket = bucket.to_string();
        let key = key.to_string();

        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return None;
        };

        let (image_type, body) = tokio::task::block_in_place(|| {
            handle.block_on(async {
                let resp = client
                    .get_object()
                    .bucket(&bucket)
                    .key(&key)
                    .send()
                    .await
                    .ok()?;
                let content_type = resp.content_type().map(|s| s.to_string());
                let image_type = crate::utils::ImageKindTypes::get_image_type(
                    content_type.as_deref(),
                    &key,
                )?;
                let body = resp.body.collect().await.ok()?.to_vec();
                Some((image_type, body))
            })
        })?;

        image_type.into_image_kind(body.into(), options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_s3_url() {
        assert_eq!(
            S3Resolver::parse_s3_url("s3://my-bucket/path/to/image.png"),
            Some(("my-bucket", "path/to/image.png"))
        );
        assert_eq!(
            S3Resolver::parse_s3_url("s3://bucket/key"),
            Some(("bucket", "key"))
        );
        assert_eq!(S3Resolver::parse_s3_url("s3://bucket/"), None);
        assert_eq!(S3Resolver::parse_s3_url("s3:///key"), None);
        assert_eq!(S3Resolver::parse_s3_url("https://example.com"), None);
    }

    #[test]
    fn is_s3_url() {
        assert!(S3Resolver::is_s3_url("s3://bucket/key"));
        assert!(!S3Resolver::is_s3_url("https://example.com"));
        assert!(!S3Resolver::is_s3_url("file:///path"));
    }
}
