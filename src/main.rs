use blockchain::cli::Cli;
use blockchain::errors::Result;

fn main() -> Result<()> {
    env_logger::init();
    let mut cli = Cli::new()?;
    cli.run()
}
