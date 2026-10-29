//! Painted file and toolbar icons. Drawn with quads so the file tree stays sharp
//! on the light theme without an icon font.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::widget::Tree;
use iced::advanced::Widget;
use iced::{Background, Border, Color, Element, Length, Rectangle, Shadow, Size};

const FOLDER: Color = Color::from_rgb(0.92, 0.72, 0.24);
const FOLDER_DARK: Color = Color::from_rgb(0.76, 0.55, 0.12);
const FOLDER_LIGHT: Color = Color::from_rgb(0.98, 0.86, 0.48);
const PAGE: Color = Color::from_rgb(0.99, 0.995, 1.0);
const PAGE_EDGE: Color = Color::from_rgb(0.70, 0.75, 0.80);
const FOLD: Color = Color::from_rgb(0.84, 0.88, 0.91);
const INK: Color = Color::from_rgb(0.58, 0.64, 0.70);
const TEAL: Color = Color::from_rgb(0.05, 0.46, 0.52);
const AMBER: Color = Color::from_rgb(0.78, 0.42, 0.08);
const INDIGO: Color = Color::from_rgb(0.36, 0.34, 0.68);
const GREEN: Color = Color::from_rgb(0.10, 0.52, 0.34);
const SLATE: Color = Color::from_rgb(0.42, 0.48, 0.56);
const VIOLET: Color = Color::from_rgb(0.42, 0.28, 0.62);
const ROSE: Color = Color::from_rgb(0.70, 0.22, 0.24);
const CHEVRON: Color = Color::from_rgb(0.38, 0.44, 0.50);
const WHITE: Color = Color::from_rgb(1.0, 1.0, 1.0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    ChevronRight,
    ChevronDown,
    Folder,
    FolderOpen,
    Verilog,
    Vhdl,
    Testbench,
    Header,
    Vcd,
    Wave,
    Saif,
    TextFile,
    File,
    NewFile,
    NewFolder,
    Rename,
    Delete,
    Refresh,
    Logo,
}

pub struct Mark {
    kind: Kind,
    px: f32,
}

pub fn mark(kind: Kind) -> Mark {
    Mark { kind, px: 16.0 }
}

impl Mark {
    pub fn size(mut self, px: f32) -> Self {
        self.px = px;
        self
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for Mark
where
    Renderer: iced::advanced::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.px), Length::Fixed(self.px))
    }

    fn layout(
        &self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.resolve(
            Length::Fixed(self.px),
            Length::Fixed(self.px),
            Size::new(self.px, self.px),
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
        _viewport: &Rectangle,
    ) {
        draw_kind(renderer, self.kind, layout.bounds());
    }
}

impl<'a, Message, Theme, Renderer> From<Mark> for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    fn from(mark: Mark) -> Self {
        Element::new(mark)
    }
}

fn draw_kind(renderer: &mut impl iced::advanced::Renderer, kind: Kind, bounds: Rectangle) {
    match kind {
        Kind::ChevronRight => chevron(renderer, bounds, false),
        Kind::ChevronDown => chevron(renderer, bounds, true),
        Kind::Folder => folder(renderer, bounds, false),
        Kind::FolderOpen => folder(renderer, bounds, true),
        Kind::Verilog => {
            document(renderer, bounds, Some(TEAL));
            lines(renderer, bounds);
        }
        Kind::Vhdl => {
            document(renderer, bounds, Some(VIOLET));
            lines(renderer, bounds);
        }
        Kind::Testbench => {
            document(renderer, bounds, Some(AMBER));
            pulse(renderer, bounds, AMBER);
        }
        Kind::Header => {
            document(renderer, bounds, Some(INDIGO));
            lines(renderer, bounds);
        }
        Kind::Vcd => wave_tile(renderer, bounds, TEAL),
        Kind::Wave => wave_tile(renderer, bounds, INDIGO),
        Kind::Saif => {
            document(renderer, bounds, Some(GREEN));
            bars(renderer, bounds);
        }
        Kind::TextFile => {
            document(renderer, bounds, Some(SLATE));
            lines(renderer, bounds);
        }
        Kind::File => document(renderer, bounds, None),
        Kind::NewFile => {
            document(renderer, bounds, Some(TEAL));
            plus_badge(renderer, bounds, TEAL);
        }
        Kind::NewFolder => {
            folder(renderer, bounds, false);
            plus_badge(renderer, bounds, TEAL);
        }
        Kind::Rename => {
            document(renderer, bounds, Some(TEAL));
            fill(renderer, cell(bounds, 6.2, 8.2, 6.2, 1.5), TEAL, 0.4, bounds);
        }
        Kind::Delete => trash(renderer, bounds),
        Kind::Refresh => refresh(renderer, bounds),
        Kind::Logo => wave_tile(renderer, bounds, TEAL),
    }
}

