use std::{
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    fs,
    hash::{Hash, Hasher},
    io::Cursor,
    path::{Path, PathBuf},
};

use ::image::{DynamicImage, ImageFormat, RgbaImage, imageops::FilterType};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ratatui::{
    buffer::Buffer,
    layout::{Rect, Size},
    style::Color,
    widgets::Widget,
};
use ratatui_image::{
    Resize,
    picker::{Picker, ProtocolType},
    protocol::Protocol,
};
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CellCacheKey {
    source: ImageSource,
    width: u16,
    height: u16,
}

#[derive(Debug, Clone, Copy)]
struct CellPixel {
    upper: Color,
    lower: Color,
    symbol: char,
}

#[derive(Debug)]
struct CellCanvas {
    size: Size,
    cells: Vec<CellPixel>,
}

pub struct CellViewport<'a> {
    canvas: &'a CellCanvas,
    offset_x: u16,
    offset_y: u16,
}

impl Widget for CellViewport<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let width = area
            .width
            .min(self.canvas.size.width.saturating_sub(self.offset_x));
        let height = area
            .height
            .min(self.canvas.size.height.saturating_sub(self.offset_y));

        for y in 0..height {
            for x in 0..width {
                let source_x = self.offset_x + x;
                let source_y = self.offset_y + y;
                let index = usize::from(source_y) * usize::from(self.canvas.size.width)
                    + usize::from(source_x);
                let pixel = self.canvas.cells[index];

                if let Some(cell) = buf.cell_mut((area.x + x, area.y + y)) {
                    cell.set_fg(pixel.upper)
                        .set_bg(pixel.lower)
                        .set_char(pixel.symbol);
                }
            }
        }
    }
}

#[derive(Debug)]
struct KittyFastImage {
    id: u32,
    pixel_width: u32,
    pixel_height: u32,
    transmit: Option<String>,
}

#[derive(Default)]
pub struct ImageManager {
    picker: Option<Picker>,
    fallback_picker: Option<Picker>,
    decoded: HashMap<ImageSource, DynamicImage>,
    protocols: HashMap<CacheKey, Protocol>,
    cell_canvases: HashMap<CellCacheKey, CellCanvas>,
    kitty_fast_images: HashMap<ImageSource, KittyFastImage>,
    failed_sources: HashSet<ImageSource>,
}

impl ImageManager {
    pub fn initialize(&mut self) {
        if self.picker.is_none() {
            self.picker = Some(Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks()));
        }
        if self.fallback_picker.is_none() {
            self.fallback_picker = Some(Picker::halfblocks());
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

    pub fn layout_size_zoomed(
        &mut self,
        source: &str,
        base_dir: Option<&Path>,
        width: u16,
        max_height: u16,
        zoom_percent: u16,
    ) -> Option<Size> {
        let fitted = self.layout_size(source, base_dir, width, max_height)?;
        let zoom_percent = u32::from(zoom_percent.max(1));

        Some(Size::new(
            scale_cells(fitted.width, zoom_percent),
            scale_cells(fitted.height, zoom_percent),
        ))
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
            let target = Size::new(key.width, key.height);

            let preferred = self.picker.as_ref().and_then(|picker| {
                picker
                    .new_protocol(image.clone(), target, Resize::Fit(None))
                    .ok()
            });

            let protocol = if let Some(protocol) = preferred {
                protocol
            } else {
                self.fallback_picker
                    .as_ref()?
                    .new_protocol(image, target, Resize::Fit(None))
                    .ok()?
            };

            self.protocols.insert(key.clone(), protocol);
        }

        self.protocols.get(&key)
    }

