mod cli;
mod serial;
mod util;
mod keys;
mod commands;

fn main() -> anyhow::Result<()> {
    cli::run()
}
