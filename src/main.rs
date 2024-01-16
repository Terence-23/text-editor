#![feature(round_char_boundary)]

mod cmdline;
mod data;

use clap::Parser;
use cmdline::{CmdConfig, Config, SharedConfig};
use data::{Line, SharedData, TextPos};

const PREFIX_SIZE: usize = 5;
const STATUS_SIZE: usize = 1;
const MESSAGE_SIZE: usize = 1;

use std::{
    cmp::min,
    fmt::format,
    io::{self, stdout, Write},
    iter,
    process::exit,
    thread,
    time::Duration,
};

use crossterm::{
    cursor::{self, Hide, MoveTo, Show},
    event::{read, KeyCode, KeyModifiers, KeyboardEnhancementFlags, PushKeyboardEnhancementFlags},
    execute, queue,
    style::{Print, StyledContent, Stylize},
    terminal::{Clear, ClearType},
};

use crate::data::Message;
fn status_fmt<'a>(s: &'a str) -> StyledContent<&'a str> {
    s.bold().on_dark_grey().white()
}

async fn screen_refresh(data: SharedData, config: SharedConfig) -> io::Result<()> {
    let mut stdout = stdout();
    loop {
        queue!(stdout, Clear(ClearType::All), MoveTo(0, 0), Hide)?;
        let data = data.read();
        if data.ended {
            break;
        }
        let vstart = data.top_visible;
        let count = min(
            data.size.0 - STATUS_SIZE - MESSAGE_SIZE,
            data.lines.len() - vstart,
        );
        let f_name = if let Some(ref f) = config.read().file {
            format!("file: {}", f.display())
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
        for i in count..(data.size.0 - STATUS_SIZE - 1 - MESSAGE_SIZE) {
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

        stdout.flush()?;
        drop(data);
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

async fn event_loop(data: SharedData, config: SharedConfig) -> io::Result<()> {
    let tab: String = iter::repeat(' ').take(config.read().tab_size).collect();
    loop {
        let event = read()?;
        #[allow(unreachable_patterns)]
        match event {
            crossterm::event::Event::Key(ke) => {
                // handling control
                if ke.modifiers.contains(KeyModifiers::CONTROL) {
                    // Ctrl+ Q
                    if ke.code == KeyCode::Char('q') {
                        data.write().ended = true;
                        break;
                    }
                    //Ctrl+S
                    else if ke.code == KeyCode::Char('s') {
                        if let Some(ref f) = config.read().file {
                            let mut w = data.write();
                            w.path = f.to_owned();
                            w.save()?;
                            w.message = Message::with_timeout(
                                format!("Saved: {}", w.path.display()),
                                Duration::from_secs(5),
                            )
                        }
                    }
                    continue;
                }

                let (prev_char_bound, next_char_bound) = data.get_next_and_prev_chars();
                let mut w = data.write();
                let pos = w.location;
                if w.ended {
                    break;
                }
                debug_assert_eq!(w.cursor_location.0, w.location.0, "Change before match");
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

                            w.cursor_location.0 = pos.0 - 1
                        } else if pos.1 > 0 {
                            w.lines[pos.0].remove(prev_char_bound); // remove previous character
                            w.location.1 = prev_char_bound; // move cursor backward
                            w.cursor_location.1 -= 1;
                        }
                    } // delete previous character if at start of line merge current into previous
                    KeyCode::Enter => {
                        let new_line = w.lines[pos.0].split_at(pos.1);
                        w.lines.insert(pos.0 + 1, new_line);
                        w.location.1 = 0;
                        w.cursor_location.1 = 0;
                        w.location.0 += 1;
                        w.cursor_location.0 += 1;
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
                            w.cursor_location.1 =
                                min(w.cursor_location.1, w.lines[w.location.0].char_len);
                        }
                    } // go one line up
                    KeyCode::Down => {
                        if pos.0 < w.lines.len() - 1 {
                            w.location.0 += 1;
                            w.location.1 = w.lines[w.location.0].get_char_pos(w.cursor_location.1);
                            w.cursor_location.0 += 1;
                            w.cursor_location.1 =
                                min(w.lines[w.location.0].char_len, w.cursor_location.1);
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
                        w.cursor_location.1 =
                            min(w.lines[w.location.0].char_len, w.cursor_location.1);
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
                        w.cursor_location.1 =
                            min(w.lines[w.location.0].char_len, w.cursor_location.1);
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
                    } // delete tab from the start of the current line
                    KeyCode::Delete => {
                        if pos.1 == w.lines[pos.0].len() && pos.0 < w.lines.len() - 1 {
                            let data = w.lines[pos.0 + 1].data.to_owned();
                            w.lines[pos.0].push_str(&data);
                            w.lines.remove(pos.0 + 1);
                        } else if pos.1 != w.lines[pos.0].len() {
                            w.lines[pos.0].remove(pos.1);
                        }
                    } // delete next character if at end of line merge the next one into current
                    KeyCode::Char(c) => {
                        w.lines[pos.0].insert(pos.1, c);
                        w.location.1 += c.len_utf8();
                        w.cursor_location.1 += 1;
                    } // insert character and advance the character pointer by 1
                    KeyCode::Null => todo!(),
                    KeyCode::Esc => todo!(),
                    _ => {}
                }
                // scroll
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
            crossterm::event::Event::Resize(w, h) => {
                data.write().size = TextPos(h.into(), w.into());
                eprintln!("RESIZED");
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
    fdata.save()?;

    Ok(())
}
