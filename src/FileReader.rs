use egui::{scroll_area::ScrollAreaOutput, Align, Key};
use std::{
    io::{self, BufReader, Cursor, Read, Seek},
};

#[derive(Debug)]
pub struct Pos {
    //MUST BE i32, PANIC WHILE UPDATING OTHERWISE!
    x: i32,
    y: i32,
}

#[derive(Debug)]
pub struct FileReader {
    pub reader: BufReader<Cursor<Vec<u8>>>,
    pub buffer: Vec<u8>,
    pub len: usize,
    pub pos: Pos,
    pub reload: bool, //flag to prevent read file in every frame
    pub displayed_text: String,
    pub buf_size: usize,
    pub default_buf_size: usize,
    pub scroll_pos: f32,
    pub full_text: String,
    pub old_text: String,
    pub is_text_changed: bool,
    pub lines_offset: usize,
    pub max_line_chars: usize,
    pub dir: bool,
}

impl Default for FileReader {
    fn default() -> Self {
        let len = 0 as usize;
        let buffer = Vec::new(); // Initialize an empty buffer
        let cursor = Cursor::new(buffer.clone());
        let reader = BufReader::new(cursor);
        Self {
            reader: reader,
            //writer: BufWriter::new(g),
            //in_file: in_file.to_string(),
            //out_file: out_file.to_string(),
            //f: f,
            buffer: buffer,
            len: len,
            pos: Pos { x: 0, y: 0 },
            reload: false,
            displayed_text: String::new(),
            buf_size: 8192 as usize,
            default_buf_size: 8192 as usize,
            scroll_pos: 0.0,
            full_text: String::new(),
            old_text: String::new(),
            is_text_changed: false,
            lines_offset: 0,
            max_line_chars: 2048,
            dir: false,
        }
    }
}

impl FileReader {
    pub fn from_string(&mut self, text: &str) -> io::Result<()> {
        self.buffer = text.as_bytes().to_vec();
        self.reader = BufReader::new(Cursor::new(self.buffer.clone()));
        self.len = text.len() as usize;
        self.displayed_text = text.to_string();
        self.full_text = text.to_string();
        self.old_text = self.displayed_text.clone();
        self.reload = true;
        Ok(())
    }

    pub fn refresh_buffer(&mut self) -> io::Result<()> {
        let smiling_face = String::from("\u{1F60A}");
        self.displayed_text = format!("{}{}", self.displayed_text.clone(), smiling_face);
        //self.displayed_text = self.displayed_text[1..].to_string();
        Ok(())
    }

    pub fn update(&mut self) -> io::Result<()> {
        if self.reload && !self.full_text.is_empty() {
            if self.update_pos() {
                println!(
                    "Reloading buffer {:?} ({}-{})",
                    self.pos,
                    self.pos.y - self.pos.x,
                    self.buf_size
                );
                let smiling_face = String::from("\u{1F60A}");

                self.lines_offset = self.buffer[0..self.pos.x as usize]
                    .iter()
                    .filter(|&&c| c == b'\n')
                    .count();
                self.reader
                    .seek(std::io::SeekFrom::Start(self.pos.x as u64))?;
                let mut buffer = vec![0; (self.pos.y - self.pos.x) as usize];

                self.reader.read_exact(&mut buffer)?;

                // Convert bytes to String
                self.displayed_text = String::from_utf8(buffer)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                self.displayed_text = self.displayed_text.replace(&smiling_face, "");
                self.old_text = self.displayed_text.clone();
            }

            self.reload = false; //make sure updates only once by itself
        }
        Ok(())
    }

