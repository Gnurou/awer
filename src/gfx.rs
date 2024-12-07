pub mod polygon;
pub mod raster;

#[cfg(feature = "gl3")]
pub mod gl3;
#[cfg(feature = "sdl2-sys")]
pub mod sdl2;

use std::any::Any;
use std::fmt::Debug;
use std::io::Cursor;
use std::io::Seek;
use std::io::SeekFrom;
use std::ops::DerefMut;

use byteorder::ReadBytesExt;
use byteorder::BE;
use polygon::Point;
use polygon::Polygon;
use tracing::debug;
use tracing::error;
use zerocopy::FromBytes;

use crate::res::ResourceManager;
use crate::scenes::InitForScene;
use crate::scenes::Scene;
use crate::sys::Snapshotable;

/// Native screen resolution of the game.
pub const SCREEN_RESOLUTION: [usize; 2] = [320, 200];

/// The two polygon segments containing polygon data.
#[derive(Debug, Copy, Clone)]
pub enum PolySegment {
    /// Used for non-playable animations.
    Cinematic,
    /// Used for interactive scenes.
    Video,
}

/// Trait for filling a single polygon defined by a slice of points.
pub trait PolygonFiller {
    /// Fill the polygon defined by `polu` with color index `color_idx` on page `dst_page_id`.
    /// `pos` is the coordinates of the center of the polygon on the page. `zoom` is a zoom factor
    /// by which every point of the polygon must be multiplied by, and then divided by 64.
    #[allow(clippy::too_many_arguments)]
    fn fill_polygon(
        &mut self,
        poly: &Polygon,
        color_idx: u8,
        dst_page_id: usize,
        pos: (i16, i16),
        offset: (i16, i16),
        zoom: u16,
    );
}

/// Structure that stores the polygonal data as-is and parses it as needed to render it using a
/// [`PolygonFiller`].
///
/// This is the original behavior of the game, and is suitable for most simple renderers.
#[derive(Default, Clone)]
pub struct SimplePolygonRenderer {
    /// Cinematic segment.
    cinematic: Vec<u8>,
    /// Video segment.
    video: Vec<u8>,
}

impl InitForScene for SimplePolygonRenderer {
    #[tracing::instrument(skip(self, resman))]
    fn init_from_scene(&mut self, resman: &ResourceManager, scene: &Scene) -> std::io::Result<()> {
        self.cinematic = resman.load_resource(scene.video1)?.data;
        self.video = if scene.video2 != 0 {
            resman.load_resource(scene.video2)?.data
        } else {
            Default::default()
        };

        Ok(())
    }
}

