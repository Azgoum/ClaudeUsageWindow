//! CPU renderer: rasterizes egui's tessellated meshes into a premultiplied
//! BGRA pixel buffer (`0xAARRGGBB` per `u32`, top-down rows).
//!
//! egui feathers every shape's edges in its tessellator, so sampling each pixel
//! center once already gives anti-aliased output; no coverage math is needed.

use std::collections::HashMap;

use egui::{
    epaint::{ClippedPrimitive, ImageDelta, Mesh, Primitive, Vertex},
    Color32, Pos2, Rect, TextureId, TexturesDelta,
};

/// A texture kept on the CPU, premultiplied RGBA.
struct Texture {
    width: usize,
    height: usize,
    pixels: Vec<Color32>,
}

/// Owns egui's textures and draws frames into caller-provided pixel buffers.
#[derive(Default)]
pub struct SoftRenderer {
    textures: HashMap<TextureId, Texture>,
}

impl SoftRenderer {
    /// Applies texture uploads, clears `canvas` to transparent, draws `primitives`
    /// (positions in points, scaled by `ppp`), then frees released textures.
    /// Consumes `textures` (egui asserts every delta is handled).
    pub fn render(
        &mut self,
        canvas: &mut [u32],
        width: usize,
        height: usize,
        primitives: &[ClippedPrimitive],
        mut textures: TexturesDelta,
        ppp: f32,
    ) {
        assert_eq!(canvas.len(), width * height, "canvas size mismatch");
        for (id, deltas) in textures.set.drain() {
            for delta in &deltas {
                self.apply(id, delta);
            }
        }
        canvas.fill(0);
        let bounds = Rect::from_min_max(Pos2::ZERO, egui::pos2(width as f32, height as f32));
        for clipped in primitives {
            let Primitive::Mesh(mesh) = &clipped.primitive else {
                continue; // paint callbacks need a GPU; the widget never uses them
            };
            let Some(tex) = self.textures.get(&mesh.texture_id) else {
                continue;
            };
            let clip = (clipped.clip_rect * ppp).intersect(bounds);
            if clip.is_positive() {
                draw_mesh(canvas, width, clip, mesh, ppp, tex);
            }
        }
        for id in textures.free.drain() {
            self.textures.remove(&id);
        }
    }

    fn apply(&mut self, id: TextureId, delta: &ImageDelta) {
        let egui::ImageData::Color(image) = &delta.image;
        let [w, h] = image.size;
        match delta.pos {
            None => {
                let texture = Texture {
                    width: w,
                    height: h,
                    pixels: image.pixels.clone(),
                };
                self.textures.insert(id, texture);
            }
            Some([x0, y0]) => {
                let Some(tex) = self.textures.get_mut(&id) else {
                    return;
                };
                for y in 0..h {
                    let dst = (y0 + y) * tex.width + x0;
                    tex.pixels[dst..dst + w].copy_from_slice(&image.pixels[y * w..(y + 1) * w]);
                }
            }
        }
    }
}

fn draw_mesh(canvas: &mut [u32], stride: usize, clip: Rect, mesh: &Mesh, ppp: f32, tex: &Texture) {
    for tri in mesh.indices.as_chunks::<3>().0 {
        let v = tri.map(|i| &mesh.vertices[i as usize]);
        draw_triangle(canvas, stride, clip, v, ppp, tex);
    }
}

fn draw_triangle(
    canvas: &mut [u32],
    stride: usize,
    clip: Rect,
    v: [&Vertex; 3],
    ppp: f32,
    tex: &Texture,
) {
    let p = v.map(|v| v.pos * ppp);
    let area = edge(p[0], p[1], p[2]);
    if area.abs() < 1e-6 {
        return;
    }
    // Reorder to positive area so the top-left rule below is stated once.
    let o = if area > 0.0 { [0, 1, 2] } else { [0, 2, 1] };
    let (p, v, area) = (o.map(|i| p[i]), o.map(|i| v[i]), area.abs());

    let min_x = p.iter().fold(f32::MAX, |m, p| m.min(p.x)).max(clip.min.x);
    let max_x = p.iter().fold(f32::MIN, |m, p| m.max(p.x)).min(clip.max.x);
    let min_y = p.iter().fold(f32::MAX, |m, p| m.min(p.y)).max(clip.min.y);
    let max_y = p.iter().fold(f32::MIN, |m, p| m.max(p.y)).min(clip.max.y);
    if min_x >= max_x || min_y >= max_y {
        return;
    }
    // A pixel center lies in [x0, x1) when x0 + 0.5 >= min_x and x + 0.5 < max_x.
    let (x0, x1) = ((min_x - 0.5).ceil() as usize, (max_x - 0.5).ceil() as usize);
    let (y0, y1) = ((min_y - 0.5).ceil() as usize, (max_y - 0.5).ceil() as usize);

    // Top-left fill rule: a pixel center exactly on an edge shared by two
    // triangles is drawn by one of them only, so translucent fills don't
    // double-blend along mesh seams.
    let edges = [(p[1], p[2]), (p[2], p[0]), (p[0], p[1])];
    let owns_boundary = edges.map(|(a, b)| is_top_left(a, b));

    for y in y0..y1 {
        for x in x0..x1 {
            let c = egui::pos2(x as f32 + 0.5, y as f32 + 0.5);
            let e = edges.map(|(a, b)| edge(a, b, c));
            if (0..3).any(|i| e[i] < 0.0 || (e[i] == 0.0 && !owns_boundary[i])) {
                continue;
            }
            let w = e.map(|e| e / area);
            let lerp =
                |f: &dyn Fn(&Vertex) -> f32| w[0] * f(v[0]) + w[1] * f(v[1]) + w[2] * f(v[2]);
            let texel = sample(tex, lerp(&|v| v.uv.x), lerp(&|v| v.uv.y));
            let color = [
                lerp(&|v| f32::from(v.color.r())),
                lerp(&|v| f32::from(v.color.g())),
                lerp(&|v| f32::from(v.color.b())),
                lerp(&|v| f32::from(v.color.a())),
            ];
            let src: [f32; 4] = std::array::from_fn(|i| color[i] * texel[i] / 255.0);
            let px = &mut canvas[y * stride + x];
            *px = blend(*px, src);
        }
    }
}

