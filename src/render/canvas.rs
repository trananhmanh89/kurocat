//! A small drawing surface with the same model as the HTML Canvas 2D API, built on tiny-skia.
//!
//! The pet's drawing code was first written against Canvas 2D. Keeping the same model here
//! (current path, transform stack, fill/stroke styles, shadows, clip) lets that code be read
//! side by side with the original.
//!
//! As in Canvas 2D:
//! - points are transformed by the current transform when they are added to the path;
//! - line width and gradients use the transform that is current when `stroke` / `fill` is called;
//! - shadows are measured in device pixels and are not affected by the transform.

use super::blur::blur_alpha;
use super::color::parse_color;
use super::text::{measure_text, text_path, FontSpec, TextAlign, TextBaseline};
use std::f32::consts::PI;
use std::rc::Rc;
use tiny_skia::{
    Color, FillRule, FilterQuality, GradientStop, LineCap, LineJoin, LinearGradient, Mask, Paint,
    Path, PathBuilder, Pixmap, PixmapPaint, Point, RadialGradient, Shader, SpreadMode, Stroke,
    Transform,
};

/// The shape of a gradient, in the coordinates that were current when it was created.
#[derive(Clone, Debug)]
enum GradientShape {
    Linear { x0: f32, y0: f32, x1: f32, y1: f32 },
    Radial { x0: f32, y0: f32, r0: f32, x1: f32, y1: f32, r1: f32 },
}

/// A linear or radial gradient (Canvas `createLinearGradient` / `createRadialGradient`).
#[derive(Clone, Debug)]
pub struct Gradient {
    shape: GradientShape,
    stops: Vec<(f32, Color)>,
}

impl Gradient {
    /// Creates a linear gradient from (x0, y0) to (x1, y1).
    ///
    /// @param x0 Start x.
    /// @param y0 Start y.
    /// @param x1 End x.
    /// @param y1 End y.
    /// @return A gradient without colour stops.
    pub fn linear(x0: f32, y0: f32, x1: f32, y1: f32) -> Gradient {
        Gradient {
            shape: GradientShape::Linear { x0, y0, x1, y1 },
            stops: Vec::new(),
        }
    }

    /// Creates a radial gradient between two circles.
    ///
    /// @param x0 Inner circle centre x.
    /// @param y0 Inner circle centre y.
    /// @param r0 Inner circle radius.
    /// @param x1 Outer circle centre x.
    /// @param y1 Outer circle centre y.
    /// @param r1 Outer circle radius.
    /// @return A gradient without colour stops.
    pub fn radial(x0: f32, y0: f32, r0: f32, x1: f32, y1: f32, r1: f32) -> Gradient {
        Gradient {
            shape: GradientShape::Radial { x0, y0, r0, x1, y1, r1 },
            stops: Vec::new(),
        }
    }

    /// Adds a colour stop. Invalid colours are ignored.
    ///
    /// @param offset Position 0..1 along the gradient.
    /// @param color CSS colour text.
    pub fn add_color_stop(&mut self, offset: f32, color: &str) {
        if let Some(parsed) = parse_color(color) {
            self.stops.push((offset.clamp(0.0, 1.0), parsed));
        }
    }

    /// Returns the largest alpha of all stops. Used to size the shadow of a gradient fill.
    ///
    /// @return Alpha 0..1.
    fn max_alpha(&self) -> f32 {
        let mut alpha: f32 = 0.0;
        for (_, color) in &self.stops {
            alpha = alpha.max(color.alpha());
        }
        alpha
    }

    /// Builds the tiny-skia shader for this gradient.
    ///
    /// @param transform The transform to apply to the gradient coordinates.
    /// @param alpha Extra opacity (global alpha) to multiply into every stop.
    /// @return The shader. Degenerate gradients become a solid colour.
    fn to_shader(&self, transform: Transform, alpha: f32) -> Shader<'static> {
        let mut stops: Vec<(f32, Color)> = self.stops.clone();
        stops.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        if stops.is_empty() {
            return Shader::SolidColor(Color::TRANSPARENT);
        }
        let fallback = with_alpha(stops[stops.len() - 1].1, alpha);
        let (mapped_stops, shape_points) = match self.shape {
            GradientShape::Linear { x0, y0, x1, y1 } => (stops, (x0, y0, x1, y1, 0.0)),
            GradientShape::Radial { x0, y0, r0, x1, y1, r1 } => {
                // tiny-skia's radial gradient starts at radius 0, so the stops are squeezed
                // into the ring between r0 and r1 instead.
                let inner = if r1 > 0.0 { (r0 / r1).clamp(0.0, 0.99) } else { 0.0 };
                let squeezed = stops
                    .into_iter()
                    .map(|(offset, color)| (inner + offset * (1.0 - inner), color))
                    .collect();
                (squeezed, (x0, y0, x1, y1, r1))
            }
        };
        let mut gradient_stops: Vec<GradientStop> = Vec::new();
        if mapped_stops.len() == 1 {
            return Shader::SolidColor(fallback);
        }
        for (offset, color) in &mapped_stops {
            gradient_stops.push(GradientStop::new(*offset, with_alpha(*color, alpha)));
        }
        let (x0, y0, x1, y1, radius) = shape_points;
        let shader = match self.shape {
            GradientShape::Linear { .. } => LinearGradient::new(
                Point::from_xy(x0, y0),
                Point::from_xy(x1, y1),
                gradient_stops,
                SpreadMode::Pad,
                transform,
            ),
            GradientShape::Radial { .. } => RadialGradient::new(
                Point::from_xy(x0, y0),
                Point::from_xy(x1, y1),
                radius.max(0.01),
                gradient_stops,
                SpreadMode::Pad,
                transform,
            ),
        };
        shader.unwrap_or(Shader::SolidColor(fallback))
    }
}

