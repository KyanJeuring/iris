use std::{
    collections::{HashMap, HashSet},
    io::{self, Write},
    num::NonZeroU16,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

use base64::Engine as _;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
};
use ratatui::{
    DefaultTerminal, Frame,
    buffer::{Buffer, CellDiffOption},
    layout::{Constraint, Layout, Rect, Size},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Padding, Paragraph, Widget},
};
use ratatui_image::{
    Image as TerminalImage,
    sliced::{SignedPosition, SlicedImage},
};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{
    image::ImageManager,
    input::InputKind,
    pdf::{PdfLayoutMode, PdfSearchHighlight, apply_search_highlights},
    renderer::{NodeId, RenderedDocument, RenderedLine, ViewDocument},
    search::{SearchMatch, SearchMatcher, SearchState},
    selection::{DocumentPosition, Selection},
    theme::Theme,
    ui,
};

const MOUSE_VERTICAL_SCROLL: usize = 3;
const MOUSE_HORIZONTAL_SCROLL: usize = 4;

pub struct App {
    name: String,
    kind: InputKind,
    base_dir: Option<PathBuf>,
    document: ViewDocument,
    theme: Theme,
    search: SearchState,
    images: ImageManager,

    rendered: RenderedDocument,
    dirty: bool,
    last_width: u16,
    last_height: u16,
    viewport_area: Rect,
    viewport_width: u16,
    viewport_height: u16,

    vertical_scroll: usize,
    horizontal_scroll: usize,
    wrap: bool,
    tab_width: usize,

    show_help: bool,
    help_scroll: usize,
    help_max_scroll: usize,
    help_page_height: usize,
    recenter_image_after_render: bool,

    selection: Option<Selection>,
    mouse_selection_anchor: Option<(DocumentPosition, DocumentPosition)>,
    mouse_selecting: bool,
    mouse_dragged: bool,
    mouse_drag_position: Option<(u16, u16)>,
}

impl App {
    pub fn new(
        name: String,
        kind: InputKind,
        base_dir: Option<PathBuf>,
        document: ViewDocument,
        theme: Theme,
        wrap: bool,
        tab_width: usize,
    ) -> Self {
        Self {
            name,
            kind,
            base_dir,
            document,
            theme,
            search: SearchState::default(),
            images: ImageManager::default(),
            rendered: RenderedDocument::default(),
            dirty: true,
            last_width: 0,
            last_height: 0,
            viewport_area: Rect::default(),
            viewport_width: 1,
            viewport_height: 1,
            vertical_scroll: 0,
            horizontal_scroll: 0,
            wrap,
            tab_width,
            show_help: false,
            help_scroll: 0,
            help_max_scroll: 0,
            help_page_height: 1,
            recenter_image_after_render: false,
            selection: None,
            mouse_selection_anchor: None,
            mouse_selecting: false,
            mouse_dragged: false,
            mouse_drag_position: None,
        }
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        self.images.initialize();
        let _mouse = MouseCaptureGuard::enable()?;

        loop {
            terminal.draw(|frame| self.draw(frame))?;

            if self.selection_needs_autoscroll() {
                if event::poll(Duration::from_millis(35))? {
                    if self.handle_event(event::read()?) {
                        break;
                    }
                } else if let Some((column, row)) = self.mouse_drag_position {
                    self.extend_mouse_selection(column, row);
                }
            } else if self.handle_event(event::read()?) {
                break;
            }
        }
        Ok(())
    }

