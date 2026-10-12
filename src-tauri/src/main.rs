// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use clap::Parser;
use handy_app_lib::CliArgs;

/// Owl: launched through the `owl` link, run the real binary instead. Tauri
/// won't resolve the app's resources (speech detection, echo removal) from a
/// symlinked executable path. The second launch has its real path, so this
/// happens once.
#[cfg(unix)]
fn run_from_real_path() {
    use std::os::unix::process::CommandExt;
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Ok(real) = exe.canonicalize() else {
        return;
    };
    if real != exe {
        let error = std::process::Command::new(&real)
            .args(std::env::args_os().skip(1))
            .exec();
        eprintln!("error: couldn't run {}: {error}", real.display());
        std::process::exit(1);
    }
}

fn main() {
    #[cfg(unix)]
    run_from_real_path();

    #[cfg(target_os = "linux")]
    {
        // DMABUF renderer causes crashes on various GPU/display server configurations
        // See: https://github.com/tauri-apps/tauri/issues/9394
        // Preserve explicit overrides so affected users can opt out with =0 (#2166).
        if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
    }

    #[cfg(target_os = "windows")]
    {
        // Avoid overlay/capture layer crashes (#2049). Set before backend
        // initialization, preserving user overrides.
        if std::env::var_os("VK_LOADER_LAYERS_DISABLE").is_none()
            && !handy_app_lib::env_flag_enabled("HANDY_KEEP_VULKAN_IMPLICIT_LAYERS")
        {
            std::env::set_var("VK_LOADER_LAYERS_DISABLE", "~implicit~");
        }
    }

    // Transcription worker: runs transcribe.cpp in isolation and never touches
    // Tauri, the CLI, or single-instance handling. Checked after the env setup
    // above so the worker inherits it (e.g. the Vulkan layer opt-out).
    if handy_app_lib::engine_supervisor::is_worker_invocation() {
        std::process::exit(handy_app_lib::engine_supervisor::run_worker());
    }
    // Owl: the one-shot speaker-labelling process (see diarization.rs).
    if std::env::args_os().nth(1).as_deref()
        == Some(std::ffi::OsStr::new(
            handy_app_lib::diarization::WORKER_FLAG,
        ))
    {
        std::process::exit(handy_app_lib::diarization::run_worker());
    }

    let cli_args = CliArgs::parse();
    // Owl's read-only commands need no app at all.
    if let Some(command) = &cli_args.command {
        if let Some(code) = handy_app_lib::owl_cli::run_offline(command) {
            std::process::exit(code);
        }
    }
    handy_app_lib::run(cli_args)
}
