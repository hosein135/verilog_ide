//! In-IDE waveform viewer powered by [wellen](https://github.com/ekiwi/wellen),
//! the VCD/FST/GHW engine from [Surfer](https://gitlab.com/surfer-project/surfer).
//!
//! Wellen is compiled with the rest of the crate (same pattern as xezim). Surfer's
//! own GUI is egui, so traces are drawn here with iced / tiny-skia.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text;
use iced::advanced::text::Renderer as TextRenderer;
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::widget::Widget;
use iced::advanced::{Clipboard, Renderer as QuadRenderer, Shell};
use iced::alignment;
use iced::event;
use iced::mouse;
use iced::{
    Background, Border, Color, Element, Event, Font, Length, Pixels, Point, Rectangle, Shadow, Size,
};
use std::path::{Path, PathBuf};

pub const NAME_COL_WIDTH: f32 = 200.0;
pub const RULER_HEIGHT: f32 = 26.0;
pub const TRACE_HEIGHT: f32 = 28.0;
const TRACE_PAD: f32 = 5.0;
const LINE: f32 = 2.0;

const BG: Color = Color::from_rgb(0.99, 0.992, 0.996);
const BG_NAME: Color = Color::from_rgb(0.955, 0.965, 0.975);
const GRID: Color = Color::from_rgb(0.82, 0.86, 0.90);
const FG: Color = Color::from_rgb(0.13, 0.16, 0.20);
const FG_MUTED: Color = Color::from_rgb(0.36, 0.42, 0.48);
const HIGH: Color = Color::from_rgb(0.08, 0.52, 0.32);
const LOW: Color = Color::from_rgb(0.16, 0.36, 0.74);
const UNK: Color = Color::from_rgb(0.75, 0.40, 0.08);
const BUS: Color = Color::from_rgb(0.05, 0.45, 0.55);
const CURSOR: Color = Color::from_rgb(0.80, 0.18, 0.18);
const SELECT: Color = Color::from_rgb(0.91, 0.94, 0.96);

/// How the IDE should open waveform files (`.vcd` / `.fst` / `.ghw`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ViewMode {
    /// Show the file in the text editor.
    TextEditor,
    /// Draw digital traces (Surfer / wellen).
    #[default]
    Waveform,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Level {
    High,
    Low,
    Unknown,
    Bus,
}

#[derive(Clone, Debug)]
struct Change {
    time: u64,
    label: String,
    level: Level,
}

#[derive(Clone, Debug)]
struct Trace {
    name: String,
    width: u32,
    changes: Vec<Change>,
}

#[derive(Clone, Debug)]
pub struct WaveView {
    timescale: String,
    t_min: u64,
    t_max: u64,
    traces: Vec<Trace>,
    view_start: f64,
    view_end: f64,
    cursor: Option<f64>,
}

#[derive(Debug, Clone)]
pub enum WaveNav {
    Zoom { factor: f32, anchor_time: f64 },
    Pan { dt: f64 },
    SetCursor { time: f64 },
}

impl WaveView {
    pub fn trace_count(&self) -> usize {
        self.traces.len()
    }

    pub fn content_height(&self) -> f32 {
        RULER_HEIGHT + self.traces.len().max(1) as f32 * TRACE_HEIGHT
    }

    pub fn time_span_label(&self) -> String {
        format!(
            "{}  ·  {} … {}  ·  {} signal{}",
            self.timescale,
            format_ticks(self.t_min, &self.timescale),
            format_ticks(self.t_max, &self.timescale),
            self.traces.len(),
            if self.traces.len() == 1 { "" } else { "s" },
        )
    }

    pub fn cursor_label(&self) -> Option<String> {
        self.cursor
            .map(|t| format!("cursor {}", format_ticks(t.round() as u64, &self.timescale)))
    }

    pub fn apply_nav(&mut self, nav: WaveNav) {
        match nav {
            WaveNav::Zoom {
                factor,
                anchor_time,
            } => self.zoom(factor, anchor_time),
            WaveNav::Pan { dt } => self.pan(dt),
            WaveNav::SetCursor { time } => {
                self.cursor = Some(time.clamp(self.t_min as f64, self.t_max.max(self.t_min) as f64));
            }
        }
    }