impl SimplePolygonRenderer {
    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(level = "trace", skip(segment, filler))]
    fn draw_polygon<F: PolygonFiller>(
        segment: &[u8],
        start_offset: u16,
        render_buffer: usize,
        pos: (i16, i16),
        offset: (i16, i16),
        zoom: u16,
        color: Option<u8>,
        filler: &mut F,
    ) {
        let op = segment[start_offset as usize];
        match op {
            op if op & 0xc0 == 0xc0 => {
                // TODO: match other properties of the color (e.g. blend) from op
                let color = match color {
                    // If we already have a color set, use it.
                    Some(color) => color,
                    // Otherwise take the color from the op.
                    None => op & 0x3f,
                };

                let poly_start = start_offset as usize + 1;
                let nb_points = segment[poly_start + 2] as usize;
                let Ok(poly) = Polygon::ref_from_bytes(
                    &segment
                        [poly_start..poly_start + 3 + nb_points * std::mem::size_of::<Point<u8>>()],
                ) else {
                    tracing::error!("poly data out of range of segment");
                    return;
                };

                filler.fill_polygon(poly, color, render_buffer, pos, offset, zoom);
            }
            0x02 => {
                Self::draw_polygon_hierarchy(
                    segment,
                    render_buffer,
                    pos,
                    offset,
                    zoom,
                    color,
                    start_offset + 1,
                    filler,
                );
            }
            _ => tracing::warn!("invalid draw_polygon op 0x{:x}", op),
        };
    }

    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(level = "trace", skip(segment, start_offset, filler))]
    fn draw_polygon_hierarchy<F: PolygonFiller>(
        segment: &[u8],
        render_buffer: usize,
        pos: (i16, i16),
        offset: (i16, i16),
        zoom: u16,
        color: Option<u8>,
        start_offset: u16,
        filler: &mut F,
    ) {
        let mut cursor = Cursor::new(segment);
        match cursor.seek(SeekFrom::Start(start_offset as u64)) {
            Ok(_) => (),
            Err(e) => {
                error!("error while seeking to draw polygon: {}", e);
                return;
            }
        }

        let offset = (
            offset.0 - cursor.read_u8().unwrap() as i16,
            offset.1 - cursor.read_u8().unwrap() as i16,
        );
        let nb_childs = cursor.read_u8().unwrap() + 1;

        for _i in 0..nb_childs {
            let word = cursor.read_u16::<BE>().unwrap();
            let (read_color, poly_offset) = (word & 0x8000 != 0, (word & 0x7fff) * 2);
            let offset = (
                offset.0 + cursor.read_u8().unwrap() as i16,
                offset.1 + cursor.read_u8().unwrap() as i16,
            );

            let color = if read_color {
                let color = Some(cursor.read_u8().unwrap() & 0x7f);
                // This is a "mask number" apparently?
                cursor.read_u8().unwrap();
                color
            } else {
                color
            };

            Self::draw_polygon(
                segment,
                poly_offset,
                render_buffer,
                pos,
                offset,
                zoom,
                color,
                filler,
            );
        }
    }

    #[tracing::instrument(level = "trace", skip(self, segment, filler))]
    #[allow(clippy::too_many_arguments)]
    pub fn draw_polygons<F: PolygonFiller>(
        &mut self,
        segment: PolySegment,
        start_offset: u16,
        dst_page_id: usize,
        pos: (i16, i16),
        offset: (i16, i16),
        zoom: u16,
        filler: &mut F,
    ) {
        let segment = match segment {
            PolySegment::Cinematic => &self.cinematic,
            PolySegment::Video => &self.video,
        }
        .as_ref();

        Self::draw_polygon(
            segment,
            start_offset,
            dst_page_id,
            pos,
            offset,
            zoom,
            None,
            filler,
        );
    }
}

/// Trait for rendering the game from the VM graphics operations.
///
/// Implementors receive the VM rendering commands as-is. They are also expected to implement
/// [`InitForScene`] in order to receive and process the graphics segments which the rendering
/// commands depend on.
pub trait GameRenderer {
    /// Fill video page `page_id` entirely with color `color_idx`.
    fn fillvideopage(&mut self, page_id: usize, color_idx: u8);
    /// Copy video page `src_page_id` into `dst_page_id`. `vscroll` is a vertical offset
    /// for the copy.
    fn copyvideopage(&mut self, src_page_id: usize, dst_page_id: usize, vscroll: i16);
    /// Draw the polygons which data starts at `offset` of `segment`.
    ///
    /// `pos` is the coordinates of the center of the polygon on the page. `zoom` is a zoom factor
    /// by which every point of the polygon must be multiplied by, and then divided by 64.
    fn draw_polygons(
        &mut self,
        segment: PolySegment,
        start_offset: u16,
        dst_page_id: usize,
        pos: (i16, i16),
        offset: (i16, i16),
        zoom: u16,
    );
    /// Draw character `c` at position `pos` of page `dst_page_id` with color `color_idx`.
    fn draw_char(&mut self, dst_page_id: usize, pos: (i16, i16), color_idx: u8, c: u8);
    /// Blit `buffer` (a bitmap of full screen size) into `dst_page_id`. This is used
    /// as an alternative to creating scenes using polys notably in later scenes of
    /// the game (maybe because of lack of time?).
    fn blit_buffer(&mut self, dst_page_id: usize, buffer: &[u8]);
}

