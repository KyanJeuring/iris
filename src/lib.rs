pub mod app;
pub mod cli;
pub mod config;
pub mod image;
pub mod input;
pub mod navigation;
pub mod options;
pub mod pdf;
pub mod renderer;
pub mod search;
pub mod selection;
pub mod theme;
mod title;
pub mod ui;

use anyhow::Result;
use clap::Parser;

use app::App;
use cli::Cli;
use config::Config;
use input::read_input_with_title;
use renderer::ViewDocument;
use theme::{Theme, ensure_bundled_themes};

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    if cli.list_themes {
        ensure_bundled_themes();
        for theme in Theme::list_available() {
            println!("{theme}");
        }
        return Ok(());
    }

    // Read input before configuration/theme setup so a short-lived stdin producer is still
    // present when Linux pipe discovery inspects /proc.
    let input = read_input_with_title(cli.input.as_deref(), cli.format, cli.title.as_deref())?;

    ensure_bundled_themes();
    let config = Config::load()?.apply_cli(&cli);
    let theme = Theme::load(&config.theme, config.icons)?;
    let document = ViewDocument::from_input(&input)?;
    let mut app = App::new(input, document, theme, config.wrap, config.tab_width);

    ratatui::run(|terminal| app.run(terminal))?;
    Ok(())
}
