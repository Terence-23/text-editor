#![feature(round_char_boundary)]

mod cmdline;
mod data;

use clap::Parser;
use cmdline::{CmdConfig, Config, SharedConfig};
use data::{FileData, Line, Prompt, PromptStatus, SharedData, TextPos};

const PREFIX_SIZE: usize = 5;
const STATUS_SIZE: usize = 1;
const MESSAGE_SIZE: usize = 1;

use std::{
    borrow::{Borrow, BorrowMut},
    cmp::min,
    io::{self, stdout, Stdout, Write},
    iter, thread,
    time::{Duration, Instant},
};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{
        read, KeyCode, KeyEvent, KeyModifiers, KeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    },
    execute, queue,
    style::{Print, StyledContent, Stylize},
    terminal::{Clear, ClearType},
};

use crate::data::{FileStatus, Message};
fn status_fmt<'a>(s: &'a str) -> StyledContent<&'a str> {
    s.bold().on_dark_grey().white()
}
fn normal_write(data: &FileData, config: &Config, stdout: &mut Stdout) -> io::Result<()> {
    queue!(stdout, Clear(ClearType::All), MoveTo(0, 0), Hide)?;
    let vstart = data.top_visible;
    let count = min(
        data.size.0 - STATUS_SIZE - MESSAGE_SIZE,
        data.lines.len() - vstart,
    );
    let f_name = if let Some(ref f) = config.file {
        format!("file: {}", f.display())
            + if data.f_status == FileStatus::Edited {
                "*"
            } else {
                " "
            }
    } else {
        "No file selected".to_owned()
    };
    let time = format!("time: {}\r\n", chrono::Local::now().format("%H:%M:%S"));
    queue!(
        stdout,
        Print(status_fmt(
            &f_name.chars().take(data.size.1).collect::<String>()
        ))
    )?;
    let l = time.chars().count();
    let fl = f_name.chars().count();
    if fl + l < data.size.1 {
        let chars = data.size.1 - l + 1 - fl;
        queue!(
            stdout,
            Print(status_fmt(
                &iter::repeat(' ').take(chars).collect::<String>()
            )),
            Print(status_fmt(&time))
        )?;
    }

    for (idx, l) in data.lines[vstart..].iter().enumerate().take(count) {
        let hstart = l.get_char_pos(data.left_visible);
        queue!(
            stdout,
            Print(format!(
                "{:0>3}| {}",
                idx + vstart,
                l.data[hstart..]
                    .chars()
                    .take(data.size.1 - PREFIX_SIZE)
                    .collect::<String>()
                    + "\r\n"
            ))
        )?;
        // eprintln!("{}", l.data)
    }
    // dbg!(count);
    for _ in count..(data.size.0 - STATUS_SIZE - 1 - MESSAGE_SIZE) {
        queue!(stdout, Print("~\r\n"))?;
    }
    if let Some(t) = data.message.show() {
        queue!(stdout, Print(status_fmt(t)))?;
    }

    queue!(
        stdout,
        MoveTo(
            (data.cursor_location.1 + PREFIX_SIZE - data.left_visible) as u16,
            (data.location.0 - data.top_visible + STATUS_SIZE) as u16
        ),
        Show
    )?;

    Ok(())
}
fn prompt_write(prompt: &Prompt, stdout: &mut Stdout, size: (usize, usize)) -> io::Result<()> {
    queue!(stdout, Hide, MoveTo(0, size.0 as u16 - 1))?;
    let offset = prompt.message.char_len + 1;
    let input: String = prompt.data.data[prompt.left_visible..]
        .chars()
        .take(size.1 - offset)
        .collect();
    queue!(
        stdout,
        Clear(ClearType::CurrentLine),
        Print(&prompt.message.data),
        Print(" "),
        Print(input)
    )?;
    queue!(
        stdout,
        MoveTo(
            (offset + prompt.cursor - prompt.left_visible) as u16,
            size.0 as u16 - 1
        ),
        Show
    )?;
    Ok(())
}

