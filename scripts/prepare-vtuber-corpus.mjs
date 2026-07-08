import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { access, constants } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const corpusPath = resolve(
    process.env.KAIGAI_BENCH_CORPUS ?? join(repoRoot, "benchmarks/corpus/jp-vtuber-corpus.json"),
);
const corpusDir = dirname(corpusPath);
const cacheDir = resolve(process.env.KAIGAI_BENCH_CACHE ?? join(corpusDir, "cache"));
const audioDir = resolve(process.env.KAIGAI_BENCH_AUDIO ?? join(corpusDir, "audio"));
const generatedDir = resolve(join(corpusDir, "generated"));

const corpus = JSON.parse(readFileSync(corpusPath, "utf8"));
const sampleRateHz = corpus.sampleRateHz ?? 16_000;

mkdirSync(cacheDir, { recursive: true });
mkdirSync(audioDir, { recursive: true });
mkdirSync(generatedDir, { recursive: true });

const ytDlp = process.env.YT_DLP ?? "yt-dlp";
const ffmpeg = await resolveFfmpeg();

for (const clip of corpus.clips) {
    const source = await ensureSource(clip);
    const destination = join(audioDir, `${clip.id}.wav`);
    if (!existsSync(destination) || process.env.KAIGAI_BENCH_REBUILD === "1") {
        run(ffmpeg, [
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-i",
            source,
            "-vn",
            "-ac",
            "1",
            "-ar",
            String(sampleRateHz),
            "-c:a",
            "pcm_s16le",
            destination,
        ]);
    }
}

const generatedManifest = {
    version: corpus.version,
    sampleRateHz,
    corpus: corpusPath,
    generatedAt: new Date().toISOString(),
    clips: corpus.clips.map((clip) => ({
        ...clip,
        audioPath: join(audioDir, `${clip.id}.wav`),
    })),
};

const generatedManifestPath = join(generatedDir, "manifest.json");
writeFileSync(generatedManifestPath, `${JSON.stringify(generatedManifest, null, 4)}\n`);
console.log(generatedManifestPath);

async function ensureSource(clip) {
    const existing = findDownloadedSource(clip.id);
    if (existing) {
        return existing;
    }

    const args = [
        "--no-playlist",
        "--no-mtime",
        "--force-keyframes-at-cuts",
        "--download-sections",
        `*${clip.start}-${addSeconds(clip.start, clip.durationSeconds)}`,
        "-f",
        "bestaudio/best",
        "--paths",
        cacheDir,
        "-o",
        `${clip.id}.%(ext)s`,
        clip.url,
    ];
    args.splice(1, 0, "--ffmpeg-location", ffmpeg === "ffmpeg" ? "ffmpeg" : dirname(ffmpeg));
    run(ytDlp, args);

    const downloaded = findDownloadedSource(clip.id);
    if (!downloaded) {
        throw new Error(`yt-dlp finished but no source file was found for ${clip.id}`);
    }
    return downloaded;
}

function findDownloadedSource(id) {
    return readdirSync(cacheDir)
        .filter((entry) => entry.startsWith(`${id}.`) && !entry.endsWith(".part"))
        .map((entry) => join(cacheDir, entry))
        .at(0);
}

function run(command, args) {
    const result = spawnSync(command, args, { stdio: "inherit" });
    if (result.error) {
        throw result.error;
    }
    if (result.status !== 0) {
        throw new Error(`${command} exited with ${result.status}`);
    }
}

function addSeconds(timestamp, secondsToAdd) {
    const seconds = parseTimestamp(timestamp) + Number(secondsToAdd);
    const rounded = Math.round(seconds);
    const hours = Math.floor(rounded / 3600);
    const minutes = Math.floor((rounded % 3600) / 60);
    const secondsPart = rounded % 60;
    return [hours, minutes, secondsPart].map((part) => String(part).padStart(2, "0")).join(":");
}

function parseTimestamp(timestamp) {
    return timestamp
        .split(":")
        .map(Number)
        .reduce((total, part) => total * 60 + part, 0);
}

async function resolveFfmpeg() {
    const candidates = [
        process.env.FFMPEG,
        which("ffmpeg"),
        join(repoRoot, "src-tauri/target/debug/resources/bin/ffmpeg-aarch64-apple-darwin"),
        join(repoRoot, "src-tauri/target/release/resources/bin/ffmpeg-aarch64-apple-darwin"),
        join(repoRoot, "src-tauri/resources/bin/ffmpeg-aarch64-apple-darwin"),
        "ffmpeg",
    ].filter(Boolean);

    for (const candidate of candidates) {
        if (candidate === "ffmpeg") {
            return candidate;
        }
        try {
            await access(candidate, constants.X_OK);
            return candidate;
        } catch {
            // Try the next candidate.
        }
    }
    return "ffmpeg";
}

function which(command) {
    const result = spawnSync("sh", ["-lc", `command -v ${command}`], { encoding: "utf8" });
    if (result.status !== 0) {
        return undefined;
    }
    return result.stdout.trim() || undefined;
}