/// What to paint with: a plain colour or a gradient (Canvas `fillStyle` / `strokeStyle`).
#[derive(Clone, Debug)]
pub enum Style {
    /// A solid colour.
    Color(Color),
    /// A gradient.
    Gradient(Gradient),
}

/// Returns a colour with its alpha multiplied by an extra factor.
///
/// @param color The colour.
/// @param factor Alpha multiplier 0..1.
/// @return The faded colour.
fn with_alpha(color: Color, factor: f32) -> Color {
    let mut faded = color;
    faded.set_alpha((color.alpha() * factor).clamp(0.0, 1.0));
    faded
}

/// Everything that `save` / `restore` keeps (Canvas drawing state).
#[derive(Clone)]
struct DrawState {
    transform: Transform,
    fill: Style,
    stroke: Style,
    line_width: f32,
    line_cap: LineCap,
    line_join: LineJoin,
    global_alpha: f32,
    shadow_blur: f32,
    shadow_color: Color,
    shadow_offset_x: f32,
    shadow_offset_y: f32,
    clip: Option<Rc<Mask>>,
    font: FontSpec,
    text_align: TextAlign,
    text_baseline: TextBaseline,
}

impl DrawState {
    /// The Canvas 2D default state.
    ///
    /// @return The default state.
    fn new() -> DrawState {
        DrawState {
            transform: Transform::identity(),
            fill: Style::Color(Color::BLACK),
            stroke: Style::Color(Color::BLACK),
            line_width: 1.0,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            global_alpha: 1.0,
            shadow_blur: 0.0,
            shadow_color: Color::TRANSPARENT,
            shadow_offset_x: 0.0,
            shadow_offset_y: 0.0,
            clip: None,
            font: FontSpec::default(),
            text_align: TextAlign::Left,
            text_baseline: TextBaseline::Alphabetic,
        }
    }
}

/// A drawing surface with a Canvas-2D-like API.
pub struct Canvas {
    pixmap: Pixmap,
    state: DrawState,
    saved: Vec<DrawState>,
    path: PathBuilder,
    current_point: Option<Point>,
    subpath_start: Option<Point>,
}

impl Canvas {
    /// Creates a transparent canvas.
    ///
    /// @param width Width in pixels (at least 1).
    /// @param height Height in pixels (at least 1).
    /// @return The canvas.
    pub fn new(width: u32, height: u32) -> Canvas {
        Canvas {
            pixmap: Pixmap::new(width.max(1), height.max(1)).expect("canvas size is valid"),
            state: DrawState::new(),
            saved: Vec::new(),
            path: PathBuilder::new(),
            current_point: None,
            subpath_start: None,
        }
    }

