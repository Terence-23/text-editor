#![feature(round_char_boundary)]

mod data;

use data::{Line, TextPos};
use parking_lot::RwLock;
use std::{
    io::{self, stdout, Write},
    sync::Arc,
    thread,
    time::Duration,
};

use crossterm::{
    cursor::MoveTo,
    event::{read, KeyCode, KeyModifiers, KeyboardEnhancementFlags, PushKeyboardEnhancementFlags},
    execute, queue,
    style::Print,
    terminal::{Clear, ClearType},
};

async fn screen_refresh(data: Arc<RwLock<data::FileData>>) -> io::Result<()> {
    let mut stdout = stdout();
    loop {
        eprintln!("START");
        queue!(stdout, Clear(ClearType::All))?;
        queue!(stdout, MoveTo(0, 0))?;
        let data = data.read();
        if data.ended {
            break;
        }
        for l in &data.lines {
            queue!(stdout, Print(l.data.to_owned() + "\r\n"))?;
            eprintln!("{}", l.data)
        }
        queue!(
            stdout,
            MoveTo(data.location.1 as u16, data.location.0 as u16)
        )?;

        stdout.flush()?;
        drop(data);
        thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

async fn event_loop(data: Arc<RwLock<data::FileData>>) -> io::Result<()> {
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

                let (prev_char_bound, next_char_bound) = data.read().get_next_and_prev_chars();
                let mut w = data.write();
                let pos = w.location;
                match ke.code {
                    KeyCode::Backspace => {
                        if pos.1 == 0 && pos.0 > 0 {
                            // calculate new position of the cursor
                            let n_pos = TextPos(pos.0 - 1, w.lines[pos.0 - 1].data.len());
                            let line = w.lines[pos.0].clone();
                            w.lines[pos.0 - 1].data.push_str(&line.data); // append line contents
                            w.lines.remove(pos.0); // remove old line
                            w.location = n_pos
                        } else {
                            w.lines[pos.0].data.remove(prev_char_bound); // remove previous character
                            w.location.1 = prev_char_bound; // move cursor backward
                        }
                    } // delete previous character if at start of line merge current into previous
                    KeyCode::Enter => {
                        let remainder = w.lines[pos.0].data[pos.1..].to_owned();
                        w.lines[pos.0].data = w.lines[pos.0].data[..pos.1].to_owned();
                        w.lines.insert(pos.0 + 1, Line::from(remainder));
                        w.location = TextPos(pos.0 + 1, 0);
                    } // insert new line behind current
                    KeyCode::Left => todo!(), // go to the left if at the start of the line go to end of previous
                    KeyCode::Right => todo!(), // go to the right if at the end of the line go to start of next
                    KeyCode::Up => todo!(),    // go one line up
                    KeyCode::Down => todo!(),  // go one line down
                    KeyCode::Home => todo!(),  // go to begin of line
                    KeyCode::End => todo!(),   // go to end of line
                    KeyCode::Tab => todo!(),   //insert tab
                    KeyCode::BackTab => todo!(), // delete tab from the start of the current line
                    KeyCode::Delete => todo!(), // delete next character if at end of line merge the next one into current
                    KeyCode::Char(c) => {
                        w.lines[pos.0].data.insert(pos.1, c);
                        w.location.1 += c.len_utf8()
                    } // insert character and advance the character pointer by 1
                    KeyCode::Null => todo!(),
                    KeyCode::Esc => todo!(),
                    _ => {}
                }
            }
            _ => {}
            crossterm::event::Event::FocusGained => todo!(),
            crossterm::event::Event::FocusLost => todo!(),

            crossterm::event::Event::Mouse(_) => todo!(),
            crossterm::event::Event::Paste(_) => todo!(),
            crossterm::event::Event::Resize(_, _) => todo!(),
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> io::Result<()> {
    println!("Hello, world!");
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(
        stdout(),
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    );
    let fdata = Arc::new(RwLock::new(data::FileData::new()));
    let event_handle = tokio::spawn(event_loop(fdata.clone()));
    tokio::spawn(screen_refresh(fdata.clone()));

    // refresh_handle.await?;
    event_handle.await??;
    crossterm::terminal::disable_raw_mode()?;

    Ok(())
}
