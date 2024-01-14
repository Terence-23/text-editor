use std::{cmp::min, path::PathBuf, sync::Arc};

use parking_lot::RwLock;

#[derive(Clone, Debug)]
pub struct Line {
    pub data: String,
    pub char_len: usize,
}

impl From<String> for Line {
    fn from(value: String) -> Self {
        Line {
            char_len: value.chars().count(),
            data: value,
        }
    }
}
impl From<&str> for Line {
    fn from(value: &str) -> Self {
        Line {
            data: value.to_owned(),
            char_len: value.chars().count(),
        }
    }
}
impl Line {
    fn new(txt: &str) -> Self {
        Self::from(txt.split('\n').next().unwrap_or(""))
    }
    pub fn remove(&mut self, idx: usize) {
        self.char_len -= 1;
        self.data.remove(idx);
    }
    pub fn split_at(&mut self, idx: usize) -> Line {
        let remainder = self.data[idx..].to_owned();
        self.data = self.data[..idx].to_owned();
        self.char_len = self.data.chars().count();
        Line::from(remainder)
    }
    pub fn push_str(&mut self, str: &str) {
        let len = str.chars().count();
        self.char_len += len;
        self.data.push_str(str);
    }
    pub fn insert(&mut self, idx: usize, c: char) {
        self.char_len += 1;
        self.data.insert(idx, c);
    }
    pub fn len(&self) -> usize {
        self.data.len()
    }
    pub fn get_char_pos(&self, idx: usize) -> usize {
        if idx == 0 {
            return 0;
        }
        self.data.ceil_char_boundary(
            self.data
                .char_indices()
                .take(idx)
                .last()
                .unwrap_or((0, '\0'))
                .0
                + 1,
        )
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
    pub size: TextPos,
    pub top_visible: usize,
    pub cursor_location: TextPos,
}
impl Drop for FileData {
    fn drop(&mut self) {
        dbg!(&self);
        let start = self.top_visible;
        let end = min(self.top_visible + self.size.0 + 1, self.lines.len());
        for l in &self.lines[start..end] {
            eprintln!("{}", l.data);
        }
    }
}
impl FileData {
    pub fn new() -> Self {
        let (w, h) = crossterm::terminal::size().unwrap();
        Self {
            lines: vec![Line::new("")],
            path: PathBuf::default(),
            cursor_location: TextPos(0, 0),
            location: TextPos(0, 0),
            ended: false,
            size: TextPos(h.into(), w.into()),
            top_visible: 0,
        }
    }
    #[allow(unused)]
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

#[derive(Clone, Debug)]
pub struct SharedData {
    data: Arc<RwLock<FileData>>,
}

impl Drop for SharedData {
    fn drop(&mut self) {
        eprintln!("Dropped file data");
        if std::thread::panicking() {
            eprintln!("panicking");
            self.data.write().ended = true;
        }
    }
}

impl SharedData {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(FileData::new())),
        }
    }
    pub fn get_next_and_prev_chars(&self) -> (usize, usize) {
        let wdata = self.data.write();
        let line_text = &wdata.lines[wdata.location.0].data;
        (
            if wdata.location.1 == 0 {
                wdata.location.1
            } else {
                line_text.floor_char_boundary(wdata.location.1 - 1)
            },
            if wdata.location.1 == line_text.len() {
                wdata.location.1
            } else {
                line_text.ceil_char_boundary(wdata.location.1 + 1)
            },
        )
    }
    pub fn write(
        &self,
    ) -> parking_lot::lock_api::RwLockWriteGuard<'_, parking_lot::RawRwLock, FileData> {
        self.data.write()
    }
    pub fn read(
        &self,
    ) -> parking_lot::lock_api::RwLockReadGuard<'_, parking_lot::RawRwLock, FileData> {
        self.data.read()
    }
}
