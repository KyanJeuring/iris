use std::{
    collections::{HashMap, VecDeque},
    fs,
    path::Path,
    sync::Arc,
};

use anyhow::{Context, Result, anyhow, bail};
use hayro::{
    RenderCache, RenderSettings, hayro_interpret::InterpreterSettings, hayro_syntax::Pdf,
    vello_cpu::color::palette::css::WHITE,
};
use image::{DynamicImage, RgbaImage};
use ratatui::layout::Size;

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

pub struct PdfDocument {
    pdf: Pdf,
    pages: Vec<PdfPageInfo>,
    layout: PdfLayout,
    layout_mode: PdfLayoutMode,
    zoom_percent: u16,
    raster_cache: HashMap<RasterKey, Arc<DynamicImage>>,
    raster_order: VecDeque<RasterKey>,
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
