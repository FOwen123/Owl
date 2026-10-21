use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone, Default)]
#[command(name = "owl", about = "Owl - Speech to Text")]
pub struct CliArgs {
    /// Owl: meetings, transcribing files, and the MCP server (`owl_cli.rs`).
    #[command(subcommand)]
    pub command: Option<crate::owl_cli::OwlCommand>,

    /// Start with the main window hidden
    #[arg(long)]
    pub start_hidden: bool,

    /// Disable the system tray icon
    #[arg(long)]
    pub no_tray: bool,

    /// Toggle transcription on/off (sent to running instance)
    #[arg(long)]
    pub toggle_transcription: bool,

    /// Toggle transcription with post-processing on/off (sent to running instance)
    #[arg(long)]
    pub toggle_post_process: bool,

    /// Cancel the current operation (sent to running instance)
    #[arg(long)]
    pub cancel: bool,

    /// Enable debug mode with verbose logging
    #[arg(long)]
    pub debug: bool,

    /// Transcribe this WAV (16 kHz mono) headlessly and exit. Runs the same
    /// batch transcription path as the app — no mic, no VAD, no download
    /// (the model must already be installed).
    #[arg(short = 'f', long, value_name = "WAV")]
    pub transcribe_file: Option<PathBuf>,

    /// Model id to load for --transcribe-file (default: the selected model).
    #[arg(long)]
    pub model: Option<String>,

    /// Hard-select the compute device for --transcribe-file by its registry
    /// index (see --list-devices). Omit to use the persisted accelerator
    /// setting. transcribe-cpp (whisper-family) models only.
    #[arg(long, value_name = "N")]
    pub device_index: Option<usize>,

    /// List the transcribe-cpp compute devices (with indices) and exit.
    #[arg(long)]
    pub list_devices: bool,

    /// List the available models (with ids) and exit. Pass an id to --model.
    /// Honors --json for machine-readable output.
    #[arg(long)]
    pub list_models: bool,

    /// Repeat the transcription N times (best_ms reports the fastest run).
    #[arg(long, value_name = "N")]
    pub repeat: Option<usize>,

    /// Emit --transcribe-file results as JSON.
    #[arg(long)]
    pub json: bool,

    /// Also run the dictation output step on the --transcribe-file text
    /// (snippets, AI cleanup, dictionary), as the post-process shortcut does.
    #[arg(long)]
    pub post_process: bool,

    /// Owl: replay the test set in this folder (set.jsonl + audio) through
    /// Owl and score it (`eval.rs`). Honors --model and --post-process; writes
    /// results/<--eval-name>.json in the folder and prints the summary.
    #[arg(long, value_name = "DIR")]
    pub eval: Option<PathBuf>,

    /// Name of the --eval run's results file (default: "run").
    #[arg(long, value_name = "NAME")]
    pub eval_name: Option<String>,

    /// Owl: write the learning log's corrected dictations as a test set to
    /// this folder, for --eval.
    #[arg(long, value_name = "DIR")]
    pub export_learning: Option<PathBuf>,

    /// Owl: run the learner once (as "Learn now" does) and print its report.
    #[arg(long)]
    pub learn: bool,
}
