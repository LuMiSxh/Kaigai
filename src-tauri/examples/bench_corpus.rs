use std::{
    collections::BTreeMap,
    env,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use kaigai_lib::benchmark::{self, BenchmarkEngine, ClipRun};
use serde::Serialize;

const MODELS: &[ModelSpec] = &[
    ModelSpec {
        id: "tiny",
        supports_translate: true,
    },
    ModelSpec {
        id: "base",
        supports_translate: true,
    },
    ModelSpec {
        id: "small",
        supports_translate: true,
    },
    ModelSpec {
        id: "medium",
        supports_translate: true,
    },
    ModelSpec {
        id: "large-v3",
        supports_translate: true,
    },
    ModelSpec {
        id: "large-v3-turbo",
        supports_translate: false,
    },
];

#[derive(Clone, Copy)]
struct ModelSpec {
    id: &'static str,
    supports_translate: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RunReport {
    generated_at: String,
    corpus: String,
    model_dir: String,
    decode_mode: String,
    window_ms: Option<u64>,
    overlap_ms: Option<u64>,
    clip_count: usize,
    runs: Vec<ClipRun>,
    summary: Vec<SummaryRow>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SummaryRow {
    model: String,
    backend: String,
    task: String,
    clip_count: usize,
    average_realtime_factor: f64,
    median_realtime_factor: f64,
    average_inference_ms: f64,
    total_empty_outputs: usize,
    watame_average_realtime_factor: Option<f64>,
    long_average_realtime_factor: Option<f64>,
    multi_speaker_average_realtime_factor: Option<f64>,
}

fn main() -> Result<(), String> {
    let corpus_path =
        env::var("KAIGAI_BENCH_CORPUS").map_or_else(|_| default_manifest_path(), PathBuf::from);
    let model_dir =
        env::var("KAIGAI_MODEL_DIR").map_or_else(|_| default_model_dir(), PathBuf::from);
    let output_path =
        env::var("KAIGAI_BENCH_OUTPUT").map_or_else(|_| default_output_path(), PathBuf::from);
    let decode_mode = env::var("KAIGAI_BENCH_DECODE").unwrap_or_else(|_| "streaming".into());
    let window_ms = env_u64("KAIGAI_BENCH_WINDOW_MS", 6_000);
    let overlap_ms = env_u64("KAIGAI_BENCH_OVERLAP_MS", 600);
    let model_filter = csv_filter("KAIGAI_BENCH_MODELS");
    let task_filter = csv_filter("KAIGAI_BENCH_TASKS");
    let backend_filter = csv_filter("KAIGAI_BENCH_BACKENDS");
    let clip_filter = csv_filter("KAIGAI_BENCH_CLIPS");
    let speaker_filter = csv_filter("KAIGAI_BENCH_SPEAKERS");

    let manifest = benchmark::read_manifest(&corpus_path)?;
    let audio = manifest
        .clips
        .iter()
        .map(|clip| {
            benchmark::read_wav_mono_16k(&clip.audio_path).map(|samples| (clip.id.clone(), samples))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;

    let mut runs = Vec::new();
    for model in MODELS {
        if !matches_filter(model_filter.as_ref(), model.id) {
            continue;
        }
        let coreml_model = model_dir.join(format!("ggml-{}.bin", model.id));
        if !coreml_model.is_file() {
            eprintln!("skip {}: missing {}", model.id, coreml_model.display());
            continue;
        }
        let tasks: &[&str] = if model.supports_translate {
            &["translate", "transcribe"]
        } else {
            &["transcribe"]
        };
        for task in tasks {
            if !matches_filter(task_filter.as_ref(), task) {
                continue;
            }
            if matches_filter(backend_filter.as_ref(), "coreml") {
                run_model(
                    &mut runs,
                    &manifest,
                    &audio,
                    clip_filter.as_ref(),
                    speaker_filter.as_ref(),
                    model.id,
                    "coreml",
                    task,
                    &coreml_model,
                    &decode_mode,
                    window_ms,
                    overlap_ms,
                )?;
            }
            if matches_filter(backend_filter.as_ref(), "metal") {
                let no_coreml_model = no_coreml_model_path(&model_dir, model.id, &coreml_model)?;
                run_model(
                    &mut runs,
                    &manifest,
                    &audio,
                    clip_filter.as_ref(),
                    speaker_filter.as_ref(),
                    model.id,
                    "metal",
                    task,
                    &no_coreml_model,
                    &decode_mode,
                    window_ms,
                    overlap_ms,
                )?;
            }
        }
    }

    let report = RunReport {
        generated_at: current_timestamp(),
        corpus: corpus_path.to_string_lossy().into_owned(),
        model_dir: model_dir.to_string_lossy().into_owned(),
        decode_mode: decode_mode.clone(),
        window_ms: (decode_mode == "streaming").then_some(window_ms),
        overlap_ms: (decode_mode == "streaming").then_some(overlap_ms),
        clip_count: manifest.clips.len(),
        summary: summarize(&runs),
        runs,
    };

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut file = File::create(&output_path).map_err(|error| error.to_string())?;
    serde_json::to_writer_pretty(&mut file, &report).map_err(|error| error.to_string())?;
    writeln!(file).map_err(|error| error.to_string())?;
    println!("{}", output_path.display());
    Ok(())
}

// One call's worth of run configuration for a benchmark CLI tool — splitting
// it into a struct wouldn't reduce what the caller has to supply, just move
// it behind another name.
#[allow(clippy::too_many_arguments)]
fn run_model(
    runs: &mut Vec<ClipRun>,
    manifest: &benchmark::CorpusManifest,
    audio: &BTreeMap<String, Vec<f32>>,
    clip_filter: Option<&Vec<String>>,
    speaker_filter: Option<&Vec<String>>,
    model: &str,
    backend: &str,
    task: &str,
    model_path: &Path,
    decode_mode: &str,
    window_ms: u64,
    overlap_ms: u64,
) -> Result<(), String> {
    eprintln!("{model} {backend} {task}");
    let mut engine = BenchmarkEngine::load(model_path, task)?;
    for clip in &manifest.clips {
        if !matches_filter(clip_filter, &clip.id) || !matches_filter(speaker_filter, &clip.speaker)
        {
            continue;
        }
        let samples = audio
            .get(&clip.id)
            .ok_or_else(|| format!("missing loaded audio for {}", clip.id))?;
        let run = if decode_mode == "streaming" {
            engine.run_clip_streaming(model, backend, task, clip, samples, window_ms, overlap_ms)?
        } else {
            engine.run_clip(model, backend, task, clip, samples)?
        };
        eprintln!(
            "  {} {}ms rtf={:.3} windows={}",
            run.clip_id, run.inference_ms, run.realtime_factor, run.window_count
        );
        runs.push(run);
    }
    Ok(())
}

// Inference timings are milliseconds well below 2^52; precision loss in the
// averaged report numbers is irrelevant at that scale.
#[allow(clippy::cast_precision_loss)]
fn summarize(runs: &[ClipRun]) -> Vec<SummaryRow> {
    let mut groups: BTreeMap<(&str, &str, &str), Vec<&ClipRun>> = BTreeMap::new();
    for run in runs {
        groups
            .entry((&run.model, &run.backend, &run.task))
            .or_default()
            .push(run);
    }

    groups
        .into_iter()
        .map(|((model, backend, task), group)| {
            let mut rtfs = group
                .iter()
                .map(|run| run.realtime_factor)
                .collect::<Vec<_>>();
            rtfs.sort_by(f64::total_cmp);
            let multi_speaker = group
                .iter()
                .filter(|run| run.speaker_profile == "multiple")
                .map(|run| run.realtime_factor)
                .collect::<Vec<_>>();
            let watame = group
                .iter()
                .filter(|run| run.speaker == "Tsunomaki Watame")
                .map(|run| run.realtime_factor)
                .collect::<Vec<_>>();
            let long = group
                .iter()
                .filter(|run| run.length_profile == "long")
                .map(|run| run.realtime_factor)
                .collect::<Vec<_>>();
            SummaryRow {
                model: model.into(),
                backend: backend.into(),
                task: task.into(),
                clip_count: group.len(),
                average_realtime_factor: average(&rtfs),
                median_realtime_factor: median(&rtfs),
                average_inference_ms: average(
                    &group
                        .iter()
                        .map(|run| run.inference_ms as f64)
                        .collect::<Vec<_>>(),
                ),
                total_empty_outputs: group.iter().filter(|run| run.text.is_empty()).count(),
                watame_average_realtime_factor: (!watame.is_empty()).then(|| average(&watame)),
                long_average_realtime_factor: (!long.is_empty()).then(|| average(&long)),
                multi_speaker_average_realtime_factor: if multi_speaker.is_empty() {
                    None
                } else {
                    Some(average(&multi_speaker))
                },
            }
        })
        .collect()
}

fn env_u64(name: &str, fallback: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

fn csv_filter(name: &str) -> Option<Vec<String>> {
    env::var(name).ok().map(|value| {
        value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect()
    })
}

fn matches_filter(filter: Option<&Vec<String>>, value: &str) -> bool {
    filter.is_none_or(|values| values.iter().any(|candidate| candidate == value))
}

// A benchmark corpus is a few dozen clips; `values.len()` never comes close
// to the point where converting it to f64 would lose precision.
#[allow(clippy::cast_precision_loss)]
fn average(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

fn median(sorted_values: &[f64]) -> f64 {
    if sorted_values.is_empty() {
        return 0.0;
    }
    sorted_values[sorted_values.len() / 2]
}

fn no_coreml_model_path(model_dir: &Path, model: &str, source: &Path) -> Result<PathBuf, String> {
    let directory = env::temp_dir().join("kaigai-bench-no-coreml").join(model);
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let destination = directory.join(format!("ggml-{model}.bin"));
    if destination.exists() {
        fs::remove_file(&destination).map_err(|error| error.to_string())?;
    }

    #[cfg(unix)]
    std::os::unix::fs::symlink(source, &destination).map_err(|error| error.to_string())?;

    #[cfg(not(unix))]
    fs::copy(source, &destination).map_err(|error| error.to_string())?;

    // Defensive cleanup in case a previous run copied a Core ML bundle into the
    // isolated directory. The real model directory is untouched.
    let encoder = directory.join(format!("ggml-{model}-encoder.mlmodelc"));
    if encoder.exists() {
        fs::remove_dir_all(encoder).map_err(|error| error.to_string())?;
    }
    let _ = model_dir;
    Ok(destination)
}

fn default_manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../benchmarks/corpus/generated/manifest.json")
}

fn default_output_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../benchmarks/results/model-matrix.json")
}

fn default_model_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(env::temp_dir)
        .join("com.lumisxh.kaigai/models")
}

fn current_timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(|_| "0".into(), |duration| duration.as_secs().to_string())
}