    fn span(&self) -> f64 {
        (self.view_end - self.view_start).max(1.0)
    }

    fn zoom(&mut self, factor: f32, anchor: f64) {
        let factor = factor.clamp(0.05, 20.0) as f64;
        let span = self.span();
        let new_span = (span * factor).clamp(1.0, ((self.t_max - self.t_min).max(1) as f64) * 4.0);
        let frac = ((anchor - self.view_start) / span).clamp(0.0, 1.0);
        let mut start = anchor - frac * new_span;
        let mut end = start + new_span;
        let min = self.t_min as f64;
        let max = self.t_max.max(self.t_min.saturating_add(1)) as f64;
        if start < min {
            end += min - start;
            start = min;
        }
        if end > max {
            start -= end - max;
            end = max;
            if start < min {
                start = min;
            }
        }
        self.view_start = start;
        self.view_end = end.max(start + 1.0);
    }

    fn pan(&mut self, dt: f64) {
        let span = self.span();
        let min = self.t_min as f64;
        let max = self.t_max.max(self.t_min.saturating_add(1)) as f64;
        let mut start = self.view_start + dt;
        let mut end = start + span;
        if start < min {
            end += min - start;
            start = min;
        }
        if end > max {
            start -= end - max;
            end = max;
            if start < min {
                start = min;
            }
        }
        self.view_start = start;
        self.view_end = end.max(start + 1.0);
    }
}

/// Load a VCD/FST/GHW with wellen and flatten it into draw-ready traces.
pub fn load_wave(path: &Path) -> Result<WaveView, String> {
    let mut waveform = wellen::simple::read(path).map_err(|e| {
        format!(
            "Surfer/wellen could not read {}: {e}",
            path.display()
        )
    })?;

    let var_refs: Vec<wellen::VarRef> = waveform.hierarchy().all_vars().collect();
    let mut meta = Vec::with_capacity(var_refs.len());
    let mut ids = Vec::with_capacity(var_refs.len());
    {
        let hier = waveform.hierarchy();
        for var_ref in var_refs {
            let var = &hier[var_ref];
            let id = var.signal_ref();
            ids.push(id);
            meta.push((
                var.full_name(hier),
                id,
                var.length(hier).unwrap_or(1),
                var.is_1bit(hier),
            ));
        }
    }

    waveform.load_signals(&ids);

    let time_table: Vec<u64> = waveform.time_table().to_vec();
    let t_min = time_table.first().copied().unwrap_or(0);
    let t_max = time_table.last().copied().unwrap_or(t_min);
    let timescale = format_timescale(waveform.hierarchy().timescale());

    let mut traces = Vec::with_capacity(meta.len());
    for (name, id, width, is_bit) in meta {
        let Some(signal) = waveform.get_signal(id) else {
            continue;
        };
        let mut changes = Vec::new();
        for (idx, value) in signal.iter_changes() {
            let time = time_table.get(idx as usize).copied().unwrap_or(t_min);
            changes.push(decode_value(value, width, is_bit, time));
        }
        if changes.is_empty() {
            changes.push(Change {
                time: t_min,
                label: "x".into(),
                level: if is_bit { Level::Unknown } else { Level::Bus },
            });
        }
        traces.push(Trace {
            name,
            width,
            changes,
        });
    }

    let view_end = if t_max > t_min {
        t_max as f64
    } else {
        (t_min as f64) + 1.0
    };

    Ok(WaveView {
        timescale,
        t_min,
        t_max,
        traces,
        view_start: t_min as f64,
        view_end,
        cursor: None,
    })
}

pub async fn load_wave_async(path: PathBuf) -> (PathBuf, Result<WaveView, String>) {
    let result = tokio::task::spawn_blocking({
        let path = path.clone();
        move || load_wave(&path)
    })
    .await
    .unwrap_or_else(|e| Err(format!("Waveform task failed: {e}")));
    (path, result)
}