    fn handle_event(&mut self, event: Event) -> bool {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => self.handle_key(key),
            Event::Mouse(mouse) => {
                self.handle_mouse(mouse);
                false
            }
            Event::Resize(_, _) => {
                self.dirty = true;
                false
            }
            _ => false,
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let [content_area, status_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());

        let zoom_percent = match &self.document {
            ViewDocument::Pdf(document) => Some(document.zoom_percent()),
            _ => self.document.image_zoom_percent(),
        };
        let title = match zoom_percent {
            Some(100) => format!(" {} [fit] ", self.name),
            Some(zoom) => format!(" {} [{zoom}%] ", self.name),
            None => format!(" {} ", self.name),
        };
        let block = Block::bordered()
            .title(title)
            .border_style(self.theme.border)
            .padding(Padding::uniform(1));
        self.viewport_area = block.inner(content_area);
        self.viewport_width = self.viewport_area.width.max(1);
        self.viewport_height = self.viewport_area.height.max(1);

        if self.document.is_pdf() {
            self.prepare_pdf_layout();
            self.clamp_scroll();

            let viewer = Paragraph::new("").style(self.theme.document).block(block);
            frame.render_widget(viewer, content_area);
            self.draw_pdf_pages(frame);
        } else {
            self.ensure_rendered(self.viewport_width, self.viewport_height);
            self.clamp_scroll();

            let lines = self.display_lines();
            let text = Text::from(lines);
            let viewer = Paragraph::new(text)
                .style(self.theme.document)
                .block(block)
                .scroll((
                    self.vertical_scroll.min(u16::MAX as usize) as u16,
                    self.horizontal_scroll.min(u16::MAX as usize) as u16,
                ));
            frame.render_widget(viewer, content_area);
            self.draw_images(frame);
        }

        let status_line = if self.search.active {
            ui::search::prompt(&self.search, &self.theme)
        } else if self.document.is_pdf() {
            let (pages, page_count, layout_mode, zoom_percent) = match &self.document {
                ViewDocument::Pdf(document) => (
                    document.row_pages_at_scroll(self.vertical_scroll),
                    document.page_count(),
                    document.layout_mode(),
                    document.zoom_percent(),
                ),
                _ => unreachable!(),
            };
            let search_position = if self.search.query.is_empty() {
                None
            } else {
                Some(
                    self.search
                        .position_label()
                        .unwrap_or((0, self.search.matches.len())),
                )
            };
            ui::status::render_pdf(
                &self.name,
                &pages,
                page_count,
                layout_mode,
                zoom_percent,
                search_position,
                &self.theme,
            )
        } else {
            let total = self.rendered.lines.len().max(1);
            let visible_end = (self.vertical_scroll + self.viewport_height as usize).min(total);
            let percent = visible_end.saturating_mul(100) / total;
            let search_position = if self.search.query.is_empty() {
                None
            } else {
                Some(
                    self.search
                        .position_label()
                        .unwrap_or((0, self.search.matches.len())),
                )
            };
            ui::status::render(
                ui::status::Status {
                    name: &self.name,
                    kind: self.kind.label(),
                    current_line: self.vertical_scroll + 1,
                    total_lines: total,
                    percent,
                    wrap: self.wrap,
                    search: search_position,
                    image: self.document.is_image(),
                    selection: self
                        .selection
                        .is_some_and(|selection| !selection.is_empty()),
                },
                &self.theme,
            )
        };
        frame.render_widget(
            Paragraph::new(status_line).style(self.theme.status),
            status_area,
        );

        if self.show_help {
            let help = ui::help::render(
                frame,
                &self.theme,
                self.document.is_markdown(),
                self.document.is_image(),
                self.document.is_pdf(),
                self.help_scroll,
            );
            self.help_scroll = help.scroll;
            self.help_max_scroll = help.max_scroll;
            self.help_page_height = help.page_height;

            if (self.document.is_image() || self.document.is_pdf())
                && let Some(sequence) = self.images.kitty_help_overlay_sequence(
                    self.theme.help.bg,
                    Size::new(help.area.width, help.area.height),
                )
            {
                frame.render_widget(PreservingGraphicsCommand(sequence), help.area);
            }
        } else {
            let sequence = self.images.hide_kitty_help_overlay_sequence();
            if !sequence.is_empty() {
                frame.render_widget(PreservingGraphicsCommand(sequence), frame.area());
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if self.show_help {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?') => self.show_help = false,
                KeyCode::Char('q') => return true,
                KeyCode::Down | KeyCode::Char('j') => self.scroll_help_down(1),
                KeyCode::Up | KeyCode::Char('k') => self.scroll_help_up(1),
                KeyCode::PageDown => {
                    self.scroll_help_down(self.help_page_height.saturating_sub(1).max(1));
                }
                KeyCode::PageUp => {
                    self.scroll_help_up(self.help_page_height.saturating_sub(1).max(1));
                }
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.scroll_help_down(self.help_page_height.saturating_div(2).max(1));
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.scroll_help_up(self.help_page_height.saturating_div(2).max(1));
                }
                KeyCode::Home | KeyCode::Char('g') => self.help_scroll = 0,
                KeyCode::End | KeyCode::Char('G') => self.help_scroll = self.help_max_scroll,
                _ => {}
            }
            return false;
        }

        if self.search.active {
            self.handle_search_key(key);
            return false;
        }

        match key.code {
            KeyCode::Char('q') => return true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.copy_selection();
            }
            KeyCode::Char('y') => self.copy_selection(),
            KeyCode::Esc => {
                self.clear_selection();
                self.clear_search();
            }
            KeyCode::Char('?') => {
                self.show_help = true;
                self.help_scroll = 0;
            }
            KeyCode::Char('/') => self.open_search(),
            KeyCode::Char('n') => self.next_match(),
            KeyCode::Char('N') => self.previous_match(),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.scroll_down(self.half_page_height())
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.scroll_up(self.half_page_height())
            }
            KeyCode::Char('h') if self.document.is_markdown() => self.next_heading(),
            KeyCode::Char('H') if self.document.is_markdown() => self.previous_heading(),
            KeyCode::Char('p') if self.document.is_pdf() => self.next_pdf_page(),
            KeyCode::Char('P') if self.document.is_pdf() => self.previous_pdf_page(),
            KeyCode::Down | KeyCode::Char('j') => self.scroll_down(1),
            KeyCode::Up | KeyCode::Char('k') => self.scroll_up(1),
            KeyCode::Right | KeyCode::Char('l') => self.scroll_right(1),
            KeyCode::Left | KeyCode::Char('h') => self.scroll_left(1),
            KeyCode::PageDown => self.scroll_down(self.page_height()),
            KeyCode::PageUp => self.scroll_up(self.page_height()),
            KeyCode::Home | KeyCode::Char('g') => self.vertical_scroll = 0,
            KeyCode::End | KeyCode::Char('G') => self.vertical_scroll = self.max_vertical_scroll(),
            KeyCode::Char('+') | KeyCode::Char('=') if self.document.is_image() => {
                self.zoom_image_in();
            }
            KeyCode::Char('-') if self.document.is_image() => {
                self.zoom_image_out();
            }
            KeyCode::Char('0') if self.document.is_image() => {
                self.reset_image_zoom();
            }
            KeyCode::Char('+') | KeyCode::Char('=') if self.document.is_pdf() => {
                self.zoom_pdf_in();
            }
            KeyCode::Char('-') if self.document.is_pdf() => {
                self.zoom_pdf_out();
            }
            KeyCode::Char('0') if self.document.is_pdf() => {
                self.reset_pdf_zoom();
            }
            KeyCode::Char('1') if self.document.is_pdf() => {
                self.set_pdf_layout_mode(PdfLayoutMode::One);
            }
            KeyCode::Char('2') if self.document.is_pdf() => {
                self.set_pdf_layout_mode(PdfLayoutMode::Two);
            }
            KeyCode::Char('a') if self.document.is_pdf() => {
                self.set_pdf_layout_mode(PdfLayoutMode::Auto);
            }
            KeyCode::Char('w') if !self.document.is_image() && !self.document.is_pdf() => {
                self.wrap = !self.wrap;
                self.dirty = true;
            }
            _ => {}
        }
        false
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.clear_search();
                self.search.cancel_input();
            }
            KeyCode::Enter => {
                self.search.accept_input();
                self.jump_to_current_match();
            }
            KeyCode::Backspace => {
                self.search.query.pop();
                self.refresh_search(true);
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search.query.push(ch);
                self.refresh_search(true);
            }
            _ => {}
        }
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) {
        if self.show_help {
            match mouse.kind {
                MouseEventKind::ScrollUp => self.scroll_help_up(MOUSE_VERTICAL_SCROLL),
                MouseEventKind::ScrollDown => self.scroll_help_down(MOUSE_VERTICAL_SCROLL),
                _ => {}
            }
            return;
        }

        let shift = mouse.modifiers.contains(KeyModifiers::SHIFT);
        match mouse.kind {
            MouseEventKind::ScrollUp if shift => self.scroll_left(MOUSE_HORIZONTAL_SCROLL),
            MouseEventKind::ScrollDown if shift => self.scroll_right(MOUSE_HORIZONTAL_SCROLL),
            MouseEventKind::ScrollUp => self.scroll_up(MOUSE_VERTICAL_SCROLL),
            MouseEventKind::ScrollDown => self.scroll_down(MOUSE_VERTICAL_SCROLL),
            MouseEventKind::ScrollLeft => self.scroll_left(MOUSE_HORIZONTAL_SCROLL),
            MouseEventKind::ScrollRight => self.scroll_right(MOUSE_HORIZONTAL_SCROLL),
            MouseEventKind::Down(MouseButton::Left)
                if !self.document.is_image() && !self.document.is_pdf() =>
            {
                self.start_mouse_selection(mouse.column, mouse.row);
            }
            MouseEventKind::Drag(MouseButton::Left) if self.mouse_selecting => {
                self.extend_mouse_selection(mouse.column, mouse.row);
            }
            MouseEventKind::Up(MouseButton::Left) if self.mouse_selecting => {
                self.finish_mouse_selection(mouse.column, mouse.row);
            }
            MouseEventKind::Down(MouseButton::Left)
                if self.document.is_image() || self.document.is_pdf() => {}
            _ => {}
        }
    }

    fn start_mouse_selection(&mut self, column: u16, row: u16) {
        let Some((start, end)) = self.document_cell_bounds(column, row) else {
            return;
        };

        self.mouse_selection_anchor = Some((start, end));
        self.mouse_selecting = true;
        self.mouse_dragged = false;
        self.mouse_drag_position = Some((column, row));
        self.selection = None;
    }

    fn extend_mouse_selection(&mut self, column: u16, row: u16) {
        let Some((anchor_start, anchor_end)) = self.mouse_selection_anchor else {
            return;
        };

        self.mouse_drag_position = Some((column, row));
        self.autoscroll_selection(column, row);

        let Some((head_start, head_end)) = self.document_cell_bounds_clamped(column, row) else {
            return;
        };

        let selection = if head_start < anchor_start {
            Selection::new(anchor_end, head_start)
        } else {
            Selection::new(anchor_start, head_end)
        };

        self.mouse_dragged = !selection.is_empty();
        self.selection = (!selection.is_empty()).then_some(selection);
    }

    fn finish_mouse_selection(&mut self, column: u16, row: u16) {
        if self.mouse_dragged {
            self.extend_mouse_selection(column, row);
        } else {
            self.selection = None;
            self.open_link_at(column, row);
        }

        self.mouse_selecting = false;
        self.mouse_selection_anchor = None;
        self.mouse_dragged = false;
        self.mouse_drag_position = None;
    }

    fn autoscroll_selection(&mut self, column: u16, row: u16) {
        let area = self.viewport_area;
        if area.width == 0 || area.height == 0 {
            return;
        }

        if row < area.y {
            self.scroll_up(1);
        } else if row >= area.y.saturating_add(area.height) {
            self.scroll_down(1);
        }

        if !self.wrap {
            if column < area.x {
                self.scroll_left(1);
            } else if column >= area.x.saturating_add(area.width) {
                self.scroll_right(1);
            }
        }
    }

    fn selection_needs_autoscroll(&self) -> bool {
        let Some((column, row)) = self.mouse_drag_position else {
            return false;
        };
        let area = self.viewport_area;
        if area.width == 0 || area.height == 0 {
            return false;
        }

        row < area.y
            || row >= area.y.saturating_add(area.height)
            || (!self.wrap && (column < area.x || column >= area.x.saturating_add(area.width)))
    }

    fn document_cell_bounds(
        &self,
        column: u16,
        row: u16,
    ) -> Option<(DocumentPosition, DocumentPosition)> {
        let area = self.viewport_area;
        if column < area.x
            || row < area.y
            || column >= area.x.saturating_add(area.width)
            || row >= area.y.saturating_add(area.height)
        {
            return None;
        }

        self.document_cell_bounds_clamped(column, row)
    }

    fn document_cell_bounds_clamped(
        &self,
        column: u16,
        row: u16,
    ) -> Option<(DocumentPosition, DocumentPosition)> {
        let area = self.viewport_area;
        if area.width == 0 || area.height == 0 || self.rendered.lines.is_empty() {
            return None;
        }

        let max_column = area.x.saturating_add(area.width).saturating_sub(1);
        let max_row = area.y.saturating_add(area.height).saturating_sub(1);
        let screen_column = column.clamp(area.x, max_column);
        let screen_row = row.clamp(area.y, max_row);

        let line_index = (self.vertical_scroll + usize::from(screen_row - area.y))
            .min(self.rendered.lines.len().saturating_sub(1));
        let display_column = self.horizontal_scroll + usize::from(screen_column - area.x);
        let line = &self.rendered.lines[line_index].plain;
        let (start, end) = grapheme_bounds_at_column(line, display_column);

        Some((
            DocumentPosition {
                line: line_index,
                byte: start,
            },
            DocumentPosition {
                line: line_index,
                byte: end,
            },
        ))
    }

    fn open_search(&mut self) {
        if self.document.is_image() {
            return;
        }

        self.clear_selection();
        self.search.clear_query();
        self.search.open();
    }

    fn clear_search(&mut self) {
        self.search.clear_query();
    }

    fn refresh_search(&mut self, jump: bool) {
        let matches = SearchMatcher::new(&self.search.query)
            .map(|matcher| self.document.search(&matcher))
            .unwrap_or_default();
        self.search.set_matches(matches);
        if jump {
            self.jump_to_current_match();
        }
    }

    fn next_match(&mut self) {
        if self.search.query.is_empty() {
            return;
        }
        self.search.next();
        self.jump_to_current_match();
    }

    fn previous_match(&mut self) {
        if self.search.query.is_empty() {
            return;
        }
        self.search.previous();
        self.jump_to_current_match();
    }

    fn jump_to_current_match(&mut self) {
        let Some(target) = self.search.current_match().cloned() else {
            return;
        };
        self.focus_search_target(&target);
        self.clamp_scroll();
    }

    fn next_heading(&mut self) {
        if let Some(heading) = self
            .rendered
            .headings
            .iter()
            .find(|heading| heading.line > self.vertical_scroll)
        {
            self.vertical_scroll = heading.line.min(self.max_vertical_scroll());
            self.horizontal_scroll = 0;
        }
    }

    fn previous_heading(&mut self) {
        let target = self
            .rendered
            .headings
            .iter()
            .rev()
            .find(|heading| heading.line < self.vertical_scroll)
            .map(|heading| heading.line);
        if let Some(line) = target {
            self.vertical_scroll = line.min(self.max_vertical_scroll());
            self.horizontal_scroll = 0;
        }
    }

    fn next_pdf_page(&mut self) {
        let target = match &self.document {
            ViewDocument::Pdf(document) => document.next_row_top(self.vertical_scroll),
            _ => None,
        };
        if let Some(top) = target {
            self.vertical_scroll = top.min(self.max_vertical_scroll());
            self.center_pdf_horizontally();
        }
    }

    fn previous_pdf_page(&mut self) {
        let target = match &self.document {
            ViewDocument::Pdf(document) => document.previous_row_top(self.vertical_scroll),
            _ => None,
        };
        if let Some(top) = target {
            self.vertical_scroll = top.min(self.max_vertical_scroll());
            self.center_pdf_horizontally();
        }
    }

    fn center_pdf_horizontally(&mut self) {
        self.horizontal_scroll = match &self.document {
            ViewDocument::Pdf(document) => {
                let page = document.page_at_scroll(self.vertical_scroll);
                document.page_horizontal_scroll(page, self.viewport_width)
            }
            _ => 0,
        };
        self.horizontal_scroll = self.horizontal_scroll.min(self.max_horizontal_scroll());
    }

    fn set_pdf_layout_mode(&mut self, mode: PdfLayoutMode) {
        if let ViewDocument::Pdf(document) = &mut self.document {
            document.set_layout_mode(mode);
        }
    }

    fn prepare_pdf_layout(&mut self) {
        let viewport = Size::new(self.viewport_width.max(1), self.viewport_height.max(1));
        let font_pixels = self.images.font_size_pixels();
        let anchor = match &self.document {
            ViewDocument::Pdf(document) => document.page_at_scroll(self.vertical_scroll),
            _ => return,
        };

        let changed = match &mut self.document {
            ViewDocument::Pdf(document) => document.rebuild_layout(viewport, font_pixels),
            _ => false,
        };

        if changed {
            let (top, left) = match &self.document {
                ViewDocument::Pdf(document) => (
                    document.page_top(anchor).unwrap_or(0),
                    document.page_horizontal_scroll(anchor, viewport.width),
                ),
                _ => (0, 0),
            };
            self.vertical_scroll = top;
            self.horizontal_scroll = left;
            self.last_width = viewport.width;
            self.last_height = viewport.height;
            self.dirty = false;
        }
    }

    fn draw_pdf_pages(&mut self, frame: &mut Frame) {
        let viewport = self.viewport_area;
        let scroll_left = self.horizontal_scroll;
        let scroll_top = self.vertical_scroll;
        let scroll_right = scroll_left.saturating_add(usize::from(viewport.width));
        let scroll_bottom = scroll_top.saturating_add(usize::from(viewport.height));
        let font_pixels = self.images.font_size_pixels();
        let search_match_color = style_background_rgb(self.theme.search_match, (91, 156, 246));
        let search_current_color = style_background_rgb(self.theme.search_current, (255, 117, 0));

        let placements = match &self.document {
            ViewDocument::Pdf(document) => document
                .layout()
                .placements
                .iter()
                .copied()
                .filter(|placement| placement.intersects(scroll_top, scroll_bottom))
                .collect::<Vec<_>>(),
            _ => return,
        };

        // Kitty image placements persist independently from Ratatui's text buffer. Keep track of
        // the PDF pages that are actually visible in both axes and explicitly delete placements
        // that left the viewport or disappeared after a layout change. Without this, a page from
        // a previous two-page spread can remain on screen after switching to one-page mode, and
        // pages can overlap while scrolling through mixed portrait/landscape rows.
        let visible_kitty_pages = placements
            .iter()
            .filter(|placement| {
                let page_left = placement.x;
                let page_right = page_left.saturating_add(usize::from(placement.width));
                page_left < scroll_right && page_right > scroll_left
            })
            .map(|placement| format!("pdf-page:{}", placement.page_index))
            .collect::<Vec<_>>();
        let mut kitty_cleanup = self
            .images
            .sync_generated_kitty_placements(&visible_kitty_pages);

        for placement in placements {
            let page_left = placement.x;
            let page_top = placement.y;
            let page_right = page_left.saturating_add(usize::from(placement.width));
            let page_bottom = page_top.saturating_add(usize::from(placement.height));
            let visible_left = scroll_left.max(page_left);
            let visible_top = scroll_top.max(page_top);
            let visible_right = scroll_right.min(page_right);
            let visible_bottom = scroll_bottom.min(page_bottom);

            if visible_left >= visible_right || visible_top >= visible_bottom {
                continue;
            }

            let size = Size::new(placement.width.max(1), placement.height.max(1));
            let highlights = self.pdf_search_highlights(placement.page_index);
            let image = match &mut self.document {
                ViewDocument::Pdf(document) => {
                    document.render_page(placement.page_index, size, font_pixels)
                }
                _ => None,
            };
            let Some(base_image) = image else {
                continue;
            };
            let highlighted_image = (!highlights.is_empty()).then(|| {
                apply_search_highlights(
                    base_image.as_ref(),
                    &highlights,
                    search_match_color,
                    search_current_color,
                )
            });
            let image = highlighted_image
                .as_ref()
                .unwrap_or_else(|| base_image.as_ref());

            let crop_x = (visible_left - page_left).min(usize::from(u16::MAX)) as u16;
            let crop_y = (visible_top - page_top).min(usize::from(u16::MAX)) as u16;
            let screen_x = (visible_left - scroll_left).min(usize::from(u16::MAX)) as u16;
            let screen_y = (visible_top - scroll_top).min(usize::from(u16::MAX)) as u16;
            let visible_width = (visible_right - visible_left).min(usize::from(u16::MAX)) as u16;
            let visible_height = (visible_bottom - visible_top).min(usize::from(u16::MAX)) as u16;
            let visible_size = Size::new(visible_width.max(1), visible_height.max(1));
            let area = Rect {
                x: viewport.x.saturating_add(screen_x),
                y: viewport.y.saturating_add(screen_y),
                width: visible_size.width,
                height: visible_size.height,
            };

            // PDFs are already rasterized at their final zoomed page size. Keep each page as its
            // own native terminal image and only change the visible source rectangle while panning.
            // On Kitty/Ghostty this means the full page is transmitted once per zoom level and
            // horizontal/vertical panning never drops to the half-block fallback renderer.
            let identity_key = format!("pdf-page:{}", placement.page_index);
            let search_key = if highlights.is_empty() {
                String::new()
            } else {
                format!(
                    ":search:{}:{}",
                    self.search.query,
                    self.search.current.unwrap_or(usize::MAX)
                )
            };
            let render_key = format!(
                "pdf:{}:{}x{}:{}x{}{}",
                placement.page_index,
                size.width,
                size.height,
                font_pixels.width,
                font_pixels.height,
                search_key
            );

            if let Some(mut sequence) = self.images.generated_kitty_viewport_sequence(
                &identity_key,
                &render_key,
                image,
                size,
                Rect::new(crop_x, crop_y, visible_size.width, visible_size.height),
            ) {
                if !kitty_cleanup.is_empty() {
                    let mut combined = std::mem::take(&mut kitty_cleanup);
                    combined.push_str(&sequence);
                    sequence = combined;
                }
                frame.render_widget(GraphicsCommand(sequence), area);
                continue;
            }

            let horizontally_clipped = crop_x != 0 || visible_size.width < size.width;
            if !horizontally_clipped {
                let Some(protocol) = self
                    .images
                    .generated_sliced_protocol(render_key, image, size)
                else {
                    continue;
                };

                let relative_y = page_top as i128 - scroll_top as i128;
                let y = relative_y.clamp(i16::MIN as i128, i16::MAX as i128) as i16;
                let x = screen_x.min(i16::MAX as u16) as i16;
                frame.render_widget(
                    SlicedImage::new(protocol, SignedPosition::from((x, y))),
                    viewport,
                );
                continue;
            }

            // Sixel/iTerm2 cannot cheaply skip arbitrary columns in a prepared SlicedProtocol.
            // Crop the already high-resolution PDF raster to the visible source rectangle and
            // encode that viewport with the terminal's preferred native protocol instead of
            // silently switching to Unicode half-block rendering. The viewport protocol is cached
            // so revisiting the same pan position does not require another encode.
            let viewport_key = format!("{render_key}:viewport");
            let Some(protocol) = self.images.generated_viewport_protocol(
                viewport_key,
                image,
                size,
                crop_x,
                crop_y,
                visible_size,
            ) else {
                continue;
            };

            frame.render_widget(TerminalImage::new(protocol), area);
        }

        // If no Kitty page was drawn this frame (for example because the viewport is currently in
        // a gap between rows), we still need to emit pending delete commands so stale placements
        // disappear immediately.
        if !kitty_cleanup.is_empty() {
            let cleanup_area = Rect::new(viewport.x, viewport.y, 1, 1);
            frame.render_widget(GraphicsCommand(kitty_cleanup), cleanup_area);
        }
    }

    fn pdf_search_highlights(&self, page_index: usize) -> Vec<PdfSearchHighlight> {
        if self.search.query.is_empty() {
            return Vec::new();
        }

        let mut highlights = Vec::new();
        for (match_index, search_match) in self.search.matches.iter().enumerate() {
            let Some((match_page, rects)) = search_match.pdf_location() else {
                continue;
            };
            if match_page != page_index {
                continue;
            }
            highlights.extend(rects.iter().copied().map(|rect| PdfSearchHighlight {
                rect,
                current: self.search.current == Some(match_index),
            }));
        }
        highlights
    }

    fn open_link_at(&mut self, column: u16, row: u16) {
        let area = self.viewport_area;
        if column < area.x
            || row < area.y
            || column >= area.x.saturating_add(area.width)
            || row >= area.y.saturating_add(area.height)
        {
            return;
        }

        let line = self.vertical_scroll + usize::from(row - area.y);
        let column = self.horizontal_scroll + usize::from(column - area.x);

        if self.document.is_markdown()
            && let Some(destination) = self
                .rendered
                .images
                .iter()
                .find(|image| {
                    let end_line = image.line + usize::from(image.height);
                    (image.line..end_line).contains(&line) && column >= image.column
                })
                .map(|image| {
                    image
                        .destination
                        .clone()
                        .unwrap_or_else(|| image.source.clone())
                })
        {
            self.open_destination(&destination);
            return;
        }

        let Some(destination) = self
            .rendered
            .links
            .iter()
            .find(|link| {
                link.line == line && column >= link.start_column && column < link.end_column
            })
            .map(|link| link.destination.clone())
        else {
            return;
        };

        self.open_destination(&destination);
    }
    fn open_destination(&mut self, destination: &str) {
        if let Some(slug) = destination.strip_prefix('#') {
            if let Some(heading) = self
                .rendered
                .headings
                .iter()
                .find(|heading| heading.slug == slug)
            {
                self.vertical_scroll = heading.line.min(self.max_vertical_scroll());
                self.horizontal_scroll = 0;
            }
            return;
        }

        let _ = if has_uri_scheme(destination) || Path::new(destination).is_absolute() {
            open::that(destination)
        } else if let Some(base_dir) = &self.base_dir {
            open::that(base_dir.join(destination))
        } else {
            open::that(destination)
        };
    }

    fn draw_images(&mut self, frame: &mut Frame) {
        if self.rendered.images.is_empty() {
            return;
        }

        if self.document.is_image() {
            self.draw_standalone_image(frame);
            return;
        }

        if self.horizontal_scroll != 0 {
            return;
        }

        let viewport = self.viewport_area;
        let scroll_top = self.vertical_scroll;
        let scroll_bottom = scroll_top + usize::from(self.viewport_height);
        let base_dir = self.base_dir.clone();
        let placements = self.rendered.images.clone();

        for image in placements {
            // Fixed image protocols cannot be vertically offset cheaply. Once the top of an image
            // scrolls above the viewport, leave the reserved rows in place until it is fully passed.
            if image.line < scroll_top || image.line >= scroll_bottom {
                continue;
            }

            let x_offset = image
                .column
                .min(usize::from(viewport.width.saturating_sub(1)));
            let available_width = viewport.width.saturating_sub(x_offset as u16);
            let y_offset = image.line - scroll_top;
            let available_height = viewport
                .height
                .saturating_sub(y_offset.min(usize::from(u16::MAX)) as u16)
                .min(image.height);

            if available_width == 0 || available_height == 0 {
                continue;
            }

            let Some(protocol) = self.images.protocol(
                &image.source,
                base_dir.as_deref(),
                available_width,
                available_height,
            ) else {
                continue;
            };

            let size = protocol.size();
            let width = size.width.min(available_width);
            let height = size.height.min(available_height);
            let centered = available_width.saturating_sub(width) / 2;
            let area = Rect {
                x: viewport
                    .x
                    .saturating_add(x_offset as u16)
                    .saturating_add(centered),
                y: viewport
                    .y
                    .saturating_add(y_offset.min(usize::from(u16::MAX)) as u16),
                width,
                height,
            };

            frame.render_widget(TerminalImage::new(protocol).allow_clipping(true), area);
        }
    }

    fn draw_standalone_image(&mut self, frame: &mut Frame) {
        let Some(image) = self.rendered.images.first().cloned() else {
            return;
        };

        let viewport = self.viewport_area;
        let scroll_left = self.horizontal_scroll;
        let scroll_top = self.vertical_scroll;
        let scroll_right = scroll_left.saturating_add(usize::from(viewport.width));
        let scroll_bottom = scroll_top.saturating_add(usize::from(viewport.height));
        let image_left = image.column;
        let image_top = image.line;
        let image_right = image_left.saturating_add(usize::from(image.width));
        let image_bottom = image_top.saturating_add(usize::from(image.height));

        let visible_left = scroll_left.max(image_left);
        let visible_top = scroll_top.max(image_top);
        let visible_right = scroll_right.min(image_right);
        let visible_bottom = scroll_bottom.min(image_bottom);

        if visible_left >= visible_right || visible_top >= visible_bottom {
            return;
        }

        let visible_width = (visible_right - visible_left).min(usize::from(u16::MAX)) as u16;
        let visible_height = (visible_bottom - visible_top).min(usize::from(u16::MAX)) as u16;
        let crop_x = (visible_left - image_left).min(usize::from(u16::MAX)) as u16;
        let crop_y = (visible_top - image_top).min(usize::from(u16::MAX)) as u16;
        let screen_x = (visible_left - scroll_left).min(usize::from(u16::MAX)) as u16;
        let screen_y = (visible_top - scroll_top).min(usize::from(u16::MAX)) as u16;
        let base_dir = self.base_dir.clone();

        let area = Rect {
            x: viewport.x.saturating_add(screen_x),
            y: viewport.y.saturating_add(screen_y),
            width: visible_width,
            height: visible_height,
        };

        if let Some(sequence) = self.images.kitty_viewport_sequence(
            &image.source,
            base_dir.as_deref(),
            Size::new(image.width, image.height),
            crop_x,
            crop_y,
            Size::new(visible_width, visible_height),
        ) {
            frame.render_widget(GraphicsCommand(sequence), area);
            return;
        }

        let pannable = image.width > viewport.width || image.height > viewport.height;

        if pannable {
            if let Some(viewport) = self.images.cell_viewport(
                &image.source,
                base_dir.as_deref(),
                Size::new(image.width, image.height),
                crop_x,
                crop_y,
            ) {
                frame.render_widget(viewport, area);
            }
            return;
        }

        let Some(protocol) = self.images.protocol(
            &image.source,
            base_dir.as_deref(),
            image.width,
            image.height,
        ) else {
            return;
        };

        frame.render_widget(TerminalImage::new(protocol), area);
    }

    fn ensure_rendered(&mut self, width: u16, height: u16) {
        if !self.dirty && self.last_width == width && self.last_height == height {
            return;
        }

        let anchor = self
            .rendered
            .lines
            .get(self.vertical_scroll)
            .and_then(|line| line.source_id);

        if self.last_width != 0 || self.last_height != 0 {
            self.clear_selection();
        }

        self.rendered = self.document.render(
            Size::new(width, height),
            &self.theme,
            self.wrap,
            self.tab_width,
            &mut self.images,
            self.base_dir.as_deref(),
        );
        self.last_width = width;
        self.last_height = height;
        self.dirty = false;

        if let Some(id) = anchor
            && let Some(line) = self
                .rendered
                .lines
                .iter()
                .position(|line| line.source_id == Some(id))
        {
            self.vertical_scroll = line;
        }

        if self.recenter_image_after_render {
            self.vertical_scroll = self.max_vertical_scroll() / 2;
            self.horizontal_scroll = self.max_horizontal_scroll() / 2;
            self.recenter_image_after_render = false;
        }

        self.clamp_scroll();
    }

    fn focus_search_target(&mut self, target: &SearchMatch) {
        if let Some((page_index, rects)) = target.pdf_location() {
            let rect = rects.first().copied();
            let viewport = Size::new(self.viewport_width.max(1), self.viewport_height.max(1));
            if let ViewDocument::Pdf(document) = &self.document
                && let Some((top, left)) =
                    document.search_scroll_position(page_index, rect, viewport)
            {
                self.vertical_scroll = top;
                self.horizontal_scroll = left;
            }
            return;
        }

        let Some((target_node_id, target_occurrence)) = target.text_location() else {
            return;
        };
        let Some(matcher) = SearchMatcher::new(&self.search.query) else {
            return;
        };
        let mut occurrence = 0usize;
        let mut fallback = None;

        for (index, line) in self.rendered.lines.iter().enumerate() {
            if line.source_id != Some(target_node_id) {
                continue;
            }
            fallback.get_or_insert(index);
            for _ in matcher.ranges(&line.plain) {
                if occurrence == target_occurrence {
                    self.vertical_scroll = index.saturating_sub(self.viewport_height as usize / 4);
                    self.horizontal_scroll = 0;
                    return;
                }
                occurrence += 1;
            }
        }

        if let Some(index) = fallback {
            self.vertical_scroll = index.saturating_sub(self.viewport_height as usize / 4);
            self.horizontal_scroll = 0;
        }
    }

    fn display_lines(&self) -> Vec<Line<'static>> {
        let mut lines = if let Some(matcher) = SearchMatcher::new(&self.search.query) {
            let searchable_nodes = self
                .search
                .matches
                .iter()
                .filter_map(SearchMatch::text_location)
                .map(|(node_id, _)| node_id)
                .collect::<HashSet<_>>();
            let current = self
                .search
                .current_match()
                .and_then(SearchMatch::text_location);
            let mut occurrences = HashMap::<NodeId, usize>::new();

            self.rendered
                .lines
                .iter()
                .map(|line| {
                    let Some(node_id) = line.source_id else {
                        return line.line.clone();
                    };
                    if !searchable_nodes.contains(&node_id) {
                        return line.line.clone();
                    }
                    let ranges = matcher.ranges(&line.plain);
                    if ranges.is_empty() {
                        return line.line.clone();
                    }
                    let start_occurrence = *occurrences.get(&node_id).unwrap_or(&0);
                    occurrences.insert(node_id, start_occurrence + ranges.len());
                    let styled_ranges = ranges
                        .into_iter()
                        .enumerate()
                        .map(|(index, (start, end))| {
                            let occurrence = start_occurrence + index;
                            let is_current =
                                current.is_some_and(|(current_node, current_occurrence)| {
                                    current_node == node_id && current_occurrence == occurrence
                                });
                            (start, end, is_current)
                        })
                        .collect::<Vec<_>>();
                    highlight_line(
                        line,
                        &styled_ranges,
                        self.theme.search_match,
                        self.theme.search_current,
                    )
                })
                .collect::<Vec<_>>()
        } else {
            self.rendered
                .lines
                .iter()
                .map(|line| line.line.clone())
                .collect::<Vec<_>>()
        };

        if let Some(selection) = self.selection.filter(|selection| !selection.is_empty()) {
            let selection_style = Style::default().add_modifier(Modifier::REVERSED);
            for (index, line) in lines.iter_mut().enumerate() {
                let plain = &self.rendered.lines[index].plain;
                let Some((start, end)) = selection.range_for_line(index, plain.len()) else {
                    continue;
                };
                *line = patch_line_range(line, plain, start, end, selection_style);
            }
        }

        lines
    }

    fn clear_selection(&mut self) {
        self.selection = None;
        self.mouse_selection_anchor = None;
        self.mouse_selecting = false;
        self.mouse_dragged = false;
        self.mouse_drag_position = None;
    }

    fn copy_selection(&mut self) {
        let Some(selection) = self.selection.filter(|selection| !selection.is_empty()) else {
            return;
        };
        let text = selected_text(&self.rendered, selection);
        if text.is_empty() {
            return;
        }

        copy_to_clipboard(&text);
        self.clear_selection();
    }

    fn zoom_image_in(&mut self) {
        if self.document.zoom_image_in() {
            self.dirty = true;
            self.recenter_image_after_render = true;
        }
    }

    fn zoom_image_out(&mut self) {
        if self.document.zoom_image_out() {
            self.dirty = true;
            self.recenter_image_after_render = true;
        }
    }

    fn reset_image_zoom(&mut self) {
        if self.document.reset_image_zoom() {
            self.dirty = true;
            self.recenter_image_after_render = true;
        }
    }

    fn zoom_pdf_in(&mut self) {
        if let ViewDocument::Pdf(document) = &mut self.document {
            document.zoom_in();
        }
    }

    fn zoom_pdf_out(&mut self) {
        if let ViewDocument::Pdf(document) = &mut self.document {
            document.zoom_out();
        }
    }

    fn reset_pdf_zoom(&mut self) {
        if let ViewDocument::Pdf(document) = &mut self.document {
            document.reset_zoom();
        }
    }

    fn scroll_down(&mut self, amount: usize) {
        self.vertical_scroll = (self.vertical_scroll + amount).min(self.max_vertical_scroll());
    }

    fn scroll_up(&mut self, amount: usize) {
        self.vertical_scroll = self.vertical_scroll.saturating_sub(amount);
    }

    fn scroll_right(&mut self, amount: usize) {
        self.horizontal_scroll =
            (self.horizontal_scroll + amount).min(self.max_horizontal_scroll());
    }

    fn scroll_left(&mut self, amount: usize) {
        self.horizontal_scroll = self.horizontal_scroll.saturating_sub(amount);
    }

    fn scroll_help_down(&mut self, amount: usize) {
        self.help_scroll = self
            .help_scroll
            .saturating_add(amount)
            .min(self.help_max_scroll);
    }

    fn scroll_help_up(&mut self, amount: usize) {
        self.help_scroll = self.help_scroll.saturating_sub(amount);
    }

    fn half_page_height(&self) -> usize {
        usize::from(self.viewport_height).saturating_div(2).max(1)
    }

    fn page_height(&self) -> usize {
        self.viewport_height.saturating_sub(1).max(1) as usize
    }

    fn max_vertical_scroll(&self) -> usize {
        match &self.document {
            ViewDocument::Pdf(document) => document
                .layout()
                .total_height
                .saturating_sub(usize::from(self.viewport_height)),
            _ => self
                .rendered
                .lines
                .len()
                .saturating_sub(self.viewport_height as usize),
        }
    }

    fn max_horizontal_scroll(&self) -> usize {
        match &self.document {
            ViewDocument::Pdf(document) => document
                .layout()
                .total_width
                .saturating_sub(usize::from(self.viewport_width)),
            _ => self
                .rendered
                .max_width
                .saturating_sub(self.viewport_width as usize),
        }
    }

    fn clamp_scroll(&mut self) {
        self.vertical_scroll = self.vertical_scroll.min(self.max_vertical_scroll());
        self.horizontal_scroll = self.horizontal_scroll.min(self.max_horizontal_scroll());
    }
}