async fn screen_refresh(data: SharedData, config: SharedConfig) -> io::Result<()> {
    let mut stdout = stdout();
    // // last output time with arbitrary offset
    // let mut written = Instant::now() - Duration::from_secs(10);
    loop {
        thread::sleep(Duration::from_millis(10));
        let mut d = data.write();
        if d.ended {
            break;
        } else if !d.redraw {
            // eprintln!("no change");
            continue;
        }

        if let Some(ref p) = d.prompt {
            prompt_write(p, &mut stdout, (d.size.0, d.size.1))?;
        } else {
            normal_write(d.borrow(), &config.read(), &mut stdout)?;
        }
        d.redraw = false;
        stdout.flush()?;
    }
    Ok(())
}

fn scroll(w: &mut FileData) {
    if w.location.0 < w.top_visible {
        w.top_visible = w.location.0
    } else if w.location.0 >= w.top_visible + w.size.0 - STATUS_SIZE - MESSAGE_SIZE {
        w.top_visible = w.location.0 + 1 + STATUS_SIZE + MESSAGE_SIZE - w.size.0
    }
    if w.cursor_location.1 < w.left_visible {
        w.left_visible = w.cursor_location.1;
    } else if w.cursor_location.1 >= w.left_visible + w.size.1 - PREFIX_SIZE {
        // eprintln!("move right");
        w.left_visible = w.cursor_location.1 + 1 + PREFIX_SIZE - w.size.1
    }
    assert_eq!(
        w.cursor_location.0, w.location.0,
        "Different height of cursor and string pointer"
    )
}
fn prompt_scroll(prompt: &mut Prompt) {
    let size = crossterm::terminal::size().unwrap().0 as usize;
    if prompt.cursor < prompt.left_visible {
        prompt.left_visible = prompt.cursor;
    } else if prompt.cursor >= prompt.left_visible + size - prompt.message.char_len {
        // eprintln!("move right");
        prompt.left_visible = prompt.cursor + 1 + prompt.message.char_len - size
    }
}

