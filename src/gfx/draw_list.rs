use std::any::Any;

use log::trace;

use super::{polygon::Polygon, Backend, Palette, Point};

#[derive(Clone)]
pub enum Op {
    FillVideoPage(u8),
    DrawPoint(i16, i16, u8),
    DrawQuad(i16, i16, u8, [[f64; 2]; 4]),
    DrawLine(i16, i16, u8, Vec<[f64; 4]>),
}

pub type DrawList = Vec<Op>;

#[derive(Clone, Copy)]
pub enum PolyRender {
    Line,
    Poly,
}

#[derive(Clone)]
pub struct DrawListBackend {
    pub palette: Palette,
    pub buffers: [DrawList; 4],
    pub framebuffer_index: usize,

    pub poly_render: PolyRender,
}

impl DrawListBackend {
    pub fn new(poly_render: PolyRender) -> DrawListBackend {
        DrawListBackend {
            palette: Default::default(),
            buffers: Default::default(),
            framebuffer_index: 0,
            poly_render,
        }
    }
}

impl Backend for DrawListBackend {
    fn set_palette(&mut self, palette: &[u8; 32]) {
        self.palette.set(palette);
    }

    fn fillvideopage(&mut self, page_id: usize, color_idx: u8) {
        let buffer = &mut self.buffers[page_id];

        buffer.clear();
        buffer.push(Op::FillVideoPage(color_idx));
    }

    fn copyvideopage(&mut self, src_page_id: usize, dst_page_id: usize, _vscroll: i16) {
        let src_content: Vec<Op> = self.buffers[src_page_id].to_vec();
        let dst_buffer = &mut self.buffers[dst_page_id];

        dst_buffer.clear();
        dst_buffer.extend(src_content);
    }

    fn fillpolygon(
        &mut self,
        dst_page_id: usize,
        x: i16,
        y: i16,
        color_idx: u8,
        polygon: &Polygon,
    ) {
        trace!("fillpolygon ({}, {}) color_idx={:2x}", x, y, color_idx);

        let buffer = &mut self.buffers[dst_page_id];

        // Special case: we just need to draw a point.
        if polygon.bbw <= 1 && polygon.bbh <= 1 {
            buffer.push(Op::DrawPoint(x, y, color_idx));
            return;
        }

        // TODO push transformation matrix with op instead!
        let offset = (polygon.bbw as f64 / 2.0, polygon.bbh as f64 / 2.0);
        // Center all pixels
        //let offset = (offset.0 + 0.5, offset.1 + 0.5);

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

    fn blitframebuffer(&mut self, page_id: usize) {
        self.framebuffer_index = page_id;
    }

    fn blit_buffer(&mut self, _dst_page_id: usize, _buffer: &[u8]) {
        todo!("not yet implemented");
    }

    fn get_snapshot(&self) -> Box<dyn Any> {
        Box::new(self.clone())
    }

    fn set_snapshot(&mut self, snapshot: Box<dyn Any>) {
        if let Ok(snapshot) = snapshot.downcast::<Self>() {
            *self = *snapshot;
        } else {
            eprintln!("Attempting to restore invalid gfx snapshot, ignoring");
        }
    }
}