fn style_background_rgb(style: Style, fallback: (u8, u8, u8)) -> (u8, u8, u8) {
    match style.bg {
        Some(Color::Rgb(red, green, blue)) => (red, green, blue),
        Some(Color::Black) => (0, 0, 0),
        Some(Color::Red) => (255, 0, 0),
        Some(Color::Green) => (0, 255, 0),
        Some(Color::Yellow) => (255, 255, 0),
        Some(Color::Blue) => (0, 0, 255),
        Some(Color::Magenta) => (255, 0, 255),
        Some(Color::Cyan) => (0, 255, 255),
        Some(Color::Gray) => (128, 128, 128),
        Some(Color::DarkGray) => (64, 64, 64),
        Some(Color::LightRed) => (255, 128, 128),
        Some(Color::LightGreen) => (128, 255, 128),
        Some(Color::LightYellow) => (255, 255, 128),
        Some(Color::LightBlue) => (128, 128, 255),
        Some(Color::LightMagenta) => (255, 128, 255),
        Some(Color::LightCyan) => (128, 255, 255),
        Some(Color::White) => (255, 255, 255),
        _ => fallback,
    }
}

fn has_uri_scheme(value: &str) -> bool {
    let Some((scheme, _)) = value.split_once(':') else {
        return false;
    };
    !scheme.is_empty()
        && scheme
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
}

