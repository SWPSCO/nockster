mod cli;
mod commands;
mod keys;
mod serial;
mod util;

fn main() -> anyhow::Result<()> {
    cli::run()
}
