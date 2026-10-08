//! Editor helpers: cursor, line metrics, and a painted line-number gutter.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text;
use iced::advanced::widget::Tree;
use iced::advanced::Widget;
use iced::alignment;
use iced::widget::scrollable::{self, AbsoluteOffset};
use iced::widget::text::LineHeight;
use iced::{
    Background, Border, Color, Element, Font, Length, Pixels, Point, Rectangle, Shadow, Size, Task,
};

pub const EDITOR_FONT_SIZE: f32 = 14.0;
/// Absolute pixels per line. Must match the text editor (do not pass this
/// as a bare `f32` — iced treats `f32` as a *relative* line-height factor).
pub const EDITOR_LINE_HEIGHT: f32 = 20.0;
pub const EDITOR_PADDING: f32 = 5.0;

const GUTTER_BG: Color = Color::from_rgb(0.06, 0.08, 0.11);
const GUTTER_ACTIVE_BG: Color = Color::from_rgb(0.10, 0.22, 0.26);
const GUTTER_FG: Color = Color::from_rgb(0.40, 0.48, 0.55);
const GUTTER_FG_ACTIVE: Color = Color::from_rgb(0.55, 0.88, 0.90);
const GUTTER_RULE: Color = Color::from_rgb(0.16, 0.22, 0.28);
const GUTTER_PAD_RIGHT: f32 = 10.0;
const GUTTER_PAD_LEFT: f32 = 8.0;
const DIGIT_WIDTH: f32 = 8.4;

const EDITOR_SCROLL_ID: &str = "verilog-ide-editor";

pub fn line_height() -> LineHeight {
    LineHeight::Absolute(Pixels(EDITOR_LINE_HEIGHT))
}

pub fn scroll_id() -> scrollable::Id {
    scrollable::Id::new(EDITOR_SCROLL_ID)
}

pub fn scroll_to_y<Message>(y: f32) -> Task<Message> {
    scrollable::scroll_to(scroll_id(), AbsoluteOffset { x: 0.0, y: y.max(0.0) })
}

pub fn scroll_by_lines<Message>(lines: i32) -> Task<Message> {
    scrollable::scroll_by(
        scroll_id(),
        AbsoluteOffset {
            x: 0.0,
            y: lines as f32 * EDITOR_LINE_HEIGHT,
        },
    )
}

pub fn cursor_line_col(content: &str, cursor: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, ch) in content.chars().enumerate() {
        if i >= cursor {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

pub fn cursor_from_line_col(content: &str, line: usize, col: usize) -> usize {
    let target_line = line + 1;
    let target_col = col + 1;
    let mut current_line = 1usize;
    let mut current_col = 1usize;
    for (i, ch) in content.chars().enumerate() {
        if current_line == target_line && current_col == target_col {
            return i;
        }
        if ch == '\n' {
            current_line += 1;
            current_col = 1;
        } else {
            current_col += 1;
        }
    }
    content.len()
}

pub fn digit_count(line_count: usize) -> u32 {
    line_count.max(1).ilog10() + 1
}

pub fn gutter_width(line_count: usize) -> f32 {
    let digits = digit_count(line_count).max(2) as f32;
    (GUTTER_PAD_LEFT + digits * DIGIT_WIDTH + GUTTER_PAD_RIGHT).max(40.0)
}

/// Height of the editor+gutter document, including padding.
pub fn content_height(line_count: usize, min_height: f32) -> f32 {
    let doc = EDITOR_PADDING * 2.0 + line_count.max(1) as f32 * EDITOR_LINE_HEIGHT;
    doc.max(min_height)
}

pub fn line_top(line_index: usize) -> f32 {
    EDITOR_PADDING + line_index as f32 * EDITOR_LINE_HEIGHT
}

/// Pixel-painted gutter. Lives in the same scrollable as the editor so numbers
/// stay locked to source lines.
pub struct LineGutter {
    line_count: usize,
    current_line: usize,
    width: f32,
    height: f32,
}

pub fn line_gutter(line_count: usize, current_line: usize, min_height: f32) -> LineGutter {
    let line_count = line_count.max(1);
    LineGutter {
        line_count,
        current_line: current_line.max(1),
        width: gutter_width(line_count),
        height: content_height(line_count, min_height),
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for LineGutter
where
    Renderer: iced::advanced::Renderer + text::Renderer<Font = Font>,
{
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.width), Length::Fixed(self.height))
    }

    fn layout(
        &self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.resolve(
            Length::Fixed(self.width),
            Length::Fixed(self.height),
            Size::new(self.width, self.height),
        ))
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: iced::mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let Some(visible) = bounds.intersection(viewport) else {
            return;
        };

        renderer.fill_quad(
            Quad {
                bounds: visible,
                border: Border::default(),
                shadow: Shadow::default(),
            },
            Background::Color(GUTTER_BG),
        );

        let first = ((visible.y - bounds.y - EDITOR_PADDING).max(0.0) / EDITOR_LINE_HEIGHT).floor()
            as usize;
        let last = (((visible.y - bounds.y + visible.height - EDITOR_PADDING).max(0.0)
            / EDITOR_LINE_HEIGHT)
            .ceil() as usize)
            .min(self.line_count.saturating_sub(1));

        if (first..=last).contains(&(self.current_line.saturating_sub(1))) {
            let y = bounds.y + line_top(self.current_line.saturating_sub(1));
            let highlight = Rectangle {
                x: visible.x,
                y,
                width: visible.width,
                height: EDITOR_LINE_HEIGHT,
            };
            if let Some(clip) = highlight.intersection(&visible) {
                renderer.fill_quad(
                    Quad {
                        bounds: clip,
                        border: Border::default(),
                        shadow: Shadow::default(),
                    },
                    Background::Color(GUTTER_ACTIVE_BG),
                );
            }
        }

        let right_x = bounds.x + bounds.width - GUTTER_PAD_RIGHT;
        let number_width = bounds.width - GUTTER_PAD_LEFT - GUTTER_PAD_RIGHT;

        for index in first..=last {
            let n = index + 1;
            let y = bounds.y + line_top(index);
            let active = n == self.current_line;
            renderer.fill_text(
                text::Text {
                    content: n.to_string(),
                    bounds: Size::new(number_width, EDITOR_LINE_HEIGHT),
                    size: Pixels(EDITOR_FONT_SIZE),
                    line_height: line_height(),
                    font: Font::MONOSPACE,
                    horizontal_alignment: alignment::Horizontal::Right,
                    vertical_alignment: alignment::Vertical::Top,
                    shaping: text::Shaping::Basic,
                    wrapping: text::Wrapping::None,
                },
                Point::new(right_x, y),
                if active { GUTTER_FG_ACTIVE } else { GUTTER_FG },
                visible,
            );
        }

        let rule = Rectangle {
            x: bounds.x + bounds.width - 1.0,
            y: visible.y,
            width: 1.0,
            height: visible.height,
        };
        renderer.fill_quad(
            Quad {
                bounds: rule,
                border: Border::default(),
                shadow: Shadow::default(),
            },
            Background::Color(GUTTER_RULE),
        );
    }
}

impl<'a, Message, Theme, Renderer> From<LineGutter> for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + text::Renderer<Font = Font> + 'a,
{
    fn from(gutter: LineGutter) -> Self {
        Element::new(gutter)
    }
}
