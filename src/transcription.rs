use anyhow::{anyhow, Result};
use log::info;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use transcribe_rs::engines::whisper::{WhisperEngine, WhisperInferenceParams};
use transcribe_rs::TranscriptionEngine;

#[derive(Clone)]
pub struct TranscriptionManager {
    engine: Arc<Mutex<Option<WhisperEngine>>>,
    model_path: PathBuf,
}

impl TranscriptionManager {
    pub fn new(model_dir: &Path) -> Self {
        Self {
            engine: Arc::new(Mutex::new(None)),
            model_path: model_dir.join("whisper-tiny.bin"),
        }
    }

    pub fn ensure_model_exists(&self) -> Result<()> {
        if self.model_path.exists() {
            return Ok(());
        }

        info!("Downloading model to {:?}", self.model_path);
        let url = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin";
        
        let runtime = tokio::runtime::Runtime::new()?;
        runtime.block_on(async {
            let resp = reqwest::get(url).await?.bytes().await?;
            std::fs::write(&self.model_path, resp)?;
            Ok::<(), anyhow::Error>(())
        })?;
        
        info!("Model downloaded.");
        Ok(())
    }

    pub fn load_model(&self) -> Result<()> {
        self.ensure_model_exists()?;
        
        let mut engine = WhisperEngine::new();
        engine.load_model(&self.model_path)
            .map_err(|e| anyhow!("Failed to load model: {}", e))?;
            
        let mut guard = self.engine.lock().unwrap();
        *guard = Some(engine);
        
        info!("Whisper model loaded.");
        Ok(())
    }

    pub fn transcribe(&self, audio_data: &[f32]) -> Result<String> {
        let mut guard = self.engine.lock().unwrap();
        let engine = guard.as_mut().ok_or(anyhow!("Engine not loaded"))?;
        
        let params = WhisperInferenceParams {
            language: Some("en".to_string()),
            ..Default::default()
        };
        
        // Use correct method: transcribe_samples
        let transcript = engine.transcribe_samples(audio_data.to_vec(), Some(params))
             .map_err(|e| anyhow!("Transcription failed: {}", e))?;
            
        Ok(transcript.text)
    }
}