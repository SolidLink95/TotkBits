use crate::Gui::TotkBitsApp;

pub struct Editor<'a> {
    pub chunk_size: i32,
    pub whole_chunk_size: i32,
    pub content: &'a String,
    pub alr_viewed: f32,
    pub alr_read: f32,
    app: &'a TotkBitsApp<'a>
}


impl<'a> Editor<'a>{
    pub fn new(app: &'a TotkBitsApp, text: &'a String, chunk_s:i32) -> Self {
        //let chunk_s = 3500;
        let alr_viewed = Editor::calc_alr_viewed(app);
        Self {
            chunk_size: chunk_s,
            whole_chunk_size: chunk_s*3,
            content: text,
            alr_viewed: alr_viewed,
            alr_read: text.len() as f32 * alr_viewed,
            app
        }
    }

    pub fn calc_alr_viewed(app: &'a TotkBitsApp) -> f32 {
        let r = app.scroll_resp.as_ref().unwrap();
        return r.state.offset.y / r.content_size.y;
    }

    pub fn calc_alr_read(&self) -> f32 {
        self.content.len() as f32 * Editor::calc_alr_viewed(self.app)
    }

    pub fn update(&mut self) {
        self.alr_viewed = Editor::calc_alr_viewed(self.app);
        self.alr_read = self.calc_alr_read();
    }


}
