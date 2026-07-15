use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Write,
    path::Path,
};

use serde::Serialize;

use super::ClipRun;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunReport {
    pub generated_at: String,
    pub corpus: String,
    pub model_dir: String,
    pub decode_mode: String,
    pub window_ms: Option<u64>,
    pub overlap_ms: Option<u64>,
    pub clip_count: usize,
    pub runs: Vec<ClipRun>,
    pub summary: Vec<SummaryRow>,
}

impl RunReport {
    #[must_use]
    pub fn new(
        corpus: String,
        model_dir: String,
        decode_mode: String,
        window_ms: Option<u64>,
        overlap_ms: Option<u64>,
        runs: Vec<ClipRun>,
    ) -> Self {
        let summary = summarize(&runs);
        let clip_count = runs
            .iter()
            .map(|run| run.clip_id.as_str())
            .collect::<BTreeSet<_>>()
            .len();
        Self {
            generated_at: current_timestamp(),
            corpus,
            model_dir,
            decode_mode,
            window_ms,
            overlap_ms,
            clip_count,
            runs,
            summary,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryRow {
    model: String,
    backend: String,
    task: String,
    clip_count: usize,
    average_realtime_factor: f64,
    median_realtime_factor: f64,
    average_inference_ms: f64,
    total_empty_outputs: usize,
    total_windows: usize,
    total_empty_windows: usize,
    median_window_inference_ms: Option<f64>,
    p95_window_inference_ms: Option<f64>,
    maximum_window_inference_ms: Option<f64>,
    watame_average_realtime_factor: Option<f64>,
    long_average_realtime_factor: Option<f64>,
    multi_speaker_average_realtime_factor: Option<f64>,
}

/// Writes a JSON benchmark report.
///
/// # Errors
/// Returns an error when the output cannot be written.
pub fn write_report(output_path: &Path, report: &RunReport) -> Result<(), String> {
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut file = File::create(output_path).map_err(|error| error.to_string())?;
    serde_json::to_writer_pretty(&mut file, report).map_err(|error| error.to_string())?;
    writeln!(file).map_err(|error| error.to_string())
}

// Benchmark timings and corpus sizes are far below f64's exact integer limit.
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
            let multi_speaker = slice_average(&group, |run| run.speaker_profile == "multiple");
            let watame = slice_average(&group, |run| run.speaker == "Tsunomaki Watame");
            let long = slice_average(&group, |run| run.length_profile == "long");
            let mut window_inference_ms = group
                .iter()
                .flat_map(|run| &run.windows)
                .map(|window| window.inference_ms as f64)
                .collect::<Vec<_>>();
            window_inference_ms.sort_by(f64::total_cmp);
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
                total_windows: group.iter().map(|run| run.windows.len()).sum(),
                total_empty_windows: group
                    .iter()
                    .flat_map(|run| &run.windows)
                    .filter(|window| window.text.is_empty())
                    .count(),
                median_window_inference_ms: option_stat(&window_inference_ms, median),
                p95_window_inference_ms: percentile_95(&window_inference_ms),
                maximum_window_inference_ms: window_inference_ms.last().copied(),
                watame_average_realtime_factor: watame,
                long_average_realtime_factor: long,
                multi_speaker_average_realtime_factor: multi_speaker,
            }
        })
        .collect()
}

fn slice_average(group: &[&ClipRun], predicate: impl Fn(&ClipRun) -> bool) -> Option<f64> {
    let values = group
        .iter()
        .filter(|run| predicate(run))
        .map(|run| run.realtime_factor)
        .collect::<Vec<_>>();
    option_stat(&values, average)
}

fn option_stat(values: &[f64], statistic: impl Fn(&[f64]) -> f64) -> Option<f64> {
    (!values.is_empty()).then(|| statistic(values))
}

#[allow(clippy::cast_precision_loss)]
fn average(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn median(sorted_values: &[f64]) -> f64 {
    let middle = sorted_values.len() / 2;
    if sorted_values.len().is_multiple_of(2) {
        f64::midpoint(sorted_values[middle - 1], sorted_values[middle])
    } else {
        sorted_values[middle]
    }
}

fn percentile_95(sorted_values: &[f64]) -> Option<f64> {
    if sorted_values.is_empty() {
        return None;
    }
    let index = (sorted_values.len() * 95).div_ceil(100);
    sorted_values.get(index.saturating_sub(1)).copied()
}

fn current_timestamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(|_| "0".into(), |duration| duration.as_secs().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_averages_the_two_middle_values() {
        assert!((median(&[1.0, 2.0, 8.0, 10.0]) - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn percentile_95_uses_nearest_rank() {
        let values = (1..=20).map(f64::from).collect::<Vec<_>>();
        assert_eq!(percentile_95(&values), Some(19.0));
    }
}
