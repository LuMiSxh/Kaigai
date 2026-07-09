use std::{fs, path::Path};

/// How to get the raw ffmpeg binary out of a downloaded payload.
enum Archive {
    /// The download itself is the raw binary.
    Raw,
    Zip(&'static str),
    /// Extracted by shelling out to the system `tar` (available on every CI
    /// runner and dev machine this pin actually targets), rather than
    /// pulling in tar/xz crates for one Linux-only entry.
    TarXz(&'static str),
}

struct FfmpegSource {
    target: &'static str,
    url: &'static str,
    sha256: &'static str,
    archive: Archive,
}

// Pinned by us, not auto-detected: every entry here was downloaded and
// hashed by hand before being committed, because not every upstream build
// publishes its own checksum (evermeet.cx's macOS builds only ship a GPG
// signature, and have no Apple Silicon build at all). Bump url/sha256
// together when refreshing a pin; `stage_ffmpeg` re-fetches automatically
// once the pinned hash here no longer matches what's already staged.
const FFMPEG_SOURCES: &[FfmpegSource] = &[
    FfmpegSource {
        target: "aarch64-apple-darwin",
        url: "https://github.com/descriptinc/ffmpeg-ffprobe-static/releases/download/b6.1.2-rc.1/ffmpeg-darwin-arm64",
        sha256: "9f865039102a1139c7057d7f21ddaacd106d602fa3af1f99b70f43d520439b8c",
        archive: Archive::Raw,
    },
    FfmpegSource {
        // BtbN republishes the contents at this URL over time (it's their
        // rolling "latest" tag), so this pin may need refreshing whenever
        // verification starts failing here.
        target: "x86_64-pc-windows-msvc",
        url: "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-n8.1-latest-win64-lgpl-8.1.zip",
        sha256: "e71db3aabcbefc9ac92f90c0e50e06fb11dff21026f091803936b0e725d4a164",
        archive: Archive::Zip("ffmpeg-n8.1-latest-win64-lgpl-8.1/bin/ffmpeg.exe"),
    },
    FfmpegSource {
        // Not a shipped release target today (publish.yml only builds macOS
        // and Windows) — this exists so CI's Linux runners, which compile
        // for this target just to lint/test, stage a real binary instead of
        // relying solely on the resources/bin/README.md placeholder.
        target: "x86_64-unknown-linux-gnu",
        url: "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-n8.1-latest-linux64-lgpl-8.1.tar.xz",
        sha256: "acbeb2219eebc24f63117bfda4457fc3f58e7fff857ad68b12b89a753fc7b783",
        archive: Archive::TarXz("ffmpeg-n8.1-latest-linux64-lgpl-8.1/bin/ffmpeg"),
    },
];

const SILERO_VAD_URL: &str =
    "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v6.2.0.bin?download=true";
const SILERO_VAD_SHA256: &str = "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987";
const SILERO_VAD_FILE: &str = "ggml-silero-v6.2.0.bin";

fn main() {
    let target = std::env::var("TARGET").expect("Cargo must provide TARGET");
    println!("cargo:rustc-env=KAIGAI_TARGET_TRIPLE={target}");
    stage_ffmpeg(&target);
    stage_silero_vad(&target);
    tauri_build::build();
}

/// Stage whisper.cpp's official cross-platform Silero VAD model. The file
/// isn't platform-specific, so unlike ffmpeg there's no per-target pin list
/// — every target we compile for, including CI's Linux lint/test runners,
/// gets the same download.
fn stage_silero_vad(target: &str) {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("Cargo must provide CARGO_MANIFEST_DIR");
    let model_dir = Path::new(&manifest_dir).join("resources/models");
    let destination = model_dir.join(SILERO_VAD_FILE);
    let marker = destination.with_extension("pin");
    if destination.is_file()
        && fs::read_to_string(&marker).is_ok_and(|pinned| pinned == SILERO_VAD_SHA256)
    {
        return;
    }

    println!("cargo:warning=fetching pinned Silero VAD model for {target}");
    fs::create_dir_all(&model_dir).expect("failed to create resources/models");
    let bytes = download(SILERO_VAD_URL);
    verify_sha256(&bytes, SILERO_VAD_SHA256, SILERO_VAD_URL);
    let temporary = destination.with_extension("part");
    fs::write(&temporary, bytes).expect("failed to write Silero VAD model");
    fs::rename(&temporary, &destination).expect("failed to install Silero VAD model");
    fs::write(marker, SILERO_VAD_SHA256).expect("failed to write Silero VAD pin marker");
}

/// Downloads the pinned ffmpeg sidecar for `target` into `resources/bin/` if
/// it isn't already staged there, verifying it against a hash we computed
/// ourselves. Targets without a pinned entry are skipped with a warning
/// rather than failing, so `cargo check`/`clippy`/`test` never need network
/// access on a target we haven't bothered pinning.
fn stage_ffmpeg(target: &str) {
    let Some(source) = FFMPEG_SOURCES.iter().find(|source| source.target == target) else {
        println!("cargo:warning=no pinned ffmpeg build for target {target}; skipping");
        return;
    };

    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("Cargo must provide CARGO_MANIFEST_DIR");
    let bin_dir = Path::new(&manifest_dir).join("resources/bin");
    let extension = if target.contains("windows") {
        ".exe"
    } else {
        ""
    };
    let destination = bin_dir.join(format!("ffmpeg-{target}{extension}"));
    let marker = destination.with_extension("pin");

    let already_staged = destination.is_file()
        && fs::read_to_string(&marker).is_ok_and(|pinned| pinned == source.sha256);
    if already_staged {
        return;
    }

    println!(
        "cargo:warning=fetching pinned ffmpeg for {target} (only happens when the pin changes)"
    );
    fs::create_dir_all(&bin_dir).expect("failed to create resources/bin");

    let bytes = download(source.url);
    verify_sha256(&bytes, source.sha256, source.url);
    let payload = match source.archive {
        Archive::Raw => bytes,
        Archive::Zip(entry) => extract_zip_entry(&bytes, entry),
        Archive::TarXz(entry) => extract_tar_xz_entry(&bytes, entry),
    };

    let temporary = destination.with_extension("part");
    fs::write(&temporary, &payload).expect("failed to write downloaded ffmpeg binary");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o755))
            .expect("failed to mark ffmpeg as executable");
    }
    fs::rename(&temporary, &destination).expect("failed to install staged ffmpeg binary");
    fs::write(&marker, source.sha256).expect("failed to write ffmpeg pin marker");
}

