pub trait Renderer {
    fn draw(&mut self, pixel_buffer: &[u8]);
}
