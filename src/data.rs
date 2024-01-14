use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Line {
    pub data: String,
}

impl From<String> for Line {
    fn from(value: String) -> Self {
        Line { data: value }
    }
}
impl Line {
    fn new(txt: &str) -> Self {
        Self {
            data: txt.split('\n').next().unwrap_or("").to_owned(),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct TextPos(pub usize, pub usize);
#[derive(Clone, Debug)]
pub struct FileData {
    pub lines: Vec<Line>,
    pub path: PathBuf,
    pub location: TextPos,
    pub ended: bool,
}
impl FileData {
    pub fn new() -> Self {
        Self {
            lines: vec![Line::new("")],
            path: PathBuf::default(),
            location: TextPos(0, 0),
            ended: false,
        }
    }
    pub fn get_next_and_prev_chars(&self) -> (usize, usize) {
        let line_text = &self.lines[self.location.0].data;
        (
            if self.location.1 == 0 {
                self.location.1
            } else {
                line_text.floor_char_boundary(self.location.1 - 1)
            },
            if self.location.1 == line_text.len() {
                self.location.1
            } else {
                line_text.ceil_char_boundary(self.location.1 + 1)
            },
        )
    }
}
