pub mod app;
pub mod cli;
pub mod config;
pub mod image;
pub mod input;
pub mod options;
pub mod renderer;
pub mod search;
pub mod theme;
pub mod ui;

use anyhow::Result;
use clap::Parser;

use app::App;
use cli::Cli;
use config::Config;
use input::read_input;
use renderer::ViewDocument;
use theme::Theme;

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    if cli.list_themes {
        for theme in Theme::list_available() {
            println!("{theme}");
        }
        return Ok(());
    }

    let config = Config::load()?.apply_cli(&cli);
    let theme = Theme::load(&config.theme, config.icons)?;
    let input = read_input(cli.input.as_deref(), cli.format)?;
    let document = ViewDocument::from_input(&input);
    let mut app = App::new(
        input.name,
        input.kind,
        input.base_dir,
        document,
        theme,
        config.wrap,
        config.tab_width,
    );

    ratatui::run(|terminal| app.run(terminal))?;
    Ok(())
}