fn highlight_line(
    rendered: &RenderedLine,
    ranges: &[(usize, usize, bool)],
    match_style: ratatui::style::Style,
    current_style: ratatui::style::Style,
) -> Line<'static> {
    let span_len = rendered
        .line
        .spans
        .iter()
        .map(|span| span.content.as_ref().len())
        .sum::<usize>();
    if span_len != rendered.plain.len() {
        return rendered.line.clone();
    }

    let mut output = Vec::<Span<'static>>::new();
    let mut global_offset = 0usize;

    for span in &rendered.line.spans {
        let text = span.content.as_ref();
        let span_start = global_offset;
        let span_end = span_start + text.len();
        let mut cuts = vec![0usize, text.len()];

        for (start, end, _) in ranges {
            if *start < span_end && *end > span_start {
                cuts.push(start.saturating_sub(span_start).min(text.len()));
                cuts.push(end.saturating_sub(span_start).min(text.len()));
            }
        }
        cuts.sort_unstable();
        cuts.dedup();

        for pair in cuts.windows(2) {
            let local_start = pair[0];
            let local_end = pair[1];
            if local_start == local_end
                || !text.is_char_boundary(local_start)
                || !text.is_char_boundary(local_end)
            {
                continue;
            }
            let global_start = span_start + local_start;
            let global_end = span_start + local_end;
            let mut style = span.style;
            if let Some((_, _, is_current)) = ranges
                .iter()
                .find(|(start, end, _)| global_start >= *start && global_end <= *end)
            {
                style = style.patch(if *is_current {
                    current_style
                } else {
                    match_style
                });
            }
            output.push(Span::styled(
                text[local_start..local_end].to_string(),
                style,
            ));
        }
        global_offset = span_end;
    }

    let mut line = rendered.line.clone();
    line.spans = output;
    line
}