    /// Changes the canvas size. The content is cleared and the drawing state is reset.
    ///
    /// @param width New width in pixels.
    /// @param height New height in pixels.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width.max(1) != self.pixmap.width() || height.max(1) != self.pixmap.height() {
            self.pixmap = Pixmap::new(width.max(1), height.max(1)).expect("canvas size is valid");
        }
        self.state = DrawState::new();
        self.saved.clear();
        self.begin_path();
    }

    /// @return Width in pixels.
    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    /// @return Height in pixels.
    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    /// Returns the drawn pixels: premultiplied RGBA, row by row.
    ///
    /// @return The pixel bytes.
    pub fn pixels(&self) -> &[u8] {
        self.pixmap.data()
    }

    /// Clears the whole canvas to transparent and resets the drawing state.
    pub fn clear(&mut self) {
        self.pixmap.fill(Color::TRANSPARENT);
        self.state = DrawState::new();
        self.saved.clear();
        self.begin_path();
    }

    // ----- State -------------------------------------------------------------------------

    /// Pushes the drawing state (Canvas `save`).
    pub fn save(&mut self) {
        self.saved.push(self.state.clone());
    }

    /// Pops the drawing state (Canvas `restore`). Does nothing when nothing was saved.
    pub fn restore(&mut self) {
        if let Some(previous) = self.saved.pop() {
            self.state = previous;
        }
    }

    /// Moves the origin (Canvas `translate`).
    ///
    /// @param x Offset x.
    /// @param y Offset y.
    pub fn translate(&mut self, x: f32, y: f32) {
        self.state.transform = self.state.transform.pre_translate(x, y);
    }

    /// Rotates the coordinate system (Canvas `rotate`).
    ///
    /// @param radians Angle in radians, clockwise.
    pub fn rotate(&mut self, radians: f32) {
        self.state.transform = self.state.transform.pre_concat(Transform::from_rotate(radians.to_degrees()));
    }

    /// Scales the coordinate system (Canvas `scale`).
    ///
    /// @param x Horizontal factor.
    /// @param y Vertical factor.
    pub fn scale(&mut self, x: f32, y: f32) {
        self.state.transform = self.state.transform.pre_scale(x, y);
    }

    /// Replaces the transform (Canvas `setTransform(a, b, c, d, e, f)`).
    ///
    /// @param a Horizontal scale.
    /// @param b Vertical skew.
    /// @param c Horizontal skew.
    /// @param d Vertical scale.
    /// @param e Horizontal offset.
    /// @param f Vertical offset.
    pub fn set_transform(&mut self, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) {
        self.state.transform = Transform::from_row(a, b, c, d, e, f);
    }

    /// @return The current transform.
    pub fn transform(&self) -> Transform {
        self.state.transform
    }

    /// Sets the fill to a CSS colour. Invalid colours are ignored, as in Canvas.
    ///
    /// @param color CSS colour text.
    pub fn set_fill_color(&mut self, color: &str) {
        if let Some(parsed) = parse_color(color) {
            self.state.fill = Style::Color(parsed);
        }
    }

    /// Sets the fill to a gradient.
    ///
    /// @param gradient The gradient.
    pub fn set_fill_gradient(&mut self, gradient: Gradient) {
        self.state.fill = Style::Gradient(gradient);
    }

    /// Sets the stroke to a CSS colour. Invalid colours are ignored, as in Canvas.
    ///
    /// @param color CSS colour text.
    pub fn set_stroke_color(&mut self, color: &str) {
        if let Some(parsed) = parse_color(color) {
            self.state.stroke = Style::Color(parsed);
        }
    }

    /// Sets the stroke to a gradient.
    ///
    /// @param gradient The gradient.
    pub fn set_stroke_gradient(&mut self, gradient: Gradient) {
        self.state.stroke = Style::Gradient(gradient);
    }

    /// @param width Stroke width in user units. Zero, negative and NaN values are ignored.
    pub fn set_line_width(&mut self, width: f32) {
        if width.is_finite() && width > 0.0 {
            self.state.line_width = width;
        }
    }

    /// @param cap How stroke ends are drawn.
    pub fn set_line_cap(&mut self, cap: LineCap) {
        self.state.line_cap = cap;
    }

    /// @param join How stroke corners are drawn.
    pub fn set_line_join(&mut self, join: LineJoin) {
        self.state.line_join = join;
    }

    /// @param alpha Opacity 0..1 applied to everything drawn. Out-of-range values are ignored.
    pub fn set_global_alpha(&mut self, alpha: f32) {
        if alpha.is_finite() && (0.0..=1.0).contains(&alpha) {
            self.state.global_alpha = alpha;
        }
    }

    /// @return The current global alpha.
    pub fn global_alpha(&self) -> f32 {
        self.state.global_alpha
    }

    /// @param blur Shadow blur in device pixels (Canvas `shadowBlur`).
    pub fn set_shadow_blur(&mut self, blur: f32) {
        if blur.is_finite() && blur >= 0.0 {
            self.state.shadow_blur = blur;
        }
    }

    /// @param color Shadow colour as CSS text. Invalid colours are ignored.
    pub fn set_shadow_color(&mut self, color: &str) {
        if let Some(parsed) = parse_color(color) {
            self.state.shadow_color = parsed;
        }
    }

    /// @param y Vertical shadow offset in device pixels.
    pub fn set_shadow_offset_y(&mut self, y: f32) {
        self.state.shadow_offset_y = y;
    }

    /// @param font Size and weight for `fill_text`.
    pub fn set_font(&mut self, font: FontSpec) {
        self.state.font = font;
    }

    /// @param align Horizontal text anchor.
    pub fn set_text_align(&mut self, align: TextAlign) {
        self.state.text_align = align;
    }

    /// @param baseline Vertical text anchor.
    pub fn set_text_baseline(&mut self, baseline: TextBaseline) {
        self.state.text_baseline = baseline;
    }

    // ----- Path building -----------------------------------------------------------------

    /// Starts a new, empty path.
    pub fn begin_path(&mut self) {
        self.path = PathBuilder::new();
        self.current_point = None;
        self.subpath_start = None;
    }

    /// Maps a point from user space to device space.
    ///
    /// @param x User x.
    /// @param y User y.
    /// @return The device point.
    fn to_device(&self, x: f32, y: f32) -> Point {
        let mut point = Point::from_xy(x, y);
        self.state.transform.map_point(&mut point);
        point
    }

    /// Starts a new sub-path at a point.
    ///
    /// @param x Point x.
    /// @param y Point y.
    pub fn move_to(&mut self, x: f32, y: f32) {
        let point = self.to_device(x, y);
        self.path.move_to(point.x, point.y);
        self.current_point = Some(point);
        self.subpath_start = Some(point);
    }

    /// Makes sure a sub-path exists, starting it at a device point when needed.
    ///
    /// @param point Device point to start at if there is no current point.
    fn ensure_subpath(&mut self, point: Point) {
        if self.current_point.is_none() {
            self.path.move_to(point.x, point.y);
            self.current_point = Some(point);
            self.subpath_start = Some(point);
        }
    }

    /// Adds a straight line.
    ///
    /// @param x End x.
    /// @param y End y.
    pub fn line_to(&mut self, x: f32, y: f32) {
        let point = self.to_device(x, y);
        if self.current_point.is_none() {
            self.ensure_subpath(point);
            return;
        }
        self.path.line_to(point.x, point.y);
        self.current_point = Some(point);
    }

    /// Adds a quadratic curve (Canvas `quadraticCurveTo`).
    ///
    /// @param cx Control point x.
    /// @param cy Control point y.
    /// @param x End x.
    /// @param y End y.
    pub fn quadratic_curve_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let control = self.to_device(cx, cy);
        let end = self.to_device(x, y);
        self.ensure_subpath(control);
        self.path.quad_to(control.x, control.y, end.x, end.y);
        self.current_point = Some(end);
    }

    /// Adds a cubic curve (Canvas `bezierCurveTo`).
    ///
    /// @param c1x First control point x.
    /// @param c1y First control point y.
    /// @param c2x Second control point x.
    /// @param c2y Second control point y.
    /// @param x End x.
    /// @param y End y.
    pub fn bezier_curve_to(&mut self, c1x: f32, c1y: f32, c2x: f32, c2y: f32, x: f32, y: f32) {
        let control1 = self.to_device(c1x, c1y);
        let control2 = self.to_device(c2x, c2y);
        let end = self.to_device(x, y);
        self.ensure_subpath(control1);
        self.path.cubic_to(control1.x, control1.y, control2.x, control2.y, end.x, end.y);
        self.current_point = Some(end);
    }

    /// Closes the current sub-path.
    pub fn close_path(&mut self) {
        if self.current_point.is_some() {
            self.path.close();
            self.current_point = self.subpath_start;
        }
    }

    /// Adds a closed rectangle.
    ///
    /// @param x Left.
    /// @param y Top.
    /// @param width Width.
    /// @param height Height.
    pub fn rect(&mut self, x: f32, y: f32, width: f32, height: f32) {
        self.move_to(x, y);
        self.line_to(x + width, y);
        self.line_to(x + width, y + height);
        self.line_to(x, y + height);
        self.close_path();
        self.move_to(x, y);
    }

    /// Adds a closed rectangle with rounded corners (Canvas `roundRect` with one radius).
    ///
    /// @param x Left.
    /// @param y Top.
    /// @param width Width.
    /// @param height Height.
    /// @param radius Corner radius (clamped to half the shorter side).
    pub fn round_rect(&mut self, x: f32, y: f32, width: f32, height: f32, radius: f32) {
        let r = radius.max(0.0).min(width.abs() / 2.0).min(height.abs() / 2.0);
        self.move_to(x + r, y);
        self.line_to(x + width - r, y);
        self.arc(x + width - r, y + r, r, -PI / 2.0, 0.0, false);
        self.line_to(x + width, y + height - r);
        self.arc(x + width - r, y + height - r, r, 0.0, PI / 2.0, false);
        self.line_to(x + r, y + height);
        self.arc(x + r, y + height - r, r, PI / 2.0, PI, false);
        self.line_to(x, y + r);
        self.arc(x + r, y + r, r, PI, PI * 1.5, false);
        self.close_path();
    }

    /// Adds a circular arc (Canvas `arc`).
    ///
    /// @param x Centre x.
    /// @param y Centre y.
    /// @param radius Radius.
    /// @param start_angle Start angle in radians.
    /// @param end_angle End angle in radians.
    /// @param counter_clockwise Draw against the clock instead of with it.
    pub fn arc(&mut self, x: f32, y: f32, radius: f32, start_angle: f32, end_angle: f32, counter_clockwise: bool) {
        self.ellipse(x, y, radius, radius, 0.0, start_angle, end_angle, counter_clockwise);
    }

    /// Adds an elliptical arc (Canvas `ellipse`).
    ///
    /// @param x Centre x.
    /// @param y Centre y.
    /// @param radius_x Horizontal radius.
    /// @param radius_y Vertical radius.
    /// @param rotation Rotation of the ellipse in radians.
    /// @param start_angle Start angle in radians.
    /// @param end_angle End angle in radians.
    /// @param counter_clockwise Draw against the clock instead of with it.
    #[allow(clippy::too_many_arguments)]
    pub fn ellipse(
        &mut self,
        x: f32,
        y: f32,
        radius_x: f32,
        radius_y: f32,
        rotation: f32,
        start_angle: f32,
        end_angle: f32,
        counter_clockwise: bool,
    ) {
        if !(radius_x.is_finite() && radius_y.is_finite()) || radius_x < 0.0 || radius_y < 0.0 {
            return;
        }
        let sweep = arc_sweep(start_angle, end_angle, counter_clockwise);
        let cos_rotation = rotation.cos();
        let sin_rotation = rotation.sin();
        let point_on_ellipse = |angle: f32| -> (f32, f32) {
            let ex = radius_x * angle.cos();
            let ey = radius_y * angle.sin();
            (x + ex * cos_rotation - ey * sin_rotation, y + ex * sin_rotation + ey * cos_rotation)
        };
        let derivative = |angle: f32| -> (f32, f32) {
            let dx = -radius_x * angle.sin();
            let dy = radius_y * angle.cos();
            (dx * cos_rotation - dy * sin_rotation, dx * sin_rotation + dy * cos_rotation)
        };

        let (start_x, start_y) = point_on_ellipse(start_angle);
        let start_device = self.to_device(start_x, start_y);
        if self.current_point.is_none() {
            self.path.move_to(start_device.x, start_device.y);
            self.subpath_start = Some(start_device);
        } else {
            self.path.line_to(start_device.x, start_device.y);
        }
        self.current_point = Some(start_device);
        if sweep.abs() < 1e-6 {
            return;
        }

        let segment_count = (sweep.abs() / (PI / 2.0)).ceil().max(1.0) as usize;
        let step = sweep / segment_count as f32;
        let handle = 4.0 / 3.0 * (step / 4.0).tan();
        let mut angle = start_angle;
        for _ in 0..segment_count {
            let next_angle = angle + step;
            let (p0x, p0y) = point_on_ellipse(angle);
            let (p3x, p3y) = point_on_ellipse(next_angle);
            let (d0x, d0y) = derivative(angle);
            let (d3x, d3y) = derivative(next_angle);
            let c1 = self.to_device(p0x + d0x * handle, p0y + d0y * handle);
            let c2 = self.to_device(p3x - d3x * handle, p3y - d3y * handle);
            let end = self.to_device(p3x, p3y);
            self.path.cubic_to(c1.x, c1.y, c2.x, c2.y, end.x, end.y);
            self.current_point = Some(end);
            angle = next_angle;
        }
    }

    // ----- Drawing -----------------------------------------------------------------------

    /// Fills the current path with the fill style (non-zero rule).
    pub fn fill(&mut self) {
        let Some(path) = self.path.clone().finish() else {
            return;
        };
        let style = self.state.fill.clone();
        self.fill_device_path(&path, &style, FillRule::Winding);
    }

    /// Strokes the current path with the stroke style.
    pub fn stroke(&mut self) {
        let Some(device_path) = self.path.clone().finish() else {
            return;
        };
        let Some(outline) = self.stroke_outline(&device_path) else {
            return;
        };
        let style = self.state.stroke.clone();
        self.fill_device_path(&outline, &style, FillRule::Winding);
    }

    /// Intersects the clip region with the current path (Canvas `clip`).
    pub fn clip(&mut self) {
        let Some(path) = self.path.clone().finish() else {
            let empty = Mask::new(self.width(), self.height()).expect("mask size is valid");
            self.state.clip = Some(Rc::new(empty));
            return;
        };
        let mask = match &self.state.clip {
            Some(existing) => {
                let mut combined = (**existing).clone();
                combined.intersect_path(&path, FillRule::Winding, true, Transform::identity());
                combined
            }
            None => {
                let mut fresh = Mask::new(self.width(), self.height()).expect("mask size is valid");
                fresh.fill_path(&path, FillRule::Winding, true, Transform::identity());
                fresh
            }
        };
        self.state.clip = Some(Rc::new(mask));
    }

    /// Fills a rectangle without touching the current path (Canvas `fillRect`).
    ///
    /// @param x Left.
    /// @param y Top.
    /// @param width Width.
    /// @param height Height.
    pub fn fill_rect(&mut self, x: f32, y: f32, width: f32, height: f32) {
        let mut builder = PathBuilder::new();
        let corners = [
            self.to_device(x, y),
            self.to_device(x + width, y),
            self.to_device(x + width, y + height),
            self.to_device(x, y + height),
        ];
        builder.move_to(corners[0].x, corners[0].y);
        for corner in &corners[1..] {
            builder.line_to(corner.x, corner.y);
        }
        builder.close();
        if let Some(path) = builder.finish() {
            let style = self.state.fill.clone();
            self.fill_device_path(&path, &style, FillRule::Winding);
        }
    }

    /// Draws text with the fill style (Canvas `fillText`).
    ///
    /// @param text The text.
    /// @param x Anchor x.
    /// @param y Anchor y.
    pub fn fill_text(&mut self, text: &str, x: f32, y: f32) {
        let font = self.state.font;
        let Some(local_path) = text_path(text, x, y, font, self.state.text_align, self.state.text_baseline) else {
            return;
        };
        let Some(device_path) = local_path.transform(self.state.transform) else {
            return;
        };
        let style = self.state.fill.clone();
        self.fill_device_path(&device_path, &style, FillRule::Winding);
    }

    /// Measures text with the current font (Canvas `measureText().width`).
    ///
    /// @param text The text.
    /// @return Width in user units.
    pub fn measure_text(&self, text: &str) -> f32 {
        measure_text(text, self.state.font)
    }

    /// Draws an image scaled into a rectangle (Canvas `drawImage(image, dx, dy, dw, dh)`).
    ///
    /// @param image The image (premultiplied).
    /// @param x Left.
    /// @param y Top.
    /// @param width Drawn width.
    /// @param height Drawn height.
    pub fn draw_image(&mut self, image: &Pixmap, x: f32, y: f32, width: f32, height: f32) {
        let scale_x = width / image.width() as f32;
        let scale_y = height / image.height() as f32;
        let transform = self.state.transform.pre_translate(x, y).pre_scale(scale_x, scale_y);
        let paint = PixmapPaint {
            opacity: self.state.global_alpha,
            quality: FilterQuality::Bilinear,
            ..PixmapPaint::default()
        };
        let clip = self.state.clip.clone();
        self.pixmap.draw_pixmap(0, 0, image.as_ref(), &paint, transform, clip.as_deref());
    }

    /// Draws another canvas on top of this one at a whole-pixel position, ignoring the
    /// transform but honouring global alpha and the clip.
    ///
    /// @param layer The canvas to draw (premultiplied pixels).
    /// @param left Device x of the layer's left edge.
    /// @param top Device y of the layer's top edge.
    pub fn composite_layer(&mut self, layer: &Canvas, left: i32, top: i32) {
        let alpha = self.state.global_alpha;
        if alpha <= 0.0 {
            return;
        }
        let canvas_width = self.width() as i32;
        let canvas_height = self.height() as i32;
        let layer_width = layer.width() as i32;
        let layer_height = layer.height() as i32;
        let clip = self.state.clip.clone();
        let clip_data = clip.as_ref().map(|mask| mask.data());
        let source = layer.pixels();
        let target = self.pixmap.data_mut();
        let start_x = left.max(0);
        let end_x = (left + layer_width).min(canvas_width);
        let start_y = top.max(0);
        let end_y = (top + layer_height).min(canvas_height);
        for y in start_y..end_y {
            for x in start_x..end_x {
                let source_index = (((y - top) * layer_width + (x - left)) * 4) as usize;
                let source_alpha = source[source_index + 3];
                if source_alpha == 0 {
                    continue;
                }
                let canvas_index = (y * canvas_width + x) as usize;
                let mut strength = alpha;
                if let Some(clip_values) = clip_data {
                    strength *= clip_values[canvas_index] as f32 / 255.0;
                }
                let target_index = canvas_index * 4;
                if strength >= 1.0 && source_alpha == 255 {
                    target[target_index..target_index + 4].copy_from_slice(&source[source_index..source_index + 4]);
                    continue;
                }
                let keep = 1.0 - (source_alpha as f32 / 255.0) * strength;
                for channel in 0..4 {
                    let blended = source[source_index + channel] as f32 * strength
                        + target[target_index + channel] as f32 * keep;
                    target[target_index + channel] = (blended + 0.5).min(255.0) as u8;
                }
            }
        }
    }

    /// Draws another canvas on top of this one at a sub-pixel position (bilinear), ignoring
    /// the transform but honouring global alpha and the clip.
    ///
    /// @param layer The canvas to draw (premultiplied pixels).
    /// @param x Device x of the layer's left edge.
    /// @param y Device y of the layer's top edge.
    pub fn composite_layer_at(&mut self, layer: &Canvas, x: f32, y: f32) {
        let whole_x = x.floor();
        let whole_y = y.floor();
        let fraction_x = ((x - whole_x) * 256.0).round() as u32;
        let fraction_y = ((y - whole_y) * 256.0).round() as u32;
        let mut left = whole_x as i32;
        let mut top = whole_y as i32;
        if fraction_x >= 256 {
            left += 1;
        }
        if fraction_y >= 256 {
            top += 1;
        }
        let fraction_x = fraction_x % 256;
        let fraction_y = fraction_y % 256;
        if fraction_x == 0 && fraction_y == 0 {
            self.composite_layer(layer, left, top);
            return;
        }

        let alpha = self.state.global_alpha;
        if alpha <= 0.0 {
            return;
        }
        let strength = (alpha.min(1.0) * 256.0).round() as u32;
        let canvas_width = self.width() as i32;
        let canvas_height = self.height() as i32;
        let layer_width = layer.width() as i32;
        let layer_height = layer.height() as i32;
        let clip = self.state.clip.clone();
        let clip_data = clip.as_ref().map(|mask| mask.data());
        let source = layer.pixels();
        let target = self.pixmap.data_mut();
        // Weights of the four source pixels around each target pixel (they sum to 65536).
        let weight_current = (256 - fraction_x) * (256 - fraction_y);
        let weight_left = fraction_x * (256 - fraction_y);
        let weight_above = (256 - fraction_x) * fraction_y;
        let weight_above_left = fraction_x * fraction_y;
        let read = |source_x: i32, source_y: i32, channel: usize| -> u32 {
            if source_x < 0 || source_y < 0 || source_x >= layer_width || source_y >= layer_height {
                return 0;
            }
            source[((source_y * layer_width + source_x) * 4) as usize + channel] as u32
        };
        let start_x = left.max(0);
        let end_x = (left + layer_width + 1).min(canvas_width);
        let start_y = top.max(0);
        let end_y = (top + layer_height + 1).min(canvas_height);
        for target_y in start_y..end_y {
            let source_y = target_y - top;
            for target_x in start_x..end_x {
                let source_x = target_x - left;
                let mut sampled = [0u32; 4];
                for channel in 0..4 {
                    let sum = read(source_x, source_y, channel) * weight_current
                        + read(source_x - 1, source_y, channel) * weight_left
                        + read(source_x, source_y - 1, channel) * weight_above
                        + read(source_x - 1, source_y - 1, channel) * weight_above_left;
                    sampled[channel] = (sum + 32768) >> 16;
                }
                if sampled[3] == 0 {
                    continue;
                }
                let canvas_index = (target_y * canvas_width + target_x) as usize;
                let mut scale = strength;
                if let Some(clip_values) = clip_data {
                    scale = (scale * clip_values[canvas_index] as u32 + 127) / 255;
                }
                if scale == 0 {
                    continue;
                }
                let source_alpha = (sampled[3] * scale) >> 8;
                let keep = 255 - source_alpha.min(255);
                let target_index = canvas_index * 4;
                for channel in 0..4 {
                    let source_value = (sampled[channel] * scale) >> 8;
                    let existing = target[target_index + channel] as u32;
                    let blended = source_value + (existing * keep + 127) / 255;
                    target[target_index + channel] = blended.min(255) as u8;
                }
            }
        }
    }

    // ----- Internals ---------------------------------------------------------------------

    /// Turns a device-space path into the outline of its stroke, using the current line style
    /// and transform.
    ///
    /// @param device_path The path in device space.
    /// @return The stroke outline in device space, or None when it is empty.
    fn stroke_outline(&self, device_path: &Path) -> Option<Path> {
        let transform = self.state.transform;
        let inverse = transform.invert()?;
        let user_path = device_path.clone().transform(inverse)?;
        let stroke = Stroke {
            width: self.state.line_width,
            line_cap: self.state.line_cap,
            line_join: self.state.line_join,
            ..Stroke::default()
        };
        let resolution = (transform.sx * transform.sy - transform.kx * transform.ky).abs().sqrt().max(0.1);
        let outline = user_path.stroke(&stroke, resolution)?;
        outline.transform(transform)
    }

    /// Paints a device-space path with a style, drawing the shadow first when one is set.
    ///
    /// @param path The path in device space.
    /// @param style The colour or gradient.
    /// @param rule The fill rule.
    fn fill_device_path(&mut self, path: &Path, style: &Style, rule: FillRule) {
        let alpha = self.state.global_alpha;
        if alpha <= 0.0 {
            return;
        }
        let (shader, shape_alpha) = match style {
            Style::Color(color) => (Shader::SolidColor(with_alpha(*color, alpha)), color.alpha() * alpha),
            Style::Gradient(gradient) => (gradient.to_shader(self.state.transform, alpha), gradient.max_alpha() * alpha),
        };
        if shape_alpha <= 0.0 {
            return;
        }
        self.draw_shadow(path, rule, shape_alpha);
        let paint = Paint {
            shader,
            anti_alias: true,
            ..Paint::default()
        };
        let clip = self.state.clip.clone();
        self.pixmap.fill_path(path, &paint, rule, Transform::identity(), clip.as_deref());
    }

    /// Draws the blurred, tinted, offset shadow of a shape (Canvas shadow model).
    ///
    /// @param path The shape in device space.
    /// @param rule The fill rule.
    /// @param shape_alpha The opacity of the shape itself (0..1).
    fn draw_shadow(&mut self, path: &Path, rule: FillRule, shape_alpha: f32) {
        let shadow_color = self.state.shadow_color;
        let blur = self.state.shadow_blur;
        let offset_x = self.state.shadow_offset_x;
        let offset_y = self.state.shadow_offset_y;
        if shadow_color.alpha() <= 0.0 || (blur <= 0.0 && offset_x == 0.0 && offset_y == 0.0) {
            return;
        }
        let sigma = blur / 2.0;
        let padding = (sigma * 3.0).ceil() + 1.0;
        let bounds = path.bounds();
        let left = (bounds.left() + offset_x - padding).floor().max(0.0);
        let top = (bounds.top() + offset_y - padding).floor().max(0.0);
        let right = (bounds.right() + offset_x + padding).ceil().min(self.width() as f32);
        let bottom = (bounds.bottom() + offset_y + padding).ceil().min(self.height() as f32);
        if right <= left || bottom <= top {
            return;
        }
        // Soft shadows are rendered at a lower resolution and scaled back up: they look the
        // same and cost a fraction of the time.
        let downscale: f32 = if sigma < 1.5 {
            1.0
        } else if sigma < 4.5 {
            2.0
        } else {
            4.0
        };
        let region_width = ((right - left) / downscale).ceil().max(1.0) as u32;
        let region_height = ((bottom - top) / downscale).ceil().max(1.0) as u32;
        let Some(mut coverage) = Mask::new(region_width, region_height) else {
            return;
        };
        let shift = Transform::from_row(
            1.0 / downscale,
            0.0,
            0.0,
            1.0 / downscale,
            (offset_x - left) / downscale,
            (offset_y - top) / downscale,
        );
        coverage.fill_path(path, rule, true, shift);

        let mut alpha_values = coverage.data().to_vec();
        blur_alpha(&mut alpha_values, region_width as usize, region_height as usize, sigma / downscale);

        let source = ShadowSource::new(&alpha_values, region_width as usize, region_height as usize, downscale);
        let strength = (shadow_color.alpha() * shape_alpha * 255.0).round().clamp(0.0, 255.0) as u32;
        let tint = [
            (shadow_color.red() * 255.0).round() as u32,
            (shadow_color.green() * 255.0).round() as u32,
            (shadow_color.blue() * 255.0).round() as u32,
        ];
        let region = (left as usize, top as usize, right as usize, bottom as usize);
        let clip = self.state.clip.clone();
        let canvas_width = self.width() as usize;
        composite_shadow(
            self.pixmap.data_mut(),
            canvas_width,
            clip.as_deref(),
            &source,
            region,
            tint,
            strength,
        );
    }
}

