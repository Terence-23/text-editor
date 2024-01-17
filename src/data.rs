use std::{
    default,
    fs::File,
    io::{self, stdout, BufWriter, Write},
    iter,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use crossterm::{cursor::Show, execute};
use parking_lot::RwLock;
use std::time::Instant;

use crate::cmdline::Config;

#[derive(Debug, Clone)]
pub struct Message {
    pub text: String,
    pub timeout: Duration,
    pub time: Instant,
}
impl Message {
    fn new(text: String) -> Self {
        Self {
            text,
            time: Instant::now(),
            timeout: Duration::from_secs(5),
        }
    }
    pub fn with_timeout(text: String, timeout: Duration) -> Self {
        Self {
            text: text,
            timeout: timeout,
            time: Instant::now(),
        }
    }
    pub fn show<'a>(&'a self) -> Option<&'a str> {
        if self.timeout >= self.time.elapsed() {
            return Some(&self.text);
        }
        return None;
    }
}

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
    /// try to remove a tab from beginning of the line returns the amount of removed characters
    pub fn back_tab(&mut self, max: usize) -> usize {
        let mut count = 0;
        while self.data.chars().next() == Some(' ') && count < max {
            self.data.remove(0);
            count += 1;
        }
        self.char_len -= count;
        count
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
    pub fn insert_str(&mut self, idx: usize, str: &str) {
        self.char_len += str.chars().count();
        self.data.insert_str(idx, str);
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
    pub fn get_char_span(&self, first: usize, last: usize) -> (usize, usize) {
        let span = last - first;
        let first = if first == 0 {
            0
        } else {
            self.data.ceil_char_boundary(
                self.data
                    .char_indices()
                    .take(first)
                    .last()
                    .unwrap_or((0, '\0'))
                    .0
                    + 1,
            )
        };
        let last = if last == 0 {
            0
        } else {
            self.data.ceil_char_boundary(
                self.data
                    .char_indices()
                    .take(span)
                    .last()
                    .unwrap_or((0, '\0'))
                    .0
                    + 1,
            )
        };
        (first, last)
    }
    pub fn get_next_and_prev_chars(&self, loc: usize) -> (usize, usize) {
        (
            if loc == 0 {
                loc
            } else {
                self.data.floor_char_boundary(loc - 1)
            },
            if loc == self.data.len() {
                loc
            } else {
                self.data.ceil_char_boundary(loc + 1)
            },
        )
    }
}
#[derive(Clone, Copy, Debug)]
pub struct TextPos(pub usize, pub usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FileStatus {
    /// file was not edited
    #[default]
    Clean,
    /// file was edited
    Edited,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PromptType {
    #[default]
    Save,
    Search,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PromptStatus {
    #[default]
    Pending,
    Cancelled,
    Success,
}
#[derive(Clone, Debug)]
pub struct Prompt {
    pub message: Line,
    pub data: Line,
    pub p_type: PromptType,
    pub location: usize,
    pub cursor: usize,
    pub status: PromptStatus,
    pub left_visible: usize,
}
impl Prompt {
    pub fn new(message: &str, p_type: PromptType) -> Prompt {
        Prompt {
            message: Line::new(message),
            data: Line::new(""),
            p_type: p_type,
            location: 0,
            cursor: 0,
            status: PromptStatus::Pending,
            left_visible: 0,
        }
    }
}
#[derive(Clone, Debug)]
pub struct FileData {
    pub lines: Vec<Line>,
    pub path: PathBuf,
    pub location: TextPos,
    pub ended: bool,
    pub size: TextPos,
    pub top_visible: usize,
    pub left_visible: usize,
    pub cursor_location: TextPos,
    pub message: Message,
    pub f_status: FileStatus,
    pub prompt: Option<Prompt>,
    pub redraw: bool,
}
impl Drop for FileData {
    fn drop(&mut self) {
        execute!(stdout(), Show).unwrap_or(());
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
            left_visible: 0,
            message: Message::new("Press Ctrl+Q to quit".to_owned()),
            f_status: FileStatus::Clean,
            prompt: None,
            redraw: true,
        }
    }
    pub fn from_path(path: &Path, config: Config) -> Self {
        let (w, h) = crossterm::terminal::size().unwrap();
        let tab: String = iter::repeat(' ').take(config.tab_size).collect();
        let mut lines: Vec<Line> =
            String::from_utf8(std::fs::read(path).unwrap_or("".bytes().collect()))
                .unwrap_or("".to_string())
                .replace('\t', &tab)
                .lines()
                .map(|s| Line::from(s))
                .collect();

        if lines.is_empty() {
            lines = vec!["".into()]
        }

        Self {
            lines,
            path: PathBuf::from(path),
            cursor_location: TextPos(0, 0),
            location: TextPos(0, 0),
            ended: false,
            size: TextPos(h.into(), w.into()),
            top_visible: 0,
            left_visible: 0,
            message: Message::new("Press Ctrl+Q to quit".to_owned()),
            f_status: FileStatus::Clean,
            prompt: None,
            redraw: true,
        }
    }
    pub fn save(&self) -> io::Result<()> {
        let mut w = BufWriter::new(File::create(self.path.to_owned())?);
        for l in self.lines.iter() {
            write!(w, "{}\r\n", l.data)?;
        }
        w.flush()?;
        Ok(())
    }
    #[allow(unused)]
    pub fn get_next_and_prev_chars(&self) -> (usize, usize) {
        self.lines[self.location.0].get_next_and_prev_chars(self.location.1)
    }
}

#[derive(Clone, Debug)]
pub struct SharedData {
    data: Arc<RwLock<FileData>>,
}

impl Drop for SharedData {
    fn drop(&mut self) {
        eprintln!("Dropped file data");
        self.data.write().ended = true;
        if std::thread::panicking() {
            eprintln!("panicking");
        }
    }
}

impl SharedData {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(FileData::new())),
        }
    }
    pub fn from_path(path: &Path, config: Config) -> Self {
        Self {
            data: Arc::new(RwLock::new(FileData::from_path(path, config))),
        }
    }
    pub fn save(&self) -> io::Result<()> {
        self.data.read().save()
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
