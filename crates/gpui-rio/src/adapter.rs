/// Defines a terminal entity using the caller's GPUI crate or module.
/// Invoke once in a shared module: `gpui_rio::bind_gpui!(gpui);`.
#[macro_export]
macro_rules! bind_gpui {
    ($gpui:path) => {
        $crate::__bind_gpui!($gpui,);
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __bind_gpui {
    ($gpui:path, $($extra:item)*) => {
        pub use self::__gpui_rio_adapter::Terminal;
        use $gpui as __gpui_rio;
        mod __gpui_rio_adapter {
            use super::__gpui_rio as gpui;
            use gpui::*;
            use $crate::{TerminalOptions, TerminalStyle, TerminalTheme};
            use $crate::__private::{anyhow, rio_vt};
            use rio_vt::{
                ansi::CursorShape,
                config::colors::{AnsiColor, ColorRgb},
                corcovado::channel::Sender,
                crosswords::{
                    Crosswords, CrosswordsSize, Mode,
                    grid::Scroll,
                    pos::{Column, Line, Pos, Side},
                    square::Wide,
                    style::StyleFlags,
                },
                event::{EventListener, Msg, RioEvent, WindowId, WindowSize, sync::FairMutex},
                performer::Machine,
                selection::{Selection, SelectionType},
            };
            use std::{
                ops::Range,
                sync::{
                    Arc,
                    atomic::{AtomicBool, Ordering},
                    mpsc,
                },
                time::Duration,
            };

            #[derive(Clone)]
            struct Listener {
                dirty: Arc<AtomicBool>,
                alive: Arc<AtomicBool>,
                events: mpsc::Sender<RioEvent>,
            }

            impl EventListener for Listener {
                fn send_event(&self, event: RioEvent, _: WindowId) {
                    match event {
                        RioEvent::CloseTerminal(_) | RioEvent::ChildExited(_, _) => {
                            self.alive.store(false, Ordering::Release);
                        }
                        RioEvent::PtyWrite(..)
                        | RioEvent::ColorRequest(..)
                        | RioEvent::TextAreaSizeRequest(..) => {
                            let _ = self.events.send(event);
                        }
                        _ => {}
                    }
                    self.dirty.store(true, Ordering::Release);
                }

                fn send_event_with_high_priority(&self, event: RioEvent, id: WindowId) {
                    self.send_event(event, id);
                }
                fn send_redraw(&self, _: WindowId) {
                    self.dirty.store(true, Ordering::Release);
                }
            }

            pub struct Terminal {
                grid: Arc<FairMutex<Crosswords<Listener>>>,
                sender: Sender<Msg>,
                alive: Arc<AtomicBool>,
                focus: FocusHandle,
                style: TerminalStyle,
                visible: bool,
                bounds: Bounds<Pixels>,
                cell: Size<Pixels>,
                selecting: bool,
                scroll_delta: f32,
                marked: String,
                _poll: Task<()>,
            }

            impl Drop for Terminal {
                fn drop(&mut self) {
                    // The PTY owner terminates and reaps the shell, away from the UI thread.
                    let _ = self.sender.send(Msg::Shutdown);
                }
            }

            impl Terminal {
                pub fn spawn(
                    options: TerminalOptions,
                    cx: &mut App,
                ) -> anyhow::Result<Entity<Self>> {
                    let TerminalOptions { command, working_directory, style } = options;
                    let (events, receiver) = mpsc::channel();
                    let listener = Listener {
                        dirty: Arc::new(AtomicBool::new(true)),
                        alive: Arc::new(AtomicBool::new(true)),
                        events,
                    };
                    let id = WindowId::from(0);
                    let grid = Arc::new(FairMutex::new(Crosswords::new(
                        CrosswordsSize::new(80, 24),
                        CursorShape::Block,
                        listener.clone(),
                        id,
                        0,
                        10_000,
                    )));
                    // Use xterm's installed terminfo rather than requiring Rio's custom entry.
                    let pty = rio_vt::teletypewriter::create_pty_with_spawn(
                        Some(&command),
                        vec![],
                        &Some(
                            working_directory
                                .to_str()
                                .ok_or_else(|| anyhow::anyhow!("Terminal directory is not valid UTF-8"))?
                                .into(),
                        ),
                        Some(vec![
                            ("TERM".into(), "xterm-256color".into()),
                            ("COLORTERM".into(), "truecolor".into()),
                        ]),
                        80,
                        24,
                        0,
                        0,
                    )?;
                    let machine = Machine::new(grid.clone(), pty, listener.clone(), id, 0)
                        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                    let sender = machine.channel();
                    let alive = listener.alive.clone();
                    // Drop the machine when the worker ends so its PTY descriptors are released.
                    drop(machine.spawn());
                    Ok(cx.new(|cx: &mut Context<Self>| {
                        let poll = cx.spawn(async move |this, cx| {
                            loop {
                                cx.background_executor()
                                    .timer(Duration::from_millis(16))
                                    .await;
                                if !this
                                    .update(cx, |this, cx| {
                                        for event in receiver.try_iter() {
                                            let reply = match event {
                                                RioEvent::PtyWrite(_, text) => text,
                                                RioEvent::ColorRequest(_, index, format) => {
                                                    format(this.resolve_index(index))
                                                }
                                                RioEvent::TextAreaSizeRequest(_, format) => {
                                                    let grid = this.grid.lock();
                                                    format(WindowSize {
                                                        rows: grid.screen_lines() as u16,
                                                        cols: grid.columns() as u16,
                                                        width: f32::from(this.bounds.size.width) as u16,
                                                        height: f32::from(this.bounds.size.height) as u16,
                                                    })
                                                }
                                                _ => continue,
                                            };
                                            this.write(reply.into_bytes());
                                        }
                                        if listener.dirty.swap(false, Ordering::AcqRel) && this.visible {
                                            cx.notify();
                                        }
                                        this.is_alive()
                                    })
                                    .unwrap_or(false)
                                {
                                    break;
                                }
                            }
                        });
                        Self {
                            grid,
                            sender,
                            alive,
                            focus: cx.focus_handle(),
                            style,
                            visible: true,
                            bounds: Bounds::default(),
                            cell: size(px(8.), px(18.)),
                            selecting: false,
                            scroll_delta: 0.,
                            marked: String::new(),
                            _poll: poll,
                        }
                    }))
                }

                pub fn focus(&self, window: &mut Window, cx: &mut App) {
                    window.focus(&self.focus, cx);
                }
                pub fn is_alive(&self) -> bool {
                    self.alive.load(Ordering::Acquire)
                }
                pub fn set_visible(&mut self, visible: bool) {
                    self.visible = visible;
                }
                /// Applies colors and font settings without restarting the command.
                /// The host should notify its view after changing the style.
                pub fn update_style(&mut self, style: TerminalStyle) {
                    self.style = style;
                }

                fn write(&self, bytes: Vec<u8>) {
                    let _ = self.sender.send(Msg::Input(bytes.into()));
                }

                fn resolve_index(&self, index: usize) -> ColorRgb {
                    let grid = self.grid.lock();
                    resolve_index(&grid, self.style.theme, index)
                }

                fn input(&mut self, text: &str, paste: bool, cx: &mut Context<Self>) {
                    let mut grid = self.grid.lock();
                    grid.scroll_display(Scroll::Bottom);
                    grid.selection = None;
                    let bracketed = paste && grid.mode().contains(Mode::BRACKETED_PASTE);
                    drop(grid);
                    let text = if paste {
                        text.replace('\x1b', "")
                            .replace("\r\n", "\r")
                            .replace('\n', "\r")
                    } else {
                        text.into()
                    };
                    self.write(if bracketed {
                        format!("\x1b[200~{text}\x1b[201~").into_bytes()
                    } else {
                        text.into_bytes()
                    });
                    cx.notify();
                }

                fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
                    let key = &event.keystroke;
                    let clipboard = if cfg!(target_os = "macos") {
                        key.modifiers.platform
                    } else {
                        key.modifiers.control && key.modifiers.shift
                    };
                    if clipboard && key.key == "v" {
                        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                            self.input(&text, true, cx);
                        }
                        cx.stop_propagation();
                        return;
                    }
                    if clipboard && key.key == "c" {
                        if let Some(text) = self.grid.lock().selection_to_string() {
                            cx.write_to_clipboard(ClipboardItem::new_string(text));
                        }
                        cx.stop_propagation();
                        return;
                    }
                    if key.modifiers.platform {
                        return;
                    }
                    let mode = self.grid.lock().mode();
                    if let Some(bytes) = key_bytes(key, mode) {
                        self.grid.lock().scroll_display(Scroll::Bottom);
                        self.write(bytes);
                        cx.stop_propagation();
                        cx.notify();
                    }
                }

                fn position(&self, point: Point<Pixels>) -> Pos {
                    let grid = self.grid.lock();
                    let row =
                        (f32::from(point.y - self.bounds.top()) / f32::from(self.cell.height)).floor() as i32;
                    let col =
                        (f32::from(point.x - self.bounds.left()) / f32::from(self.cell.width)).floor() as usize;
                    Pos::new(
                        Line(row.clamp(0, grid.screen_lines() as i32 - 1) - grid.display_offset() as i32),
                        Column(col.min(grid.columns() - 1)),
                    )
                }

                fn paint(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
                    self.bounds = bounds;
                    let font_size = px(if self.style.font_size.is_finite() {
                        self.style.font_size.clamp(1., 256.)
                    } else { 13. });
                    let font = font(terminal_font_family(self.style.font_family.clone().into(), cx));
                    let run = TextRun {
                        len: 1,
                        font: font.clone(),
                        color: rgb_color(self.style.theme.foreground),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let measure = window
                        .text_system()
                        .shape_line("M".into(), font_size, &[run], None);
                    self.cell = size(measure.width.max(px(1.)), (font_size * 1.35).ceil());
                    let cols = (f32::from(bounds.size.width) / f32::from(self.cell.width))
                        .floor()
                        .clamp(2., 1000.) as usize;
                    let rows = (f32::from(bounds.size.height) / f32::from(self.cell.height))
                        .floor()
                        .clamp(1., 1000.) as usize;
                    let mut grid = self.grid.lock();
                    if cols != grid.columns() || rows != grid.screen_lines() {
                        grid.resize(CrosswordsSize::new(cols, rows));
                        let _ = self.sender.send(Msg::Resize(WindowSize {
                            rows: rows as u16,
                            cols: cols as u16,
                            width: f32::from(bounds.size.width) as u16,
                            height: f32::from(bounds.size.height) as u16,
                        }));
                    }
                    let selection = grid
                        .selection
                        .as_ref()
                        .and_then(|selection| selection.to_range(&*grid));
                    let cursor = grid.cursor();
                    let offset = grid.display_offset() as i32;
                    window.paint_quad(fill(bounds, rgb_color(self.style.theme.background)));
                    for row in 0..rows {
                        for col in 0..cols {
                            let pos = Pos::new(Line(row as i32 - offset), Column(col));
                            let square = grid.grid[pos];
                            if square.wide() == Wide::Spacer {
                                continue;
                            }
                            let style = grid.grid.style_of(&square);
                            let mut fg = resolve_color(&grid, self.style.theme, style.fg);
                            let mut bg = resolve_color(&grid, self.style.theme, style.bg);
                            if style.flags.contains(StyleFlags::INVERSE) {
                                std::mem::swap(&mut fg, &mut bg);
                            }
                            if style.flags.contains(StyleFlags::DIM) {
                                fg = fg * 0.66;
                            }
                            let selected = selection
                                .as_ref()
                                .is_some_and(|selection| selection.contains(pos));
                            let cursor_here = self.focus.is_focused(window)
                                && cursor.content != CursorShape::Hidden
                                && cursor.pos == Pos::new(Line(row as i32), Column(col))
                                && offset == 0;
                            if selected || (cursor_here && cursor.content == CursorShape::Block) {
                                std::mem::swap(&mut fg, &mut bg);
                            }
                            let origin = bounds.origin + point(self.cell.width * col, self.cell.height * row);
                            let width = if square.wide() == Wide::Wide {
                                self.cell.width * 2.
                            } else {
                                self.cell.width
                            };
                            // The canvas already has the default background. Painting each
                            // empty cell again bloats every frame, especially in a full pane.
                            if bg != self.style.theme.background {
                                window.paint_quad(fill(
                                    Bounds::new(origin, size(width, self.cell.height)),
                                    rgb_color(bg),
                                ));
                            }
                            let text: String = grid
                                .grid
                                .cell_text(pos)
                                .map(|c| if c == '\0' { ' ' } else { c })
                                .collect();
                            if (text != " "
                                || style
                                    .flags
                                    .intersects(StyleFlags::ALL_UNDERLINES | StyleFlags::STRIKEOUT))
                                && !style.flags.contains(StyleFlags::HIDDEN)
                            {
                                let mut font = font.clone();
                                if style.flags.contains(StyleFlags::BOLD) {
                                    font.weight = FontWeight::BOLD;
                                }
                                if style.flags.contains(StyleFlags::ITALIC) {
                                    font.style = FontStyle::Italic;
                                }
                                let run = TextRun {
                                    len: text.len(),
                                    font,
                                    color: rgb_color(fg),
                                    background_color: None,
                                    underline: style
                                        .flags
                                        .intersects(StyleFlags::ALL_UNDERLINES)
                                        .then_some(UnderlineStyle {
                                            color: Some(rgb_color(fg)),
                                            thickness: px(1.),
                                            wavy: style.flags.contains(StyleFlags::UNDERCURL),
                                        }),
                                    strikethrough: style.flags.contains(StyleFlags::STRIKEOUT).then_some(
                                        StrikethroughStyle {
                                            color: Some(rgb_color(fg)),
                                            thickness: px(1.),
                                        },
                                    ),
                                };
                                let line =
                                    window
                                        .text_system()
                                        .shape_line(text.into(), font_size, &[run], None);
                                let _ = line.paint(origin, self.cell.height, TextAlign::Left, None, window, cx);
                            }
                            if cursor_here
                                && matches!(cursor.content, CursorShape::Beam | CursorShape::Underline)
                            {
                                let rect = if cursor.content == CursorShape::Beam {
                                    Bounds::new(origin, size(px(2.), self.cell.height))
                                } else {
                                    Bounds::new(
                                        origin + point(px(0.), self.cell.height - px(2.)),
                                        size(width, px(2.)),
                                    )
                                };
                                window.paint_quad(fill(rect, rgb_color(self.style.theme.foreground)));
                            }
                        }
                    }
                    grid.reset_damage();
                    grid.damage_event_in_flight = false;
                    drop(grid);
                    if !self.marked.is_empty() && self.focus.is_focused(window) {
                        let text = self.marked.replace(['\n', '\r'], " ");
                        let run = TextRun {
                            len: text.len(),
                            font,
                            color: rgb_color(self.style.theme.foreground),
                            background_color: Some(rgb_color(self.style.theme.background)),
                            underline: Some(UnderlineStyle {
                                color: None,
                                thickness: px(1.),
                                wavy: false,
                            }),
                            strikethrough: None,
                        };
                        let line = window
                            .text_system()
                            .shape_line(text.into(), font_size, &[run], None);
                        let origin = bounds.origin
                            + point(
                                self.cell.width * cursor.pos.col.0,
                                self.cell.height * cursor.pos.row.0 as f32,
                            );
                        let _ = line.paint(origin, self.cell.height, TextAlign::Left, None, window, cx);
                    }
                }
            }

            // Generic aliases and missing theme fonts can silently become proportional
            // GPUI fallbacks. Only accept an installed family before trying known monospace fonts.
            fn terminal_font_family(family: SharedString, cx: &App) -> SharedString {
                static INSTALLED: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
                let installed = INSTALLED.get_or_init(|| cx.text_system().all_font_names());
                if !family.eq_ignore_ascii_case("monospace")
                    && family != ".SystemUIFont"
                    && installed.iter().any(|name| name == family.as_ref())
                {
                    return family;
                }
                static MONO: std::sync::OnceLock<SharedString> = std::sync::OnceLock::new();
                MONO.get_or_init(|| {
                    [
                        "Menlo",
                        "DejaVu Sans Mono",
                        "Liberation Mono",
                        "Noto Sans Mono",
                        "Ubuntu Mono",
                        "Monaco",
                        "Courier New",
                    ]
                    .into_iter()
                    .find(|candidate| installed.iter().any(|name| name == candidate))
                    .map(SharedString::from)
                    .unwrap_or(family)
                })
                .clone()
            }

            fn rgb_color(color: ColorRgb) -> Hsla {
                rgb((u32::from(color.r) << 16) | (u32::from(color.g) << 8) | u32::from(color.b)).into()
            }

            fn resolve_index(grid: &Crosswords<Listener>, theme: TerminalTheme, index: usize) -> ColorRgb {
                if index < 269
                    && let Some(color) = grid.colors()[index]
                {
                    ColorRgb::from_color_arr(color)
                } else {
                    theme.color(index)
                }
            }

            fn resolve_color(grid: &Crosswords<Listener>, theme: TerminalTheme, color: AnsiColor) -> ColorRgb {
                match color {
                    AnsiColor::Spec(color) => color,
                    AnsiColor::Indexed(index) => resolve_index(grid, theme, index as usize),
                    AnsiColor::Named(color) => resolve_index(grid, theme, color as usize),
                }
            }

            fn key_bytes(key: &Keystroke, mode: Mode) -> Option<Vec<u8>> {
                let m = key.modifiers;
                let modifier = 1 + usize::from(m.shift) + 2 * usize::from(m.alt) + 4 * usize::from(m.control);
                let cursor = match key.key.as_str() {
                    "up" => Some('A'),
                    "down" => Some('B'),
                    "right" => Some('C'),
                    "left" => Some('D'),
                    "home" => Some('H'),
                    "end" => Some('F'),
                    _ => None,
                };
                if let Some(final_char) = cursor {
                    return Some(
                        if modifier > 1 {
                            format!("\x1b[1;{modifier}{final_char}")
                        } else if mode.contains(Mode::APP_CURSOR) {
                            format!("\x1bO{final_char}")
                        } else {
                            format!("\x1b[{final_char}")
                        }
                        .into_bytes(),
                    );
                }
                let numbered = match key.key.as_str() {
                    "insert" => Some(2),
                    "delete" => Some(3),
                    "pageup" => Some(5),
                    "pagedown" => Some(6),
                    "f5" => Some(15),
                    "f6" => Some(17),
                    "f7" => Some(18),
                    "f8" => Some(19),
                    "f9" => Some(20),
                    "f10" => Some(21),
                    "f11" => Some(23),
                    "f12" => Some(24),
                    _ => None,
                };
                if let Some(n) = numbered {
                    return Some(
                        if modifier > 1 {
                            format!("\x1b[{n};{modifier}~")
                        } else {
                            format!("\x1b[{n}~")
                        }
                        .into_bytes(),
                    );
                }
                if let Some(c) = match key.key.as_str() {
                    "f1" => Some('P'),
                    "f2" => Some('Q'),
                    "f3" => Some('R'),
                    "f4" => Some('S'),
                    _ => None,
                } {
                    return Some(
                        if modifier > 1 {
                            format!("\x1b[1;{modifier}{c}")
                        } else {
                            format!("\x1bO{c}")
                        }
                        .into_bytes(),
                    );
                }
                let text = match key.key.as_str() {
                    "enter" => "\r",
                    "backspace" if m.alt => "\x1b\x7f",
                    "backspace" => "\x7f",
                    "escape" => "\x1b",
                    "tab" if m.shift => "\x1b[Z",
                    "tab" => "\t",
                    _ if m.control => {
                        let c = match key.key.as_str() {
                            "space" | "@" | "2" => 0,
                            "[" => 27,
                            "\\" => 28,
                            "]" => 29,
                            "^" | "6" => 30,
                            "_" | "-" => 31,
                            k if k.len() == 1 && k.as_bytes()[0].is_ascii_alphabetic() => {
                                k.as_bytes()[0].to_ascii_uppercase() & 0x1f
                            }
                            _ => return None,
                        };
                        return Some(if m.alt { vec![27, c] } else { vec![c] });
                    }
                    _ if m.alt => {
                        return key
                            .key_char
                            .as_ref()
                            .map(|text| format!("\x1b{text}").into_bytes());
                    }
                    _ => return None,
                };
                Some(text.as_bytes().to_vec())
            }

            impl Render for Terminal {
                fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                    let entity = cx.entity();
                    let paint_entity = entity.clone();
                    div()
                        .id("rio-terminal")
                        .size_full()
                        .overflow_hidden()
                        .track_focus(&self.focus)
                        .on_key_down(cx.listener(Self::key_down))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, event: &MouseDownEvent, window, cx| {
                                this.focus(window, cx);
                                let pos = this.position(event.position);
                                this.grid.lock().selection =
                                    Some(Selection::new(SelectionType::Simple, pos, Side::Left));
                                this.selecting = true;
                                cx.notify();
                            }),
                        )
                        .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                            if this.selecting && event.pressed_button == Some(MouseButton::Left) {
                                let pos = this.position(event.position);
                                if let Some(selection) = &mut this.grid.lock().selection {
                                    selection.update(pos, Side::Right);
                                }
                                cx.notify();
                            } else {
                                this.selecting = false;
                            }
                        }))
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|this, _, _, _| this.selecting = false),
                        )
                        .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                            let delta = event.delta.pixel_delta(this.cell.height).y;
                            this.scroll_delta += f32::from(delta) / f32::from(this.cell.height);
                            let lines = this.scroll_delta.trunc() as i32;
                            this.scroll_delta -= lines as f32;
                            this.grid.lock().scroll_display(Scroll::Delta(lines));
                            cx.stop_propagation();
                            cx.notify();
                        }))
                        .child(
                            canvas(
                                |_, _, _| {},
                                move |bounds, _, window, cx| {
                                    let focus = paint_entity.read(cx).focus.clone();
                                    window.handle_input(
                                        &focus,
                                        ElementInputHandler::new(bounds, entity.clone()),
                                        cx,
                                    );
                                    paint_entity.update(cx, |this, cx| this.paint(bounds, window, cx));
                                },
                            )
                            .size_full(),
                        )
                }
            }

            impl EntityInputHandler for Terminal {
                fn text_for_range(
                    &mut self,
                    range: Range<usize>,
                    actual: &mut Option<Range<usize>>,
                    _: &mut Window,
                    _: &mut Context<Self>,
                ) -> Option<String> {
                    let utf16: Vec<_> = self.marked.encode_utf16().collect();
                    let slice = utf16.get(range.clone())?;
                    *actual = Some(range);
                    Some(String::from_utf16_lossy(slice))
                }
                fn selected_text_range(
                    &mut self,
                    _: bool,
                    _: &mut Window,
                    _: &mut Context<Self>,
                ) -> Option<UTF16Selection> {
                    let end = self.marked.encode_utf16().count();
                    Some(UTF16Selection {
                        range: end..end,
                        reversed: false,
                    })
                }
                fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
                    (!self.marked.is_empty()).then(|| 0..self.marked.encode_utf16().count())
                }
                fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
                    self.marked.clear();
                    cx.notify();
                }
                fn replace_text_in_range(
                    &mut self,
                    _: Option<Range<usize>>,
                    text: &str,
                    _: &mut Window,
                    cx: &mut Context<Self>,
                ) {
                    self.marked.clear();
                    self.input(text, false, cx);
                }
                fn replace_and_mark_text_in_range(
                    &mut self,
                    _: Option<Range<usize>>,
                    text: &str,
                    _: Option<Range<usize>>,
                    _: &mut Window,
                    cx: &mut Context<Self>,
                ) {
                    self.marked = text.into();
                    cx.notify();
                }
                fn bounds_for_range(
                    &mut self,
                    _: Range<usize>,
                    _: Bounds<Pixels>,
                    _: &mut Window,
                    _: &mut Context<Self>,
                ) -> Option<Bounds<Pixels>> {
                    let cursor = self.grid.lock().cursor();
                    Some(Bounds::new(
                        self.bounds.origin
                            + point(
                                self.cell.width * cursor.pos.col.0,
                                self.cell.height * cursor.pos.row.0 as f32,
                            ),
                        self.cell,
                    ))
                }
                fn character_index_for_point(
                    &mut self,
                    _: Point<Pixels>,
                    _: &mut Window,
                    _: &mut Context<Self>,
                ) -> Option<usize> {
                    None
                }
            }

            $($extra)*
        }
    };
}
