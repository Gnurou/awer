use super::PistonBackend;
use crate::gfx::{
    draw_list::{DrawListBackend, Op, PolyRender},
    Backend, Color, Palette,
};

use log::debug;
use opengl_graphics::GlGraphics;
use piston::input::RenderArgs;

use super::super::SCREEN_RESOLUTION;

trait Renderer {
    fn drawdisplaylist(&mut self, draw_list: &DrawListBackend, transform: [[f64; 3]; 2]);
}

fn infer_transparent_color(palette: &Palette) -> [f32; 4] {
    palette.0[0..8]
        .iter()
        .zip(palette.0[8..16].iter())
        .map(|(c1, c2)| Color::blend(c1, c2))
        .fold(Default::default(), |dst, c| Color::mix(&dst, &c))
        .normalize(0.5)
}

fn lookup_palette(palette: &Palette, color: u8) -> [f32; 4] {
    match color {
        // TODO we should copy from buffer 0 in this case, need a proper shader?
        0x11 => [0.0, 0.0, 0.0, 0.0],
        0x10 => infer_transparent_color(palette),
        _ => {
            let color = if color > 0xf { 0x0 } else { color };
            palette.lookup(color).normalize(1.0)
        }
    }
}

impl Renderer for GlGraphics {
    fn drawdisplaylist(&mut self, draw_list: &DrawListBackend, transform: [[f64; 3]; 2]) {
        use graphics::*;
        let framebuffer_index = draw_list.framebuffer_index;
        let buffer = &draw_list.buffers[framebuffer_index];
        let palette = &draw_list.palette;

        debug!("display list {} ({})", framebuffer_index, buffer.len());

        let drawstate = DrawState::default();

        for op in buffer {
            match op {
                Op::FillVideoPage(color_idx) => {
                    let color = lookup_palette(palette, *color_idx);

                    clear(color, self);
                }
                Op::DrawPoint(x, y, color_idx) => {
                    const POINT_SIZE: f64 = 0.4;
                    const POINT_VERTICES: [[f64; 2]; 4] = [
                        [0.5 - POINT_SIZE, 0.5 - POINT_SIZE],
                        [0.5 + POINT_SIZE, 0.5 - POINT_SIZE],
                        [0.5 + POINT_SIZE, 0.5 + POINT_SIZE],
                        [0.5 - POINT_SIZE, 0.5 + POINT_SIZE],
                    ];
                    let matrix = transform.trans(*x as f64, *y as f64);
                    let color = lookup_palette(palette, *color_idx);
                    let poly = polygon::Polygon::new(color);

                    poly.draw(&POINT_VERTICES, &DrawState::default(), matrix, self);
                }
                Op::DrawLine(x, y, color_idx, lines) => {
                    const LINE_THICKNESS: f64 = 0.5;
                    let matrix = transform.trans(*x as f64, *y as f64);
                    let color = lookup_palette(palette, *color_idx);
                    let line = line::Line::new(color, LINE_THICKNESS).shape(line::Shape::Bevel);

                    for l in lines {
                        line.draw(*l, &drawstate, matrix, self);
                    }
                }
                Op::DrawQuad(x, y, color_idx, vertices) => {
                    let matrix = transform.trans(*x as f64, *y as f64);
                    let color = lookup_palette(palette, *color_idx);
                    let poly = polygon::Polygon::new(color);

                    poly.draw(vertices, &DrawState::default(), matrix, self);
                }
            }
        }
    }
}

pub struct PistonGlGfx {
    gl: GlGraphics,
    draw_list: DrawListBackend,
}

pub fn new(poly_render: PolyRender) -> PistonGlGfx {
    PistonGlGfx {
        gl: GlGraphics::new(super::OPENGL_VERSION),
        draw_list: DrawListBackend::new(poly_render),
    }
}

impl PistonBackend for PistonGlGfx {
    fn render(&mut self, args: &RenderArgs) {
        use graphics::clear;
        use graphics::Transformed;
        /*
        use crate::gfx::SCREEN_RATIO;

        let window_w = args.window_size[0];
        let window_h = args.window_size[1];

        let (tx, ty, sx, sy) = if window_w / SCREEN_RATIO < window_h {
            let sx = window_w / SCREEN_RESOLUTION[0] as f64;
            let sy = sx * SCREEN_RATIO;
            (0.0, 100.0, sx, sy)
        } else {
            let sy = window_h / (SCREEN_RESOLUTION[0] as f64 / SCREEN_RATIO);
            let sx = sy / SCREEN_RATIO;
            (100.0, 0.0, sx, sy)
        };

        println!("{} {} {} {}", tx, ty, sx, sy);

        let context = self.gl.draw_begin(args.viewport());
        let transform = context.transform
            .trans(tx, ty)
            .scale(sx, sy)
        ;
        */
        let context = self.gl.draw_begin(args.viewport());
        let scale = (
            args.window_size[0] / SCREEN_RESOLUTION[0] as f64,
            args.window_size[1] / SCREEN_RESOLUTION[1] as f64,
        );
        let transform = context.transform.scale(scale.0, scale.1);
        clear([0.0, 0.0, 0.0, 1.0], &mut self.gl);
        self.gl.drawdisplaylist(&self.draw_list, transform);
        self.gl.draw_end();
    }

    fn as_gfx(&mut self) -> &mut dyn Backend {
        &mut self.draw_list
    }
}
