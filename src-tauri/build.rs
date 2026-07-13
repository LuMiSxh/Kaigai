use std::{fs, path::Path};

/// How to pull the actual binary out of a downloaded payload.
enum Archive {
    /// Download is the binary, nothing to extract.
    Raw,
    Zip(&'static str),
    /// Shells out to the system `tar` instead of pulling in a tar/xz crate
    /// for the one entry that needs it.
    TarXz(&'static str),
}

struct SidecarSource {
    target: &'static str,
    url: &'static str,
    sha256: &'static str,
    archive: Archive,
}

// Hand-pinned, not auto-detected: some upstreams (evermeet.cx's macOS
// builds) only ship a GPG signature, not a checksum, so we downloaded and
// hashed these ourselves. Bump url + sha256 together to refresh a pin —
// `stage_sidecar` re-fetches on its own once the hash here stops matching.
const FFMPEG_SOURCES: &[SidecarSource] = &[
    SidecarSource {
        target: "aarch64-apple-darwin",
        url: "https://github.com/descriptinc/ffmpeg-ffprobe-static/releases/download/b6.1.2-rc.1/ffmpeg-darwin-arm64",
        sha256: "9f865039102a1139c7057d7f21ddaacd106d602fa3af1f99b70f43d520439b8c",
        archive: Archive::Raw,
    },
    SidecarSource {
        // BtbN's `latest` tag is a moving target — same file name, contents
        // replaced in place on every rebuild — so it silently breaks this
        // pin every so often. Their dated `autobuild-*` tags are immutable
        // (the filename itself is commit-hash-suffixed), so pin to one of
        // those instead. Bump to a newer `autobuild-*` tag when the n8.1
        // branch itself moves on.
        target: "x86_64-pc-windows-msvc",
        url: "https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-07-10-13-44/ffmpeg-n8.1.2-22-g94138f6973-win64-lgpl-8.1.zip",
        sha256: "fb8cad4111deb1eb46f7ece876b58621c41df2a472b77b1630b6f799c9a9b9b2",
        archive: Archive::Zip("ffmpeg-n8.1.2-22-g94138f6973-win64-lgpl-8.1/bin/ffmpeg.exe"),
    },
    SidecarSource {
        // See the Windows entry above for why this points at a dated
        // `autobuild-*` tag rather than `latest`.
        target: "x86_64-unknown-linux-gnu",
        url: "https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-07-10-13-44/ffmpeg-n8.1.2-22-g94138f6973-linux64-lgpl-8.1.tar.xz",
        sha256: "42f964d0b2bb6a5460fd309119e8623d04192ea15bdf08bc330b214a40aa9814",
        archive: Archive::TarXz("ffmpeg-n8.1.2-22-g94138f6973-linux64-lgpl-8.1/bin/ffmpeg"),
    },
];

// yt-dlp needs an external JS runtime to solve YouTube's "n" signature
// challenge, or it silently returns fewer formats instead of erroring.
// QuickJS-NG ships tiny (~2MB vs Deno's ~30-50MB) single-file binaries for
// all three targets, so we bundle it instead of hoping Deno/Node is on PATH.
const QUICKJS_SOURCES: &[SidecarSource] = &[
    SidecarSource {
        target: "aarch64-apple-darwin",
        url: "https://github.com/quickjs-ng/quickjs/releases/download/v0.15.1/qjs-darwin",
        sha256: "badc31a289050d56f1d184651736bfa6399ef0ad40db6b210b8a88a3d34be36a",
        archive: Archive::Raw,
    },
    SidecarSource {
        target: "x86_64-pc-windows-msvc",
        url: "https://github.com/quickjs-ng/quickjs/releases/download/v0.15.1/qjs-windows-x86_64.exe",
        sha256: "5ea527b0405f0f3d11904c8722a4f1df9b631a4beed2bf988d0a831eb9f8e913",
        archive: Archive::Raw,
    },
    SidecarSource {
        target: "x86_64-unknown-linux-gnu",
        url: "https://github.com/quickjs-ng/quickjs/releases/download/v0.15.1/qjs-linux-x86_64",
        sha256: "c015660c38e7960669b112dafa3740cd6ce29b3d42066a64da1bd042fbccac07",
        archive: Archive::Raw,
    },
];

const SILERO_VAD_URL: &str =
    "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v6.2.0.bin?download=true";
const SILERO_VAD_SHA256: &str = "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987";
const SILERO_VAD_FILE: &str = "ggml-silero-v6.2.0.bin";
const DOWNLOAD_ATTEMPTS: u32 = 3;

fn main() {
    let target = std::env::var("TARGET").expect("Cargo must provide TARGET");
    println!("cargo:rustc-env=KAIGAI_TARGET_TRIPLE={target}");
    println!("cargo:rerun-if-env-changed=KAIGAI_SKIP_RESOURCE_DOWNLOADS");

    let skip_downloads =
        std::env::var("KAIGAI_SKIP_RESOURCE_DOWNLOADS").is_ok_and(|value| value == "1");
    if skip_downloads {
        println!("cargo:warning=skipping runtime resource downloads");
    } else {
        stage_sidecar("ffmpeg", &target, FFMPEG_SOURCES);
        stage_sidecar("qjs", &target, QUICKJS_SOURCES);
        stage_silero_vad(&target);
    }
    tauri_build::build();
}

/// Stages whisper.cpp's Silero VAD model. Not platform-specific like ffmpeg,
/// so every target gets the same download — no per-target pin list needed.
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

/// Downloads the pinned `tool_name` sidecar for `target` into
/// `resources/bin/` if it isn't already staged there, and checks it against
/// our own hash. Unpinned targets are skipped with a warning.
fn stage_sidecar(tool_name: &str, target: &str, sources: &[SidecarSource]) {
    let Some(source) = sources.iter().find(|source| source.target == target) else {
        println!("cargo:warning=no pinned {tool_name} build for target {target}; skipping");
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
    let destination = bin_dir.join(format!("{tool_name}-{target}{extension}"));
    let marker = destination.with_extension("pin");

    let already_staged = destination.is_file()
        && fs::read_to_string(&marker).is_ok_and(|pinned| pinned == source.sha256);
    if already_staged {
        return;
    }

    println!(
        "cargo:warning=fetching pinned {tool_name} for {target} (only happens when the pin changes)"
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
    fs::write(&temporary, &payload).expect("failed to write downloaded sidecar binary");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o755))
            .expect("failed to mark sidecar as executable");
    }
    fs::rename(&temporary, &destination).expect("failed to install staged sidecar binary");
    fs::write(&marker, source.sha256).expect("failed to write sidecar pin marker");
}

/// ureq caps `read_to_vec()` at 10MB by default; ffmpeg downloads run up to
/// ~200MB (the Windows zip), so raise the limit well above that.
const MAX_DOWNLOAD_BYTES: u64 = 256 * 1024 * 1024;

fn download(url: &str) -> Vec<u8> {
    for attempt in 1..=DOWNLOAD_ATTEMPTS {
        match download_once(url) {
            Ok(bytes) => return bytes,
            Err(error) if attempt < DOWNLOAD_ATTEMPTS => {
                println!(
                    "cargo:warning={error}; retrying resource download ({}/{DOWNLOAD_ATTEMPTS})",
                    attempt + 1
                );
                std::thread::sleep(std::time::Duration::from_secs(u64::from(attempt) * 2));
            }
            Err(error) => panic!("{error}"),
        }
    }
    unreachable!("download loop must return or panic")
}

fn download_once(url: &str) -> Result<Vec<u8>, String> {
    let mut response = ureq::get(url)
        .call()
        .map_err(|error| format!("failed to download resource from {url}: {error}"))?;
    response
        .body_mut()
        .with_config()
        .limit(MAX_DOWNLOAD_BYTES)
        .read_to_vec()
        .map_err(|error| format!("failed to read resource download from {url}: {error}"))
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

/// Pulls one file out of a `.tar.xz` via the system `tar`, since it's only
/// needed for the Linux ffmpeg pin and every Linux box already has it.
fn extract_tar_xz_entry(archive_bytes: &[u8], entry_path: &str) -> Vec<u8> {
    use std::process::Command;

    // Write to a real file rather than piping in: the extracted ffmpeg
    // binary is big enough that feeding tar over stdin while also reading
    // its stdout risks both sides blocking on a full pipe.
    let out_dir = std::env::var("OUT_DIR").expect("Cargo must provide OUT_DIR");
    let archive_path = Path::new(&out_dir).join("sidecar-download.tar.xz");
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
