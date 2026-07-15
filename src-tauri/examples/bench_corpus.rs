use std::{
    collections::BTreeMap,
    env,
    path::{Path, PathBuf},
};

use kaigai_lib::benchmark::{self, BenchmarkEngine, ClipRun, PipelineConfig, RunLabels, RunReport};

const MODELS: &[BuiltinModelSpec] = &[
    BuiltinModelSpec {
        id: "tiny",
        supports_translate: true,
    },
    BuiltinModelSpec {
        id: "base",
        supports_translate: true,
    },
    BuiltinModelSpec {
        id: "small",
        supports_translate: true,
    },
    BuiltinModelSpec {
        id: "medium",
        supports_translate: true,
    },
    BuiltinModelSpec {
        id: "large-v3",
        supports_translate: true,
    },
    BuiltinModelSpec {
        id: "large-v3-turbo",
        supports_translate: false,
    },
];

#[derive(Clone, Copy)]
struct BuiltinModelSpec {
    id: &'static str,
    supports_translate: bool,
}

#[derive(Clone)]
struct ModelSpec {
    id: String,
    supports_translate: bool,
    path: Option<PathBuf>,
}

struct RunConfig {
    clip_filter: Option<Vec<String>>,
    speaker_filter: Option<Vec<String>>,
    decode_mode: String,
    window_ms: u64,
    overlap_ms: u64,
    language: String,
    vad_model_path: PathBuf,
    pipeline: PipelineConfig,
}

struct CliConfig {
    corpus_path: PathBuf,
    model_dir: PathBuf,
    output_path: PathBuf,
    model_filter: Option<Vec<String>>,
    task_filter: Option<Vec<String>>,
    backend_filter: Option<Vec<String>>,
    models: Vec<ModelSpec>,
    run: RunConfig,
}

struct ModelRun<'a> {
    labels: RunLabels<'a>,
    model_path: &'a Path,
}

