// Copyright 2025 the Understory Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Xtask utilities for Understory, currently only integrates Kompari.

use clap::Parser;
use kompari::DirDiffConfig;
use kompari_tasks::args::Command as KompariCommand;
use kompari_tasks::{Actions, Args, Task};
use std::path::Path;
use std::process::Command;

struct ActionsImpl {
    backend: SnapshotBackend,
}

#[derive(Copy, Clone, Debug)]
enum SnapshotBackend {
    VelloCpu,
    Skia,
    Vello,
}

impl SnapshotBackend {
    fn dir(self) -> &'static str {
        match self {
            Self::VelloCpu => "vello_cpu",
            Self::Skia => "skia",
            Self::Vello => "vello",
        }
    }
}

#[derive(Copy, Clone, Debug)]
enum SnapshotTestMode {
    Normal,
    Accept,
    GenerateAll,
}

impl Actions for ActionsImpl {
    fn generate_all_tests(&self) -> kompari::Result<()> {
        run_generate_all(self.backend)
    }
}

fn clean_dir(dir: &Path) -> kompari::Result<()> {
    std::fs::create_dir_all(dir)?;
    for path in kompari::list_image_dir(dir)? {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

fn kompari_args_first(arg: &str) -> bool {
    matches!(
        arg,
        "report" | "review" | "clean" | "dead-snapshots" | "size-check"
    )
}

fn parse_backend_value(value: &str) -> Option<SnapshotBackend> {
    match value {
        "vello_cpu" | "vello-cpu" | "vello_cpu_snapshots" => Some(SnapshotBackend::VelloCpu),
        "skia" | "skia_snapshots" => Some(SnapshotBackend::Skia),
        "vello" | "vello_snapshots" | "gpu" => Some(SnapshotBackend::Vello),
        _ => None,
    }
}

fn strip_backend_flag(
    args: &[String],
    default_backend: SnapshotBackend,
) -> (SnapshotBackend, Vec<String>) {
    let mut backend = default_backend;
    let mut remaining = Vec::with_capacity(args.len());

    let mut i = 0_usize;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--backend" || arg == "-b" {
            if let Some(value) = args.get(i + 1) {
                if let Some(b) = parse_backend_value(value) {
                    backend = b;
                } else {
                    remaining.push(arg.clone());
                    remaining.push(value.clone());
                }
                i += 2;
                continue;
            }
        } else if let Some(value) = arg.strip_prefix("--backend=")
            && let Some(b) = parse_backend_value(value)
        {
            backend = b;
            i += 1;
            continue;
        }

        remaining.push(arg.clone());
        i += 1;
    }

    (backend, remaining)
}

fn run_kompari_for_backend(
    backend: SnapshotBackend,
    raw_kompari_args: Vec<String>,
) -> kompari::Result<()> {
    // Kompari expects its own argv; we pass through the kompari args after removing
    // any xtask-only flags like `--backend`.
    let mut argv = Vec::with_capacity(raw_kompari_args.len() + 1);
    argv.push("xtask".to_string());
    argv.extend(raw_kompari_args);
    let args = Args::parse_from(argv);
    snapshots_command(backend.dir(), backend, args)
}

fn run_generate_all(backend: SnapshotBackend) -> kompari::Result<()> {
    run_snapshot_tests(backend, SnapshotTestMode::GenerateAll, None, Vec::new())
}

fn run_snapshot_tests(
    backend: SnapshotBackend,
    mode: SnapshotTestMode,
    case: Option<String>,
    extra_args: Vec<String>,
) -> kompari::Result<()> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut cmd = Command::new(cargo);
    cmd.args(["test", "-p", "understory_imaging_snapshot_tests"]);
    match backend {
        SnapshotBackend::VelloCpu => {
            cmd.args(["--features", "vello_cpu", "--test", "vello_cpu_snapshots"]);
        }
        SnapshotBackend::Skia => {
            cmd.args(["--features", "skia", "--test", "skia_snapshots"]);
        }
        SnapshotBackend::Vello => {
            cmd.args(["--features", "vello", "--test", "vello_snapshots"]);
        }
    }

    match mode {
        SnapshotTestMode::Normal => {}
        SnapshotTestMode::Accept => {
            cmd.env("UNDERSTORY_IMAGING_TEST", "accept");
        }
        SnapshotTestMode::GenerateAll => {
            cmd.env("UNDERSTORY_IMAGING_TEST", "generate-all");
        }
    }

    if let Some(case) = case {
        cmd.env("UNDERSTORY_IMAGING_CASE", case);
    }

    cmd.args(extra_args);
    cmd.status()?;
    Ok(())
}