/// Proxy implementation for containers of `GameRenderer`.
impl<R: GameRenderer + ?Sized, C: DerefMut<Target = R>> GameRenderer for C {
    fn fillvideopage(&mut self, page_id: usize, color_idx: u8) {
        self.deref_mut().fillvideopage(page_id, color_idx)
    }

    fn copyvideopage(&mut self, src_page_id: usize, dst_page_id: usize, vscroll: i16) {
        self.deref_mut()
            .copyvideopage(src_page_id, dst_page_id, vscroll)
    }

    fn draw_polygons(
        &mut self,
        segment: PolySegment,
        start_offset: u16,
        dst_page_id: usize,
        pos: (i16, i16),
        offset: (i16, i16),
        zoom: u16,
    ) {
        self.deref_mut()
            .draw_polygons(segment, start_offset, dst_page_id, pos, offset, zoom)
    }

    fn draw_char(&mut self, dst_page_id: usize, pos: (i16, i16), color_idx: u8, c: u8) {
        self.deref_mut().draw_char(dst_page_id, pos, color_idx, c)
    }

    fn blit_buffer(&mut self, dst_page_id: usize, buffer: &[u8]) {
        self.deref_mut().blit_buffer(dst_page_id, buffer)
    }
}

/// Trait for displaying an indexed-color buffer using a given palette on the screen.
pub trait Display {
    /// Show `page_id` on the screen, using `palette` to render its actual colors.
    fn blitframebuffer(&mut self, page_id: usize, palette: &Palette);
}

/// Proxy implementation for containers of `Display`.
impl<D: Display + ?Sized, C: DerefMut<Target = D>> Display for C {
    fn blitframebuffer(&mut self, page_id: usize, palette: &Palette) {
        self.deref_mut().blitframebuffer(page_id, palette)
    }
}

/// Trait providing the methods necessary for the VM to render the game.
pub trait Gfx: InitForScene + GameRenderer + Display + Snapshotable<State = Box<dyn Any>> {}

/// Proxy implementation for containers of `Gfx`.
impl<G: Gfx + ?Sized, C: DerefMut<Target = G>> Gfx for C {}

/// A single color from a game's palette which components have been normalized to cover the u8
/// range.
///
/// We use a C representation aligned to 32 bits so this can safely be passed to shaders.
#[repr(C, align(4))]
#[derive(Debug, Default, Clone)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub const PALETTE_SIZE: usize = 16;

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct Palette([Color; PALETTE_SIZE]);

impl Palette {
    /// Sets the current |palette| from a raw PALETTE resource.
    pub fn set(&mut self, palette: &[u8; 32]) {
        for i in 0..16 {
            let c1 = palette[i * 2];
            let c2 = palette[i * 2 + 1];

            let b = c2 & 0x0f;
            let g = (c2 & 0xf0) >> 4;
            let r = c1 & 0x0f;

            let col = &mut self.0[i];
            // We only have 4 bits worth of intensity per color. Copy them to
            // the high bits so we have enough luminosity.
            col.r = (r << 4) | r;
            col.g = (g << 4) | g;
            col.b = (b << 4) | b;

            debug!("palette[{:x}] = {:x?}", i, col);
        }
    }

    /// Return the RGB color corresponding to |color_idx|.
    /// A palette only has 16 colors, so this method will panic if |color_idx|
    /// is bigger than 0xf.
    pub fn lookup(&self, color_idx: u8) -> &Color {
        assert!(color_idx <= 0xf);

        &self.0[color_idx as usize]
    }

    #[allow(dead_code)]
    pub fn as_ptr(&self) -> *const Color {
        self.0.as_ptr()
    }
}