    pub fn kitty_viewport_sequence(
        &mut self,
        source: &str,
        base_dir: Option<&Path>,
        canvas: Size,
        offset_x: u16,
        offset_y: u16,
        visible: Size,
    ) -> Option<String> {
        let fast_kitty = self.picker.as_ref().is_some_and(|picker| {
            picker.protocol_type() == ProtocolType::Kitty && !picker.tmux_detected()
        });
        if !fast_kitty {
            return None;
        }

        let source = resolve_source(source, base_dir)?;
        if self.failed_sources.contains(&source) {
            return None;
        }

        self.ensure_decoded(&source)?;

        if !self.kitty_fast_images.contains_key(&source) {
            let image = self.decoded.get(&source)?;
            let fast_image = build_kitty_fast_image(&source, image)?;
            self.kitty_fast_images.insert(source.clone(), fast_image);
        }

        let fast_image = self.kitty_fast_images.get_mut(&source)?;
        let (x, y, width, height) = source_rect_for_viewport(
            fast_image.pixel_width,
            fast_image.pixel_height,
            canvas,
            offset_x,
            offset_y,
            visible,
        );

        let mut sequence = fast_image.transmit.take().unwrap_or_default();
        sequence.push_str(&format!(
            "\x1b_Ga=p,i={},p=1,q=2,x={x},y={y},w={width},h={height},c={},r={},C=1,z=-1\x1b\\",
            fast_image.id, visible.width, visible.height
        ));

        Some(sequence)
    }

    pub fn cell_viewport(
        &mut self,
        source: &str,
        base_dir: Option<&Path>,
        canvas: Size,
        offset_x: u16,
        offset_y: u16,
    ) -> Option<CellViewport<'_>> {
        let source = resolve_source(source, base_dir)?;
        if self.failed_sources.contains(&source) {
            return None;
        }

        let canvas = Size::new(canvas.width.max(1), canvas.height.max(1));
        let key = CellCacheKey {
            source: source.clone(),
            width: canvas.width,
            height: canvas.height,
        };

        if !self.cell_canvases.contains_key(&key) {
            self.ensure_decoded(&source)?;

            if self.cell_canvases.len() >= 8 {
                self.cell_canvases.clear();
            }

            let image = self.decoded.get(&source)?;
            let cell_canvas = build_cell_canvas(image, canvas);
            self.cell_canvases.insert(key.clone(), cell_canvas);
        }