fn decode_value(
    value: wellen::SignalValueRef<'_>,
    width: u32,
    is_bit: bool,
    time: u64,
) -> Change {
    match value {
        wellen::SignalValueRef::BitVec(bits) => {
            let raw = bits.bit_string();
            if is_bit || bits.width() <= 1 {
                let level = match raw.as_bytes().first().copied().unwrap_or(b'x') {
                    b'1' | b'h' | b'H' => Level::High,
                    b'0' | b'l' | b'L' => Level::Low,
                    _ => Level::Unknown,
                };
                Change {
                    time,
                    label: raw,
                    level,
                }
            } else {
                Change {
                    time,
                    label: bus_label(&raw, width),
                    level: Level::Bus,
                }
            }
        }
        wellen::SignalValueRef::Real(r) => Change {
            time,
            label: format!("{r}"),
            level: Level::Bus,
        },
        wellen::SignalValueRef::String(s) => Change {
            time,
            label: s.to_string(),
            level: Level::Bus,
        },
        wellen::SignalValueRef::Event => Change {
            time,
            label: "*".into(),
            level: Level::High,
        },
    }
}

fn bus_label(bits: &str, width: u32) -> String {
    if bits.chars().all(|c| c == '0' || c == '1') {
        match u128::from_str_radix(bits, 2) {
            Ok(v) => format!("{width}'h{v:x}"),
            Err(_) => bits.to_string(),
        }
    } else if bits.len() > 24 {
        format!("{}…", &bits[..20])
    } else {
        bits.to_string()
    }
}

fn format_timescale(ts: Option<wellen::Timescale>) -> String {
    match ts {
        Some(t) => format!("{}{}", t.factor, unit_suffix(t.unit)),
        None => "1".into(),
    }
}

fn unit_suffix(unit: wellen::TimescaleUnit) -> &'static str {
    use wellen::TimescaleUnit::*;
    match unit {
        ZeptoSeconds => "zs",
        AttoSeconds => "as",
        FemtoSeconds => "fs",
        PicoSeconds => "ps",
        NanoSeconds => "ns",
        MicroSeconds => "us",
        MilliSeconds => "ms",
        Seconds => "s",
        Unknown => "t",
    }
}

fn format_ticks(ticks: u64, timescale: &str) -> String {
    format!("{ticks} {timescale}")
}

#[derive(Default)]
struct DragState {
    last_x: Option<f32>,
    dragging: bool,
}

pub struct WaveformCanvas<'a, Message> {
    view: &'a WaveView,
    on_nav: Box<dyn Fn(WaveNav) -> Message + 'a>,
}

pub fn canvas<'a, Message: 'a>(
    view: &'a WaveView,
    on_nav: impl Fn(WaveNav) -> Message + 'a,
) -> WaveformCanvas<'a, Message> {
    WaveformCanvas {
        view,
        on_nav: Box::new(on_nav),
    }
}

impl<'a, Message> WaveformCanvas<'a, Message> {
    fn time_of_x(&self, x: f32, bounds: Rectangle) -> Option<f64> {
        let wave_x = bounds.x + NAME_COL_WIDTH;
        let wave_w = (bounds.width - NAME_COL_WIDTH).max(1.0);
        if x < wave_x {
            return None;
        }
        let frac = ((x - wave_x) / wave_w) as f64;
        Some(self.view.view_start + frac.clamp(0.0, 1.0) * self.view.span())
    }

    fn dt_of_dx(&self, dx: f32, bounds: Rectangle) -> f64 {
        let wave_w = (bounds.width - NAME_COL_WIDTH).max(1.0);
        -(dx as f64) / (wave_w as f64) * self.view.span()
    }
}

