//! Drawing primitives: glass, glows, shadows, ring arcs, icons, flags, spinner.
//!
//! Everything is a plain egui shape or mesh, so it goes through both the
//! window's CPU rasterizer (`crate::render`) and wgpu in snapshot tests. Meshes
//! built here carry their own anti-aliasing: a one-pixel feather fading to
//! transparent, centered on the true edge (what egui's tessellator does).

use std::f32::consts::{PI, SQRT_2, TAU};

use egui::{
    emath::Rot2, epaint::Mesh, pos2, vec2, Color32, CornerRadius, Painter, Pos2, Rect, Shape,
    Stroke, StrokeKind, Vec2,
};

use super::colors;

/// Glass corner radius, points.
pub const GLASS_RADIUS: f32 = 16.0;
/// `.tauri .wd.warn { box-shadow: inset 0 0 22px ... }`.
const WARN_GLOW_BLUR: f32 = 22.0;
/// Points per quarter circle in rounded-rect outlines.
const CORNER_POINTS: usize = 16;
/// Concentric rings per blurred band (inner glow, soft shadow side).
const BAND_STEPS: usize = 16;
/// `.wd .flags svg { border-radius: 2px }`.
const FLAG_RADIUS: f32 = 2.0;
/// Waiting spinner: one turn per 0.8 s.
const SPIN_PERIOD: f64 = 0.8;

const CLEAR: Color32 = Color32::TRANSPARENT;

/// One physical pixel, in points.
fn one_px(p: &Painter) -> f32 {
    1.0 / p.pixels_per_point()
}

/// Premultiplied color scaled by `f` (0..=1): the same hue, `f` times as opaque.
fn scale(c: Color32, f: f32) -> Color32 {
    let f = f.clamp(0.0, 1.0);
    let m = |v: u8| (f32::from(v) * f).round() as u8;
    Color32::from_rgba_premultiplied(m(c.r()), m(c.g()), m(c.b()), m(c.a()))
}

/// Per-channel premultiplied lerp (CSS gradients interpolate premultiplied sRGB).
fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgba_premultiplied(
        m(a.r(), b.r()),
        m(a.g(), b.g()),
        m(a.b(), b.b()),
        m(a.a(), b.a()),
    )
}

