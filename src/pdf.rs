use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::Path,
    sync::Arc,
};

use anyhow::{Context, Result, anyhow, bail};
use hayro::{
    RenderCache, RenderSettings,
    hayro_interpret::{
        BlendMode, ClipPath, Context as InterpretContext, Device, GlyphDrawMode, Image,
        InterpreterCache, InterpreterSettings, Paint, PathDrawMode, SoftMask, TransformExt,
        font::Glyph, hayro_cmap::BfString, interpret_page,
    },
    hayro_syntax::Pdf,
    vello_cpu::{
        color::palette::css::WHITE,
        kurbo::{Affine, BezPath, Point, Rect as KurboRect, Shape},
    },
};
use image::{DynamicImage, RgbaImage};
use ratatui::layout::Size;

use crate::search::{SearchMatch, SearchMatcher, SearchRect};

const COLUMN_GAP: u16 = 2;
const ROW_GAP: u16 = 2;
const MIN_PAIR_SCALE_RATIO: f32 = 0.68;
const MAX_RASTER_CACHE_ENTRIES: usize = 8;
const ZOOM_STEP: u16 = 25;
const MIN_ZOOM: u16 = 25;
const MAX_ZOOM: u16 = 400;
const FIT_ZOOM: u16 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PdfLayoutMode {
    #[default]
    Auto,
    One,
    Two,
}