fn normal_input(ke: KeyEvent, w: &mut FileData, config: &mut Config) {
    let tab: String = iter::repeat(' ').take(config.tab_size).collect();
    let (prev_char_bound, next_char_bound) = w.get_next_and_prev_chars();
    let pos = w.location;
    match ke.code {
        KeyCode::Backspace => {
            if pos.1 == 0 && pos.0 > 0 {
                // calculate new position of the cursor
                let n_pos = TextPos(pos.0 - 1, w.lines[pos.0 - 1].len());
                let line = w.lines[pos.0].clone();
                w.cursor_location.1 = w.lines[pos.0 - 1].char_len;
                w.lines[pos.0 - 1].push_str(&line.data); // append line contents
                w.lines.remove(pos.0); // remove old line
                w.location = n_pos;

                w.cursor_location.0 = pos.0 - 1;
                w.f_status = FileStatus::Edited;
            } else if pos.1 > 0 {
                w.lines[pos.0].remove(prev_char_bound); // remove previous character
                w.location.1 = prev_char_bound; // move cursor backward
                w.cursor_location.1 -= 1;
                w.f_status = FileStatus::Edited;
            }
        } // delete previous character if at start of line merge current into previous
        KeyCode::Enter => {
            let new_line = w.lines[pos.0].split_at(pos.1);
            w.lines.insert(pos.0 + 1, new_line);
            w.location.1 = 0;
            w.cursor_location.1 = 0;
            w.location.0 += 1;
            w.cursor_location.0 += 1;
            w.f_status = FileStatus::Edited;
        } // insert new line behind current
        KeyCode::Left => {
            if pos.1 > 0 {
                w.location.1 = prev_char_bound;
                w.cursor_location.1 -= 1;
            } else if pos.0 > 0 {
                w.location.0 -= 1;
                w.location.1 = w.lines[w.location.0].len();
                w.cursor_location.0 -= 1;
                w.cursor_location.1 = w.lines[w.location.0].char_len;
            }
        } // go to the left if at the start of the line go to end of previous
        KeyCode::Right => {
            if pos.1 < w.lines[pos.0].len() {
                w.location.1 = next_char_bound;
                w.cursor_location.1 += 1;
                dbg!(w.cursor_location, w.size, w.left_visible);
                assert!(w.cursor_location.1 < w.size.1 + w.left_visible);
            } else if pos.0 < w.lines.len() - 1 {
                w.location.1 = 0;
                w.location.0 += 1;
                w.cursor_location = w.location.to_owned();
            }
        } // go to the right if at the end of the line go to start of next
        KeyCode::Up => {
            if pos.0 > 0 {
                w.location.0 -= 1;
                w.location.1 = w.lines[w.location.0].get_char_pos(w.cursor_location.1);
                eprintln!("{}", w.cursor_location.0);
                w.cursor_location.0 -= 1;
                w.cursor_location.1 = min(w.cursor_location.1, w.lines[w.location.0].char_len);
            }
        } // go one line up
        KeyCode::Down => {
            if pos.0 < w.lines.len() - 1 {
                w.location.0 += 1;
                w.location.1 = w.lines[w.location.0].get_char_pos(w.cursor_location.1);
                w.cursor_location.0 += 1;
                w.cursor_location.1 = min(w.lines[w.location.0].char_len, w.cursor_location.1);
            }
        } // go one line down
        KeyCode::PageDown => {
            w.location.0 += w.size.0;
            w.cursor_location.0 += w.size.0;
            if w.location.0 >= w.lines.len() {
                w.location.0 = w.lines.len() - 1;
                w.cursor_location.0 = w.lines.len() - 1;
            }
            w.location.1 = w.lines[w.location.0].get_char_pos(w.cursor_location.1);
            w.cursor_location.1 = min(w.lines[w.location.0].char_len, w.cursor_location.1);
        }
        KeyCode::PageUp => {
            if pos.0 > w.size.0 {
                w.location.0 -= w.size.0;
                w.cursor_location.0 -= w.size.0;
            } else {
                w.location.0 = 0;
                w.cursor_location.0 = 0;
            }
            w.location.1 = w.lines[w.location.0].get_char_pos(w.cursor_location.1);
            w.cursor_location.1 = min(w.lines[w.location.0].char_len, w.cursor_location.1);
        }
        KeyCode::Home => {
            w.location.1 = 0;
            w.cursor_location.1 = 0;
        } // go to begin of line
        KeyCode::End => {
            w.location.1 = w.lines[w.location.0].len();
            w.cursor_location.1 = w.lines[w.location.0].char_len;
        } // go to end of line
        KeyCode::Tab => {
            w.lines[pos.0].insert_str(pos.1, &tab);
            w.location.1 += tab.len();
            w.cursor_location.1 += tab.len();
            w.f_status = FileStatus::Edited;
        } //insert tab
        KeyCode::BackTab => {
            let chars = w.lines[pos.0].back_tab(tab.len());
            if w.location.1 > chars {
                w.location.1 -= chars;
            } else {
                w.location.1 = 0;
            }
            if w.cursor_location.1 > chars {
                w.cursor_location.1 -= chars;
            } else {
                w.cursor_location.1 = 0;
            }
            if chars > 0 {
                w.f_status = FileStatus::Edited;
            }
        } // delete tab from the start of the current line
        KeyCode::Delete => {
            if pos.1 == w.lines[pos.0].len() && pos.0 < w.lines.len() - 1 {
                let data = w.lines[pos.0 + 1].data.to_owned();
                w.lines[pos.0].push_str(&data);
                w.lines.remove(pos.0 + 1);
                w.f_status = FileStatus::Edited;
            } else if pos.1 != w.lines[pos.0].len() {
                w.lines[pos.0].remove(pos.1);
                w.f_status = FileStatus::Edited;
            }
        } // delete next character if at end of line merge the next one into current
        KeyCode::Char(c) => {
            w.lines[pos.0].insert(pos.1, c);
            w.location.1 += c.len_utf8();
            w.cursor_location.1 += 1;
            w.f_status = FileStatus::Edited;
        } // insert character and advance the character pointer by 1
        // KeyCode::Null => todo!(),
        // KeyCode::Esc => todo!(),
        _ => {}
    }
    scroll(w.borrow_mut());
}
fn prompt_input(prompt: &mut Prompt, ke: KeyEvent) {
    let (prev_char_bound, next_char_bound) = prompt.data.get_next_and_prev_chars(prompt.location);
    let pos = prompt.location;
    match ke.code {
        KeyCode::Backspace => {
            if pos > 0 {
                prompt.data.remove(prev_char_bound);
                prompt.location = prev_char_bound;
                prompt.cursor -= 1;
            }
        }
        KeyCode::Enter => {
            prompt.status = PromptStatus::Success;
        }
        KeyCode::Left => {
            if pos > 0 {
                prompt.location = prev_char_bound;
                prompt.cursor -= 1;
            }
        }
        KeyCode::Right => {
            if pos < prompt.data.len() {
                prompt.location = next_char_bound;
                prompt.cursor += 1;
            }
        }
        KeyCode::Delete => {
            if pos != prompt.data.len() {
                prompt.data.remove(pos);
            }
        }
        KeyCode::Char(c) => {
            prompt.data.insert(pos, c);
            prompt.location += c.len_utf8();
            prompt.cursor += 1;
        }
        KeyCode::Esc => prompt.status = PromptStatus::Cancelled,
        _ => {}
    }
    prompt_scroll(prompt);
}

