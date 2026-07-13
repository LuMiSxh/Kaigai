use super::coreml;

pub(super) struct ModelDefinition {
    pub id: &'static str,
    pub label: &'static str,
    pub provider: &'static str,
    /// What sets this model apart, shown to the user while picking a model.
    pub description: &'static str,
    /// Hugging Face repo the GGML weights (and Core ML archive, if any) are
    /// downloaded from — not every model lives in `ggerganov/whisper.cpp`.
    pub repo: &'static str,
    pub size_bytes: u64,
    pub sha256: &'static str,
    pub core_ml: Option<coreml::CoreMlDefinition>,
    /// Whether this model supports the Whisper `translate` task.
    /// Distilled/turbo models are fine-tuned on transcription only and
    /// silently ignore the translate flag, outputting the source language.
    pub supports_translate: bool,
}

pub(super) const MODELS: [ModelDefinition; 6] = [
    ModelDefinition {
        id: "tiny",
        label: "Tiny",
        provider: "OpenAI",
        description: "Fastest and lightest. Expect noticeably more mistakes, especially in Japanese — best for quick tests, not daily use.",
        repo: "ggerganov/whisper.cpp",
        size_bytes: 77_691_713,
        sha256: "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21",
        core_ml: Some(coreml::CoreMlDefinition {
            size_bytes: 15_037_446,
            sha256: "c88cbd2648e1f5415092bcf5256add463a0f19943e6938f46e8d4ffdebd47739",
        }),
        supports_translate: true,
    },
    ModelDefinition {
        id: "base",
        label: "Base",
        provider: "OpenAI",
        description: "A step up from Tiny in accuracy, still very fast. Fine for casual viewing where occasional mistakes are okay.",
        repo: "ggerganov/whisper.cpp",
        size_bytes: 147_951_465,
        sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
        core_ml: Some(coreml::CoreMlDefinition {
            size_bytes: 37_922_638,
            sha256: "7e6ab77041942572f239b5b602f8aaa1c3ed29d73e3d8f20abea03a773541089",
        }),
        supports_translate: true,
    },
    ModelDefinition {
        id: "small",
        label: "Small",
        provider: "OpenAI",
        description: "Fastest model that stayed usable in the VTuber benchmark. Best when low latency matters more than accuracy; expect more artifacts than Medium.",
        repo: "ggerganov/whisper.cpp",
        size_bytes: 487_601_967,
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
        core_ml: Some(coreml::CoreMlDefinition {
            size_bytes: 163_083_239,
            sha256: "de43fb9fed471e95c19e60ae67575c2bf09e8fb607016da171b06ddad313988b",
        }),
        supports_translate: true,
    },
    ModelDefinition {
        id: "medium",
        label: "Medium",
        provider: "OpenAI",
        description: "Recommended balanced default from the VTuber benchmark. More accurate than Small while still comfortably real-time on Apple Silicon with Neural Engine enabled.",
        repo: "ggerganov/whisper.cpp",
        size_bytes: 1_533_763_059,
        sha256: "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208",
        core_ml: Some(coreml::CoreMlDefinition {
            size_bytes: 567_829_413,
            sha256: "79b0b8d436d47d3f24dd3afc91f19447dd686a4f37521b2f6d9c30a642133fbd",
        }),
        supports_translate: true,
    },
    ModelDefinition {
        id: "large-v3",
        label: "Large v3",
        provider: "OpenAI",
        description: "Accuracy-first option. Best on difficult audio, but much heavier than Medium; use when quality matters more than latency.",
        repo: "ggerganov/whisper.cpp",
        size_bytes: 3_095_033_483,
        sha256: "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2",
        core_ml: Some(coreml::CoreMlDefinition {
            size_bytes: 1_175_711_232,
            sha256: "47837be7594a29429ec08620043390c4d6d467f8bd362df09e9390ace76a55a4",
        }),
        supports_translate: true,
    },
    ModelDefinition {
        id: "large-v3-turbo",
        label: "Large v3 Turbo",
        provider: "OpenAI",
        description: "A distilled Large v3: about 6x faster with nearly identical Japanese accuracy. The best accuracy-to-speed tradeoff for live captioning. Translation to English is not supported by this model.",
        repo: "ggerganov/whisper.cpp",
        size_bytes: 1_624_555_275,
        sha256: "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
        core_ml: Some(coreml::CoreMlDefinition {
            size_bytes: 1_173_393_014,
            sha256: "84bedfe895bd7b5de6e8e89a0803dfc5addf8c0c5bc4c937451716bf7cf7988a",
        }),
        supports_translate: false,
    },
];

pub(super) fn definition(id: &str) -> Result<&'static ModelDefinition, String> {
    MODELS
        .iter()
        .find(|model| model.id == id)
        .ok_or_else(|| format!("unknown Whisper model: {id}"))
}
