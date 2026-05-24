mod app;
mod colors;
mod cursor;
mod direction;
mod split;
mod text;
mod view_bounds;

use anyhow::Result;
use app::State;
use clap::Parser;
use ratatui::{Terminal, prelude::CrosstermBackend};
use std::{io::IsTerminal, path::PathBuf};

use crate::app::App;

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Files to open in the pager
    files: Vec<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let stdout = std::io::stdout().lock();
    let backend = CrosstermBackend::new(stdout);

    let mut state = State::new();

    for path in args.files {
        state.viewer_mut().open_file(&path)?;
    }

    if !std::io::stdin().is_terminal() {
        state
            .viewer_mut()
            .open_stream(String::from("Pipe Stream"), Box::new(std::io::stdin()))?;
    }

    let terminal = Terminal::new(backend)?;
    App::new(state, terminal).run()
}

fn regex_compile(pattern: &str) -> std::result::Result<regex::bytes::Regex, regex::Error> {
    regex::bytes::RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
}
