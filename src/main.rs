mod app;
mod config;
mod handlers;
mod mal;
mod player;
mod screens;
mod utils;

use crate::app::App;
use crossterm::event::EnableMouseCapture;
use crossterm::event::PushKeyboardEnhancementFlags;
use crossterm::event::KeyboardEnhancementFlags;
use crossterm::execute;
use anyhow::Result;
use config::Config;

fn parse_cli() -> bool {
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-v" | "--version" => {
                println!("{}", env!("CARGO_PKG_VERSION"));
                return true;
            }
            "-e" | "--edit" => {
                Config::open_in_editor();
                return true;
            }
            "-c" | "--config-path" => {
                return true;
            }
            "-h" | "--help" => {
                println!("Usage: mal-tui [OPTIONS]");
                println!();
                println!("Options:");
                println!("  -h, --help       Show this help message");
                println!("  -v, --version    Show version information");
                println!("  -e, --edit       Edit the configuration file");
                return true;
            }
            _ => {}
        }
    }

    false
}

#[tokio::main]
async fn main() -> Result<()> {
    // when invoked via symlink as `fzf` or `mpv`,
    if let Some(name) = std::env::args()
        .next()
        .and_then(|p| std::path::Path::new(&p).file_name().map(|s| s.to_string_lossy().into_owned()))
    {
        match name.as_str() {
            "fzf" => { player::fzf::run(); return Ok(()); }
            "mpv" => { player::mpv::run(); return Ok(()); }
            _ => {}
        }
    }

    let run_command = parse_cli();
    if run_command {
        return Ok(());
    }

    // grab picker before anything else
    let _ = utils::terminalCapabilities::get_picker();
    Config::migrate_from_mal_cli();
    let terminal = ratatui::init();
    let config = Config::init();
    let _ = std::fs::create_dir_all(Config::data_dir()).is_ok();

    // enable mouse capture
    if config.navigation.enable_mouse_capture {
        execute!(std::io::stderr(), EnableMouseCapture)?;
    }
        execute!(
        std::io::stdout(),
            PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
            | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
        )
    )?;

    // start the app
    let mut app = App::new(terminal);
    app.run()?;

    Ok(())
}