impl<Message> Widget<Message, iced::Theme, iced::Renderer> for WaveformCanvas<'_, Message>
where
    Message: 'static,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<DragState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(DragState::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(self.view.content_height()))
    }

    fn layout(
        &self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let height = self.view.content_height();
        layout::Node::new(limits.resolve(Length::Fill, Length::Fixed(height), Size::new(0.0, height)))
    }

    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) -> event::Status {
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<DragState>();
        let pos = cursor.position_in(bounds);

        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let Some(p) = pos else {
                    return event::Status::Ignored;
                };
                if p.x < NAME_COL_WIDTH {
                    return event::Status::Ignored;
                }
                let lines = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => y,
                    mouse::ScrollDelta::Pixels { y, .. } => y / 40.0,
                };
                if lines.abs() < f32::EPSILON {
                    return event::Status::Ignored;
                }
                let factor = if lines > 0.0 { 0.8_f32 } else { 1.25 };
                if let Some(time) = self.time_of_x(bounds.x + p.x, bounds) {
                    shell.publish((self.on_nav)(WaveNav::Zoom {
                        factor,
                        anchor_time: time,
                    }));
                    return event::Status::Captured;
                }
                event::Status::Ignored
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(p) = pos else {
                    return event::Status::Ignored;
                };
                if p.x < NAME_COL_WIDTH {
                    return event::Status::Ignored;
                }
                state.dragging = true;
                state.last_x = Some(bounds.x + p.x);
                if let Some(time) = self.time_of_x(bounds.x + p.x, bounds) {
                    shell.publish((self.on_nav)(WaveNav::SetCursor { time }));
                }
                event::Status::Captured
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if state.dragging {
                    state.dragging = false;
                    state.last_x = None;
                    event::Status::Captured
                } else {
                    event::Status::Ignored
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if !state.dragging {
                    return event::Status::Ignored;
                }
                if let Some(last) = state.last_x {
                    let dx = position.x - last;
                    if dx.abs() > 0.5 {
                        shell.publish((self.on_nav)(WaveNav::Pan {
                            dt: self.dt_of_dx(dx, bounds),
                        }));
                    }
                }
                state.last_x = Some(position.x);
                event::Status::Captured
            }
            _ => event::Status::Ignored,
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<DragState>();
        if state.dragging {
            return mouse::Interaction::Grabbing;
        }
        if cursor
            .position_in(layout.bounds())
            .is_some_and(|p| p.x >= NAME_COL_WIDTH)
        {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &iced::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let Some(visible) = bounds.intersection(viewport) else {
            return;
        };
        fill(renderer, visible, BG);

        let name_w = NAME_COL_WIDTH.min(bounds.width);
        let wave = Rectangle {
            x: bounds.x + name_w,
            y: bounds.y,
            width: (bounds.width - name_w).max(0.0),
            height: bounds.height,
        };

        draw_names(renderer, self.view, bounds, visible);
        if wave.width > 1.0 {
            if let Some(clip) = wave.intersection(&visible) {
                draw_waves(renderer, self.view, wave, clip);
            }
        }
    }
}

fn draw_names(renderer: &mut iced::Renderer, view: &WaveView, bounds: Rectangle, visible: Rectangle) {
    let name_rect = Rectangle {
        x: bounds.x,
        y: bounds.y,
        width: NAME_COL_WIDTH.min(bounds.width),
        height: bounds.height,
    };
    if let Some(clip) = name_rect.intersection(&visible) {
        fill(renderer, clip, BG_NAME);
    }

    let ruler = Rectangle {
        x: bounds.x,
        y: bounds.y,
        width: NAME_COL_WIDTH.min(bounds.width),
        height: RULER_HEIGHT,
    };
    if let Some(clip) = ruler.intersection(&visible) {
        fill(renderer, clip, SELECT);
        paint_text(
            renderer,
            "Signal",
            Point::new(bounds.x + 8.0, bounds.y + 5.0),
            FG_MUTED,
            visible,
            12.0,
            alignment::Horizontal::Left,
        );
    }

    for (i, trace) in view.traces.iter().enumerate() {
        let y = bounds.y + RULER_HEIGHT + i as f32 * TRACE_HEIGHT;
        let row = Rectangle {
            x: bounds.x,
            y,
            width: NAME_COL_WIDTH.min(bounds.width),
            height: TRACE_HEIGHT,
        };
        if row.intersection(&visible).is_none() {
            continue;
        }
        if i % 2 == 1 {
            if let Some(clip) = row.intersection(&visible) {
                fill(renderer, clip, SELECT);
            }
        }
        let label = if trace.width > 1 {
            format!("{} [{}]", trace.name, trace.width)
        } else {
            trace.name.clone()
        };
        paint_text(
            renderer,
            &label,
            Point::new(bounds.x + 8.0, y + 6.0),
            FG,
            visible,
            12.0,
            alignment::Horizontal::Left,
        );
    }

    let rule = Rectangle {
        x: bounds.x + NAME_COL_WIDTH.min(bounds.width) - 1.0,
        y: visible.y,
        width: 1.0,
        height: visible.height,
    };
    if let Some(clip) = rule.intersection(&visible) {
        fill(renderer, clip, GRID);
    }
}