fn snapshots_command(dir: &str, backend: SnapshotBackend, args: Args) -> kompari::Result<()> {
    let tests_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("understory_imaging_snapshot_tests")
        .join("tests");

    let snapshots_path = tests_path.join("snapshots").join(dir);
    let current_path = tests_path.join("current").join(dir);

    std::fs::create_dir_all(&snapshots_path)?;
    std::fs::create_dir_all(&current_path)?;

    let mut diff_config = DirDiffConfig::new(snapshots_path, current_path);
    diff_config.set_ignore_right_missing(true);

    match &args.command {
        KompariCommand::Report(_) | KompariCommand::Review(_) => {
            clean_dir(diff_config.right_path())?;
            run_generate_all(backend)?;
            let diff = diff_config.create_diff()?;
            if diff.results().is_empty() {
                println!("No snapshot differences found.");
            }
        }
        KompariCommand::Clean | KompariCommand::DeadSnapshots(_) | KompariCommand::SizeCheck(_) => {
        }
    }

    let mut task = Task::new(diff_config, Box::new(ActionsImpl { backend }));
    task.set_report_output_path(tests_path.join(format!("report-{dir}.html")));
    task.run(&args)?;
    Ok(())
}

fn main() -> kompari::Result<()> {
    // Backwards-compatible behavior:
    // - `cargo xtask report|review|...` defaults to vello_cpu snapshots, but now accepts
    //   `--backend <vello_cpu|skia|vello>` (or `-b ...`) to select a backend.
    // - `cargo xtask snapshots-<backend> <kompari-args...>` still works.
    //
    // New (preferred) behavior:
    // - `cargo xtask snapshots [--backend <...>] <kompari-subcommand> ...`
    let raw = std::env::args().collect::<Vec<_>>();
    let first = raw.get(1).map(String::as_str);

    if let Some(arg1) = first {
        if arg1 == "snapshots" {
            let (backend, remaining) = strip_backend_flag(&raw[2..], SnapshotBackend::VelloCpu);
            if remaining.is_empty() {
                eprintln!(
                    "Usage: cargo xtask snapshots [--backend <vello_cpu|skia|vello>] <kompari-subcommand> [args...]\n       cargo xtask snapshots [--backend <...>] test [--case <pattern>] [--accept|--generate-all] [-- <cargo test args...>]"
                );
                return Ok(());
            }

            if remaining[0] == "test" {
                let mut mode = SnapshotTestMode::Normal;
                let mut accept_seen = false;
                let mut generate_all_seen = false;
                let mut case = None::<String>;
                let mut extra_args = Vec::<String>::new();

                let mut i = 1_usize;
                while i < remaining.len() {
                    let arg = &remaining[i];

                    if arg == "--" {
                        extra_args.extend_from_slice(&remaining[i + 1..]);
                        break;
                    }

                    if arg == "--accept" || arg == "--bless" {
                        accept_seen = true;
                        mode = SnapshotTestMode::Accept;
                        i += 1;
                        continue;
                    }

                    if arg == "--generate-all" {
                        generate_all_seen = true;
                        mode = SnapshotTestMode::GenerateAll;
                        i += 1;
                        continue;
                    }

                    if arg == "--case" || arg == "-c" {
                        if let Some(value) = remaining.get(i + 1) {
                            case = Some(value.clone());
                            i += 2;
                            continue;
                        }
                    } else if let Some(value) = arg.strip_prefix("--case=") {
                        case = Some(value.to_string());
                        i += 1;
                        continue;
                    }

                    extra_args.push(arg.clone());
                    i += 1;
                }

                if accept_seen && generate_all_seen {
                    eprintln!("`--accept` and `--generate-all` are mutually exclusive.");
                    return Ok(());
                }

                return run_snapshot_tests(backend, mode, case, extra_args);
            }

            return run_kompari_for_backend(backend, remaining);
        }

        if kompari_args_first(arg1) {
            let (backend, remaining) = strip_backend_flag(&raw[1..], SnapshotBackend::VelloCpu);
            return run_kompari_for_backend(backend, remaining);
        }
    }

    #[derive(Parser, Debug)]
    #[command(version, about, long_about = None)]
    struct Cli {
        #[clap(subcommand)]
        command: CliCommand,
    }

    #[expect(
        clippy::enum_variant_names,
        reason = "New commands won't, so leave it for now."
    )]
    #[derive(Parser, Debug)]
    enum CliCommand {
        SnapshotsVelloCpu(Args),
        SnapshotsSkia(Args),
        SnapshotsVello(Args),
    }

    // Remove the binary name so clap sees subcommands.
    // If parsing fails, let clap print the help/errors.
    let cli = Cli::parse();
    match cli.command {
        CliCommand::SnapshotsVelloCpu(args) => {
            snapshots_command("vello_cpu", SnapshotBackend::VelloCpu, args)
        }
        CliCommand::SnapshotsSkia(args) => snapshots_command("skia", SnapshotBackend::Skia, args),
        CliCommand::SnapshotsVello(args) => {
            snapshots_command("vello", SnapshotBackend::Vello, args)
        }
    }
}
