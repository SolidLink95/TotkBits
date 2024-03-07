use crate::file_format::BinTextFile::OpenedFile;

#[derive(PartialEq)]
pub enum ActiveTab {
    DiretoryTree,
    TextBox,
    Settings,
}


pub struct TotkBitsApp<'a> {
    pub opened_file: OpenedFile<'a>,     //path to opened file in string

}

impl Default for TotkBitsApp<'_> {
    fn default() -> Self {
        Self {
            opened_file: OpenedFile::default(),
        }
    }
}

impl<'a> TotkBitsApp<'_> {}