fn folder(renderer: &mut impl iced::advanced::Renderer, bounds: Rectangle, open: bool) {
    if open {
        fill(renderer, cell(bounds, 1.0, 2.2, 14.0, 7.4), FOLDER_DARK, 1.4, bounds);
        fill(renderer, cell(bounds, 1.6, 6.2, 13.2, 8.0), FOLDER, 1.5, bounds);
        fill(renderer, cell(bounds, 2.4, 7.2, 11.6, 1.2), FOLDER_LIGHT, 0.6, bounds);
    } else {
        fill(renderer, cell(bounds, 1.2, 2.4, 6.6, 3.4), FOLDER_DARK, 1.1, bounds);
        fill(renderer, cell(bounds, 1.0, 4.8, 14.0, 9.4), FOLDER, 1.6, bounds);
        fill(renderer, cell(bounds, 1.8, 6.0, 12.4, 1.3), FOLDER_LIGHT, 0.6, bounds);
    }
}

fn document(renderer: &mut impl iced::advanced::Renderer, bounds: Rectangle, stripe: Option<Color>) {
    let page = cell(bounds, 2.0, 1.0, 12.0, 14.0);
    let s = unit(bounds);
    renderer.fill_quad(
        Quad {
            bounds: page,
            border: Border {
                color: PAGE_EDGE,
                width: s.max(0.7),
                radius: (1.5 * s).into(),
            },
            shadow: Shadow::default(),
        },
        Background::Color(PAGE),
    );
    fill(renderer, cell(bounds, 10.0, 1.0, 4.0, 3.6), FOLD, 0.6, bounds);
    if let Some(color) = stripe {
        fill(renderer, cell(bounds, 2.15, 1.15, 2.5, 13.7), color, 1.1, bounds);
    }
}

fn lines(renderer: &mut impl iced::advanced::Renderer, bounds: Rectangle) {
    fill(renderer, cell(bounds, 6.1, 5.0, 5.6, 1.15), INK, 0.5, bounds);
    fill(renderer, cell(bounds, 6.1, 7.6, 4.1, 1.15), INK, 0.5, bounds);
    fill(renderer, cell(bounds, 6.1, 10.2, 5.0, 1.15), INK, 0.5, bounds);
}

fn pulse(renderer: &mut impl iced::advanced::Renderer, bounds: Rectangle, color: Color) {
    fill(renderer, cell(bounds, 5.8, 6.2, 2.3, 1.45), color, 0.4, bounds);
    fill(renderer, cell(bounds, 7.6, 6.2, 1.45, 4.4), color, 0.4, bounds);
    fill(renderer, cell(bounds, 7.6, 9.15, 2.8, 1.45), color, 0.4, bounds);
}

fn bars(renderer: &mut impl iced::advanced::Renderer, bounds: Rectangle) {
    fill(renderer, cell(bounds, 6.0, 9.4, 1.7, 3.2), GREEN, 0.4, bounds);
    fill(renderer, cell(bounds, 8.3, 7.0, 1.7, 5.6), GREEN, 0.4, bounds);
    fill(renderer, cell(bounds, 10.6, 5.2, 1.7, 7.4), GREEN, 0.4, bounds);
}

fn wave_tile(renderer: &mut impl iced::advanced::Renderer, bounds: Rectangle, bg: Color) {
    fill(renderer, cell(bounds, 1.0, 1.0, 14.0, 14.0), bg, 3.2, bounds);
    fill(renderer, cell(bounds, 3.0, 4.3, 3.2, 1.7), WHITE, 0.4, bounds);
    fill(renderer, cell(bounds, 6.2, 4.3, 1.7, 6.6), WHITE, 0.4, bounds);
    fill(renderer, cell(bounds, 6.2, 9.2, 3.2, 1.7), WHITE, 0.4, bounds);
    fill(renderer, cell(bounds, 9.4, 4.3, 1.7, 6.6), WHITE, 0.4, bounds);
    fill(renderer, cell(bounds, 9.4, 4.3, 3.6, 1.7), WHITE, 0.4, bounds);
}