        let cell_canvas = self.cell_canvases.get(&key)?;
        Some(CellViewport {
            canvas: cell_canvas,
            offset_x: offset_x.min(canvas.width.saturating_sub(1)),
            offset_y: offset_y.min(canvas.height.saturating_sub(1)),
        })
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

fn build_kitty_fast_image(source: &ImageSource, image: &DynamicImage) -> Option<KittyFastImage> {
    let mut png = Cursor::new(Vec::new());
    image.write_to(&mut png, ImageFormat::Png).ok()?;

    let encoded = STANDARD.encode(png.into_inner());
    let id = kitty_image_id(source);

    Some(KittyFastImage {
        id,
        pixel_width: image.width().max(1),
        pixel_height: image.height().max(1),
        transmit: Some(kitty_transmit_png(id, &encoded)),
    })
}

fn kitty_image_id(source: &ImageSource) -> u32 {
    let mut hasher = DefaultHasher::new();
    std::process::id().hash(&mut hasher);
    source.hash(&mut hasher);

    let id = hasher.finish() as u32;
    if id == 0 { 1 } else { id }
}

fn kitty_transmit_png(id: u32, encoded: &str) -> String {
    const CHUNK_SIZE: usize = 4096;

    let chunks = encoded.as_bytes().chunks(CHUNK_SIZE);
    let chunk_count = encoded.len().div_ceil(CHUNK_SIZE);
    let mut sequence = String::with_capacity(encoded.len() + chunk_count.saturating_mul(64));

    for (index, chunk) in chunks.enumerate() {
        let more = u8::from(index + 1 < chunk_count);

        if index == 0 {
            sequence.push_str(&format!("\x1b_Ga=t,f=100,t=d,i={id},q=2,m={more};"));
        } else {
            sequence.push_str(&format!("\x1b_Gq=2,m={more};"));
        }

        sequence.push_str(std::str::from_utf8(chunk).expect("base64 output is valid ASCII"));
        sequence.push_str("\x1b\\");
    }

    sequence
}

fn source_rect_for_viewport(
    pixel_width: u32,
    pixel_height: u32,
    canvas: Size,
    offset_x: u16,
    offset_y: u16,
    visible: Size,
) -> (u32, u32, u32, u32) {
    let canvas_width = u64::from(canvas.width.max(1));
    let canvas_height = u64::from(canvas.height.max(1));
    let pixel_width = u64::from(pixel_width.max(1));
    let pixel_height = u64::from(pixel_height.max(1));

    let x0 = u64::from(offset_x).saturating_mul(pixel_width) / canvas_width;
    let y0 = u64::from(offset_y).saturating_mul(pixel_height) / canvas_height;

    let x1_cells = u64::from(offset_x.saturating_add(visible.width));
    let y1_cells = u64::from(offset_y.saturating_add(visible.height));

    let x1 = x1_cells
        .saturating_mul(pixel_width)
        .div_ceil(canvas_width)
        .min(pixel_width);
    let y1 = y1_cells
        .saturating_mul(pixel_height)
        .div_ceil(canvas_height)
        .min(pixel_height);

    let x0 = x0.min(pixel_width.saturating_sub(1));
    let y0 = y0.min(pixel_height.saturating_sub(1));
    let width = x1.saturating_sub(x0).max(1);
    let height = y1.saturating_sub(y0).max(1);

    (x0 as u32, y0 as u32, width as u32, height as u32)
}

fn scale_cells(value: u16, zoom_percent: u32) -> u16 {
    let scaled = (u32::from(value)
        .saturating_mul(zoom_percent)
        .saturating_add(50))
        / 100;
    scaled.clamp(1, u32::from(u16::MAX)) as u16
}

fn build_cell_canvas(image: &DynamicImage, size: Size) -> CellCanvas {
    let pixel_height = u32::from(size.height).saturating_mul(2).max(1);
    let resized = image
        .resize_exact(
            u32::from(size.width).max(1),
            pixel_height,
            FilterType::Triangle,
        )
        .to_rgb8();

    let mut cells = vec![
        CellPixel {
            upper: Color::Rgb(0, 0, 0),
            lower: Color::Rgb(0, 0, 0),
            symbol: '▀',
        };
        usize::from(size.width) * usize::from(size.height)
    ];

    for (y, row) in resized.rows().enumerate() {
        for (x, pixel) in row.enumerate() {
            let index = x + usize::from(size.width) * (y / 2);
            let color = Color::Rgb(pixel[0], pixel[1], pixel[2]);

            if y % 2 == 0 {
                cells[index].upper = color;
            } else {
                cells[index].lower = color;
            }
        }
    }

    for cell in &mut cells {
        cell.optimize_symbol();
    }

    CellCanvas { size, cells }
}

impl CellPixel {
    fn optimize_symbol(&mut self) {
        if self.upper == self.lower {
            self.symbol = ' ';
            return;
        }

        if luminance(self.lower) > luminance(self.upper) {
            std::mem::swap(&mut self.upper, &mut self.lower);
            self.symbol = '▄';
        }
    }
}

fn luminance(color: Color) -> u32 {
    let (r, g, b) = match color {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (0, 0, 0),
    };

    2126 * u32::from(r) + 7152 * u32::from(g) + 722 * u32::from(b)
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

    #[test]
    fn builds_cached_cell_canvas_at_terminal_cell_resolution() {
        let image = DynamicImage::ImageRgb8(::image::RgbImage::from_pixel(
            8,
            8,
            ::image::Rgb([255, 117, 0]),
        ));
        let canvas = build_cell_canvas(&image, Size::new(4, 3));

        assert_eq!(canvas.size, Size::new(4, 3));
        assert_eq!(canvas.cells.len(), 12);
    }
}
