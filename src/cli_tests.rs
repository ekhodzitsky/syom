use crate::cli::{Cli, run};
use crate::error::SyomError;
use clap::Parser;
use std::path::PathBuf;

#[test]
fn test_parse_keeps_input_and_output() -> Result<(), String> {
    let cli =
        Cli::try_parse_from(["syom", "in.mp4", "-o", "out.wav"]).map_err(|e| e.to_string())?;
    assert_eq!(cli.input, PathBuf::from("in.mp4"));
    assert_eq!(cli.output, Some(PathBuf::from("out.wav")));
    Ok(())
}

#[test]
fn test_run_missing_file_is_media() {
    let cli = Cli {
        input: PathBuf::from("/no/such/syom-input.mp4"),
        output: Some(PathBuf::from("/tmp/syom-out.wav")),
    };
    assert!(matches!(run(cli), Err(SyomError::Media(_))));
}