async fn event_loop(data: SharedData, config: SharedConfig) -> io::Result<()> {
    // eprintln!("start event_loop");
    loop {
        let event = read()?;
        // eprintln!("loop event");
        #[allow(unreachable_patterns)]
        match event {
            crossterm::event::Event::Key(ke) => {
                let mut w = data.write();
                w.redraw = true;
                // handling control
                if ke.modifiers.contains(KeyModifiers::CONTROL) {
                    // Ctrl+ Q
                    if ke.code == KeyCode::Char('q') {
                        if w.f_status == FileStatus::Edited {
                            w.message = Message::with_timeout(
                                "Press Ctrl + Q again to quit".to_owned(),
                                Duration::from_secs(5),
                            );
                            drop(w);
                            thread::sleep(Duration::from_millis(11));
                            w = data.write();
                            if let crossterm::event::Event::Key(ke) = read()? {
                                if ke.code == KeyCode::Char('q')
                                    && ke.modifiers.contains(KeyModifiers::CONTROL)
                                {
                                    w.ended = true;

                                    break;
                                }
                            }
                        } else {
                            w.ended = true;
                            break;
                        }
                    }
                    //Ctrl+S
                    else if ke.code == KeyCode::Char('s') {
                        if let Some(ref f) = config.read().file {
                            w.path = f.to_owned();
                            w.save()?;
                            w.message = Message::with_timeout(
                                format!("Saved: {}", w.path.display()),
                                Duration::from_secs(5),
                            );
                            w.f_status = FileStatus::Clean;
                        } else {
                            w.prompt = Some(Prompt::new("Path: ", data::PromptType::Save));
                        }
                    }
                    // Ctrl + f : Search
                    else if ke.code == KeyCode::Char('f') {
                        w.prompt = Some(Prompt::new("Search: ", data::PromptType::Search));
                    }
                    continue;
                }
                if w.ended {
                    break;
                }
                debug_assert_eq!(w.cursor_location.0, w.location.0, "Change before match");
                let pos = w.location;
                eprintln!("redraw");
                if let Some(ref mut p) = w.prompt {
                    prompt_input(p, ke);
                    match p.status {
                        PromptStatus::Pending => {}
                        PromptStatus::Cancelled => {
                            w.prompt = None;
                        }
                        PromptStatus::Success => {
                            match p.p_type {
                                data::PromptType::Save => {
                                    let f = &p.data.data;
                                    config.write().file = Some(f.into());
                                    w.path = f.into();
                                    w.save()?;
                                    w.f_status = FileStatus::Clean;
                                    w.message = Message::with_timeout(
                                        format!("Saved: {}", w.path.display()),
                                        Duration::from_secs(5),
                                    );
                                }
                                data::PromptType::Search => {
                                    let mut find = None;
                                    let text = p.data.data.to_owned();
                                    if let Some(f) = w.lines[pos.0].data[pos.1..].find(&text) {
                                        eprintln!(
                                            "Found in line from cursor: {} at {}, {}",
                                            &w.lines[pos.0].data[pos.1..],
                                            pos.0,
                                            pos.1 + f
                                        );

                                        find = Some((pos.0, f + pos.1));
                                    }
                                    if find == None {
                                        for (idx, l) in w.lines[pos.0 + 1..].iter().enumerate() {
                                            if let Some(f) = l.data.find(&text) {
                                                eprintln!(
                                                    "Found in next lines {} at {}, {}",
                                                    l.data,
                                                    pos.0 + 1 + idx,
                                                    f
                                                );
                                                find = Some((idx + pos.0 + 1, f));
                                                break;
                                            }
                                        }
                                    }
                                    if find == None {
                                        for (idx, l) in w.lines[..=pos.0].iter().enumerate() {
                                            if let Some(f) = l.data.find(&text) {
                                                eprintln!(
                                                    "From top in line: {} at {}, {}",
                                                    l.data, idx, f
                                                );
                                                find = Some((idx, f));
                                                break;
                                            }
                                        }
                                    }

                                    //scroll
                                    if let Some(loc) = find {
                                        w.location.0 = loc.0;
                                        w.location.1 = loc.1;
                                        w.cursor_location.0 = loc.0;
                                        // get char index of loc.1
                                        let mut count = 0;
                                        let mut iter = w.lines[loc.0].data.char_indices();
                                        while let Some((id, _)) = iter.next() {
                                            if loc.1 == id {
                                                break;
                                            }
                                            count += 1
                                        }
                                        w.cursor_location.1 = count;
                                        w.message = Message::with_timeout(
                                            format!(
                                                "Found: \"{}\" at Ln:{}, Col:{}",
                                                text, loc.0, w.cursor_location.1
                                            ),
                                            Duration::from_secs(5),
                                        )
                                    } else {
                                        w.message = Message::with_timeout(
                                            format!("Phrase: \"{}\" not found", text),
                                            Duration::from_secs(5),
                                        )
                                    }
                                    scroll(&mut w);
                                }
                            }

                            w.prompt = None;
                        }
                    }
                } else {
                    normal_input(ke, &mut w, &mut config.write());
                }
            }
            crossterm::event::Event::Resize(w, h) => {
                data.write().size = TextPos(h.into(), w.into());
                eprintln!("RESIZED");
                data.write().redraw = true;
            }
            _ => {}
            crossterm::event::Event::FocusGained => todo!(),
            crossterm::event::Event::FocusLost => todo!(),

            crossterm::event::Event::Mouse(_) => todo!(),
            crossterm::event::Event::Paste(_) => todo!(),
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let cmd_config = CmdConfig::parse();
    if cmd_config.check_actions()? {
        return Ok(());
    }
    let config = Config::from(cmd_config);
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(
        stdout(),
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    )?;
    let fdata = if let Some(ref f) = config.file {
        SharedData::from_path(f, config.clone())
    } else {
        SharedData::new()
    };
    let sc = SharedConfig::new(config);
    let event_handle = tokio::spawn(event_loop(fdata.clone(), sc.clone()));
    let refresh_handle = tokio::spawn(screen_refresh(fdata.clone(), sc.clone()));

    refresh_handle.await??;
    event_handle.await??;
    crossterm::terminal::disable_raw_mode()?;
    execute!(stdout(), Clear(ClearType::All), Show)?;
    // fdata.save()?;

    Ok(())
}
