//! readio — binary entry point.
//!
//! Everything of substance lives in the library (see `lib.rs`); this file owns
//! terminal setup, the event loop, and teardown.

use std::io::{self, Stdout};
use std::panic;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture, Event,
    EventStream,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui_image::picker::Picker;

use readio::app::{self, App};
use readio::cli::{self, Parsed};
use readio::config::Config;
use readio::i18n;
use readio::library::Library;
use readio::paths;
use readio::store::Store;

/// Frame budget: 30fps is enough for phrase streaming and keeps a laptop cool.
const FRAME: Duration = Duration::from_millis(33);

fn main() -> Result<()> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match cli::parse(&argv) {
        Ok(Parsed::Run(args)) => args,
        Ok(Parsed::Help) => {
            println!("{}", cli::usage());
            return Ok(());
        }
        Ok(Parsed::Version) => {
            println!("readio {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Err(message) => {
            eprintln!("readio: {message}");
            eprintln!("{}", i18n::t("cli.short_usage"));
            std::process::exit(2);
        }
    };

    // `--home` decides where the config file is, so it has to be applied first.
    if let Some(home) = args.home.as_deref() {
        paths::set_home(home);
    }
    // Then the config decides the language everything else is reported in.
    let (config, _) = Config::load();
    i18n::set(config.language);

    let store = Store::load();
    let mut library = Library::load();

    // Importing happens before the TUI so a bad path fails in plain text
    // instead of inside a full-screen app.
    let (opened, note) = match args.path.as_deref() {
        Some(path) => match app::import_and_open(path, args.mode, &mut library) {
            Ok((book, note)) => (Some(book), Some(note)),
            Err(err) => {
                eprintln!("readio: {err:#}");
                std::process::exit(1);
            }
        },
        None => (None, None),
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    runtime.block_on(run(App::new(library, store, opened, note)))
}

async fn run(mut app: App) -> Result<()> {
    let mut terminal = setup()?;

    // The query temporarily owns terminal input, so it must happen after the
    // alternate screen is active and before EventStream starts reading. A
    // Halfblocks result is rejected by Scrollback: unsupported terminals get
    // an honest placeholder instead of a pixelated approximation.
    app.sb
        .set_image_picker(Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks()));

    let mut events = EventStream::new();
    let mut ticker = tokio::time::interval(FRAME);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let result = loop {
        if app.quit {
            break Ok(());
        }
        tokio::select! {
            _ = ticker.tick() => {
                app.on_tick();
                if let Err(err) = terminal.draw(|frame| app.draw(frame)) {
                    break Err(err.into());
                }
            }
            maybe_event = events.next() => {
                match maybe_event {
                    Some(Ok(event)) => {
                        app.trace_input(&event);
                        match event {
                            Event::Key(key) => app.on_key(key),
                            Event::Mouse(mouse) => app.on_mouse(mouse),
                            Event::Paste(text) => app.on_paste(text),
                            Event::Resize(_, _) => {}
                            _ => {}
                        }
                        // Redraw immediately so typing never feels laggy.
                        if let Err(err) = terminal.draw(|frame| app.draw(frame)) {
                            break Err(err.into());
                        }
                    }
                    Some(Err(err)) => break Err(err.into()),
                    None => break Ok(()),
                }
            }
        }
    };

    app.save_progress();
    restore(&mut terminal)?;
    result
}

type Tui = Terminal<CrosstermBackend<Stdout>>;

fn setup() -> Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;

    // A panic in raw mode leaves an unusable terminal; always undo first.
    let hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let mut stdout = io::stdout();
        let _ = execute!(
            stdout,
            DisableBracketedPaste,
            DisableMouseCapture,
            LeaveAlternateScreen
        );
        let _ = disable_raw_mode();
        hook(info);
    }));

    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.hide_cursor()?;
    terminal.clear()?;
    Ok(terminal)
}

fn restore(terminal: &mut Tui) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableBracketedPaste,
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;
    Ok(())
}