/// ffmpeg downloads run from 47MB (macOS) to 199MB (Windows zip); ureq caps
/// `read_to_vec()` at 10MB by default, so raise it well above either.
const MAX_DOWNLOAD_BYTES: u64 = 256 * 1024 * 1024;

fn download(url: &str) -> Vec<u8> {
    let mut response = ureq::get(url)
        .call()
        .unwrap_or_else(|error| panic!("failed to download resource from {url}: {error}"));
    response
        .body_mut()
        .with_config()
        .limit(MAX_DOWNLOAD_BYTES)
        .read_to_vec()
        .unwrap_or_else(|error| panic!("failed to read resource download from {url}: {error}"))
}

fn verify_sha256(bytes: &[u8], expected: &str, url: &str) {
    use sha2::{Digest, Sha256};
    let actual = format!("{:x}", Sha256::digest(bytes));
    assert!(
        actual == expected,
        "resource download from {url} failed checksum verification: expected {expected}, got {actual}"
    );
}

fn extract_zip_entry(archive_bytes: &[u8], entry_path: &str) -> Vec<u8> {
    use std::io::Read;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(archive_bytes))
        .unwrap_or_else(|error| panic!("ffmpeg archive is not a valid zip: {error}"));
    let mut entry = archive
        .by_name(entry_path)
        .unwrap_or_else(|error| panic!("ffmpeg archive is missing {entry_path}: {error}"));
    let mut bytes = Vec::new();
    entry
        .read_to_end(&mut bytes)
        .unwrap_or_else(|error| panic!("failed to read {entry_path} from ffmpeg archive: {error}"));
    bytes
}

/// Shells out to the system `tar` to pull one file out of a `.tar.xz`. Only
/// ever used for the Linux ffmpeg pin, so relying on `tar`/`xz` already being
/// on the box (true on every GitHub-hosted Linux runner) beats adding tar/xz
/// crates for a single entry.
fn extract_tar_xz_entry(archive_bytes: &[u8], entry_path: &str) -> Vec<u8> {
    use std::process::Command;

    // Feed tar from a real file rather than stdin: the extracted ffmpeg
    // binary is well over a pipe buffer's worth of bytes, and piping a large
    // archive in over stdin while also capturing a large extraction over
    // stdout risks both sides blocking on a full OS pipe at once.
    let out_dir = std::env::var("OUT_DIR").expect("Cargo must provide OUT_DIR");
    let archive_path = Path::new(&out_dir).join("ffmpeg-download.tar.xz");
    fs::write(&archive_path, archive_bytes).expect("failed to write archive to OUT_DIR");

    let output = Command::new("tar")
        .args(["-xJO", "-f"])
        .arg(&archive_path)
        .arg(entry_path)
        .output()
        .unwrap_or_else(|error| panic!("failed to run tar to extract {entry_path}: {error}"));
    let _ = fs::remove_file(&archive_path);
    assert!(
        output.status.success(),
        "tar failed to extract {entry_path}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