/// Divides by 255 with rounding, for values up to 255 × 255.
///
/// @param value The value.
/// @return value / 255, rounded.
fn divide_by_255(value: u32) -> u32 {
    (value + 128 + ((value + 128) >> 8)) >> 8
}

/// Where one canvas pixel samples a (possibly smaller) shadow buffer along one axis.
#[derive(Clone, Copy)]
struct SamplePosition {
    /// First source index.
    first: usize,
    /// Second source index (equal to `first` at the edge).
    second: usize,
    /// Weight of `second`, 0..256.
    weight: u32,
}

/// Works out the sample positions for every canvas pixel along one axis.
///
/// @param target_length Number of canvas pixels.
/// @param source_length Number of buffer pixels.
/// @param downscale Canvas pixels per buffer pixel.
/// @return One position per canvas pixel.
fn sample_positions(target_length: usize, source_length: usize, downscale: f32) -> Vec<SamplePosition> {
    let mut positions = Vec::with_capacity(target_length);
    let last = source_length.saturating_sub(1);
    for index in 0..target_length {
        if downscale <= 1.0 {
            let clamped = index.min(last);
            positions.push(SamplePosition { first: clamped, second: clamped, weight: 0 });
            continue;
        }
        let source = ((index as f32 + 0.5) / downscale - 0.5).clamp(0.0, last as f32);
        let first = source.floor() as usize;
        let second = (first + 1).min(last);
        let weight = ((source - first as f32) * 256.0).round() as u32;
        positions.push(SamplePosition { first, second, weight });
    }
    positions
}

