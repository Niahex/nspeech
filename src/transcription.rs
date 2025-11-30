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
            // Optimisation : Utilisation du modèle "base" quantifié (q5_1)
            // Plus léger (RAM) et plus rapide sur CPU/GPU
            model_path: model_dir.join("ggml-base-q5_1.bin"),
        }
    }

    pub fn ensure_model_exists(&self) -> Result<()> {
        if self.model_path.exists() {
            return Ok(());
        }

        info!("Downloading optimized model to {:?}", self.model_path);
        // URL du modèle quantifié officiel
        let url = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-q5_1.bin";
        
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
        
        info!("Whisper model loaded (Optimized q5_1).");
        Ok(())
    }

    pub fn transcribe(&self, audio_data: &[f32]) -> Result<String> {
        let mut guard = self.engine.lock().unwrap();
        let engine = guard.as_mut().ok_or(anyhow!("Engine not loaded"))?;
        
        let params = WhisperInferenceParams {
            language: Some("fr".to_string()),
            // Optimisation : Désactiver les sorties debug inutiles pour gagner un peu de temps CPU
            print_progress: false,
            print_realtime: false,
            print_timestamps: false,
            ..Default::default()
        };
        
        let transcript = TranscriptionEngine::transcribe_samples(engine, audio_data.to_vec(), Some(params))
             .map_err(|e| anyhow!("Transcription failed: {}", e))?;
            
        Ok(transcript.text)
    }
}