fn grapheme_bounds_at_column(text: &str, target_column: usize) -> (usize, usize) {
    let mut column = 0usize;

    for (byte, grapheme) in text.grapheme_indices(true) {
        let width = UnicodeWidthStr::width(grapheme).max(1);
        let next_column = column.saturating_add(width);
        if target_column < next_column {
            return (byte, byte + grapheme.len());
        }
        column = next_column;
    }

    (text.len(), text.len())
}

fn patch_line_range(
    line: &Line<'static>,
    plain: &str,
    start: usize,
    end: usize,
    patch: Style,
) -> Line<'static> {
    if start >= end || end > plain.len() {
        return line.clone();
    }

    let span_len = line
        .spans
        .iter()
        .map(|span| span.content.as_ref().len())
        .sum::<usize>();
    if span_len != plain.len() {
        return line.clone();
    }

    let mut output = Vec::<Span<'static>>::new();
    let mut global_offset = 0usize;

    for span in &line.spans {
        let text = span.content.as_ref();
        let span_start = global_offset;
        let span_end = span_start + text.len();
        let mut cuts = vec![0usize, text.len()];

        if start > span_start && start < span_end {
            cuts.push(start - span_start);
        }
        if end > span_start && end < span_end {
            cuts.push(end - span_start);
        }
        cuts.sort_unstable();
        cuts.dedup();

        for pair in cuts.windows(2) {
            let local_start = pair[0];
            let local_end = pair[1];
            if local_start == local_end
                || !text.is_char_boundary(local_start)
                || !text.is_char_boundary(local_end)
            {
                continue;
            }

            let global_start = span_start + local_start;
            let global_end = span_start + local_end;
            let style = if global_start < end && global_end > start {
                span.style.patch(patch)
            } else {
                span.style
            };
            output.push(Span::styled(
                text[local_start..local_end].to_string(),
                style,
            ));
        }

        global_offset = span_end;
    }

    let mut output_line = line.clone();
    output_line.spans = output;
    output_line
}