fn draw_waves(renderer: &mut iced::Renderer, view: &WaveView, wave: Rectangle, visible: Rectangle) {
    let ruler = Rectangle {
        x: wave.x,
        y: wave.y,
        width: wave.width,
        height: RULER_HEIGHT,
    };
    if let Some(clip) = ruler.intersection(&visible) {
        fill(renderer, clip, SELECT);
    }

    let span = view.span();
    let t0 = view.view_start;
    let ticks = nice_ticks(t0, t0 + span, 8);
    for t in ticks {
        let x = time_to_x(t, t0, span, wave);
        if x < wave.x || x > wave.x + wave.width {
            continue;
        }
        let grid = Rectangle {
            x,
            y: wave.y,
            width: 1.0,
            height: wave.height,
        };
        if let Some(clip) = grid.intersection(&visible) {
            fill(renderer, clip, GRID);
        }
        paint_text(
            renderer,
            &format_ticks(t.round() as u64, &view.timescale),
            Point::new(x + 3.0, wave.y + 5.0),
            FG_MUTED,
            visible,
            11.0,
            alignment::Horizontal::Left,
        );
    }

    for (i, trace) in view.traces.iter().enumerate() {
        let y = wave.y + RULER_HEIGHT + i as f32 * TRACE_HEIGHT;
        let row = Rectangle {
            x: wave.x,
            y,
            width: wave.width,
            height: TRACE_HEIGHT,
        };
        if row.intersection(&visible).is_none() {
            continue;
        }
        if i % 2 == 1 {
            if let Some(clip) = row.intersection(&visible) {
                fill(renderer, clip, Color::from_rgba(0.08, 0.12, 0.16, 0.04));
            }
        }
        draw_trace(renderer, trace, view, wave, y, visible);
    }

    if let Some(cursor) = view.cursor {
        if cursor >= t0 && cursor <= t0 + span {
            let x = time_to_x(cursor, t0, span, wave);
            let line = Rectangle {
                x,
                y: wave.y,
                width: 1.5,
                height: wave.height,
            };
            if let Some(clip) = line.intersection(&visible) {
                fill(renderer, clip, CURSOR);
            }
        }
    }
}

fn draw_trace(
    renderer: &mut iced::Renderer,
    trace: &Trace,
    view: &WaveView,
    wave: Rectangle,
    y: f32,
    visible: Rectangle,
) {
    let t0 = view.view_start;
    let span = view.span();
    let t1 = t0 + span;
    let high_y = y + TRACE_PAD;
    let low_y = y + TRACE_HEIGHT - TRACE_PAD - LINE;
    let mid_y = y + TRACE_HEIGHT * 0.5;

    let start_idx = trace
        .changes
        .partition_point(|c| (c.time as f64) <= t0)
        .saturating_sub(1);

    for i in start_idx..trace.changes.len() {
        let ch = &trace.changes[i];
        let next_t = trace
            .changes
            .get(i + 1)
            .map(|c| c.time as f64)
            .unwrap_or(view.t_max.max(view.t_min.saturating_add(1)) as f64);
        let seg_start = (ch.time as f64).max(t0);
        let seg_end = next_t.min(t1);
        if seg_end <= t0 {
            continue;
        }
        if seg_start >= t1 {
            break;
        }

        let x0 = time_to_x(seg_start, t0, span, wave).max(wave.x);
        let x1 = time_to_x(seg_end, t0, span, wave).min(wave.x + wave.width);
        if x1 - x0 < 0.5 {
            continue;
        }

        match ch.level {
            Level::High | Level::Low | Level::Unknown => {
                let (yy, color) = match ch.level {
                    Level::High => (high_y, HIGH),
                    Level::Low => (low_y, LOW),
                    _ => (mid_y - LINE * 0.5, UNK),
                };
                hline(renderer, x0, x1, yy, color, visible);
                if i > start_idx {
                    let prev = &trace.changes[i.saturating_sub(1)];
                    let prev_y = match prev.level {
                        Level::High => high_y,
                        Level::Low => low_y,
                        _ => mid_y - LINE * 0.5,
                    };
                    if (prev_y - yy).abs() > 0.5 {
                        vline(renderer, x0, prev_y.min(yy), prev_y.max(yy) + LINE, color, visible);
                    }
                }
            }
            Level::Bus => {
                let rect = Rectangle {
                    x: x0,
                    y: high_y,
                    width: (x1 - x0).max(1.0),
                    height: (low_y - high_y).max(LINE),
                };
                if let Some(clip) = rect.intersection(&visible) {
                    fill(renderer, clip, Color::from_rgba(BUS.r, BUS.g, BUS.b, 0.18));
                    hline(renderer, x0, x1, high_y, BUS, visible);
                    hline(renderer, x0, x1, low_y, BUS, visible);
                    vline(renderer, x0, high_y, low_y + LINE, BUS, visible);
                    vline(renderer, x1 - LINE, high_y, low_y + LINE, BUS, visible);
                }
                if x1 - x0 > 36.0 {
                    paint_text(
                        renderer,
                        &ch.label,
                        Point::new(x0 + 6.0, y + 6.0),
                        BUS,
                        visible,
                        11.0,
                        alignment::Horizontal::Left,
                    );
                }
            }
        }
    }
}