fn trash(renderer: &mut impl iced::advanced::Renderer, bounds: Rectangle) {
    fill(renderer, cell(bounds, 4.2, 3.2, 7.6, 1.6), ROSE, 0.4, bounds);
    fill(renderer, cell(bounds, 6.6, 1.8, 2.8, 1.6), ROSE, 0.4, bounds);
    fill(renderer, cell(bounds, 4.6, 5.2, 6.8, 9.0), ROSE, 1.2, bounds);
    fill(renderer, cell(bounds, 6.4, 6.6, 1.2, 5.6), WHITE, 0.3, bounds);
    fill(renderer, cell(bounds, 8.4, 6.6, 1.2, 5.6), WHITE, 0.3, bounds);
}

fn plus_badge(renderer: &mut impl iced::advanced::Renderer, bounds: Rectangle, color: Color) {
    fill(renderer, cell(bounds, 8.4, 8.4, 6.8, 6.8), color, 3.4, bounds);
    fill(renderer, cell(bounds, 9.7, 11.15, 4.2, 1.5), WHITE, 0.4, bounds);
    fill(renderer, cell(bounds, 11.05, 9.8, 1.5, 4.2), WHITE, 0.4, bounds);
}

fn refresh(renderer: &mut impl iced::advanced::Renderer, bounds: Rectangle) {
    let mut deg = 40.0_f32;
    while deg <= 300.0 {
        let rad = deg.to_radians();
        let x = 8.0 + 5.15 * rad.cos() - 0.85;
        let y = 8.0 + 5.15 * rad.sin() - 0.85;
        fill(renderer, cell(bounds, x, y, 1.7, 1.7), TEAL, 0.8, bounds);
        deg += 18.0;
    }
    let tip = 308.0_f32.to_radians();
    let tx = 8.0 + 5.15 * tip.cos();
    let ty = 8.0 + 5.15 * tip.sin();
    let dx = -tip.sin();
    let dy = tip.cos();
    let px = -dy;
    let py = dx;
    fill(renderer, cell(bounds, tx - 0.8, ty - 0.8, 1.8, 1.8), TEAL, 0.5, bounds);
    fill(
        renderer,
        cell(bounds, tx - dx * 2.3 + px * 1.5 - 0.7, ty - dy * 2.3 + py * 1.5 - 0.7, 1.6, 1.6),
        TEAL,
        0.4,
        bounds,
    );
    fill(
        renderer,
        cell(bounds, tx - dx * 2.3 - px * 1.5 - 0.7, ty - dy * 2.3 - py * 1.5 - 0.7, 1.6, 1.6),
        TEAL,
        0.4,
        bounds,
    );
}

fn chevron(renderer: &mut impl iced::advanced::Renderer, bounds: Rectangle, down: bool) {
    let pts: &[(f32, f32)] = if down {
        &[
            (3.2, 5.4),
            (4.6, 6.8),
            (6.0, 8.2),
            (7.4, 9.6),
            (8.8, 8.2),
            (10.2, 6.8),
            (11.6, 5.4),
            (3.2, 6.6),
            (4.6, 8.0),
            (6.0, 9.4),
            (7.4, 10.8),
            (8.8, 9.4),
            (10.2, 8.0),
            (11.6, 6.6),
        ]
    } else {
        &[
            (5.2, 3.2),
            (6.6, 4.6),
            (8.0, 6.0),
            (9.4, 7.4),
            (8.0, 8.8),
            (6.6, 10.2),
            (5.2, 11.6),
            (6.4, 3.2),
            (7.8, 4.6),
            (9.2, 6.0),
            (10.6, 7.4),
            (9.2, 8.8),
            (7.8, 10.2),
            (6.4, 11.6),
        ]
    };
    for (x, y) in pts {
        fill(renderer, cell(bounds, *x, *y, 1.7, 1.7), CHEVRON, 0.7, bounds);
    }
}

fn unit(bounds: Rectangle) -> f32 {
    bounds.width / 16.0
}

fn cell(bounds: Rectangle, x: f32, y: f32, w: f32, h: f32) -> Rectangle {
    let s = unit(bounds);
    Rectangle {
        x: bounds.x + x * s,
        y: bounds.y + y * s,
        width: (w * s).max(0.6),
        height: (h * s).max(0.6),
    }
}

fn fill(
    renderer: &mut impl iced::advanced::Renderer,
    bounds: Rectangle,
    color: Color,
    radius: f32,
    icon: Rectangle,
) {
    let radius = radius * unit(icon);
    renderer.fill_quad(
        Quad {
            bounds,
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.0,
                radius: radius.into(),
            },
            shadow: Shadow::default(),
        },
        Background::Color(color),
    );
}