fn selected_text(rendered: &RenderedDocument, selection: Selection) -> String {
    let (start, end) = selection.normalized();
    if start == end || start.line >= rendered.lines.len() {
        return String::new();
    }

    let end_line = end.line.min(rendered.lines.len().saturating_sub(1));
    let mut output = String::new();

    for line_index in start.line..=end_line {
        let line = &rendered.lines[line_index].plain;
        let from = if line_index == start.line {
            start.byte.min(line.len())
        } else {
            0
        };
        let to = if line_index == end.line {
            end.byte.min(line.len())
        } else {
            line.len()
        };

        if from <= to && line.is_char_boundary(from) && line.is_char_boundary(to) {
            output.push_str(&line[from..to]);
        }

        if line_index < end_line {
            output.push('\n');
        }
    }

    output
}

fn copy_to_clipboard(text: &str) {
    if copy_with_local_command(text) {
        return;
    }

    let encoded = base64::engine::general_purpose::STANDARD.encode(text);
    let mut stdout = io::stdout().lock();
    let _ = write!(stdout, "\x1b]52;c;{encoded}\x07");
    let _ = stdout.flush();
}

fn copy_with_local_command(text: &str) -> bool {
    const COMMANDS: [(&str, &[&str]); 5] = [
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
        ("xsel", &["--clipboard", "--input"]),
        ("pbcopy", &[]),
        ("clip.exe", &[]),
    ];

    COMMANDS
        .iter()
        .any(|(program, args)| run_clipboard_command(program, args, text))
}

