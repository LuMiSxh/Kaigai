use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

use criterion::{Criterion, criterion_group, criterion_main};
use kaigai_lib::benchmark::{self, BenchmarkEngine};

fn bench_translation_corpus(criterion: &mut Criterion) {
    let manifest_path = env::var("KAIGAI_BENCH_CORPUS").map_or_else(
        |_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../benchmarks/corpus/generated/manifest.json")
        },
        PathBuf::from,
    );
    if !manifest_path.is_file() {
        eprintln!(
            "skip corpus benchmark: {} does not exist; run pnpm bench:prepare first",
            manifest_path.display()
        );
        return;
    }

    let model_dir = env::var("KAIGAI_MODEL_DIR").map_or_else(
        |_| {
            dirs::data_dir()
                .unwrap_or_else(env::temp_dir)
                .join("com.lumisxh.kaigai/models")
        },
        PathBuf::from,
    );
    let model = env::var("KAIGAI_BENCH_MODEL").unwrap_or_else(|_| "small".into());
    let backend = env::var("KAIGAI_BENCH_BACKEND").unwrap_or_else(|_| "coreml".into());
    let task = env::var("KAIGAI_BENCH_TASK").unwrap_or_else(|_| "translate".into());

    let model_path = model_path(&model_dir, &model, &backend).expect("resolve model path");
    if !model_path.is_file() {
        eprintln!("skip corpus benchmark: missing {}", model_path.display());
        return;
    }

    let manifest = benchmark::read_manifest(&manifest_path).expect("read manifest");
    let audio = manifest
        .clips
        .iter()
        .map(|clip| {
            benchmark::read_wav_mono_16k(&clip.audio_path).map(|samples| (clip.id.clone(), samples))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()
        .expect("read audio");

    let mut group = criterion.benchmark_group(format!("{model}-{backend}-{task}"));
    for clip in &manifest.clips {
        let samples = audio.get(&clip.id).expect("loaded clip audio");
        group.bench_function(&clip.id, |bencher| {
            bencher.iter_batched(
                || BenchmarkEngine::load(&model_path, &task).expect("load model"),
                |mut engine| {
                    engine
                        .run_clip(&model, &backend, &task, clip, samples)
                        .expect("run inference");
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn model_path(model_dir: &Path, model: &str, backend: &str) -> Result<PathBuf, String> {
    let source = model_dir.join(format!("ggml-{model}.bin"));
    if backend == "metal" {
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

        Ok(destination)
    } else {
        Ok(source)
    }
}

criterion_group!(benches, bench_translation_corpus);
criterion_main!(benches);