fn main() -> Result<(), String> {
    let config = CliConfig::from_env()?;
    let manifest = benchmark::read_manifest(&config.corpus_path)?;
    let audio = manifest
        .clips
        .iter()
        .map(|clip| {
            benchmark::read_wav_mono_16k(&clip.audio_path).map(|samples| (clip.id.clone(), samples))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let runs = run_matrix(&config, &manifest, &audio)?;
    let windowed = matches!(config.run.decode_mode.as_str(), "streaming" | "pipeline");
    let report = RunReport::new(
        config.corpus_path.to_string_lossy().into_owned(),
        config.model_dir.to_string_lossy().into_owned(),
        config.run.decode_mode.clone(),
        windowed.then_some(config.run.window_ms),
        windowed.then_some(config.run.overlap_ms),
        runs,
    );

    benchmark::write_report(&config.output_path, &report)?;
    println!("{}", config.output_path.display());
    Ok(())
}

impl CliConfig {
    fn from_env() -> Result<Self, String> {
        let corpus_path = env::var("KAIGAI_BENCH_CORPUS")
            .map_or_else(|_| benchmark::default_manifest_path(), PathBuf::from);
        let model_dir = env::var("KAIGAI_MODEL_DIR")
            .map_or_else(|_| benchmark::default_model_dir(), PathBuf::from);
        let output_path = env::var("KAIGAI_BENCH_OUTPUT")
            .map_or_else(|_| benchmark::default_output_path(), PathBuf::from);
        let decode_mode = env::var("KAIGAI_BENCH_DECODE").unwrap_or_else(|_| "streaming".into());
        if !matches!(decode_mode.as_str(), "streaming" | "whole" | "pipeline") {
            return Err(format!(
                "KAIGAI_BENCH_DECODE must be streaming, whole, or pipeline; got {decode_mode}"
            ));
        }
        let window_ms = env_u64("KAIGAI_BENCH_WINDOW_MS", 6_000)?;
        let overlap_ms = env_u64("KAIGAI_BENCH_OVERLAP_MS", 600)?;
        if window_ms == 0 {
            return Err("KAIGAI_BENCH_WINDOW_MS must be positive".into());
        }
        if overlap_ms >= window_ms {
            return Err("KAIGAI_BENCH_OVERLAP_MS must be smaller than the window".into());
        }
        let vad_sensitivity =
            env::var("KAIGAI_BENCH_VAD_SENSITIVITY").unwrap_or_else(|_| "balanced".into());
        if !matches!(vad_sensitivity.as_str(), "high" | "balanced" | "strict") {
            return Err("KAIGAI_BENCH_VAD_SENSITIVITY must be high, balanced, or strict".into());
        }
        let pipeline = PipelineConfig {
            vad_sensitivity,
            minimum_chunk_ms: env_u32("KAIGAI_BENCH_MINIMUM_CHUNK_MS", 1_000)?,
            maximum_chunk_ms: u32::try_from(window_ms)
                .map_err(|_| "KAIGAI_BENCH_WINDOW_MS is too large")?,
            end_silence_ms: env_u32("KAIGAI_BENCH_END_SILENCE_MS", 600)?,
            overlap_ms: u32::try_from(overlap_ms)
                .map_err(|_| "KAIGAI_BENCH_OVERLAP_MS is too large")?,
        };
        let vad_model_path = env::var("KAIGAI_BENCH_VAD_MODEL").map_or_else(
            |_| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("resources/models/ggml-silero-v6.2.0.bin")
            },
            PathBuf::from,
        );
        if decode_mode == "pipeline" && !vad_model_path.is_file() {
            return Err(format!("missing VAD model: {}", vad_model_path.display()));
        }
        Ok(Self {
            corpus_path,
            model_dir,
            output_path,
            model_filter: csv_filter("KAIGAI_BENCH_MODELS"),
            task_filter: csv_filter("KAIGAI_BENCH_TASKS"),
            backend_filter: csv_filter("KAIGAI_BENCH_BACKENDS"),
            models: configured_models()?,
            run: RunConfig {
                clip_filter: csv_filter("KAIGAI_BENCH_CLIPS"),
                speaker_filter: csv_filter("KAIGAI_BENCH_SPEAKERS"),
                decode_mode,
                window_ms,
                overlap_ms,
                language: env::var("KAIGAI_BENCH_LANGUAGE").unwrap_or_else(|_| "ja".into()),
                vad_model_path,
                pipeline,
            },
        })
    }
}

fn run_matrix(
    config: &CliConfig,
    manifest: &benchmark::CorpusManifest,
    audio: &BTreeMap<String, Vec<f32>>,
) -> Result<Vec<ClipRun>, String> {
    let mut runs = Vec::new();
    for model in &config.models {
        if !matches_filter(config.model_filter.as_deref(), &model.id) {
            continue;
        }
        let coreml_model = model
            .path
            .clone()
            .unwrap_or_else(|| config.model_dir.join(format!("ggml-{}.bin", model.id)));
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
            if !matches_filter(config.task_filter.as_deref(), task) {
                continue;
            }
            if matches_filter(config.backend_filter.as_deref(), "coreml") {
                run_model(
                    &mut runs,
                    manifest,
                    audio,
                    &ModelRun {
                        labels: RunLabels {
                            model: &model.id,
                            backend: "coreml",
                            task,
                        },
                        model_path: &coreml_model,
                    },
                    &config.run,
                )?;
            }
            if matches_filter(config.backend_filter.as_deref(), "metal") {
                let no_coreml_model = benchmark::isolated_model_path(&coreml_model, &model.id)?;
                run_model(
                    &mut runs,
                    manifest,
                    audio,
                    &ModelRun {
                        labels: RunLabels {
                            model: &model.id,
                            backend: "metal",
                            task,
                        },
                        model_path: &no_coreml_model,
                    },
                    &config.run,
                )?;
            }
        }
    }
    Ok(runs)
}