fn run_clipboard_command(program: &str, args: &[&str], text: &str) -> bool {
    let Ok(mut child) = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };

    let wrote = child
        .stdin
        .take()
        .is_some_and(|mut stdin| stdin.write_all(text.as_bytes()).is_ok());
    let succeeded = child.wait().is_ok_and(|status| status.success());
    wrote && succeeded
}

struct GraphicsCommand(String);

struct PreservingGraphicsCommand(String);

impl Widget for PreservingGraphicsCommand {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let Some(cell) = buf.cell_mut((area.x, area.y)) else {
            return;
        };

        let visible_symbol = cell.symbol().to_string();
        let mut symbol = self.0;
        symbol.push_str(&visible_symbol);
        cell.set_symbol(&symbol)
            .set_diff_option(CellDiffOption::ForcedWidth(NonZeroU16::MIN));
    }
}

impl Widget for GraphicsCommand {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let Some(cell) = buf.cell_mut((area.x, area.y)) else {
            return;
        };

        let mut symbol = self.0;
        symbol.push(' ');
        cell.set_symbol(&symbol)
            .set_diff_option(CellDiffOption::ForcedWidth(NonZeroU16::MIN));
    }
}

struct MouseCaptureGuard;

impl MouseCaptureGuard {
    fn enable() -> io::Result<Self> {
        execute!(io::stdout(), EnableMouseCapture)?;
        Ok(Self)
    }
}

