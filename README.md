# usvg-remote-resolvers

`usvg-remote-resolvers` is a Rust library that provides remote resource resolvers for the `usvg` library. It allows you to fetch and resolve external resources such as images and fonts referenced in SVG files.

## Features

- Fetch remote images and fonts
- Integrate seamlessly with `usvg`
- Customizable resolvers

## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
usvg-remote-resolvers = "0.1"
```

## Usage

```rust
use usvg_remote_resolvers::BlockingReqwestResolver;
let resolver = BlockingReqwestResolver::default();
let mut options = usvg::Options::default();
options.image_href_resolver.resolve_string = resolver.into_fn();

let tree = usvg::Tree::from_str(
    r#"<svg xmlns="http://www.w3.org/2000/svg">
        <image href="https://example.com/sample.png" />
    </svg>"#,
    &options,
)
.unwrap();
```

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

## Contributing

Contributions are welcome! Please open an issue or submit a pull request.

## Contact

For any questions or suggestions, please open an issue on GitHub.