/// Twice the signed area of (a, b, c); for a positive-area triangle every
/// interior point gives a positive value for each of its three edges.
fn edge(a: Pos2, b: Pos2, c: Pos2) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

/// For a triangle with positive `edge` area (clockwise on a y-down screen,
/// e.g. (0,0) → (1,0) → (0,1)): a top edge runs left to right with the
/// interior below it, a left edge runs upward.
fn is_top_left(a: Pos2, b: Pos2) -> bool {
    (a.y == b.y && b.x > a.x) || b.y < a.y
}

/// Bilinear sample, premultiplied RGBA in 0..=255.
fn sample(tex: &Texture, u: f32, v: f32) -> [f32; 4] {
    let fx = (u * tex.width as f32 - 0.5).clamp(0.0, (tex.width - 1) as f32);
    let fy = (v * tex.height as f32 - 0.5).clamp(0.0, (tex.height - 1) as f32);
    let (ix, iy) = (fx as usize, fy as usize);
    let (ax, ay) = (fx - ix as f32, fy - iy as f32);
    let (ix1, iy1) = ((ix + 1).min(tex.width - 1), (iy + 1).min(tex.height - 1));
    let at = |x: usize, y: usize| {
        let c = tex.pixels[y * tex.width + x];
        [c.r(), c.g(), c.b(), c.a()].map(f32::from)
    };
    let (c00, c10, c01, c11) = (at(ix, iy), at(ix1, iy), at(ix, iy1), at(ix1, iy1));
    std::array::from_fn(|i| {
        let top = c00[i] + (c10[i] - c00[i]) * ax;
        let bottom = c01[i] + (c11[i] - c01[i]) * ax;
        top + (bottom - top) * ay
    })
}

/// Premultiplied source-over in gamma space (what egui's own renderers do).
fn blend(dst: u32, src: [f32; 4]) -> u32 {
    let keep = 1.0 - src[3] / 255.0;
    let channel = |s: f32, shift: u32| {
        let d = ((dst >> shift) & 0xff) as f32;
        ((s + d * keep).round().clamp(0.0, 255.0) as u32) << shift
    };
    channel(src[3], 24) | channel(src[0], 16) | channel(src[1], 8) | channel(src[2], 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{epaint::WHITE_UV, Color32};

    fn white_texture() -> TexturesDelta {
        let image = egui::ColorImage::new([1, 1], vec![Color32::WHITE]);
        let mut delta = TexturesDelta::default();
        let full = ImageDelta::full(image, egui::TextureOptions::NEAREST);
        delta.push(TextureId::default(), full);
        delta
    }

    fn quad(rect: Rect, color: Color32) -> ClippedPrimitive {
        let mut mesh = Mesh::default();
        mesh.add_colored_rect(rect, color);
        for v in &mut mesh.vertices {
            v.uv = WHITE_UV;
        }
        ClippedPrimitive {
            clip_rect: Rect::EVERYTHING,
            primitive: Primitive::Mesh(mesh),
        }
    }

    fn render(prims: &[ClippedPrimitive], w: usize, h: usize) -> Vec<u32> {
        let mut canvas = vec![0xdead_beef; w * h];
        SoftRenderer::default().render(&mut canvas, w, h, prims, white_texture(), 1.0);
        canvas
    }

    #[test]
    fn blend_is_premultiplied_source_over() {
        assert_eq!(blend(0, [10.0, 20.0, 30.0, 128.0]), 0x800a_141e);
        assert_eq!(blend(0xff00_00ff, [0.0, 0.0, 0.0, 0.0]), 0xff00_00ff);
        assert_eq!(blend(0xffff_ffff, [0.0, 0.0, 0.0, 255.0]), 0xff00_0000);
    }

    #[test]
    fn translucent_quad_covers_each_pixel_once() {
        // Two triangles share the diagonal; no pixel may be blended twice.
        let half_red = Color32::from_rgba_premultiplied(100, 0, 0, 100);
        let canvas = render(
            &[quad(
                Rect::from_min_max(Pos2::ZERO, egui::pos2(8.0, 8.0)),
                half_red,
            )],
            8,
            8,
        );
        assert!(canvas.iter().all(|&px| px == 0x6464_0000), "{canvas:x?}");
    }

    #[test]
    fn outside_the_shape_stays_transparent_and_clip_is_honored() {
        let mut prim = quad(
            Rect::from_min_max(Pos2::ZERO, egui::pos2(4.0, 4.0)),
            Color32::WHITE,
        );
        prim.clip_rect = Rect::from_min_max(Pos2::ZERO, egui::pos2(2.0, 4.0));
        let canvas = render(&[prim], 6, 4);
        for y in 0..4 {
            for x in 0..6 {
                let expected = if x < 2 { 0xffff_ffff } else { 0 };
                assert_eq!(canvas[y * 6 + x], expected, "pixel ({x}, {y})");
            }
        }
    }
}
