#![feature(round_char_boundary)]

mod data;

use data::{Line, SharedData, TextPos};

use std::{
    cmp::min,
    io::{self, stdout, Write},
    thread,
    time::Duration,
};

use crossterm::{
    cursor::{self, Hide, MoveTo, Show},
    event::{read, KeyCode, KeyModifiers, KeyboardEnhancementFlags, PushKeyboardEnhancementFlags},
    execute, queue,
    style::Print,
    terminal::{Clear, ClearType},
};

async fn screen_refresh(data: SharedData) -> io::Result<()> {
    let mut stdout = stdout();
    loop {
        queue!(stdout, Clear(ClearType::All), MoveTo(0, 0), Hide)?;
        let data = data.read();
        if data.ended {
            break;
        }
        let start = data.top_visible;
        let end = min(data.top_visible + data.size.0, data.lines.len());
        for (idx, l) in data.lines[start..end].iter().enumerate() {
            queue!(
                stdout,
                Print(format!(
                    "{:0>3}| {}",
                    idx + start,
                    l.data.to_owned() + if idx < data.size.0 - 1 { "\r\n" } else { "" }
                ))
            )?;
            // eprintln!("{}", l.data)
        }
        for i in end - start..data.size.0 {
            queue!(
                stdout,
                if i < data.size.0 - 1 {
                    Print("~\r\n")
                } else {
                    Print("~")
                }
            )?;
        }
        queue!(
            stdout,
            MoveTo(
                (data.cursor_location.1 + 5) as u16,
                (data.location.0 - data.top_visible) as u16
            ),
            Show
        )?;

        stdout.flush()?;
        drop(data);
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

async fn event_loop(data: SharedData) -> io::Result<()> {
    loop {
        let event = read()?;
        #[allow(unreachable_patterns)]
        match event {
            crossterm::event::Event::Key(ke) => {
                if ke.modifiers.contains(KeyModifiers::CONTROL) {
                    if ke.code == KeyCode::Char('q') {
                        data.write().ended = true;
                        break;
                    }
                    continue;
                }

                let (prev_char_bound, next_char_bound) = data.get_next_and_prev_chars();
                let mut w = data.write();
                let pos = w.location;
                assert_eq!(w.cursor_location.0, w.location.0, "Change before match");
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
                        assert_eq!(w.cursor_location.0, w.location.0, "Change before enter");
                        let new_line = w.lines[pos.0].split_at(pos.1);
                        w.lines.insert(pos.0 + 1, new_line);
                        w.location.1 = 0;
                        w.cursor_location.1 = 0;
                        w.location.0 += 1;
                        w.cursor_location.0 += 1;
                        assert_eq!(w.cursor_location.0, w.location.0, "Change after enter");
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
                    KeyCode::Tab => todo!(),     //insert tab
                    KeyCode::BackTab => todo!(), // delete tab from the start of the current line
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
                if w.location.0 < w.top_visible {
                    w.top_visible = w.location.0
                } else if w.location.0 >= w.top_visible + w.size.0 {
                    w.top_visible = w.location.0 - w.size.0 + 1
                }
                assert_eq!(w.cursor_location.0, w.location.0, "Change")
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
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(
        stdout(),
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    )?;
    let fdata = SharedData::new();
    let event_handle = tokio::spawn(event_loop(fdata.clone()));
    tokio::spawn(screen_refresh(fdata.clone()));

    // refresh_handle.await?;
    event_handle.await??;
    crossterm::terminal::disable_raw_mode()?;
    execute!(stdout(), Clear(ClearType::All), Show)?;

    Ok(())
}
