use std::io::{self, stdout};

use crossterm::{
    event::{read, KeyCode},
    execute,
    style::Print,
};

fn event_loop() -> io::Result<()> {
    let mut stdout = stdout();
    loop {
        let event = read()?;
        #[allow(unreachable_patterns)]
        match event {
            crossterm::event::Event::Key(ke) => {
                if ke.code == KeyCode::Char('q') {
                    break;
                }
                let txt = match ke.code {
                    KeyCode::Enter => ("\r\n").to_owned(),
                    KeyCode::Tab => "   ".to_owned(),
                    KeyCode::Char(c) => c.to_string(),
                    _ => "".to_owned(),
                };

                execute!(stdout, Print(txt))?;
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

fn main() -> io::Result<()> {
    println!("Hello, world!");
    crossterm::terminal::enable_raw_mode()?;
    event_loop()?;
    crossterm::terminal::disable_raw_mode()?;

    Ok(())
}
