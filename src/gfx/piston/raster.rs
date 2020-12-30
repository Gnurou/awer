use super::PistonBackend;
use crate::gfx::{raster::RasterBackend, Backend, Color, Palette};

use image as im;
use piston::input::RenderArgs;

use opengl_graphics as gl;

use super::super::SCREEN_RESOLUTION;

/// A software backend that aims at rendering the game identically to what
/// the original does.
pub struct PistonRasterBackend {
    raster: RasterBackend,
    framebuffer: im::RgbaImage,

    gl: gl::GlGraphics,
    texture: gl::Texture,
}

pub fn new() -> PistonRasterBackend {
    // Black screen by default.
    let framebuffer = im::RgbaImage::from_pixel(
        SCREEN_RESOLUTION[0] as u32,
        SCREEN_RESOLUTION[1] as u32,
        im::Rgba([0, 0, 0, 255]),
    );

    let texture = gl::Texture::from_image(
        &framebuffer,
        &gl::TextureSettings::new().filter(gl::Filter::Nearest),
    );

    PistonRasterBackend {
        raster: RasterBackend::new(),
        gl: gl::GlGraphics::new(super::OPENGL_VERSION),
        texture,
        framebuffer,
    }
}

fn lookup_palette(palette: &Palette, color: u8) -> im::Rgba<u8> {
    let &Color { r, g, b } = palette.lookup(color);

    im::Rgba([r, g, b, 255])
}

impl PistonBackend for PistonRasterBackend {
    fn render(&mut self, args: &RenderArgs) {
        use crate::gfx::SCREEN_RATIO;
        use graphics::*;

        let window_w = args.window_size[0];
        let window_h = args.window_size[1];

        let image = Image::new().rect(if window_w / SCREEN_RATIO < window_h {
            let w = window_w;
            let h = window_w / SCREEN_RATIO;
            [0.0, (window_h - h) / 2.0, w, h]
        } else {
            let w = window_h * SCREEN_RATIO;
            let h = window_h;
            [(window_w - w) / 2.0, 0.0, w, h]
        });

        // Translate the indexed pixels into RGBA values using the palette.
        for pixel in self
            .framebuffer
            .pixels_mut()
            .zip(self.raster.get_framebuffer().pixels().iter())
        {
            *pixel.0 = lookup_palette(self.raster.get_palette(), *pixel.1);
        }

        self.texture.update(&self.framebuffer);

        let context = self.gl.draw_begin(args.viewport());
        clear([0.0, 0.0, 0.0, 1.0], &mut self.gl);
        image.draw(
            &self.texture,
            &DrawState::default(),
            context.transform,
            &mut self.gl,
        );
        self.gl.draw_end();
    }

    fn as_gfx(&mut self) -> &mut dyn Backend {
        &mut self.raster
    }
}
