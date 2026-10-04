mod cli;
mod commands;
mod keys;
mod serial;
mod ui;
mod util;

fn main() -> anyhow::Result<()> {
    cli::run()
}