/// A blurred shadow coverage buffer, possibly at a lower resolution than the canvas, with
/// precomputed bilinear sample positions.
struct ShadowSource<'a> {
    alpha: &'a [u8],
    width: usize,
    columns: Vec<SamplePosition>,
    rows: Vec<SamplePosition>,
}

impl<'a> ShadowSource<'a> {
    /// @param alpha The blurred coverage, `width × height` values.
    /// @param width Buffer width.
    /// @param height Buffer height.
    /// @param downscale Canvas pixels per buffer pixel.
    /// @return The source, covering `width × downscale` by `height × downscale` canvas pixels.
    fn new(alpha: &'a [u8], width: usize, height: usize, downscale: f32) -> ShadowSource<'a> {
        let target_width = (width as f32 * downscale).ceil() as usize;
        let target_height = (height as f32 * downscale).ceil() as usize;
        ShadowSource {
            alpha,
            width,
            columns: sample_positions(target_width, width, downscale),
            rows: sample_positions(target_height, height, downscale),
        }
    }

    /// Reads the coverage at a canvas pixel inside the shadow region.
    ///
    /// @param local_x Pixel x relative to the region's left edge.
    /// @param row The sample position of the pixel's row.
    /// @return Coverage 0..255.
    fn sample(&self, local_x: usize, row: SamplePosition) -> u32 {
        let Some(column) = self.columns.get(local_x) else {
            return 0;
        };
        let top_left = self.alpha[row.first * self.width + column.first] as u32;
        let top_right = self.alpha[row.first * self.width + column.second] as u32;
        let bottom_left = self.alpha[row.second * self.width + column.first] as u32;
        let bottom_right = self.alpha[row.second * self.width + column.second] as u32;
        let top_row = top_left * (256 - column.weight) + top_right * column.weight;
        let bottom_row = bottom_left * (256 - column.weight) + bottom_right * column.weight;
        (top_row * (256 - row.weight) + bottom_row * row.weight) >> 16
    }
}

/// Blends a tinted shadow into premultiplied RGBA pixels (source-over).
///
/// @param pixels The canvas pixels.
/// @param canvas_width Canvas width in pixels.
/// @param clip Optional clip mask (canvas sized).
/// @param source The blurred coverage.
/// @param region (left, top, right, bottom) of the shadow on the canvas, in pixels.
/// @param tint Shadow colour (unpremultiplied red, green, blue, 0..255).
/// @param strength Shadow opacity (shadow colour alpha × shape alpha), 0..255.
fn composite_shadow(
    pixels: &mut [u8],
    canvas_width: usize,
    clip: Option<&Mask>,
    source: &ShadowSource,
    region: (usize, usize, usize, usize),
    tint: [u32; 3],
    strength: u32,
) {
    let (left, top, right, bottom) = region;
    let clip_data = clip.map(|mask| mask.data());
    for y in top..bottom {
        let Some(row) = source.rows.get(y - top).copied() else {
            break;
        };
        for x in left..right {
            let coverage = source.sample(x - left, row);
            if coverage == 0 {
                continue;
            }
            let mut alpha = divide_by_255(coverage * strength);
            let canvas_index = y * canvas_width + x;
            if let Some(clip_values) = clip_data {
                alpha = divide_by_255(alpha * clip_values[canvas_index] as u32);
            }
            if alpha == 0 {
                continue;
            }
            let keep = 255 - alpha;
            let offset = canvas_index * 4;
            for channel in 0..3 {
                let existing = pixels[offset + channel] as u32;
                pixels[offset + channel] = divide_by_255(tint[channel] * alpha + existing * keep).min(255) as u8;
            }
            let existing_alpha = pixels[offset + 3] as u32;
            pixels[offset + 3] = (alpha + divide_by_255(existing_alpha * keep)).min(255) as u8;
        }
    }
}

/// Works out the signed sweep of an arc the way Canvas does.
///
/// @param start_angle Start angle in radians.
/// @param end_angle End angle in radians.
/// @param counter_clockwise Direction.
/// @return The sweep in radians (negative when counter-clockwise).
fn arc_sweep(start_angle: f32, end_angle: f32, counter_clockwise: bool) -> f32 {
    let full_turn = PI * 2.0;
    let raw = end_angle - start_angle;
    if !counter_clockwise {
        if raw >= full_turn {
            return full_turn;
        }
        let mut sweep = raw % full_turn;
        if sweep < 0.0 {
            sweep += full_turn;
        }
        sweep
    } else {
        if raw <= -full_turn {
            return -full_turn;
        }
        let mut sweep = raw % full_turn;
        if sweep > 0.0 {
            sweep -= full_turn;
        }
        sweep
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_sweep_matches_canvas_rules() {
        assert!((arc_sweep(0.0, PI, false) - PI).abs() < 1e-5);
        assert!((arc_sweep(0.0, PI, true) + PI).abs() < 1e-5);
        assert!((arc_sweep(0.0, PI * 2.0, false) - PI * 2.0).abs() < 1e-5);
        assert!((arc_sweep(PI, 0.0, false) - PI).abs() < 1e-5);
    }

    #[test]
    fn full_ellipse_has_expected_bounds() {
        let mut canvas = Canvas::new(100, 100);
        canvas.begin_path();
        canvas.ellipse(50.0, 40.0, 20.0, 10.0, 0.0, 0.0, PI * 2.0, false);
        let path = canvas.path.clone().finish().unwrap();
        let bounds = path.bounds();
        assert!((bounds.left() - 30.0).abs() < 0.5);
        assert!((bounds.right() - 70.0).abs() < 0.5);
        assert!((bounds.top() - 30.0).abs() < 0.5);
        assert!((bounds.bottom() - 50.0).abs() < 0.5);
    }

    #[test]
    fn transform_applies_to_points_when_added() {
        let mut canvas = Canvas::new(100, 100);
        canvas.translate(10.0, 20.0);
        canvas.begin_path();
        canvas.move_to(0.0, 0.0);
        canvas.translate(50.0, 0.0);
        canvas.line_to(0.0, 10.0);
        let bounds = canvas.path.clone().finish().unwrap().bounds();
        assert!((bounds.left() - 10.0).abs() < 1e-4);
        assert!((bounds.right() - 60.0).abs() < 1e-4);
    }

    #[test]
    fn fill_and_shadow_draw_pixels() {
        let mut canvas = Canvas::new(60, 60);
        canvas.set_fill_color("#ff0000");
        canvas.set_shadow_color("rgba(0,0,0,0.5)");
        canvas.set_shadow_blur(6.0);
        canvas.set_shadow_offset_y(4.0);
        canvas.begin_path();
        canvas.rect(20.0, 20.0, 10.0, 10.0);
        canvas.fill();
        let pixels = canvas.pixels();
        let inside = (25 * 60 + 25) * 4;
        assert_eq!(pixels[inside + 3], 255);
        let below = (33 * 60 + 25) * 4;
        assert!(pixels[below + 3] > 0, "shadow should appear below the square");
    }
}