    pub fn update_text_changed(&mut self) -> io::Result<()> {
        //self.f.unlock()?;
        if self.displayed_text.len() < self.buf_size as usize {
            self.buffer = self.displayed_text.as_bytes().to_vec();
            self.len = self.displayed_text.len() as usize;
            //write_string_to_file(&self.in_file, &self.displayed_text)?;
            println!("Text updated!");
        } else {
            let chunk_1st = &self.full_text[0..self.pos.x as usize];
            let mut chunk_end = "";
            if self.pos.y < self.full_text.len() as i32 {
                chunk_end = &self.full_text[self.pos.y as usize..];
            }
            let new_text = format!("{}{}{}", chunk_1st, self.displayed_text, chunk_end);
            //write_string_to_file(&self.in_file, &new_text)?;
            self.buffer = new_text.as_bytes().to_vec();
            self.len = new_text.len() as usize;
            self.full_text = new_text;
            println!("Text updated! {:?}", self.pos);
        }
        self.reader = BufReader::new(Cursor::new(self.buffer.clone()));
        self.reader
            .seek(std::io::SeekFrom::Start(self.pos.x as u64))?;

        self.old_text = self.displayed_text.clone();
        if self.buf_size < self.default_buf_size {
            self.buf_size = self.default_buf_size.clone();
        }
        //self.buf_size = self.default_buf_size.max(self.full_text.len() as usize);
        //self.f.lock_exclusive()?;
        Ok(())
    }

    pub fn add_pos(&mut self, val: i32) {
        self.pos.x += val;
        self.pos.y += val;
    }

    pub fn set_pos(&mut self, x: i32, y: i32) {
        self.pos.x = x;
        self.pos.y = y;
    }
    pub fn pos_at_top(&mut self) -> bool {
        if self.pos.x <= 0 {
            self.pos.x = 0;
            self.pos.y = self.buf_size as i32;
            return true;
        }
        return false;
    }

    pub fn pos_at_bottom(&mut self) -> bool {
        let max_size = (self.len - 1).max(0);
        if self.pos.y >= max_size as i32 {
            self.pos.y = max_size as i32;
            self.pos.x = (max_size - self.buf_size) as i32;
            return true;
        }
        false
    }

    pub fn update_pos(&mut self) -> bool {
        //if self.pos_at_top() {return false;}
        //if self.pos_at_bottom() {return false;}
        let max_size = (self.len).max(0) as i32;
        self.pos.x = self.pos.x.max(0);
        self.pos.x = self.pos.x.min(max_size);
        self.pos.y = self.pos.y.max(0);
        self.pos.y = self.pos.y.min(max_size);
        self.pos.y = self.pos.y.max(self.pos.x);
        self.pos.y = self.pos.y.max(self.buf_size as i32);
        self.pos.x = self.pos.x.min(self.len as i32 - 1 - self.buf_size as i32);

        self.pos.x = self.pos.x.max(0);
        //self.advance_to_newline();
        return true;
    }

    pub fn advance_to_newline(&mut self) {
        let max_chars: i32 = 2048; //if the line is too long get this many bytes
        let mut ind: i32 = 0;
        let mut x = self.pos.x.clone();
        let mut y = self.pos.y.clone();
        let len = self.buffer.len() as i32;
        let mut change: i32 = -1;
        if self.dir {
            change = 1;
        }
        self.dir = !self.dir;
        loop {
            if x == 0 || ind >= max_chars {
                break;
            }
            if self.buffer.get(x as usize) == Some(&b'\n') {
                break;
            }
            x -= change;
            ind += 1;
        }
        self.pos.x = x as i32;
        ind = 0;
        loop {
            if y >= len || ind >= max_chars {
                break;
            }
            if self.buffer.get(y as usize) == Some(&b'\n') {
                break;
            }
            y += change;
            ind += 1;
        }
        self.pos.y = y as i32;
    }

