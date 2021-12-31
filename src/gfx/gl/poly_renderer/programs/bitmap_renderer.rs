use std::cell::RefCell;

use gl::types::{GLint, GLuint};

use crate::gfx::{
    gl::{poly_renderer::programs::Program, IndexedTexture},
    raster::IndexedImage,
    SCREEN_RESOLUTION,
};

use anyhow::Result;

/// A renderer for arbitrary bitmaps. Useful to quickly draw the title screen
/// or some of the hard-coded backgrounds by the end of the game.
pub struct BitmapRenderer {
    source_texture: RefCell<IndexedTexture>,
    source_fbo: GLuint,
}

impl Drop for BitmapRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteFramebuffers(1, &self.source_fbo);
        }
    }
}

impl Program for BitmapRenderer {}

impl BitmapRenderer {
    pub fn new() -> Result<BitmapRenderer> {
        let mut source_fbo = 0;

        unsafe {
            gl::GenFramebuffers(1, &mut source_fbo);
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, source_fbo);
            gl::DrawBuffers(1, [gl::COLOR_ATTACHMENT0].as_ptr());
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);
        }

        Ok(BitmapRenderer {
            source_fbo,
            source_texture: RefCell::new(IndexedTexture::new(
                SCREEN_RESOLUTION[0],
                SCREEN_RESOLUTION[1],
            )),
        })
    }

    pub fn draw_bitmap(&self, image: &IndexedImage) {
        // TODO super inefficient as we do this for every frame!
        // The texture should rather be in the command, and be refcounted?
        self.source_texture.borrow_mut().set_data(image, 0, 0);

        unsafe {
            // We draw the bitmap over the entire viewport - get the correct
            // coordinates first.
            let mut viewport: [GLint; 4] = [0; 4];
            gl::GetIntegerv(gl::VIEWPORT, viewport.as_mut_ptr());

            gl::BindFramebuffer(gl::READ_FRAMEBUFFER, self.source_fbo);
            gl::FramebufferTexture(
                gl::READ_FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                self.source_texture.borrow().as_tex_id(),
                0,
            );
            if gl::CheckFramebufferStatus(gl::READ_FRAMEBUFFER) != gl::FRAMEBUFFER_COMPLETE {
                panic!("Error while setting framebuffer up");
            }
            gl::BlitFramebuffer(
                0,
                0,
                SCREEN_RESOLUTION[0] as GLint,
                SCREEN_RESOLUTION[1] as GLint,
                viewport[0],
                viewport[1],
                viewport[2],
                viewport[3],
                gl::COLOR_BUFFER_BIT,
                gl::NEAREST,
            );
            gl::BindFramebuffer(gl::READ_FRAMEBUFFER, 0);
        }
    }
}
