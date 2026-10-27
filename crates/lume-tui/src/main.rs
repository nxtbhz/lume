mod app;
mod args;
mod commands;
mod explorer;
mod input;
mod prompt;
mod render;
mod search;
mod terminal;
mod vim;

#[cfg(test)]
mod tests;

use std::env;
use std::io::{stdout, BufWriter, Write};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEvent, KeyEventKind, MouseEvent};

use app::App;
use args::{parse_cli_args, print_help, CliCommand};
use terminal::TerminalGuard;

enum AppEvent {
    Input(KeyEvent),
    Mouse(MouseEvent),
    Resize(u16, u16),
    FocusLost,
}

fn run<W: Write>(app: &mut App, writer: &mut W) -> std::io::Result<()> {
    let (tx, rx) = crossbeam_channel::unbounded();

    let input_tx = tx.clone();
    std::thread::spawn(move || loop {
        let app_event = match event::read() {
            Ok(Event::Key(key)) if key.kind != KeyEventKind::Release => AppEvent::Input(key),
            Ok(Event::Mouse(mouse)) => AppEvent::Mouse(mouse),
            Ok(Event::Resize(w, h)) => AppEvent::Resize(w, h),
            Ok(Event::FocusLost) => AppEvent::FocusLost,
            _ => continue,
        };
        if input_tx.send(app_event).is_err() {
            break;
        }
    });

    app.adjust_viewport();
    app.render(writer)?;

    while app.running {
        match rx.recv_timeout(Duration::from_millis(16)) {
            Ok(AppEvent::Input(key)) => {
                app.handle_key(key)?;
                app.render(writer)?;
            }
            Ok(AppEvent::Mouse(mouse)) => {
                app.handle_mouse(mouse)?;
                app.render(writer)?;
            }
            Ok(AppEvent::Resize(w, h)) => {
                app.terminal_width = w;
                app.terminal_height = h;
                app.adjust_viewport();
                app.render(writer)?;
            }
            Ok(AppEvent::FocusLost) => {
                if app.editor.is_dirty() && app.editor.file_path().is_some() {
                    if let Err(e) = app.editor.save_async() {
                        app.set_status(format!("Save error: {}", e));
                    }
                    let _ = app.poll_async_saves();
                    app.render(writer)?;
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                let has_syntax_update = app.has_pending_syntax_response();
                let autosaved = app.editor.check_autosave();
                let has_save_event = app.poll_async_saves();
                if autosaved || has_syntax_update || has_save_event {
                    app.render(writer)?;
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let cli_cmd = parse_cli_args(&args);

    let targets = match cli_cmd {
        CliCommand::Help => {
            print_help();
            return Ok(());
        }
        CliCommand::Version => {
            println!("lume 0.1.0 (M1 functional MVP)");
            return Ok(());
        }
        CliCommand::BenchStartup(targets) => {
            let start = Instant::now();
            let mut app = App::open_targets(targets)?;
            let mut sink = std::io::sink();
            app.render(&mut sink)?;
            let duration = start.elapsed();
            println!("Cold startup time: {:.2?}", duration);
            return Ok(());
        }
        CliCommand::Run(targets) => targets,
    };

    let _guard = TerminalGuard::init()?;
    let mut app = App::open_targets(targets)?;
    let mut out = BufWriter::with_capacity(128 * 1024, stdout());

    run(&mut app, &mut out)?;

    Ok(())
}