fn run_model(
    runs: &mut Vec<ClipRun>,
    manifest: &benchmark::CorpusManifest,
    audio: &BTreeMap<String, Vec<f32>>,
    model_run: &ModelRun<'_>,
    config: &RunConfig,
) -> Result<(), String> {
    let RunLabels {
        model,
        backend,
        task,
    } = model_run.labels;
    eprintln!("{model} {backend} {task}");
    let mut engine =
        BenchmarkEngine::load_with_language(model_run.model_path, task, &config.language)?;
    for clip in &manifest.clips {
        if !matches_filter(config.clip_filter.as_deref(), &clip.id)
            || !matches_filter(config.speaker_filter.as_deref(), &clip.speaker)
        {
            continue;
        }
        let samples = audio
            .get(&clip.id)
            .ok_or_else(|| format!("missing loaded audio for {}", clip.id))?;
        let run = match config.decode_mode.as_str() {
            "streaming" => engine.run_clip_streaming(
                model,
                backend,
                task,
                clip,
                samples,
                config.window_ms,
                config.overlap_ms,
            )?,
            "pipeline" => engine.run_clip_pipeline(
                &model_run.labels,
                clip,
                samples,
                &config.vad_model_path,
                &config.pipeline,
            )?,
            _ => engine.run_clip(model, backend, task, clip, samples)?,
        };
        eprintln!(
            "  {} {}ms rtf={:.3} windows={}",
            run.clip_id, run.inference_ms, run.realtime_factor, run.window_count
        );
        runs.push(run);
    }
    Ok(())
}

fn configured_models() -> Result<Vec<ModelSpec>, String> {
    let mut models = MODELS
        .iter()
        .map(|model| ModelSpec {
            id: model.id.into(),
            supports_translate: model.supports_translate,
            path: None,
        })
        .collect::<Vec<_>>();

    if let Ok(path) = env::var("KAIGAI_BENCH_MODEL_PATH") {
        let path = PathBuf::from(path);
        if !path.is_file() {
            return Err(format!(
                "KAIGAI_BENCH_MODEL_PATH does not exist: {}",
                path.display()
            ));
        }
        let id = env::var("KAIGAI_BENCH_MODEL_ID")
            .map_err(|_| "KAIGAI_BENCH_MODEL_ID is required with KAIGAI_BENCH_MODEL_PATH")?;
        if id.trim().is_empty() {
            return Err("KAIGAI_BENCH_MODEL_ID must not be empty".into());
        }
        models.push(ModelSpec {
            id,
            supports_translate: env_bool("KAIGAI_BENCH_MODEL_SUPPORTS_TRANSLATE", true)?,
            path: Some(path),
        });
    }

    Ok(models)
}

fn env_u64(name: &str, fallback: u64) -> Result<u64, String> {
    match env::var(name) {
        Ok(value) => value
            .parse()
            .map_err(|error| format!("{name} must be an unsigned integer: {error}")),
        Err(env::VarError::NotPresent) => Ok(fallback),
        Err(error) => Err(format!("failed to read {name}: {error}")),
    }
}

fn env_u32(name: &str, fallback: u32) -> Result<u32, String> {
    match env::var(name) {
        Ok(value) => value
            .parse()
            .map_err(|error| format!("{name} must be an unsigned integer: {error}")),
        Err(env::VarError::NotPresent) => Ok(fallback),
        Err(error) => Err(format!("failed to read {name}: {error}")),
    }
}

fn env_bool(name: &str, fallback: bool) -> Result<bool, String> {
    match env::var(name) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" => Ok(true),
            "0" | "false" | "no" => Ok(false),
            _ => Err(format!("{name} must be true/false or 1/0, got {value}")),
        },
        Err(env::VarError::NotPresent) => Ok(fallback),
        Err(error) => Err(format!("failed to read {name}: {error}")),
    }
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

fn matches_filter(filter: Option<&[String]>, value: &str) -> bool {
    filter.is_none_or(|values| values.iter().any(|candidate| candidate == value))
}