fn time_to_x(t: f64, t0: f64, span: f64, wave: Rectangle) -> f32 {
    wave.x + (((t - t0) / span).clamp(0.0, 1.0) as f32) * wave.width
}

fn nice_ticks(start: f64, end: f64, target: usize) -> Vec<f64> {
    let span = (end - start).max(1.0);
    let raw = span / target.max(2) as f64;
    let mag = 10_f64.powf(raw.log10().floor());
    let mut step = mag;
    for m in [1.0, 2.0, 5.0, 10.0] {
        if mag * m >= raw * 0.6 {
            step = mag * m;
            break;
        }
    }
    let mut t = (start / step).floor() * step;
    let mut out = Vec::new();
    while t <= end + step * 0.01 {
        if t >= start - step * 0.01 {
            out.push(t);
        }
        t += step;
        if out.len() > 32 {
            break;
        }
    }
    out
}

fn fill(renderer: &mut iced::Renderer, bounds: Rectangle, color: Color) {
    QuadRenderer::fill_quad(
        renderer,
        Quad {
            bounds,
            border: Border::default(),
            shadow: Shadow::default(),
        },
        Background::Color(color),
    );
}

fn hline(
    renderer: &mut iced::Renderer,
    x0: f32,
    x1: f32,
    y: f32,
    color: Color,
    visible: Rectangle,
) {
    let rect = Rectangle {
        x: x0,
        y,
        width: (x1 - x0).max(1.0),
        height: LINE,
    };
    if let Some(clip) = rect.intersection(&visible) {
        fill(renderer, clip, color);
    }
}

fn vline(
    renderer: &mut iced::Renderer,
    x: f32,
    y0: f32,
    y1: f32,
    color: Color,
    visible: Rectangle,
) {
    let rect = Rectangle {
        x,
        y: y0,
        width: LINE,
        height: (y1 - y0).max(1.0),
    };
    if let Some(clip) = rect.intersection(&visible) {
        fill(renderer, clip, color);
    }
}

fn paint_text(
    renderer: &mut iced::Renderer,
    content: &str,
    position: Point,
    color: Color,
    clip: Rectangle,
    size: f32,
    align: alignment::Horizontal,
) {
    TextRenderer::fill_text(
        renderer,
        text::Text {
            content: content.to_string(),
            bounds: Size::new(NAME_COL_WIDTH - 12.0, TRACE_HEIGHT),
            size: Pixels(size),
            line_height: text::LineHeight::Absolute(Pixels(TRACE_HEIGHT - 4.0)),
            font: Font::MONOSPACE,
            horizontal_alignment: align,
            vertical_alignment: alignment::Vertical::Top,
            shaping: text::Shaping::Basic,
            wrapping: text::Wrapping::None,
        },
        position,
        color,
        clip,
    );
}

impl<'a, Message> From<WaveformCanvas<'a, Message>> for Element<'a, Message, iced::Theme, iced::Renderer>
where
    Message: 'static,
{
    fn from(widget: WaveformCanvas<'a, Message>) -> Self {
        Element::new(widget)
    }
}
