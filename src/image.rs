use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use ::image::{DynamicImage, RgbaImage};
use ratatui::layout::Size;
use ratatui_image::{Resize, picker::Picker, protocol::Protocol};
use resvg::{tiny_skia, usvg};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ImageSource {
    Local(PathBuf),
    Remote(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    source: ImageSource,
    width: u16,
    height: u16,
}

#[derive(Default)]
pub struct ImageManager {
    picker: Option<Picker>,
    decoded: HashMap<ImageSource, DynamicImage>,
    protocols: HashMap<CacheKey, Protocol>,
    failed_sources: HashSet<ImageSource>,
}

impl ImageManager {
    pub fn initialize(&mut self) {
        if self.picker.is_none() {
            self.picker = Some(Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks()));
        }
    }

    pub fn layout_size(
        &mut self,
        source: &str,
        base_dir: Option<&Path>,
        width: u16,
        max_height: u16,
    ) -> Option<Size> {
        let source = resolve_source(source, base_dir)?;
        self.ensure_decoded(&source)?;

        let image = self.decoded.get(&source)?;
        let picker = self.picker.as_ref()?;
        let available = Size::new(width.max(1), max_height.max(1));

        Some(Resize::Fit(None).size_for(image, picker.font_size(), available))
    }

    pub fn protocol(
        &mut self,
        source: &str,
        base_dir: Option<&Path>,
        width: u16,
        height: u16,
    ) -> Option<&Protocol> {
        let source = resolve_source(source, base_dir)?;
        if self.failed_sources.contains(&source) {
            return None;
        }

        let key = CacheKey {
            source: source.clone(),
            width: width.max(1),
            height: height.max(1),
        };

        if !self.protocols.contains_key(&key) {
            self.ensure_decoded(&source)?;

            let image = self.decoded.get(&source)?.clone();
            let picker = self.picker.as_ref()?;
            let protocol = picker
                .new_protocol(image, Size::new(key.width, key.height), Resize::Fit(None))
                .ok()?;

            self.protocols.insert(key.clone(), protocol);
        }

        self.protocols.get(&key)
    }

    fn ensure_decoded(&mut self, source: &ImageSource) -> Option<()> {
        if self.failed_sources.contains(source) {
            return None;
        }

        if !self.decoded.contains_key(source) {
            let Some(image) = load_image(source) else {
                self.failed_sources.insert(source.clone());
                return None;
            };

            self.decoded.insert(source.clone(), image);
        }

        Some(())
    }
}

fn load_image(source: &ImageSource) -> Option<DynamicImage> {
    match source {
        ImageSource::Local(path) => {
            let bytes = fs::read(path).ok()?;
            decode_image(&bytes, Some(path), looks_like_svg_path(path))
        }
        ImageSource::Remote(url) => {
            let mut response = ureq::get(url.as_str()).call().ok()?;
            let bytes = response.body_mut().read_to_vec().ok()?;
            decode_image(&bytes, None, looks_like_svg_url(url))
        }
    }
}

fn decode_image(bytes: &[u8], source_path: Option<&Path>, svg_hint: bool) -> Option<DynamicImage> {
    if svg_hint || looks_like_svg_bytes(bytes) {
        return rasterize_svg(bytes, source_path);
    }

    ::image::load_from_memory(bytes).ok()
}

fn rasterize_svg(bytes: &[u8], source_path: Option<&Path>) -> Option<DynamicImage> {
    let resources_dir = source_path.and_then(Path::parent).map(Path::to_path_buf);

    let mut options = usvg::Options {
        resources_dir,
        ..usvg::Options::default()
    };
    options.fontdb_mut().load_system_fonts();

    let tree = usvg::Tree::from_data(bytes, &options).ok()?;
    let size = tree.size().to_int_size();
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height())?;

    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

    let pixels = pixmap.take_demultiplied();
    let image = RgbaImage::from_raw(size.width(), size.height(), pixels)?;

    Some(DynamicImage::ImageRgba8(image))
}

fn looks_like_svg_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("svg"))
}

fn looks_like_svg_url(url: &str) -> bool {
    let path = url.split(['?', '#']).next().unwrap_or(url);

    path.rsplit_once('.')
        .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("svg"))
}

fn looks_like_svg_bytes(bytes: &[u8]) -> bool {
    let prefix = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    let trimmed = prefix.trim_start_matches('\u{feff}').trim_start();

    trimmed.starts_with("<svg")
        || trimmed.starts_with("<?xml") && trimmed.contains("<svg")
        || trimmed.starts_with("<!--") && trimmed.contains("<svg")
}

fn resolve_source(source: &str, base_dir: Option<&Path>) -> Option<ImageSource> {
    if source.starts_with("http://") || source.starts_with("https://") {
        return Some(ImageSource::Remote(source.to_string()));
    }

    if source.starts_with("data:") {
        return None;
    }

    let source = source.strip_prefix("file://").unwrap_or(source);
    let path = Path::new(source);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else if let Some(base_dir) = base_dir {
        base_dir.join(path)
    } else {
        path.to_path_buf()
    };

    Some(ImageSource::Local(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_images_from_document_directory() {
        let base = Path::new("/tmp/iris-doc");
        assert_eq!(
            resolve_source("images/example.png", Some(base)),
            Some(ImageSource::Local(base.join("images/example.png")))
        );
    }

    #[test]
    fn keeps_remote_images_remote() {
        assert_eq!(
            resolve_source("https://example.com/image.png", None),
            Some(ImageSource::Remote(
                "https://example.com/image.png".to_string()
            ))
        );
    }

    #[test]
    fn ignores_data_urls_for_now() {
        assert!(resolve_source("data:image/png;base64,AAAA", None).is_none());
    }

    #[test]
    fn decodes_supported_local_image() {
        let source = ImageSource::Local(PathBuf::from("tests/fixtures/assets/iris.png"));
        let image = load_image(&source).expect("fixture image should decode");
        assert_eq!(image.width(), 1);
        assert_eq!(image.height(), 1);
    }

    #[test]
    fn detects_svg_content_without_svg_extension() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"></svg>"#;
        assert!(looks_like_svg_bytes(svg));
    }

    #[test]
    fn rasterizes_svg_content() {
        let svg = br##"
            <svg xmlns="http://www.w3.org/2000/svg" width="20" height="10">
                <rect width="20" height="10" fill="#ff7500" />
            </svg>
        "##;

        let image = decode_image(svg, None, true).expect("SVG should rasterize");
        assert_eq!(image.width(), 20);
        assert_eq!(image.height(), 10);
    }
}
