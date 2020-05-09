use super::PistonBackend;
use crate::gfx::{Backend, Color, Palette, Point, Polygon};

use std::any::Any;

use log::{debug, error, trace};
use opengl_graphics::GlGraphics;
use piston::input::RenderArgs;

use super::super::SCREEN_RESOLUTION;

#[derive(Clone)]
enum Op {
    FillVideoPage(u8),
    DrawPoint(i16, i16, u8),
    DrawQuad(i16, i16, u8, [[f64; 2]; 4]),
    DrawLine(i16, i16, u8, Vec<[f64; 4]>),
}

type DrawList = Vec<Op>;

#[derive(Clone, Copy)]
pub enum PolyRender {
    Line,
    Poly,
}

trait Renderer {
    fn drawdisplaylist(
        &mut self,
        buffer: &[DrawList; 4],
        buffer_idx: usize,
        palette: &Palette,
        transform: [[f64; 3]; 2],
    );
}

fn lookup_palette(palette: &Palette, color: u8) -> [f32; 4] {
    let color = if color > 0xf { 0x0 } else { color };

    let &Color { r, g, b } = palette.lookup(color);
    let blend = if color == 0x10 { 0.5 } else { 1.0 };

    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, blend]
}

impl Renderer for GlGraphics {
    fn drawdisplaylist(
        &mut self,
        buffer: &[DrawList; 4],
        buffer_idx: usize,
        palette: &Palette,
        transform: [[f64; 3]; 2],
    ) {
        use graphics::*;

        debug!("display list {} ({})", buffer_idx, buffer[buffer_idx].len());

        let drawstate = DrawState::default();

        for op in &buffer[buffer_idx] {
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

    palette: Palette,

    /// The game uses 3 buffers: 1 front (currently displayed), 1 back
    /// (currently being drawn), one background (contains the static parts
    /// of a scene to be copied in one operation). The VM maintains which
    /// is which.
    buffer: [DrawList; 4],

    /// Buffer to display during the next render operation.
    /// TODO: change to optional in case there is nothing to do?
    render_buffer: usize,

    poly_render: PolyRender,
}

pub fn new() -> PistonGlGfx {
    PistonGlGfx {
        gl: GlGraphics::new(super::OPENGL_VERSION),
        palette: Default::default(),
        buffer: Default::default(),
        render_buffer: 0,
        poly_render: PolyRender::Poly,
    }
}

impl PistonGlGfx {
    pub fn set_poly_render(mut self, poly_render: PolyRender) -> Self {
        self.poly_render = poly_render;
        self
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
        self.gl
            .drawdisplaylist(&self.buffer, self.render_buffer, &self.palette, transform);
        self.gl.draw_end();
    }

    fn as_gfx(&mut self) -> &mut dyn Backend {
        self
    }
}

impl Backend for PistonGlGfx {
    fn set_palette(&mut self, palette: &[u8; 32]) {
        self.palette.set(palette);
    }

    fn fillvideopage(&mut self, page_id: usize, color_idx: u8) {
        let buffer = &mut self.buffer[page_id];

        buffer.clear();
        buffer.push(Op::FillVideoPage(color_idx));
    }

    fn copyvideopage(&mut self, src_page_id: usize, dst_page_id: usize, _vscroll: i16) {
        let src_content: Vec<Op> = self.buffer[src_page_id].to_vec();
        let dst_buffer = &mut self.buffer[dst_page_id];

        dst_buffer.clear();
        dst_buffer.extend(src_content);
    }

    fn fillpolygon(
        &mut self,
        render_buffer: usize,
        x: i16,
        y: i16,
        color_idx: u8,
        polygon: &Polygon,
    ) {
        let buffer = &mut self.buffer[render_buffer];

        // TODO push transformation matrix with op instead!
        let offset = (polygon.bbw as f64 / 2.0, polygon.bbh as f64 / 2.0);
        // Center all pixels
        //let offset = (offset.0 + 0.5, offset.1 + 0.5);

        trace!("fillpolygon ({}, {}) color_idx={:2x}", x, y, color_idx);

        // Special case: we just need to draw a point.
        if polygon.bbw <= 1 && polygon.bbh <= 1 {
            buffer.push(Op::DrawPoint(x, y, color_idx));
            return;
        }

        // Fix 1-pixel lines
        // TODO instead of this, parse the vertices top-down and see if two
        // of them end up at the same position. Add some distance when this is
        // the case.
        // TODO or better, since we always have 4 points, slide them a bit
        // around the center of the object. That way a dot is a dot, and a line
        // a line.
        if polygon.bbw <= 1 {
            buffer.push(Op::DrawLine(
                x,
                y,
                color_idx,
                [[0.0, -offset.1, 0.0, offset.1]].to_vec(),
            ));
            return;
        }
        if polygon.bbh <= 1 {
            buffer.push(Op::DrawLine(
                x,
                y,
                color_idx,
                [[-offset.0, 0.0, offset.0, 0.0]].to_vec(),
            ));
            return;
        }

        let vertices = polygon
            .points
            .iter()
            .map(|pt| [pt.x as f64 - offset.0, pt.y as f64 - offset.1])
            .collect::<Vec<[f64; 2]>>();

        match self.poly_render {
            PolyRender::Line => {
                let mut lines: Vec<[f64; 4]> = vec![];
                let v_len = vertices.len();
                for i in 0..vertices.len() - 1 {
                    lines.push([
                        vertices[i][0],
                        vertices[i][1],
                        vertices[i + 1][0],
                        vertices[i + 1][1],
                    ]);
                }
                lines.push([
                    vertices[v_len - 1][0],
                    vertices[v_len - 1][1],
                    vertices[0][0],
                    vertices[0][1],
                ]);

                buffer.push(Op::DrawLine(x, y, color_idx, lines));
            }
            PolyRender::Poly => {
                let lines: Vec<(Point<f64>, Point<f64>)> = polygon.line_iter().collect();
                for quad in lines.iter().zip(lines.iter().skip(1)) {
                    let line1 = quad.0;
                    let line2 = quad.1;
                    buffer.push(Op::DrawQuad(
                        x,
                        y,
                        color_idx,
                        [
                            [line1.0.x - offset.0, line1.0.y - offset.1],
                            [line1.1.x - offset.0, line1.1.y - offset.1],
                            [line2.1.x - offset.0, line2.1.y - offset.1],
                            [line2.0.x - offset.0, line2.0.y - offset.1],
                        ],
                    ))
                }
            }
        };
    }

    fn blitframebuffer(&mut self, buffer_id: usize) {
        self.render_buffer = buffer_id;
    }

    fn blit_buffer(&mut self, _dst_page_id: usize, _buffer: &[u8]) {
        error!("not yet implemented");
    }

    fn get_snapshot(&self) -> Box<dyn Any> {
        Box::new(GLGfxSnapshot {
            palette: self.palette.clone(),
            buffer: self.buffer.clone(),
            render_buffer: self.render_buffer,
        })
    }

    fn set_snapshot(&mut self, snapshot: Box<dyn Any>) {
        if let Ok(snapshot) = snapshot.downcast::<GLGfxSnapshot>() {
            self.palette = snapshot.palette;
            self.buffer = snapshot.buffer;
            self.render_buffer = snapshot.render_buffer;
        } else {
            eprintln!("Attempting to restore invalid gfx snapshot, ignoring");
        }
    }
}

struct GLGfxSnapshot {
    palette: Palette,
    buffer: [DrawList; 4],
    render_buffer: usize,
}