impl Drop for MouseCaptureGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), DisableMouseCapture);
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;

    #[test]
    fn grapheme_bounds_follow_terminal_columns() {
        let text = "a你🙂";

        assert_eq!(grapheme_bounds_at_column(text, 0), (0, 1));
        assert_eq!(grapheme_bounds_at_column(text, 1), (1, 4));
        assert_eq!(grapheme_bounds_at_column(text, 2), (1, 4));
        assert_eq!(grapheme_bounds_at_column(text, 3), (4, 8));
        assert_eq!(grapheme_bounds_at_column(text, 4), (4, 8));
        assert_eq!(grapheme_bounds_at_column(text, 5), (8, 8));
    }

    #[test]
    fn copies_rendered_text_across_lines() {
        let rendered = RenderedDocument {
            lines: vec![
                RenderedLine {
                    line: Line::from("Hello, world"),
                    plain: "Hello, world".to_string(),
                    source_id: None,
                },
                RenderedLine {
                    line: Line::from("Second line"),
                    plain: "Second line".to_string(),
                    source_id: None,
                },
            ],
            ..RenderedDocument::default()
        };
        let selection = Selection::new(
            DocumentPosition { line: 0, byte: 7 },
            DocumentPosition { line: 1, byte: 6 },
        );

        assert_eq!(selected_text(&rendered, selection), "world\nSecond");
    }
}