/// Complementary error function (Abramowitz & Stegun 7.1.26, |error| < 1.5e-7).
fn erfc(x: f32) -> f32 {
    let z = f64::from(x.abs());
    let t = 1.0 / (1.0 + 0.327_591_1 * z);
    let poly = t
        * (0.254_829_592
            + t * (-0.284_496_736
                + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    let r = poly * (-z * z).exp();
    (if x >= 0.0 { r } else { 2.0 - r }) as f32
}

/// Coverage of a shape blurred by a Gaussian of deviation `sigma`, at signed
/// distance `d` from its edge (positive outside): `½·erfc(d / σ√2)`. This is
/// CSS `box-shadow` blur, with σ = blur radius / 2.
fn blurred_edge(d: f32, sigma: f32) -> f32 {
    0.5 * erfc(d / (sigma * SQRT_2))
}

/// The outline of the rounded rect `rect`/`radius` offset by `d` (outward when
/// positive), clockwise on screen from the left end of the top-left corner;
/// always `4 * (CORNER_POINTS + 1)` points. A negative offset larger than the
/// rect collapses it to its center line instead of turning it inside out.
fn rrect_outline(rect: Rect, radius: f32, d: f32) -> Vec<Pos2> {
    let c = rect.center();
    let hw = (rect.width() / 2.0 + d).max(0.0);
    let hh = (rect.height() / 2.0 + d).max(0.0);
    let r = (radius + d).clamp(0.0, hw.min(hh));
    let (dx, dy) = (hw - r, hh - r);
    let corners = [
        (vec2(-dx, -dy), PI),
        (vec2(dx, -dy), 1.5 * PI),
        (vec2(dx, dy), 0.0),
        (vec2(-dx, dy), 0.5 * PI),
    ];
    let mut pts = Vec::with_capacity(4 * (CORNER_POINTS + 1));
    for (off, start) in corners {
        for i in 0..=CORNER_POINTS {
            let a = start + 0.5 * PI * i as f32 / CORNER_POINTS as f32;
            pts.push(c + off + r * vec2(a.cos(), a.sin()));
        }
    }
    pts
}

fn add_quad(mesh: &mut Mesh, a: u32, b: u32, c: u32, d: u32) {
    mesh.add_triangle(a, b, c);
    mesh.add_triangle(a, c, d);
}

/// Concentric outlines of a rounded rect, one per stop `(offset, opacity)`,
/// joined by quads; each vertex is `color(pos)` scaled by its stop's opacity.
/// With `fill`, a fan from the center closes the first outline.
fn band_mesh(
    rect: Rect,
    radius: f32,
    stops: &[(f32, f32)],
    fill: bool,
    color: impl Fn(Pos2) -> Color32,
) -> Mesh {
    let mut mesh = Mesh::default();
    let n = (4 * (CORNER_POINTS + 1)) as u32;
    let mut prev: Option<u32> = None;
    for &(d, opacity) in stops {
        let base = mesh.vertices.len() as u32;
        for q in rrect_outline(rect, radius, d) {
            mesh.colored_vertex(q, scale(color(q), opacity));
        }
        match prev {
            Some(prev) => {
                for i in 0..n {
                    let j = (i + 1) % n;
                    add_quad(&mut mesh, prev + i, prev + j, base + j, base + i);
                }
            }
            None if fill => {
                let center = mesh.vertices.len() as u32;
                mesh.colored_vertex(rect.center(), scale(color(rect.center()), opacity));
                for i in 0..n {
                    mesh.add_triangle(center, base + i, base + (i + 1) % n);
                }
            }
            None => {}
        }
        prev = Some(base);
    }
    mesh
}

/// The glass panel filling `rect`: fill, 1 px top inner highlight, 1 px border.
/// With `warn` (near limit) the border turns red and a red glow lines the inside.
pub fn glass(p: &Painter, rect: Rect, warn: bool) {
    let corners = CornerRadius::same(GLASS_RADIUS as u8);
    p.rect_filled(rect, corners, colors::GLASS_FILL);
    // Inset shadows paint above the background, below the border, inside the padding box.
    let padding = rect.shrink(1.0);
    let inner_radius = GLASS_RADIUS - 1.0;
    if warn {
        inner_glow(p, padding, inner_radius, colors::WARN_GLOW, WARN_GLOW_BLUR);
    }
    p.add(top_highlight(
        padding,
        inner_radius,
        colors::GLASS_HIGHLIGHT,
    ));
    let border = if warn {
        colors::WARN_BORDER
    } else {
        colors::GLASS_BORDER
    };
    p.rect_stroke(rect, corners, Stroke::new(1.0, border), StrokeKind::Inside);
}

/// CSS `inset 0 1px 0 color` on a rounded padding box: what lies inside the box
/// but outside the box moved 1 pt down. A 1 pt band under the top edge that
/// follows the corner arcs and thins to nothing where they turn vertical.
fn top_highlight(padding: Rect, radius: f32, color: Color32) -> Mesh {
    let mut mesh = Mesh::default();
    // The top-left and top-right corners, from the left side over the top to the right side.
    let top = &rrect_outline(padding, radius, 0.0)[..2 * (CORNER_POINTS + 1)];
    for &q in top {
        mesh.colored_vertex(q, color);
        mesh.colored_vertex(q + vec2(0.0, 1.0), color);
    }
    for i in 0..top.len() as u32 - 1 {
        let (a, b) = (2 * i, 2 * i + 2);
        add_quad(&mut mesh, a, b, b + 1, a + 1);
    }
    mesh
}

/// A glow along the inside of a rounded rect: CSS `box-shadow: inset 0 0 <width> color`.
/// The alpha falls off from the edge as a Gaussian-blurred edge (σ = width / 2,
/// half the color's alpha at the edge, ~0 at 3σ), built from concentric
/// rounded-rect rings.
pub fn inner_glow(p: &Painter, rect: Rect, radius: f32, color: Color32, width: f32) {
    if width > 0.0 && color.a() > 0 {
        p.add(inner_glow_mesh(rect, radius, color, width));
    }
}

fn inner_glow_mesh(rect: Rect, radius: f32, color: Color32, width: f32) -> Mesh {
    let sigma = width / 2.0;
    let depth = 3.0 * sigma;
    let stops: Vec<(f32, f32)> = (0..=BAND_STEPS)
        .map(|k| {
            let inside = depth * k as f32 / BAND_STEPS as f32;
            (-inside, blurred_edge(inside, sigma))
        })
        .collect();
    band_mesh(rect, radius, &stops, false, |_| color)
}

/// A rounded rect filled with a vertical gradient from `top` to `bottom`, anti-aliased.
pub fn gradient_button(p: &Painter, rect: Rect, radius: f32, top: Color32, bottom: Color32) {
    let px = one_px(p);
    let h = rect.height().max(f32::EPSILON);
    let color = |q: Pos2| lerp_color(top, bottom, (q.y - rect.top()) / h);
    p.add(band_mesh(
        rect,
        radius,
        &[(-px / 2.0, 1.0), (px / 2.0, 0.0)],
        true,
        color,
    ));
}

/// CSS `box-shadow: 0 <offset_y> <blur> color` of a rounded rect (drawn under it).
pub fn soft_shadow(p: &Painter, rect: Rect, radius: f32, offset_y: f32, blur: f32, color: Color32) {
    let rect = rect.translate(vec2(0.0, offset_y));
    if blur <= 0.0 {
        p.rect_filled(rect, CornerRadius::same(radius.round() as u8), color);
        return;
    }
    let sigma = blur / 2.0;
    let depth = 3.0 * sigma;
    let stops: Vec<(f32, f32)> = (0..=2 * BAND_STEPS)
        .map(|k| {
            let d = -depth + depth * k as f32 / BAND_STEPS as f32;
            (d, blurred_edge(d, sigma))
        })
        .collect();
    p.add(band_mesh(rect, radius, &stops, true, |_| color));
}

/// A round-capped arc of the circle (`center`, `r`), `width` wide, from
/// `from_turn` to `to_turn` (turns from 12 o'clock, clockwise). `px` is one
/// physical pixel in points: every outer edge gets a `px` feather. Empty when
/// `to_turn - from_turn <= 0`. A sweep whose caps would touch or overlap across the
/// gap (≥ 1 − the two caps' angular extent, ~95 % for the r 23 ring) is a closed
/// ring without caps, so a translucent arc never blends twice where they meet.
pub fn arc_mesh(
    center: Pos2,
    r: f32,
    width: f32,
    from_turn: f32,
    to_turn: f32,
    color: Color32,
    px: f32,
) -> Mesh {
    arc(center, r, width, from_turn, to_turn, color, px, true)
}

#[expect(clippy::too_many_arguments)]
fn arc(
    center: Pos2,
    r: f32,
    width: f32,
    from: f32,
    to: f32,
    color: Color32,
    px: f32,
    caps: bool,
) -> Mesh {
    let mut mesh = Mesh::default();
    let sweep = to - from;
    let finite = [center.x, center.y, r, width, from, to, px]
        .iter()
        .all(|v| v.is_finite());
    if !finite || sweep <= 0.0 || r <= 0.0 || width <= 0.0 || px <= 0.0 {
        return mesh;
    }
    let hw = width / 2.0;
    // Each round cap reaches `hw + px / 2` past its end: both together span this many turns.
    let caps_turns = 2.0 * (hw + px / 2.0) / (TAU * r);
    let closed = sweep >= 1.0 || (caps && sweep >= 1.0 - caps_turns);
    let sweep = if closed { 1.0 } else { sweep };
    // A stroke thinner than a pixel becomes a fainter one-pixel stroke.
    let (solid, color) = if hw >= px / 2.0 {
        (hw - px / 2.0, color)
    } else {
        (0.0, scale(color, width / px))
    };
    let feather = solid + px;
    // Cross-section, inner to outer: clear, solid, centerline, solid, clear. The
    // centerline vertex is the caps' center, so caps share the strip's end edge.
    let section = [
        (-feather, CLEAR),
        (-solid, color),
        (0.0, color),
        (solid, color),
        (feather, CLEAR),
    ];
    let segs = ((TAU * (r + feather) * sweep / (2.0 * px)).ceil() as usize).max(1);
    let sections = if closed { segs } else { segs + 1 };
    for k in 0..sections {
        let n = radial(from + sweep * k as f32 / segs as f32);
        for (offset, c) in section {
            mesh.colored_vertex(center + n * (r + offset).max(0.0), c);
        }
    }
    for k in 0..segs {
        let a = (5 * k) as u32;
        let b = (5 * ((k + 1) % sections)) as u32;
        for i in 0..4 {
            add_quad(&mut mesh, a + i, a + i + 1, b + i + 1, b + i);
        }
    }
    if caps && !closed {
        let ends = [(from, -1.0, 0), (from + sweep, 1.0, 5 * segs as u32)];
        for (turn, sign, section_start) in ends {
            add_cap(&mut mesh, center, r, turn, sign, section_start, solid, px);
        }
    }
    mesh
}

/// Unit vector from the center toward `turn` (12 o'clock = up, clockwise).
fn radial(turn: f32) -> Vec2 {
    let a = TAU * turn;
    vec2(a.sin(), -a.cos())
}

/// A half disc closing the arc at `turn`, bulging along `sign` × the clockwise
/// tangent. Its diameter is the end cross-section starting at vertex `s`.
#[expect(clippy::too_many_arguments)]
fn add_cap(
    mesh: &mut Mesh,
    center: Pos2,
    r: f32,
    turn: f32,
    sign: f32,
    s: u32,
    solid: f32,
    px: f32,
) {
    let a = TAU * turn;
    let n = radial(turn);
    let t = sign * vec2(a.cos(), a.sin());
    let end = center + n * r;
    let color = mesh.vertices[s as usize + 2].color;
    let feather = solid + px;
    let steps = ((PI * feather / (2.0 * px)).ceil() as usize).max(4);
    let (mut prev_solid, mut prev_clear) = (s + 3, s + 4);
    for j in 1..=steps {
        let (solid_i, clear_i) = if j == steps {
            (s + 1, s)
        } else {
            let phi = PI * j as f32 / steps as f32;
            let u = n * phi.cos() + t * phi.sin();
            let i = mesh.vertices.len() as u32;
            mesh.colored_vertex(end + u * solid, color);
            mesh.colored_vertex(end + u * feather, CLEAR);
            (i, i + 1)
        };
        mesh.add_triangle(s + 2, prev_solid, solid_i);
        add_quad(mesh, prev_solid, prev_clear, clear_i, solid_i);
        (prev_solid, prev_clear) = (solid_i, clear_i);
    }
}

/// The Waiting spinner centered on `center`: a 12 pt circle with a 2 pt ring
/// `rgba(255,255,255,.2)` and a white top quarter, one turn per 0.8 s of `t`
/// (seconds); static when `reduced`.
pub fn spinner(p: &Painter, center: Pos2, t: f64, reduced: bool) {
    let px = one_px(p);
    let (r, width) = (5.0, 2.0);
    p.add(arc(
        center,
        r,
        width,
        0.0,
        1.0,
        colors::SPIN_TRACK,
        px,
        false,
    ));
    let turn = if reduced {
        0.0
    } else {
        (t / SPIN_PERIOD).rem_euclid(1.0) as f32
    };
    // CSS border-top on a circle: a quarter centered on 12 o'clock with radial ends.
    let head = arc(
        center,
        r,
        width,
        turn - 0.125,
        turn + 0.125,
        Color32::WHITE,
        px,
        false,
    );
    p.add(head);
}

/// Lucide icons (24-unit viewBox, round caps and joins).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    RefreshCw,
    Minus,
    X,
}

/// The icon's subpaths in viewBox units, arcs flattened.
fn icon_paths(kind: Icon) -> Vec<Vec<Pos2>> {
    match kind {
        // M5 12h14
        Icon::Minus => vec![vec![pos2(5.0, 12.0), pos2(19.0, 12.0)]],
        // M18 6 6 18 / m6 6 12 12
        Icon::X => vec![
            vec![pos2(18.0, 6.0), pos2(6.0, 18.0)],
            vec![pos2(6.0, 6.0), pos2(18.0, 18.0)],
        ],
        Icon::RefreshCw => {
            // M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8
            let mut top = vec![pos2(3.0, 12.0)];
            svg_arc(&mut top, pos2(12.0, 3.0), 9.0, false, true);
            svg_arc(&mut top, pos2(18.74, 5.74), 9.75, false, true);
            top.push(pos2(21.0, 8.0));
            // M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16
            let mut bottom = vec![pos2(21.0, 12.0)];
            svg_arc(&mut bottom, pos2(12.0, 21.0), 9.0, false, true);
            svg_arc(&mut bottom, pos2(5.26, 18.26), 9.75, false, true);
            bottom.push(pos2(3.0, 16.0));
            vec![
                top,
                // M21 3v5h-5
                vec![pos2(21.0, 3.0), pos2(21.0, 8.0), pos2(16.0, 8.0)],
                bottom,
                // M8 16H3v5
                vec![pos2(8.0, 16.0), pos2(3.0, 16.0), pos2(3.0, 21.0)],
            ]
        }
    }
}

/// Appends the SVG arc (`A r r 0 large sweep to`, circular, no rotation) from
/// the path's last point to `to`, flattened to chords of at most 0.5 unit.
fn svg_arc(path: &mut Vec<Pos2>, to: Pos2, r: f32, large: bool, sweep: bool) {
    let from = *path.last().expect("an arc starts at a point");
    let half = (from - to) / 2.0;
    let d2 = half.length_sq();
    if d2 == 0.0 {
        return;
    }
    // SVG scales a radius that is too small up to the half chord.
    let r = r.max(d2.sqrt());
    let k = ((r * r - d2) / d2).max(0.0).sqrt() * if large == sweep { -1.0 } else { 1.0 };
    let center = from.lerp(to, 0.5) + vec2(k * half.y, -k * half.x);
    let a0 = (from - center).angle();
    let mut delta = (to - center).angle() - a0;
    // sweep = 1 is the positive angle direction: clockwise on a y-down screen.
    if sweep && delta < 0.0 {
        delta += TAU;
    } else if !sweep && delta > 0.0 {
        delta -= TAU;
    }
    let steps = ((delta.abs() * r / 0.5).ceil() as usize).max(1);
    for i in 1..steps {
        let a = a0 + delta * i as f32 / steps as f32;
        path.push(center + r * vec2(a.cos(), a.sin()));
    }
    path.push(to);
}

/// Draws `kind` fitted in `rect` (square, centered). `stroke` is in viewBox
/// units like Lucide's `strokeWidth` (2 = 1 pt at 12 pt).
pub fn icon(p: &Painter, kind: Icon, rect: Rect, color: Color32, stroke: f32) {
    icon_rotated(p, kind, rect, color, stroke, 0.0);
}

/// [`icon`] turned clockwise by `turns` around the rect's center (the refresh spin).
pub fn icon_rotated(p: &Painter, kind: Icon, rect: Rect, color: Color32, stroke: f32, turns: f32) {
    let s = rect.width().min(rect.height()) / 24.0;
    let rot = Rot2::from_angle(TAU * turns);
    let map = |q: Pos2| rect.center() + rot * ((q - pos2(12.0, 12.0)) * s);
    let width = stroke * s;
    for path in icon_paths(kind) {
        let pts: Vec<Pos2> = path.iter().map(|&q| map(q)).collect();
        // Round caps and joins: a disc on each end and on each sharp corner.
        for (i, &q) in pts.iter().enumerate() {
            let sharp = i > 0 && i + 1 < pts.len() && {
                let (a, b) = (pts[i] - pts[i - 1], pts[i + 1] - pts[i]);
                a.normalized().dot(b.normalized()) < 0.9
            };
            if i == 0 || i + 1 == pts.len() || sharp {
                p.circle_filled(q, width / 2.0, color);
            }
        }
        p.add(Shape::line(pts, Stroke::new(width, color)));
    }
}

/// `viewBox` `vw`×`vh` fitted in `rect` like SVG's default `xMidYMid meet`.
fn fit(rect: Rect, vw: f32, vh: f32) -> Rect {
    let s = (rect.width() / vw).min(rect.height() / vh);
    Rect::from_center_size(rect.center(), vec2(vw * s, vh * s))
}

fn cross(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}

/// Twice the signed area; positive for clockwise on a y-down screen.
fn signed_area(poly: &[Pos2]) -> f32 {
    (0..poly.len())
        .map(|i| cross(poly[i].to_vec2(), poly[(i + 1) % poly.len()].to_vec2()))
        .sum()
}

/// Sutherland–Hodgman: `subject` (convex) clipped by `clip` (convex, clockwise).
fn clip_convex(subject: &[Pos2], clip: &[Pos2]) -> Vec<Pos2> {
    let mut out = subject.to_vec();
    for i in 0..clip.len() {
        let (a, b) = (clip[i], clip[(i + 1) % clip.len()]);
        let e = b - a;
        if e.length_sq() < 1e-12 || out.is_empty() {
            continue;
        }
        let side = |q: Pos2| cross(e, q - a);
        let input = std::mem::take(&mut out);
        for j in 0..input.len() {
            let (p, q) = (input[j], input[(j + 1) % input.len()]);
            let (sp, sq) = (side(p), side(q));
            if (sp >= 0.0) != (sq >= 0.0) {
                out.push(p + (q - p) * (sp / (sp - sq)));
            }
            if sq >= 0.0 {
                out.push(q);
            }
        }
    }
    out
}

/// Fills `poly` (convex, viewBox units mapped by `map`) clipped to `clip`.
fn fill_clipped(p: &Painter, poly: &[Pos2], clip: &[Pos2], color: Color32) {
    let mut pts = clip_convex(poly, clip);
    if pts.len() < 3 {
        return;
    }
    if signed_area(&pts) < 0.0 {
        pts.reverse();
    }
    p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
}

fn rect_poly(x0: f32, y0: f32, x1: f32, y1: f32) -> [Pos2; 4] {
    [pos2(x0, y0), pos2(x1, y0), pos2(x1, y1), pos2(x0, y1)]
}

/// The flag's clip: the 2 pt rounded box of `rect` intersected with the fitted viewBox.
fn flag_clip(rect: Rect, view: Rect) -> Vec<Pos2> {
    let rounded = rrect_outline(rect, FLAG_RADIUS, 0.0);
    let v = rect_poly(view.left(), view.top(), view.right(), view.bottom());
    clip_convex(&rounded, &v)
}

/// The French flag (`viewBox 0 0 3 2`) fitted in `rect`, corners rounded 2 pt.
pub fn flag_fr(p: &Painter, rect: Rect) {
    let view = fit(rect, 3.0, 2.0);
    let clip = flag_clip(rect, view);
    let x = |u: f32| view.left() + view.width() * u / 3.0;
    // Stacked so each later stripe's anti-aliased edge blends over the previous
    // one instead of leaving a seam of background between them.
    let stripes = [
        (0.0, colors::FR_BLUE),
        (1.0, Color32::WHITE),
        (2.0, colors::FR_RED),
    ];
    for (u, color) in stripes {
        let poly = rect_poly(x(u), view.top(), view.right(), view.bottom());
        fill_clipped(p, &poly, &clip, color);
    }
}

/// The Union Jack (`viewBox 0 0 60 30`) fitted in `rect`, corners rounded 2 pt.
pub fn flag_uk(p: &Painter, rect: Rect) {
    let view = fit(rect, 60.0, 30.0);
    let clip = flag_clip(rect, view);
    let s = view.width() / 60.0;
    let map = |q: Pos2| view.min + q.to_vec2() * s;
    let fill = |poly: &[Pos2], color: Color32| {
        let poly: Vec<Pos2> = poly.iter().map(|&q| map(q)).collect();
        fill_clipped(p, &poly, &clip, color);
    };
    // A butt-capped stroke of `width` along a → b, as a quad.
    let stroke = |a: Pos2, b: Pos2, width: f32| -> [Pos2; 4] {
        let n = (b - a).normalized().rot90() * (width / 2.0);
        [a + n, b + n, b - n, a - n]
    };
    let diagonals = [
        (pos2(0.0, 0.0), pos2(60.0, 30.0)),
        (pos2(60.0, 0.0), pos2(0.0, 30.0)),
    ];

    // M0,0 v30 h60 v-30 z
    fill(&rect_poly(0.0, 0.0, 60.0, 30.0), colors::UK_BLUE);
    // M0,0 L60,30 M60,0 L0,30, white, 6
    for (a, b) in diagonals {
        fill(&stroke(a, b, 6.0), Color32::WHITE);
    }
    // The same in red, 4, clipped to `M30,15 h30 v15 z v15 h-30 z h-30 v-15 z v-15 h30 z`:
    // four triangles around the center, one per half diagonal (counter-changed).
    let c = pos2(30.0, 15.0);
    let triangles = [
        [c, pos2(60.0, 15.0), pos2(60.0, 30.0)],
        [c, pos2(30.0, 30.0), pos2(0.0, 30.0)],
        [c, pos2(0.0, 15.0), pos2(0.0, 0.0)],
        [c, pos2(30.0, 0.0), pos2(60.0, 0.0)],
    ];
    for (a, b) in diagonals {
        let red = stroke(a, b, 4.0);
        for tri in &triangles {
            let mut tri = tri.to_vec();
            if signed_area(&tri) < 0.0 {
                tri.reverse();
            }
            fill(&clip_convex(&red, &tri), colors::UK_RED);
        }
    }
    // M30,0 v30 M0,15 h60: white 10, then red 6.
    for (half, color) in [(5.0, Color32::WHITE), (3.0, colors::UK_RED)] {
        fill(&rect_poly(30.0 - half, 0.0, 30.0 + half, 30.0), color);
        fill(&rect_poly(0.0, 15.0 - half, 60.0, 15.0 + half), color);
    }
}

/// Test sheet of every primitive, 260×120 pt from `origin` (snapshot and
/// software-render comparisons).
#[cfg(test)]
pub(crate) fn paint_primitives_sheet(p: &Painter, origin: Pos2) {
    let at = |x: f32, y: f32| origin + vec2(x, y);
    let px = one_px(p);
    p.rect_filled(
        Rect::from_min_size(origin, vec2(260.0, 120.0)),
        0,
        Color32::from_rgb(0x2a, 0x2a, 0x33),
    );
    // The ring: r 23, stroke 6, track under the arc.
    let ring = |center: Pos2, used: f32, color: Color32| {
        p.add(arc_mesh(center, 23.0, 6.0, 0.0, 1.0, colors::TRACK, px));
        p.add(arc_mesh(center, 23.0, 6.0, 0.0, used, color, px));
    };
    for (i, used) in [0.0, 0.03, 0.62, 1.0].into_iter().enumerate() {
        ring(at(31.0 + 64.0 * i as f32, 30.0), used, colors::GREEN);
    }
    ring(at(31.0, 90.0), 0.4, colors::NO_LIMIT_ARC);

    let icon_rect = |x: f32| Rect::from_min_size(at(x, 64.0), vec2(12.0, 12.0));
    icon(p, Icon::Minus, icon_rect(66.0), colors::MUTE, 2.0);
    icon(p, Icon::X, icon_rect(82.0), Color32::WHITE, 2.0);
    icon(p, Icon::RefreshCw, icon_rect(98.0), colors::MUTE, 2.0);
    icon_rotated(
        p,
        Icon::RefreshCw,
        icon_rect(114.0),
        Color32::WHITE,
        2.0,
        0.125,
    );

    let fr = Rect::from_min_size(at(68.0, 84.0), vec2(16.0, 11.0));
    let uk = Rect::from_min_size(at(89.0, 84.0), vec2(16.0, 11.0));
    flag_fr(p, fr);
    flag_uk(p, uk);
    let outline = |r: Rect, w: f32, c: Color32| {
        p.rect_stroke(r, 2, Stroke::new(w, c), StrokeKind::Outside);
    };
    outline(fr, 1.0, colors::FLAG_OUTLINE);
    outline(uk, 1.5, colors::FLAG_OUTLINE_ACTIVE);

    spinner(p, at(74.0, 108.0), 0.0, true);
    spinner(p, at(94.0, 108.0), 0.2, false);

    glass(
        p,
        Rect::from_min_size(at(132.0, 64.0), vec2(72.0, 52.0)),
        true,
    );

    let button = Rect::from_min_size(at(210.0, 70.0), vec2(46.0, 28.0));
    soft_shadow(p, button, 10.0, 6.0, 18.0, colors::BTN_SHADOW);
    gradient_button(p, button, 10.0, colors::BTN_FROM, colors::BTN_TO);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_mesh_empty_at_zero() {
        let mesh = arc_mesh(
            Pos2::new(32.0, 32.0),
            23.0,
            6.0,
            0.0,
            0.0,
            Color32::WHITE,
            1.0,
        );
        assert!(mesh.vertices.is_empty());
        assert!(mesh.indices.is_empty());
        let backwards = arc_mesh(
            Pos2::new(32.0, 32.0),
            23.0,
            6.0,
            0.5,
            0.2,
            Color32::WHITE,
            1.0,
        );
        assert!(backwards.vertices.is_empty());
    }

    #[test]
    fn arc_mesh_has_no_nan() {
        let cases = [
            (23.0, 6.0, 0.0, 0.03, 1.0),
            (23.0, 6.0, 0.0, 0.62, 1.0),
            (23.0, 6.0, 0.0, 1.0, 1.0),
            (23.0, 6.0, 0.25, 0.250_000_1, 1.0),
            (23.0, 6.0, 0.9, 1.4, 1.0 / 1.5),
            (2.0, 6.0, 0.0, 0.5, 1.0),
            (23.0, 0.3, 0.0, 0.5, 1.0),
        ];
        for (r, w, from, to, px) in cases {
            let mesh = arc_mesh(pos2(32.0, 32.0), r, w, from, to, Color32::WHITE, px);
            assert!(!mesh.vertices.is_empty(), "{r} {w} {from} {to}");
            assert!(mesh.is_valid());
            let finite = |v: &egui::epaint::Vertex| v.pos.x.is_finite() && v.pos.y.is_finite();
            assert!(mesh.vertices.iter().all(finite), "{r} {w} {from} {to}");
        }
    }

    #[test]
    fn arc_mesh_starts_at_twelve_and_runs_clockwise() {
        let c = pos2(32.0, 32.0);
        // A quarter: from 12 o'clock to 3 o'clock, plus round caps of radius 3.
        let mesh = arc_mesh(c, 23.0, 6.0, 0.0, 0.25, Color32::WHITE, 1.0);
        let bounds = mesh.calc_bounds();
        assert!((bounds.top() - (32.0 - 26.5)).abs() < 0.01, "{bounds:?}");
        assert!((bounds.right() - (32.0 + 26.5)).abs() < 0.01, "{bounds:?}");
        // The start cap reaches 3.5 left of 12 o'clock, nothing beyond.
        assert!((bounds.left() - (32.0 - 3.5)).abs() < 0.01, "{bounds:?}");
        assert!((bounds.bottom() - (32.0 + 3.5)).abs() < 0.01, "{bounds:?}");
    }

    #[test]
    fn arc_mesh_segments_follow_arc_length() {
        // A full ring has no caps: 5 vertices per cross-section, ≥ 1 section per 2 px.
        let mesh = arc_mesh(pos2(0.0, 0.0), 23.0, 6.0, 0.0, 1.0, Color32::WHITE, 1.0);
        let outer_len = TAU * 26.5;
        assert!(mesh.vertices.len() / 5 >= (outer_len / 2.0) as usize);
        let hidpi = arc_mesh(pos2(0.0, 0.0), 23.0, 6.0, 0.0, 1.0, Color32::WHITE, 0.5);
        assert!(hidpi.vertices.len() > mesh.vertices.len());
    }

    /// How many of `mesh`'s triangles contain `q` (strictly inside).
    fn coverage(mesh: &Mesh, q: Pos2) -> usize {
        let cross = |a: Pos2, b: Pos2| (b - a).x * (q - a).y - (b - a).y * (q - a).x;
        mesh.indices
            .chunks(3)
            .filter(|t| {
                let [a, b, c] = [0, 1, 2].map(|i| mesh.vertices[t[i] as usize].pos);
                let d = [cross(a, b), cross(b, c), cross(c, a)];
                d.iter().all(|&v| v > 1e-4) || d.iter().all(|&v| v < -1e-4)
            })
            .count()
    }

    #[test]
    fn near_full_arc_never_covers_a_point_twice() {
        // Translucent arcs at 96-99.9 %: the end cap would overlap the start cap and blend twice.
        let c = pos2(32.0, 32.0);
        for used in [0.96, 0.97, 0.98, 0.99, 0.999] {
            let mesh = arc_mesh(c, 23.0, 6.0, 0.0, used, colors::TRACK, 1.0);
            for k in 0..200 {
                let turn = 0.9 + 0.15 * k as f32 / 200.0;
                for dr in [-2.3, -0.7, 0.4, 1.9] {
                    let q = c + radial(turn) * (23.0 + dr);
                    let n = coverage(&mesh, q);
                    assert!(n <= 1, "{used}: {n} triangles at turn {turn} dr {dr}");
                }
            }
        }
        // Far from full, the caps stay (the gap is visible).
        let open = arc_mesh(c, 23.0, 6.0, 0.0, 0.9, colors::TRACK, 1.0);
        assert_eq!(coverage(&open, c + radial(0.95) * 23.0), 0);
        assert_eq!(coverage(&open, c + radial(0.905) * 23.4), 1);
    }

    #[test]
    fn blurred_edge_is_half_at_the_edge_and_fades_by_three_sigma() {
        assert!((blurred_edge(0.0, 11.0) - 0.5).abs() < 1e-6);
        assert!(blurred_edge(33.0, 11.0) < 0.002);
        assert!(blurred_edge(-33.0, 11.0) > 0.998);
        assert!(blurred_edge(5.0, 11.0) < blurred_edge(2.0, 11.0));
    }

    #[test]
    fn inner_glow_has_at_least_twelve_rings_inside_the_rect() {
        let rect = Rect::from_min_size(pos2(10.0, 10.0), vec2(242.0, 144.0));
        let mesh = inner_glow_mesh(rect, 15.0, colors::WARN_GLOW, 22.0);
        let rings = mesh.vertices.len() / (4 * (CORNER_POINTS + 1));
        assert!(rings >= 12, "{rings} rings");
        let bounds = mesh.calc_bounds();
        assert!(rect.expand(0.01).contains_rect(bounds), "{bounds:?}");
        // Strongest at the edge (half the color's alpha), fading inward.
        let edge = mesh.vertices[0].color.a();
        let deep = mesh.vertices.last().unwrap().color.a();
        assert_eq!(edge, colors::WARN_GLOW.a().div_ceil(2));
        assert!(deep < edge, "{edge} {deep}");
    }

    #[test]
    fn top_highlight_is_one_point_under_the_top_edge_between_the_sides() {
        let padding = Rect::from_min_size(pos2(1.0, 1.0), vec2(242.0, 144.0));
        let mesh = top_highlight(padding, 15.0, colors::GLASS_HIGHLIGHT);
        let b = mesh.calc_bounds();
        assert_eq!((b.left(), b.right()), (padding.left(), padding.right()));
        // It ends on the sides, where the corner arcs turn vertical (and the band thins to nothing).
        assert!(
            (b.top() - padding.top()).abs() < 1e-4
                && (b.bottom() - (padding.top() + 15.0 + 1.0)).abs() < 1e-4,
            "{b:?}"
        );
        // On the straight top edge the band is exactly rows 1..2; in the corners it follows the arc.
        let mut mid = mesh
            .vertices
            .iter()
            .filter(|v| (v.pos.x - 122.0).abs() < 100.0);
        assert!(mid.all(|v| v.pos.y == 1.0 || v.pos.y == 2.0));
    }

    #[test]
    fn svg_arc_finds_lucide_centers() {
        // a9 9 0 0 1 9-9 from (3,12): the quarter circle around (12,12).
        let mut path = vec![pos2(3.0, 12.0)];
        svg_arc(&mut path, pos2(12.0, 3.0), 9.0, false, true);
        assert_eq!(*path.last().unwrap(), pos2(12.0, 3.0));
        for q in &path {
            assert!(
                ((*q - pos2(12.0, 12.0)).length() - 9.0).abs() < 1e-3,
                "{q:?}"
            );
            assert!(q.x <= 12.0 + 1e-3 && q.y <= 12.0 + 1e-3, "{q:?}");
        }
    }

    #[test]
    fn clip_convex_cuts_a_square_by_a_triangle() {
        let square = rect_poly(0.0, 0.0, 2.0, 2.0);
        let tri = [pos2(0.0, 0.0), pos2(2.0, 0.0), pos2(0.0, 2.0)];
        let out = clip_convex(&square, &tri);
        assert!((signed_area(&out) / 2.0 - 2.0).abs() < 1e-5, "{out:?}");
        assert!(clip_convex(&square, &rect_poly(5.0, 5.0, 6.0, 6.0)).is_empty());
    }

    fn harness_sheet() -> egui_kittest::Harness<'static> {
        egui_kittest::Harness::builder()
            .with_size(vec2(260.0, 120.0))
            .with_pixels_per_point(1.0)
            .build_ui(|ui| {
                let p = ui.ctx().layer_painter(egui::LayerId::background());
                paint_primitives_sheet(&p, Pos2::ZERO);
            })
    }

    #[test]
    fn primitives_sheet() {
        let mut harness = harness_sheet();
        harness.snapshot("primitives_sheet");
    }

    /// The sheet through the window's CPU rasterizer (`render.rs`), RGBA.
    fn soft_render_sheet(ppp: f32) -> image::RgbaImage {
        let (w, h) = ((260.0 * ppp).round() as u32, (120.0 * ppp).round() as u32);
        let ctx = egui::Context::default();
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(260.0, 120.0))),
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(ppp);
        let out = ctx.run_pass(input, |ctx| {
            paint_primitives_sheet(&ctx.layer_painter(egui::LayerId::background()), Pos2::ZERO);
        });
        let primitives = ctx.tessellate(out.shapes, out.pixels_per_point);
        let mut canvas = vec![0u32; (w * h) as usize];
        crate::render::SoftRenderer::default().render(
            &mut canvas,
            w as usize,
            h as usize,
            &primitives,
            out.textures_delta,
            out.pixels_per_point,
        );
        image::RgbaImage::from_fn(w, h, |x, y| {
            let [b, g, r, a] = canvas[(y * w + x) as usize].to_le_bytes();
            image::Rgba([r, g, b, a])
        })
    }

    /// The window's CPU renderer draws the sheet like wgpu does (the snapshot).
    /// Set `LIMITRINGS_DUMP_DIR` to also save it at 1x and 4x for inspection.
    #[test]
    fn primitives_sheet_soft_render_matches_snapshot() {
        let soft = soft_render_sheet(1.0);
        if let Some(dir) = std::env::var_os("LIMITRINGS_DUMP_DIR") {
            let dir = std::path::PathBuf::from(dir);
            soft.save(dir.join("primitives_sheet_soft@1x.png")).unwrap();
            soft_render_sheet(4.0)
                .save(dir.join("primitives_sheet_soft@4x.png"))
                .unwrap();
        }
        let snapshot = image::open(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/snapshots/primitives_sheet.png"
        ))
        .unwrap()
        .to_rgba8();
        assert_eq!(soft.dimensions(), snapshot.dimensions());
        let mut worst = 0u8;
        let mut off = 0usize;
        for (a, b) in soft.pixels().zip(snapshot.pixels()) {
            let d = (0..4).map(|i| a[i].abs_diff(b[i])).max().unwrap();
            worst = worst.max(d);
            off += usize::from(d > 8);
        }
        eprintln!("soft vs wgpu: worst channel diff {worst}, {off} pixels off by > 8");
        assert!(
            off <= 50,
            "{off} pixels differ by more than 8 (worst {worst})"
        );
    }
}
