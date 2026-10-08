use camino::Utf8PathBuf;
use clap::Parser;
use simple_eyre::{
    eyre::{anyhow, Context},
    Result,
};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Input file
    infile: Utf8PathBuf,

    /// Output file
    #[arg(short, long)]
    outfile: Option<Utf8PathBuf>,
}

fn main() -> Result<()> {
    simplelog::TermLogger::init(
        simplelog::LevelFilter::Warn,
        simplelog::Config::default(),
        simplelog::TerminalMode::Stderr,
        simplelog::ColorChoice::Auto,
    )?;

    simple_eyre::install()?;

    let args = Args::parse();
    let report = wave2saif::write_saif(
        args.infile.as_std_path(),
        args.outfile.as_deref().map(|path| path.as_std_path()),
    )
    .map_err(|e| anyhow!(e))
    .with_context(|| format!("Failed to convert {}", args.infile))?;

    if report.ignored_wide > 0 {
        log::warn!(
            "{} multi-bit signal{} ignored",
            report.ignored_wide,
            if report.ignored_wide == 1 { "" } else { "s" }
        );
    }

    Ok(())
}
