"""Local engine adapters. Every file must exist before native constructors run."""

from pathlib import Path


class ZipformerEngine:
    def __init__(self, root: Path, *, threads=2, hotwords=True):
        self.files = {name: root / filename for name, filename in {
            "encoder": "encoder-epoch-99-avg-1.int8.onnx",
            "decoder": "decoder-epoch-99-avg-1.onnx",
            "joiner": "joiner-epoch-99-avg-1.int8.onnx",
            "tokens": "tokens.txt", "bpe_vocab": "bpe.vocab",
        }.items()}
        if hotwords:
            self.files["hotwords_file"] = Path(__file__).resolve().parent.parent / "configs/developer-hotwords.txt"
        for path in self.files.values():
            if not path.is_file():
                raise FileNotFoundError(f"local model artifact missing: {path.name}")
        import sherpa_onnx
        self.config = {
            "engine": "zipformer-bilingual-2023-02-20-int8",
            "provider": "cpu", "num_threads": threads,
            "decoding_method": "modified_beam_search" if hotwords else "greedy_search",
            "max_active_paths": 8, "hotwords_score": 2.0,
            "hotwords": ["JAVA", "KOTLIN"] if hotwords else [],
            "modeling_unit": "cjkchar+bpe", "enable_endpoint_detection": False,
        }
        self.recognizer = sherpa_onnx.OnlineRecognizer.from_transducer(
            **{key: str(path) for key, path in self.files.items()},
            num_threads=threads, provider="cpu", modeling_unit="cjkchar+bpe",
            decoding_method=self.config["decoding_method"], max_active_paths=8,
            hotwords_score=2.0, enable_endpoint_detection=False,
        )

    def new_session(self):
        stream = self.recognizer.create_stream()
        return OnlineSession(self.recognizer, stream)


class OnlineSession:
    def __init__(self, recognizer, stream):
        self.recognizer, self.stream = recognizer, stream

    def accept(self, samples):
        self.stream.accept_waveform(16000, samples)
        while self.recognizer.is_ready(self.stream):
            self.recognizer.decode_stream(self.stream)
        return self.recognizer.get_result(self.stream)

    def finish(self):
        self.stream.input_finished()
        while self.recognizer.is_ready(self.stream):
            self.recognizer.decode_stream(self.stream)
        return self.recognizer.get_result(self.stream)


class SenseVoiceEngine:
    def __init__(self, root: Path, *, threads=2):
        self.files = {"model": root / "model.int8.onnx", "tokens": root / "tokens.txt"}
        for path in self.files.values():
            if not path.is_file():
                raise FileNotFoundError(f"local model artifact missing: {path.name}")
        import sherpa_onnx
        self.config = {"engine": "sensevoice-small-2024-07-17-int8", "provider": "cpu",
                       "num_threads": threads, "language": "auto", "use_itn": False,
                       "native_streaming": False}
        self.recognizer = sherpa_onnx.OfflineRecognizer.from_sense_voice(
            **{key: str(path) for key, path in self.files.items()},
            num_threads=threads, provider="cpu", language="auto", use_itn=False)

    def new_session(self):
        return OfflineSession(self.recognizer)


class QwenEngine:
    def __init__(self, root: Path, *, threads=2):
        self.files = {name: root / filename for name, filename in {
            "conv_frontend": "conv_frontend.onnx", "encoder": "encoder.int8.onnx",
            "decoder": "decoder.int8.onnx", "tokenizer_vocab": "tokenizer/vocab.json",
            "tokenizer_merges": "tokenizer/merges.txt",
            "tokenizer_config": "tokenizer/tokenizer_config.json",
        }.items()}
        for path in self.files.values():
            if not path.is_file():
                raise FileNotFoundError(f"local model artifact missing: {path.name}")
        for path in sorted((root / "tokenizer").rglob("*")):
            if path.is_file():
                self.files[str(path.relative_to(root))] = path
        import sherpa_onnx
        self.config = {"engine": "qwen3-asr-0.6b-2026-03-25-int8", "provider": "cpu",
                       "num_threads": threads, "max_total_len": 1024, "max_new_tokens": 512,
                       "temperature": 1e-6, "top_p": 0.8, "seed": 42,
                       "decoding_method": "greedy_search", "hotwords": "",
                       "native_streaming": False}
        self.recognizer = sherpa_onnx.OfflineRecognizer.from_qwen3_asr(
            **{key: str(self.files[key]) for key in ("conv_frontend", "encoder", "decoder")},
            tokenizer=str(root / "tokenizer"), num_threads=threads, provider="cpu",
            max_total_len=1024, max_new_tokens=512, temperature=1e-6, top_p=0.8, seed=42)

    def new_session(self):
        return OfflineSession(self.recognizer)


class OfflineSession:
    def __init__(self, recognizer):
        self.recognizer = recognizer
        self.chunks = []

    def accept(self, samples):
        self.chunks.append(samples)
        return ""

    def finish(self):
        import numpy as np
        stream = self.recognizer.create_stream()
        stream.accept_waveform(16000, np.concatenate(self.chunks).astype(np.float32, copy=False))
        self.recognizer.decode_stream(stream)
        return stream.result.text
