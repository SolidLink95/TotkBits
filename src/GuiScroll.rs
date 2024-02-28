use egui::{scroll_area::ScrollAreaOutput};





pub struct EfficientScroll {
    response: Option<ScrollAreaOutput<()>>,
    text: String,
    text_chunk: String,
    chunk: usize,
    scrolled: f32,
}

impl Default for EfficientScroll {
    fn default() -> Self {
        Self {
            response: None,
            text: String::new(),
            text_chunk: String::new(),
            chunk: 4096,
            scrolled: 0.0
        }
    }
}

impl EfficientScroll {
    pub fn update(&mut self) {
        if let Some(r) = &self.response {
            let scrolled = r.state.offset.y;
            if scrolled != self.scrolled {
                let diff = self.scrolled - scrolled;
                self.scrolled = scrolled;
                let size = r.content_size.y;
                let fract = diff / size;
                if fract.abs() >= 0.01 { //process only if difference is bigger than 1%

                }
            }


        }
    }

}