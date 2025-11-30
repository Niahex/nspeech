# nspeech

A GTK4 Speech-to-Text application for NixOS/Linux using Whisper and Vulkan acceleration.

## Requirements

- Nix with flakes enabled.
- A GPU compatible with Vulkan (optional but recommended for speed).

## Usage

1.  **Run the application:**
    ```bash
    nix develop -c cargo run
    ```
    On the first run, it will download the `whisper-tiny.en.bin` model automatically.

2.  **Interface:**
    - Click **"Start Recording"** to begin capturing audio.
    - Click **"Stop Recording"** to stop and transcribe.
    - The transcription will appear in the text area.

## Features

- **Local Processing:** Everything runs on your device using `whisper.cpp` via `transcribe-rs`.
- **GPU Acceleration:** Uses Vulkan for inference.
- **GTK4 Interface:** Native Linux look and feel.
- **Wayland Support:** Fully compatible.

## Configuration

To change the model, edit `src/transcription.rs` and change the download URL or filename.
Supported models: `ggml-tiny.en.bin`, `ggml-base.en.bin`, etc.