impl PdfLayoutMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::One => "1",
            Self::Two => "2",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PdfPageInfo {
    pub index: usize,
    pub width_points: f32,
    pub height_points: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfPagePlacement {
    pub page_index: usize,
    pub row: usize,
    pub row_top: usize,
    pub x: usize,
    pub y: usize,
    pub width: u16,
    pub height: u16,
}

impl PdfPagePlacement {
    pub fn bottom(self) -> usize {
        self.y.saturating_add(usize::from(self.height))
    }

    pub fn intersects(self, top: usize, bottom: usize) -> bool {
        self.y < bottom && self.bottom() > top
    }
}

#[derive(Debug, Clone, Default)]
pub struct PdfLayout {
    pub placements: Vec<PdfPagePlacement>,
    pub total_width: usize,
    pub total_height: usize,
    pub viewport: Size,
    pub font_pixels: Size,
    pub mode: PdfLayoutMode,
    pub zoom_percent: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct RasterKey {
    page_index: usize,
    width_cells: u16,
    height_cells: u16,
    font_width: u16,
    font_height: u16,
}

#[derive(Debug, Clone)]
struct PdfTextRun {
    text: String,
    rect: SearchRect,
}

#[derive(Debug, Clone, Default)]
struct PdfTextPage {
    runs: Vec<PdfTextRun>,
    exact: String,
    exact_owner: Vec<usize>,
    folded: String,
    folded_owner: Vec<usize>,
}

impl PdfTextPage {
    fn new(runs: Vec<PdfTextRun>) -> Self {
        let mut page = Self {
            runs,
            ..Self::default()
        };

        for (run_index, run) in page.runs.iter().enumerate() {
            for ch in run.text.chars().filter(|ch| !ch.is_whitespace()) {
                let before = page.exact.len();
                page.exact.push(ch);
                page.exact_owner
                    .extend(std::iter::repeat_n(run_index, page.exact.len() - before));

                for folded in ch.to_lowercase() {
                    let before = page.folded.len();
                    page.folded.push(folded);
                    page.folded_owner
                        .extend(std::iter::repeat_n(run_index, page.folded.len() - before));
                }
            }
        }

        page
    }

    fn matches(&self, matcher: &SearchMatcher, page_index: usize) -> Vec<SearchMatch> {
        let key = normalized_search_key(matcher.query(), matcher.case_sensitive());
        if key.is_empty() {
            return Vec::new();
        }

        let (haystack, owner) = if matcher.case_sensitive() {
            (&self.exact, &self.exact_owner)
        } else {
            (&self.folded, &self.folded_owner)
        };
        if haystack.is_empty() || key.len() > haystack.len() {
            return Vec::new();
        }

        let mut output = Vec::new();
        let mut from = 0usize;
        while from <= haystack.len() {
            let Some(relative) = haystack[from..].find(&key) else {
                break;
            };
            let start = from + relative;
            let end = start + key.len();
            if end > owner.len() {
                break;
            }

            let mut run_ids = owner[start..end].to_vec();
            run_ids.dedup();
            let rects = self.group_lines(&run_ids);
            if !rects.is_empty() {
                output.push(SearchMatch::pdf(page_index, rects));
            }
            from = end;
        }

        output
    }

    fn group_lines(&self, run_ids: &[usize]) -> Vec<SearchRect> {
        let mut rects = run_ids
            .iter()
            .filter_map(|index| self.runs.get(*index).map(|run| run.rect))
            .collect::<Vec<_>>();
        rects.sort_by(|a, b| a.y.total_cmp(&b.y).then_with(|| a.x.total_cmp(&b.x)));

        let mut lines = Vec::<SearchRect>::new();
        for rect in rects {
            if let Some(last) = lines.last_mut() {
                let line_height = last.height.max(rect.height).max(0.001);
                if (rect.center_y() - last.center_y()).abs() <= line_height * 0.65 {
                    *last = last.union(rect);
                    continue;
                }
            }
            lines.push(rect);
        }

        for rect in &mut lines {
            let x_pad = (rect.height * 0.08).clamp(0.001, 0.006);
            let y_pad = (rect.height * 0.12).clamp(0.001, 0.008);
            let right = (rect.right() + x_pad).min(1.0);
            let bottom = (rect.bottom() + y_pad).min(1.0);
            rect.x = (rect.x - x_pad).max(0.0);
            rect.y = (rect.y - y_pad).max(0.0);
            rect.width = (right - rect.x).max(0.0);
            rect.height = (bottom - rect.y).max(0.0);
        }

        lines
    }
}

#[derive(Default)]
struct PdfTextDevice {
    runs: Vec<PdfTextRun>,
    page_width: f32,
    page_height: f32,
}

impl<'a> Device<'a> for PdfTextDevice {
    fn set_soft_mask(&mut self, _: Option<SoftMask<'a>>) {}
    fn set_blend_mode(&mut self, _: BlendMode) {}
    fn draw_path(&mut self, _: &BezPath, _: Affine, _: &Paint<'a>, _: &PathDrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}

    fn draw_glyph(
        &mut self,
        glyph: &Glyph<'a>,
        transform: Affine,
        glyph_transform: Affine,
        _: &Paint<'a>,
        _: &GlyphDrawMode,
    ) {
        let text = match glyph.as_unicode() {
            Some(BfString::Char(ch)) => ch.to_string(),
            Some(BfString::String(text)) => text,
            None => return,
        };
        if text.is_empty() || text.chars().all(char::is_whitespace) {
            return;
        }
        if self.page_width <= 0.0 || self.page_height <= 0.0 {
            return;
        }

        let matrix = transform * glyph_transform;
        let bbox = match glyph {
            Glyph::Outline(outline) => (matrix * outline.outline()).bounding_box(),
            Glyph::Type3(_) => {
                let point = matrix * Point::ZERO;
                KurboRect::new(point.x, point.y - 1.0, point.x + 1.0, point.y + 1.0)
            }
        };
        if !bbox.x0.is_finite()
            || !bbox.y0.is_finite()
            || !bbox.x1.is_finite()
            || !bbox.y1.is_finite()
        {
            return;
        }

        let x0 = (bbox.x0.min(bbox.x1) as f32 / self.page_width).clamp(0.0, 1.0);
        let y0 = (bbox.y0.min(bbox.y1) as f32 / self.page_height).clamp(0.0, 1.0);
        let x1 = (bbox.x0.max(bbox.x1) as f32 / self.page_width).clamp(0.0, 1.0);
        let y1 = (bbox.y0.max(bbox.y1) as f32 / self.page_height).clamp(0.0, 1.0);
        let min_width = (1.0 / self.page_width).min(0.01);
        let min_height = (1.0 / self.page_height).min(0.01);

        self.runs.push(PdfTextRun {
            text,
            rect: SearchRect::new(x0, y0, (x1 - x0).max(min_width), (y1 - y0).max(min_height)),
        });
    }

    fn draw_image(&mut self, _: Image<'a, '_>, _: Affine) {}
    fn pop_clip_path(&mut self) {}
    fn pop_transparency_group(&mut self) {}
}

#[derive(Debug, Clone, Copy)]
pub struct PdfSearchHighlight {
    pub rect: SearchRect,
    pub current: bool,
}

pub struct PdfDocument {
    pdf: Pdf,
    pages: Vec<PdfPageInfo>,
    layout: PdfLayout,
    layout_mode: PdfLayoutMode,
    zoom_percent: u16,
    raster_cache: HashMap<RasterKey, Arc<DynamicImage>>,
    raster_order: VecDeque<RasterKey>,
    text_pages: Option<Vec<PdfTextPage>>,
}

impl PdfDocument {
    pub fn open(path: &Path) -> Result<Self> {
        let data =
            fs::read(path).with_context(|| format!("failed to read PDF '{}'", path.display()))?;
        let pdf = Pdf::new(data)
            .map_err(|error| anyhow!("failed to parse PDF '{}': {error:?}", path.display()))?;

        let pages = pdf
            .pages()
            .iter()
            .enumerate()
            .map(|(index, page)| {
                // render_dimensions() already applies the page's PDF rotation, so mixed
                // portrait/landscape documents use their effective displayed geometry.
                let (width_points, height_points) = page.render_dimensions();
                PdfPageInfo {
                    index,
                    width_points: width_points.max(1.0),
                    height_points: height_points.max(1.0),
                }
            })
            .collect::<Vec<_>>();

        if pages.is_empty() {
            bail!("PDF contains no pages: {}", path.display());
        }

        Ok(Self {
            pdf,
            pages,
            layout: PdfLayout::default(),
            layout_mode: PdfLayoutMode::Auto,
            zoom_percent: FIT_ZOOM,
            raster_cache: HashMap::new(),
            raster_order: VecDeque::new(),
            text_pages: None,
        })
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn layout(&self) -> &PdfLayout {
        &self.layout
    }

    pub fn layout_mode(&self) -> PdfLayoutMode {
        self.layout_mode
    }

    pub fn set_layout_mode(&mut self, mode: PdfLayoutMode) -> bool {
        if self.layout_mode == mode {
            return false;
        }

        self.layout_mode = mode;
        true
    }

    pub fn zoom_percent(&self) -> u16 {
        self.zoom_percent
    }

    pub fn zoom_in(&mut self) -> bool {
        self.set_zoom(self.zoom_percent.saturating_add(ZOOM_STEP).min(MAX_ZOOM))
    }

    pub fn zoom_out(&mut self) -> bool {
        self.set_zoom(self.zoom_percent.saturating_sub(ZOOM_STEP).max(MIN_ZOOM))
    }

    pub fn reset_zoom(&mut self) -> bool {
        self.set_zoom(FIT_ZOOM)
    }

    fn set_zoom(&mut self, zoom_percent: u16) -> bool {
        if self.zoom_percent == zoom_percent {
            return false;
        }

        self.zoom_percent = zoom_percent;
        true
    }

    pub fn page_at_scroll(&self, scroll: usize) -> usize {
        self.row_pages_at_scroll(scroll)
            .first()
            .copied()
            .unwrap_or(0)
    }

    pub fn page_top(&self, page_index: usize) -> Option<usize> {
        self.layout
            .placements
            .iter()
            .find(|placement| placement.page_index == page_index)
            .map(|placement| placement.y)
    }

    pub fn page_horizontal_scroll(&self, page_index: usize, viewport_width: u16) -> usize {
        let Some(placement) = self
            .layout
            .placements
            .iter()
            .find(|placement| placement.page_index == page_index)
        else {
            return 0;
        };

        placement
            .x
            .saturating_add(usize::from(placement.width) / 2)
            .saturating_sub(usize::from(viewport_width) / 2)
            .min(
                self.layout
                    .total_width
                    .saturating_sub(usize::from(viewport_width)),
            )
    }

    pub fn visible_pages(&self, scroll: usize, viewport_height: u16) -> Vec<usize> {
        let bottom = scroll.saturating_add(usize::from(viewport_height));
        let mut pages = self
            .layout
            .placements
            .iter()
            .filter(|placement| placement.intersects(scroll, bottom))
            .map(|placement| placement.page_index)
            .collect::<Vec<_>>();
        pages.sort_unstable();
        pages.dedup();
        pages
    }

    pub fn row_pages_at_scroll(&self, scroll: usize) -> Vec<usize> {
        let Some(current) = self
            .layout
            .placements
            .iter()
            .filter(|placement| placement.row_top <= scroll)
            .max_by_key(|placement| placement.row_top)
            .or_else(|| self.layout.placements.first())
        else {
            return Vec::new();
        };

        let mut pages = self
            .layout
            .placements
            .iter()
            .filter(|placement| placement.row == current.row)
            .map(|placement| placement.page_index)
            .collect::<Vec<_>>();
        pages.sort_unstable();
        pages
    }

    pub fn next_row_top(&self, scroll: usize) -> Option<usize> {
        let current_top = self.current_row_top(scroll)?;
        self.layout
            .placements
            .iter()
            .map(|placement| placement.row_top)
            .filter(|top| *top > current_top)
            .min()
    }

    pub fn previous_row_top(&self, scroll: usize) -> Option<usize> {
        let current_top = self.current_row_top(scroll)?;
        self.layout
            .placements
            .iter()
            .map(|placement| placement.row_top)
            .filter(|top| *top < current_top)
            .max()
    }

    fn current_row_top(&self, scroll: usize) -> Option<usize> {
        self.layout
            .placements
            .iter()
            .map(|placement| placement.row_top)
            .filter(|top| *top <= scroll)
            .max()
            .or_else(|| {
                self.layout
                    .placements
                    .first()
                    .map(|placement| placement.row_top)
            })
    }

    pub fn rebuild_layout(&mut self, viewport: Size, font_pixels: Size) -> bool {
        let viewport = Size::new(viewport.width.max(1), viewport.height.max(1));
        let font_pixels = Size::new(font_pixels.width.max(1), font_pixels.height.max(1));

        if self.layout.viewport == viewport
            && self.layout.font_pixels == font_pixels
            && self.layout.mode == self.layout_mode
            && self.layout.zoom_percent == self.zoom_percent
        {
            return false;
        }

        self.layout = build_layout(
            &self.pages,
            viewport,
            font_pixels,
            self.layout_mode,
            self.zoom_percent,
        );
        true
    }

    pub fn render_page(
        &mut self,
        page_index: usize,
        size: Size,
        font_pixels: Size,
    ) -> Option<Arc<DynamicImage>> {
        let size = Size::new(size.width.max(1), size.height.max(1));
        let font_pixels = Size::new(font_pixels.width.max(1), font_pixels.height.max(1));
        let key = RasterKey {
            page_index,
            width_cells: size.width,
            height_cells: size.height,
            font_width: font_pixels.width,
            font_height: font_pixels.height,
        };

        if let Some(image) = self.raster_cache.get(&key) {
            return Some(Arc::clone(image));
        }

        let page = self.pdf.pages().get(page_index)?;
        let (page_width, page_height) = page.render_dimensions();
        let pixel_width = u32::from(size.width)
            .saturating_mul(u32::from(font_pixels.width))
            .clamp(1, u32::from(u16::MAX));
        let pixel_height = u32::from(size.height)
            .saturating_mul(u32::from(font_pixels.height))
            .clamp(1, u32::from(u16::MAX));

        let scale = (pixel_width as f32 / page_width.max(1.0))
            .min(pixel_height as f32 / page_height.max(1.0))
            .max(0.01);

        let cache = RenderCache::new();
        let pixmap = hayro::render(
            page,
            &cache,
            &InterpreterSettings::default(),
            &RenderSettings {
                x_scale: scale,
                y_scale: scale,
                width: Some(pixel_width as u16),
                height: Some(pixel_height as u16),
                bg_color: WHITE,
            },
        );
        let rgba = pixmap
            .take()
            .into_iter()
            .flat_map(|pixel| [pixel.r, pixel.g, pixel.b, pixel.a])
            .collect::<Vec<_>>();
        let raster = RgbaImage::from_raw(pixel_width, pixel_height, rgba)?;
        let image = Arc::new(DynamicImage::ImageRgba8(raster));

        self.insert_raster(key, Arc::clone(&image));
        Some(image)
    }

    pub fn search(&mut self, matcher: &SearchMatcher) -> Vec<SearchMatch> {
        self.ensure_text_index();
        self.text_pages
            .as_ref()
            .map(|pages| {
                pages
                    .iter()
                    .enumerate()
                    .flat_map(|(page_index, page)| page.matches(matcher, page_index))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn search_scroll_position(
        &self,
        page_index: usize,
        rect: Option<SearchRect>,
        viewport: Size,
    ) -> Option<(usize, usize)> {
        let placement = self
            .layout
            .placements
            .iter()
            .find(|placement| placement.page_index == page_index)?;
        let viewport = Size::new(viewport.width.max(1), viewport.height.max(1));

        let (target_x, target_y) = if let Some(rect) = rect {
            (
                placement.x.saturating_add(
                    (rect.center_x().clamp(0.0, 1.0) * f32::from(placement.width)) as usize,
                ),
                placement.y.saturating_add(
                    (rect.center_y().clamp(0.0, 1.0) * f32::from(placement.height)) as usize,
                ),
            )
        } else {
            (
                placement.x.saturating_add(usize::from(placement.width) / 2),
                placement.y,
            )
        };

        let top = target_y
            .saturating_sub(usize::from(viewport.height) / 3)
            .min(
                self.layout
                    .total_height
                    .saturating_sub(usize::from(viewport.height)),
            );
        let left = target_x
            .saturating_sub(usize::from(viewport.width) / 2)
            .min(
                self.layout
                    .total_width
                    .saturating_sub(usize::from(viewport.width)),
            );
        Some((top, left))
    }

    fn ensure_text_index(&mut self) {
        if self.text_pages.is_some() {
            return;
        }

        let cache = InterpreterCache::new();
        let mut pages = Vec::with_capacity(self.pages.len());
        for page in self.pdf.pages().iter() {
            let (page_width, page_height) = page.render_dimensions();
            let mut context = InterpretContext::new(
                page.initial_transform(true).to_kurbo(),
                KurboRect::new(
                    0.0,
                    0.0,
                    f64::from(page_width.max(1.0)),
                    f64::from(page_height.max(1.0)),
                ),
                &cache,
                page.xref(),
                InterpreterSettings::default(),
            );
            let mut device = PdfTextDevice {
                runs: Vec::new(),
                page_width: page_width.max(1.0),
                page_height: page_height.max(1.0),
            };
            interpret_page(page, &mut context, &mut device);
            pages.push(PdfTextPage::new(device.runs));
        }
        self.text_pages = Some(pages);
    }

    fn insert_raster(&mut self, key: RasterKey, image: Arc<DynamicImage>) {
        if self.raster_cache.len() >= MAX_RASTER_CACHE_ENTRIES {
            while self.raster_cache.len() >= MAX_RASTER_CACHE_ENTRIES {
                let Some(oldest) = self.raster_order.pop_front() else {
                    break;
                };
                self.raster_cache.remove(&oldest);
            }
        }

        self.raster_order.push_back(key);
        self.raster_cache.insert(key, image);
    }
}

fn normalized_search_key(query: &str, case_sensitive: bool) -> String {
    let chars = query.chars().filter(|ch| !ch.is_whitespace());
    if case_sensitive {
        chars.collect()
    } else {
        chars.flat_map(char::to_lowercase).collect()
    }
}

pub fn apply_search_highlights(
    image: &DynamicImage,
    highlights: &[PdfSearchHighlight],
    normal_color: (u8, u8, u8),
    current_color: (u8, u8, u8),
) -> DynamicImage {
    if highlights.is_empty() {
        return image.clone();
    }

    let mut raster = image.to_rgba8();
    let width = raster.width().max(1);
    let height = raster.height().max(1);

    for highlight in highlights {
        let rect = highlight.rect;
        let x0 = (rect.x.clamp(0.0, 1.0) * width as f32).floor() as u32;
        let y0 = (rect.y.clamp(0.0, 1.0) * height as f32).floor() as u32;
        let x1 = (rect.right().clamp(0.0, 1.0) * width as f32)
            .ceil()
            .max((x0 + 1) as f32) as u32;
        let y1 = (rect.bottom().clamp(0.0, 1.0) * height as f32)
            .ceil()
            .max((y0 + 1) as f32) as u32;
        let x1 = x1.min(width);
        let y1 = y1.min(height);
        let color = if highlight.current {
            current_color
        } else {
            normal_color
        };
        let alpha = if highlight.current { 0.48 } else { 0.26 };

        for y in y0.min(height.saturating_sub(1))..y1 {
            for x in x0.min(width.saturating_sub(1))..x1 {
                let pixel = raster.get_pixel_mut(x, y);
                pixel.0[0] = blend_channel(pixel.0[0], color.0, alpha);
                pixel.0[1] = blend_channel(pixel.0[1], color.1, alpha);
                pixel.0[2] = blend_channel(pixel.0[2], color.2, alpha);
            }
        }
    }

    DynamicImage::ImageRgba8(raster)
}

fn blend_channel(base: u8, overlay: u8, alpha: f32) -> u8 {
    (f32::from(base) * (1.0 - alpha) + f32::from(overlay) * alpha)
        .round()
        .clamp(0.0, 255.0) as u8
}

fn build_layout(
    pages: &[PdfPageInfo],
    viewport: Size,
    font_pixels: Size,
    mode: PdfLayoutMode,
    zoom_percent: u16,
) -> PdfLayout {
    let mut placements = Vec::with_capacity(pages.len());
    let mut row_widths = Vec::<usize>::new();
    let mut y = 0usize;
    let mut index = 0usize;
    let mut row = 0usize;

    while index < pages.len() {
        let first = pages[index];
        let second = pages.get(index + 1).copied();

        let paired = match (mode, second) {
            (PdfLayoutMode::One, _) | (_, None) => None,
            (_, Some(second)) if is_landscape(first) || is_landscape(second) => None,
            (PdfLayoutMode::Auto, Some(second)) => {
                pair_sizes(first, second, viewport, font_pixels, true)
            }
            (PdfLayoutMode::Two, Some(second)) => {
                pair_sizes(first, second, viewport, font_pixels, false)
            }
        };

        if let (Some((first_base, second_base)), Some(second)) = (paired, second) {
            let first_size = zoom_size(first_base, zoom_percent);
            let second_size = zoom_size(second_base, zoom_percent);
            let row_height = first_size.height.max(second_size.height);
            let row_width = usize::from(first_size.width)
                .saturating_add(usize::from(COLUMN_GAP))
                .saturating_add(usize::from(second_size.width));

            placements.push(PdfPagePlacement {
                page_index: first.index,
                row,
                row_top: y,
                x: 0,
                y: y + usize::from(row_height.saturating_sub(first_size.height) / 2),
                width: first_size.width,
                height: first_size.height,
            });
            placements.push(PdfPagePlacement {
                page_index: second.index,
                row,
                row_top: y,
                x: usize::from(first_size.width).saturating_add(usize::from(COLUMN_GAP)),
                y: y + usize::from(row_height.saturating_sub(second_size.height) / 2),
                width: second_size.width,
                height: second_size.height,
            });

            row_widths.push(row_width);
            y = y
                .saturating_add(usize::from(row_height))
                .saturating_add(usize::from(ROW_GAP));
            index += 2;
            row += 1;
            continue;
        }

        let size = zoom_size(
            fit_page(first, viewport.width, viewport.height, font_pixels),
            zoom_percent,
        );
        placements.push(PdfPagePlacement {
            page_index: first.index,
            row,
            row_top: y,
            x: 0,
            y,
            width: size.width,
            height: size.height,
        });
        row_widths.push(usize::from(size.width));
        y = y
            .saturating_add(usize::from(size.height))
            .saturating_add(usize::from(ROW_GAP));
        index += 1;
        row += 1;
    }

    let total_width = row_widths
        .iter()
        .copied()
        .max()
        .unwrap_or(1)
        .max(usize::from(viewport.width));

    for placement in &mut placements {
        let row_width = row_widths.get(placement.row).copied().unwrap_or(1);
        placement.x = placement
            .x
            .saturating_add(total_width.saturating_sub(row_width) / 2);
    }

    let total_height = y.saturating_sub(usize::from(ROW_GAP)).max(1);
    PdfLayout {
        placements,
        total_width,
        total_height,
        viewport,
        font_pixels,
        mode,
        zoom_percent,
    }
}

fn is_landscape(page: PdfPageInfo) -> bool {
    page.width_points > page.height_points
}

fn pair_sizes(
    first: PdfPageInfo,
    second: PdfPageInfo,
    viewport: Size,
    font_pixels: Size,
    require_useful_scale: bool,
) -> Option<(Size, Size)> {
    let available_width = viewport.width.saturating_sub(COLUMN_GAP);
    if available_width < 2 {
        return None;
    }

    let first_weight = width_at_height(first, viewport.height, font_pixels);
    let second_weight = width_at_height(second, viewport.height, font_pixels);
    let total_weight = first_weight + second_weight;

    let mut first_width = if total_weight > 0.0 {
        (f32::from(available_width) * first_weight / total_weight).round() as u16
    } else {
        available_width / 2
    };
    first_width = first_width.clamp(1, available_width.saturating_sub(1));
    let second_width = available_width.saturating_sub(first_width).max(1);

    let first_single = fit_page(first, viewport.width, viewport.height, font_pixels);
    let second_single = fit_page(second, viewport.width, viewport.height, font_pixels);
    let first_pair = fit_page(first, first_width, viewport.height, font_pixels);
    let second_pair = fit_page(second, second_width, viewport.height, font_pixels);

    if require_useful_scale {
        let first_ratio = scale_ratio(first_pair, first_single);
        let second_ratio = scale_ratio(second_pair, second_single);

        if first_ratio < MIN_PAIR_SCALE_RATIO || second_ratio < MIN_PAIR_SCALE_RATIO {
            return None;
        }
    }

    Some((first_pair, second_pair))
}

fn width_at_height(page: PdfPageInfo, height: u16, font_pixels: Size) -> f32 {
    let height_px = f32::from(height.max(1)) * f32::from(font_pixels.height.max(1));
    let width_px = height_px * page.width_points.max(1.0) / page.height_points.max(1.0);
    (width_px / f32::from(font_pixels.width.max(1))).max(1.0)
}

fn scale_ratio(pair: Size, single: Size) -> f32 {
    let width_ratio = f32::from(pair.width) / f32::from(single.width.max(1));
    let height_ratio = f32::from(pair.height) / f32::from(single.height.max(1));
    width_ratio.min(height_ratio)
}

fn fit_page(page: PdfPageInfo, max_width: u16, max_height: u16, font_pixels: Size) -> Size {
    let max_width = max_width.max(1);
    let max_height = max_height.max(1);
    let box_width_px = f32::from(max_width) * f32::from(font_pixels.width.max(1));
    let box_height_px = f32::from(max_height) * f32::from(font_pixels.height.max(1));
    let scale = (box_width_px / page.width_points.max(1.0))
        .min(box_height_px / page.height_points.max(1.0))
        .max(0.01);

    let width_px = page.width_points * scale;
    let height_px = page.height_points * scale;
    let width_cells = (width_px / f32::from(font_pixels.width.max(1)))
        .ceil()
        .clamp(1.0, f32::from(max_width)) as u16;
    let height_cells = (height_px / f32::from(font_pixels.height.max(1)))
        .ceil()
        .clamp(1.0, f32::from(max_height)) as u16;

    Size::new(width_cells, height_cells)
}

fn zoom_size(size: Size, zoom_percent: u16) -> Size {
    let zoom = u32::from(zoom_percent.max(1));
    Size::new(
        scale_cells(size.width, zoom),
        scale_cells(size.height, zoom),
    )
}

fn scale_cells(value: u16, zoom_percent: u32) -> u16 {
    let scaled = u32::from(value)
        .saturating_mul(zoom_percent)
        .saturating_add(50)
        / 100;
    scaled.clamp(1, u32::from(u16::MAX)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(index: usize, width: f32, height: f32) -> PdfPageInfo {
        PdfPageInfo {
            index,
            width_points: width,
            height_points: height,
        }
    }

    #[test]
    fn wide_viewport_pairs_portrait_pages() {
        let pages = vec![page(0, 595.0, 842.0), page(1, 595.0, 842.0)];
        let layout = build_layout(
            &pages,
            Size::new(160, 45),
            Size::new(8, 16),
            PdfLayoutMode::Auto,
            FIT_ZOOM,
        );

        assert_eq!(layout.placements.len(), 2);
        assert_eq!(layout.placements[0].y, layout.placements[1].y);
        assert!(layout.placements[0].x < layout.placements[1].x);
    }

    #[test]
    fn narrow_viewport_stacks_portrait_pages() {
        let pages = vec![page(0, 595.0, 842.0), page(1, 595.0, 842.0)];
        let layout = build_layout(
            &pages,
            Size::new(70, 45),
            Size::new(8, 16),
            PdfLayoutMode::Auto,
            FIT_ZOOM,
        );

        assert_eq!(layout.placements.len(), 2);
        assert!(layout.placements[1].y > layout.placements[0].y);
    }

    #[test]
    fn mixed_orientation_keeps_landscape_page_on_its_own_row() {
        let pages = vec![
            page(0, 595.0, 842.0),
            page(1, 842.0, 595.0),
            page(2, 595.0, 842.0),
        ];
        let layout = build_layout(
            &pages,
            Size::new(120, 42),
            Size::new(8, 16),
            PdfLayoutMode::Auto,
            FIT_ZOOM,
        );

        assert!(layout.placements[1].y > layout.placements[0].y);
    }

    #[test]
    fn auto_mode_never_pairs_landscape_pages() {
        let pages = vec![page(0, 842.0, 595.0), page(1, 842.0, 595.0)];
        let layout = build_layout(
            &pages,
            Size::new(240, 42),
            Size::new(8, 16),
            PdfLayoutMode::Auto,
            FIT_ZOOM,
        );

        assert!(layout.placements[1].row > layout.placements[0].row);
        assert!(layout.placements[1].y > layout.placements[0].y);
    }

    #[test]
    fn forced_one_page_mode_stacks_even_on_wide_viewport() {
        let pages = vec![page(0, 595.0, 842.0), page(1, 595.0, 842.0)];
        let layout = build_layout(
            &pages,
            Size::new(180, 45),
            Size::new(8, 16),
            PdfLayoutMode::One,
            FIT_ZOOM,
        );

        assert!(layout.placements[1].row > layout.placements[0].row);
        assert!(layout.placements[1].y > layout.placements[0].y);
    }

    #[test]
    fn forced_two_page_mode_pairs_even_when_auto_would_stack() {
        let pages = vec![page(0, 595.0, 842.0), page(1, 595.0, 842.0)];
        let layout = build_layout(
            &pages,
            Size::new(70, 45),
            Size::new(8, 16),
            PdfLayoutMode::Two,
            FIT_ZOOM,
        );

        assert_eq!(layout.placements[0].row, layout.placements[1].row);
        assert_eq!(layout.placements[0].row_top, layout.placements[1].row_top);
    }

    #[test]
    fn forced_two_page_mode_keeps_landscape_page_on_its_own_row() {
        let pages = vec![page(0, 595.0, 842.0), page(1, 842.0, 595.0)];
        let layout = build_layout(
            &pages,
            Size::new(160, 45),
            Size::new(8, 16),
            PdfLayoutMode::Two,
            FIT_ZOOM,
        );

        assert!(layout.placements[1].row > layout.placements[0].row);
        assert!(layout.placements[1].y > layout.placements[0].y);
    }

    #[test]
    fn forced_two_page_mode_never_pairs_two_landscape_pages() {
        let pages = vec![page(0, 842.0, 595.0), page(1, 842.0, 595.0)];
        let layout = build_layout(
            &pages,
            Size::new(240, 45),
            Size::new(8, 16),
            PdfLayoutMode::Two,
            FIT_ZOOM,
        );

        assert!(layout.placements[1].row > layout.placements[0].row);
        assert!(layout.placements[1].y > layout.placements[0].y);
    }

    #[test]
    fn extracts_searchable_text_from_pdf_page() {
        let content = b"BT /F1 18 Tf 20 70 Td (Hello PDF Search) Tj ET";
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_string(),
            format!(
                "<< /Length {} >>\nstream\n{}\nendstream",
                content.len(),
                String::from_utf8_lossy(content)
            ),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];

        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, object).as_bytes());
        }
        let xref = pdf.len();
        pdf.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
        for offset in offsets {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!("trailer\n<< /Root 1 0 R /Size 6 >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
        );

        let path = std::env::temp_dir().join(format!("iris-pdf-search-{}.pdf", std::process::id()));
        fs::write(&path, pdf).unwrap();
        let mut document = PdfDocument::open(&path).unwrap();
        let matches = document.search(&SearchMatcher::new("pdf search").unwrap());
        let _ = fs::remove_file(path);

        assert_eq!(matches.len(), 1);
        let (page_index, rects) = matches[0].pdf_location().unwrap();
        assert_eq!(page_index, 0);
        assert!(!rects.is_empty());
    }

    #[test]
    fn pdf_text_search_ignores_whitespace_and_uses_smart_case() {
        let page = PdfTextPage::new(vec![
            PdfTextRun {
                text: "HMAC".to_string(),
                rect: SearchRect::new(0.10, 0.10, 0.08, 0.02),
            },
            PdfTextRun {
                text: "Authentication".to_string(),
                rect: SearchRect::new(0.20, 0.10, 0.18, 0.02),
            },
            PdfTextRun {
                text: "hmac".to_string(),
                rect: SearchRect::new(0.10, 0.20, 0.08, 0.02),
            },
        ]);

        let insensitive = SearchMatcher::new("hmac authentication").unwrap();
        let matches = page.matches(&insensitive, 4);
        assert_eq!(matches.len(), 1);
        let (page_index, rects) = matches[0].pdf_location().unwrap();
        assert_eq!(page_index, 4);
        assert_eq!(rects.len(), 1);
        assert!(rects[0].right() >= 0.38);

        let sensitive = SearchMatcher::new("HMAC").unwrap();
        assert_eq!(page.matches(&sensitive, 4).len(), 1);
    }

    #[test]
    fn pdf_text_search_keeps_multiline_match_as_multiple_rectangles() {
        let page = PdfTextPage::new(vec![
            PdfTextRun {
                text: "Health".to_string(),
                rect: SearchRect::new(0.10, 0.10, 0.10, 0.02),
            },
            PdfTextRun {
                text: "Monitor".to_string(),
                rect: SearchRect::new(0.10, 0.16, 0.12, 0.02),
            },
        ]);
        let matcher = SearchMatcher::new("Health Monitor").unwrap();
        let matches = page.matches(&matcher, 0);
        let (_, rects) = matches[0].pdf_location().unwrap();
        assert_eq!(rects.len(), 2);
        assert!(rects[0].y < rects[1].y);
    }

    #[test]
    fn raster_highlight_changes_pixels_inside_match() {
        let source = DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            100,
            100,
            image::Rgba([255, 255, 255, 255]),
        ));
        let highlighted = apply_search_highlights(
            &source,
            &[PdfSearchHighlight {
                rect: SearchRect::new(0.25, 0.25, 0.25, 0.25),
                current: true,
            }],
            (0, 0, 255),
            (255, 117, 0),
        );
        let pixels = highlighted.to_rgba8();
        assert_eq!(pixels.get_pixel(5, 5).0, [255, 255, 255, 255]);
        assert_ne!(pixels.get_pixel(30, 30).0, [255, 255, 255, 255]);
    }

    #[test]
    fn zoom_increases_layout_dimensions_and_enables_horizontal_pan() {
        let pages = vec![page(0, 595.0, 842.0)];
        let fit = build_layout(
            &pages,
            Size::new(100, 40),
            Size::new(8, 16),
            PdfLayoutMode::One,
            FIT_ZOOM,
        );
        let zoomed = build_layout(
            &pages,
            Size::new(100, 40),
            Size::new(8, 16),
            PdfLayoutMode::One,
            200,
        );

        assert!(zoomed.placements[0].width > fit.placements[0].width);
        assert!(zoomed.placements[0].height > fit.placements[0].height);
        assert!(zoomed.total_width >= usize::from(zoomed.placements[0].width));
    }
}
