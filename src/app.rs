use std::{
    collections::{HashMap, HashSet},
    io,
    path::{Path, PathBuf},
};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span, Text},
    widgets::{Block, Padding, Paragraph},
};
use ratatui_image::Image as TerminalImage;

use crate::{
    image::ImageManager,
    input::InputKind,
    renderer::{NodeId, RenderedDocument, RenderedLine, ViewDocument},
    search::{SearchMatch, SearchMatcher, SearchState},
    theme::Theme,
    ui,
};

const MOUSE_VERTICAL_SCROLL: usize = 3;
const MOUSE_HORIZONTAL_SCROLL: usize = 4;

#[derive(Debug, Clone, Copy)]
enum PendingPrefix {
    Previous,
    Next,
}

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
    viewport_area: Rect,
    viewport_width: u16,
    viewport_height: u16,

    vertical_scroll: usize,
    horizontal_scroll: usize,
    wrap: bool,
    tab_width: usize,

    show_help: bool,
    pending_prefix: Option<PendingPrefix>,
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
            viewport_area: Rect::default(),
            viewport_width: 1,
            viewport_height: 1,
            vertical_scroll: 0,
            horizontal_scroll: 0,
            wrap,
            tab_width,
            show_help: false,
            pending_prefix: None,
        }
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        self.images.initialize();
        let _mouse = MouseCaptureGuard::enable()?;

        loop {
            terminal.draw(|frame| self.draw(frame))?;

            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if self.handle_key(key) {
                        break;
                    }
                }
                Event::Mouse(mouse) => self.handle_mouse(mouse),
                Event::Resize(_, _) => self.dirty = true,
                _ => {}
            }
        }
        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        let [content_area, status_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());

        let block = Block::bordered()
            .title(format!(" {} ", self.name))
            .border_style(self.theme.status_accent)
            .padding(Padding::uniform(1));
        self.viewport_area = block.inner(content_area);
        self.viewport_width = self.viewport_area.width.max(1);
        self.viewport_height = self.viewport_area.height.max(1);

        self.ensure_rendered(self.viewport_width);
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

        let status_line = if self.search.active {
            ui::search::prompt(&self.search, &self.theme)
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
                },
                &self.theme,
            )
        };
        frame.render_widget(
            Paragraph::new(status_line).style(self.theme.status),
            status_area,
        );

        if self.show_help {
            ui::help::render(frame, &self.theme, self.document.is_markdown());
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if self.show_help {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?') => self.show_help = false,
                KeyCode::Char('q') => return true,
                _ => {}
            }
            return false;
        }

        if self.search.active {
            self.handle_search_key(key);
            return false;
        }

        if let Some(prefix) = self.pending_prefix.take() {
            self.handle_prefixed_key(prefix, key);
            return false;
        }

        match key.code {
            KeyCode::Char('q') => return true,
            KeyCode::Esc => self.clear_search(),
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('/') => self.open_search(),
            KeyCode::Char('n') => self.next_match(),
            KeyCode::Char('N') => self.previous_match(),
            KeyCode::Down | KeyCode::Char('j') => self.scroll_down(1),
            KeyCode::Up | KeyCode::Char('k') => self.scroll_up(1),
            KeyCode::Right | KeyCode::Char('l') => self.scroll_right(1),
            KeyCode::Left | KeyCode::Char('h') => self.scroll_left(1),
            KeyCode::PageDown => self.scroll_down(self.page_height()),
            KeyCode::PageUp => self.scroll_up(self.page_height()),
            KeyCode::Home | KeyCode::Char('g') => self.vertical_scroll = 0,
            KeyCode::End | KeyCode::Char('G') => self.vertical_scroll = self.max_vertical_scroll(),
            KeyCode::Char('w') => {
                self.wrap = !self.wrap;
                self.dirty = true;
            }
            KeyCode::Char('[') if self.document.is_markdown() => {
                self.pending_prefix = Some(PendingPrefix::Previous)
            }
            KeyCode::Char(']') if self.document.is_markdown() => {
                self.pending_prefix = Some(PendingPrefix::Next)
            }
            _ => {}
        }
        false
    }

    fn handle_prefixed_key(&mut self, prefix: PendingPrefix, key: KeyEvent) {
        match (prefix, key.code) {
            (PendingPrefix::Previous, KeyCode::Char('h')) => self.previous_heading(),
            (PendingPrefix::Next, KeyCode::Char('h')) => self.next_heading(),
            _ => {}
        }
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
            MouseEventKind::Down(MouseButton::Left) => {
                self.open_link_at(mouse.column, mouse.row);
            }
            _ => {}
        }
    }

    fn open_search(&mut self) {
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

        if let Some(destination) = self
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
        if self.horizontal_scroll != 0 || self.rendered.images.is_empty() {
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

    fn ensure_rendered(&mut self, width: u16) {
        if !self.dirty && self.last_width == width {
            return;
        }

        let anchor = self
            .rendered
            .lines
            .get(self.vertical_scroll)
            .and_then(|line| line.source_id);

        self.rendered = self.document.render(
            width,
            &self.theme,
            self.wrap,
            self.tab_width,
            &mut self.images,
            self.base_dir.as_deref(),
        );
        self.last_width = width;
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

        self.clamp_scroll();
    }

    fn focus_search_target(&mut self, target: &SearchMatch) {
        let Some(matcher) = SearchMatcher::new(&self.search.query) else {
            return;
        };
        let mut occurrence = 0usize;
        let mut fallback = None;

        for (index, line) in self.rendered.lines.iter().enumerate() {
            if line.source_id != Some(target.node_id) {
                continue;
            }
            fallback.get_or_insert(index);
            for _ in matcher.ranges(&line.plain) {
                if occurrence == target.occurrence {
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
        let Some(matcher) = SearchMatcher::new(&self.search.query) else {
            return self
                .rendered
                .lines
                .iter()
                .map(|line| line.line.clone())
                .collect();
        };
        let searchable_nodes = self
            .search
            .matches
            .iter()
            .map(|match_| match_.node_id)
            .collect::<HashSet<_>>();
        let current = self.search.current_match();
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
                        let is_current = current.is_some_and(|match_| {
                            match_.node_id == node_id && match_.occurrence == occurrence
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
            .collect()
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

    fn page_height(&self) -> usize {
        self.viewport_height.saturating_sub(1).max(1) as usize
    }

    fn max_vertical_scroll(&self) -> usize {
        self.rendered
            .lines
            .len()
            .saturating_sub(self.viewport_height as usize)
    }

    fn max_horizontal_scroll(&self) -> usize {
        self.rendered
            .max_width
            .saturating_sub(self.viewport_width as usize)
    }

    fn clamp_scroll(&mut self) {
        self.vertical_scroll = self.vertical_scroll.min(self.max_vertical_scroll());
        self.horizontal_scroll = self.horizontal_scroll.min(self.max_horizontal_scroll());
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