    pub fn scroll_test(ui: &egui::Ui, resp: &Option<ScrollAreaOutput<()>>, scroll_offset: f32) {
        if let Some(r) = resp {
            let visible_rect = r.inner_rect.clone();
            // Create a new rectangle adjusted by the offset
            let target_rect = egui::Rect::from_min_size(
                visible_rect.min + egui::vec2(0.0, scroll_offset + 3.0),
                visible_rect.size(),
            );
            // Scroll to the new rectangle
            ui.scroll_to_rect(target_rect, Some(egui::Align::Center));
            ui.scroll_to_cursor(Some(Align::Center));
        }
    }

    pub fn check_for_changes(
        &mut self,
        ctx: &egui::Context,
        _ui: &egui::Ui,
        resp: &Option<ScrollAreaOutput<()>>,
    ) -> f32 {
        let mut res: f32 = 0.0;
        ctx.input(|i| {
            //handle scrolling and page up/down
            let mut scrolled: i32 = 0;
            if i.raw_scroll_delta.y != 0.0 {
                scrolled = -i.raw_scroll_delta.y as i32;
                //Self::scroll_test( ui, resp, i.raw_scroll_delta.y);
            } else if i.key_pressed(Key::PageDown) {
                scrolled = ScrollValues::PageDown.value();
                if self.pos.y == self.buffer.len() as i32 {
                    res = ScrollValues::ScrollBottom.value() as f32;
                }
                //Gui::scroll_test(resp, ui, -(scrolled as f32));
                //Self::scroll_test( ui, resp, -scrolled as f32);
            } else if i.key_pressed(Key::PageUp) {
                scrolled = ScrollValues::PageUp.value();
                if self.pos.x == 0 {
                    res = ScrollValues::ScrollTop.value() as f32;
                }
                //Self::scroll_test( ui, resp, -scrolled as f32);
            }
            if scrolled != 0 {
                if self.displayed_text != self.old_text {
                    println!("Text changed!");
                    //self.scroll(ui, 50.0);
                    //self.is_text_changed = true;
                    self.update_text_changed();
                }
                //println!("Scrolled {}", scrolled);
                self.add_pos(scrolled as i32);
                //self.scroll(ui, -scrolled as f32);
                self.reload = true;
                if let Some(r) = resp {
                    //res = -(scrolled as f32);
                    self.scroll_pos = r.state.offset.y;
                }
            } else if let Some(r) = resp {
                let offset: i32 = ScrollValues::Segment.value();
                if r.state.offset.y.floor() != self.scroll_pos.floor() {
                    let diff = (r.state.offset.y - self.scroll_pos) as i32;
                    if diff.abs() > offset || (r.content_size.y - r.state.offset.y) < offset as f32
                    {
                        //res = -(diff as f32);
                        self.add_pos(diff);
                        //self.scroll(ui, -diff as f32);
                        println!("{} - {} = {}", self.scroll_pos, r.state.offset.y, diff);
                        self.reload = true;
                        self.scroll_pos = r.state.offset.y;

                        if self.displayed_text != self.old_text && !self.is_text_changed {
                            println!("Text changed!");
                            //self.is_text_changed = true;
                            self.update_text_changed();
                        }
                    }
                }
            }
        });
        res
        //handle manual scrollbar changes
    }

    pub fn get_status(&self, arg: String) -> String {
        format!(
            "{:?} len {} text len {} buf size {} scroll pos {:.1}, arg {}",
            self.pos,
            self.len,
            self.displayed_text.len(),
            self.buf_size,
            self.scroll_pos,
            arg
        )
    }
}

enum ScrollValues {
    PageDown,
    PageUp,
    ScrollDown,
    ScrollUp,
    ScrollTop,
    ScrollBottom,
    Segment,
}

impl ScrollValues {
    fn value(&self) -> i32 {
        match self {
            ScrollValues::PageDown => 500,
            ScrollValues::PageUp => -500,
            ScrollValues::ScrollDown => 50,
            ScrollValues::ScrollUp => -50,
            ScrollValues::ScrollTop => -9999999,
            ScrollValues::ScrollBottom => 9999999,
            ScrollValues::Segment => 200,
        }
    }
}
