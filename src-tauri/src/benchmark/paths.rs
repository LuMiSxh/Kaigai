use std::{
    env, fs,
    path::{Path, PathBuf},
};

/// Isolates a model from its Core ML encoder for Metal-only runs.
///
/// # Errors
/// Returns an error when the temporary model directory cannot be updated.
pub fn isolated_model_path(source: &Path, model: &str) -> Result<PathBuf, String> {
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

    let encoder = directory.join(format!("ggml-{model}-encoder.mlmodelc"));
    if encoder.exists() {
        fs::remove_dir_all(encoder).map_err(|error| error.to_string())?;
    }
    Ok(destination)
}

#[must_use]
pub fn default_manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../benchmarks/corpus/generated/manifest.json")
}

#[must_use]
pub fn default_output_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../benchmarks/results/model-matrix.json")
}

#[must_use]
pub fn default_model_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(env::temp_dir)
        .join("com.lumisxh.kaigai/models")
}
