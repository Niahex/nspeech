use gtk4::prelude::*;
use gtk4::{Application, ApplicationWindow, Button, Box, Orientation, TextView, ScrolledWindow, TextBuffer};
use std::sync::{Arc, Mutex};
use std::thread;
use crate::audio::{AudioRecorder, AudioEvent};
use crate::transcription::TranscriptionManager;

struct AppState {
    recorder: Arc<Mutex<AudioRecorder>>,
    transcriber: TranscriptionManager,
    is_recording: bool,
}

enum AppMsg {
    InitSuccess(Arc<Mutex<AudioRecorder>>, TranscriptionManager),
    InitError(String),
    TranscriptionSuccess(String),
    TranscriptionError(String),
    AudioStopped(Vec<f32>), // Utilisé pour l'arrêt manuel ET automatique
    AudioStartError(String),
}

pub fn build_ui(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("nSpeech")
        .default_width(600)
        .default_height(400)
        .build();

    let vbox = Box::new(Orientation::Vertical, 10);
    vbox.set_margin_top(10);
    vbox.set_margin_bottom(10);
    vbox.set_margin_start(10);
    vbox.set_margin_end(10);

    let buffer = TextBuffer::new(None);
    let text_view = TextView::with_buffer(&buffer);
    text_view.set_editable(false);
    text_view.set_wrap_mode(gtk4::WrapMode::Word);
    
    let scrolled_window = ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .min_content_height(300)
        .child(&text_view)
        .build();

    let record_button = Button::with_label("Initializing...");
    record_button.set_sensitive(false);

    vbox.append(&scrolled_window);
    vbox.append(&record_button);

    window.set_child(Some(&vbox));
    window.present();

    // App State
    let state = Arc::new(Mutex::new(None::<AppState>));
    
    // Main Async Channel for UI
    let (sender, receiver) = async_channel::unbounded();
    
    // Channel for Audio Events (MPSC -> Bridge -> Async Channel)
    let (audio_event_tx, audio_event_rx) = std::sync::mpsc::channel();

    // Bridge Thread: Audio Events (MPSC) -> UI Events (Async)
    let sender_bridge = sender.clone();
    thread::spawn(move || {
        while let Ok(event) = audio_event_rx.recv() {
            match event {
                AudioEvent::AutoStopped(samples) => {
                    let _ = sender_bridge.send_blocking(AppMsg::AudioStopped(samples));
                }
            }
        }
    });

    // Init Thread
    let sender_init = sender.clone();
    thread::spawn(move || {
        let recorder = match AudioRecorder::new(audio_event_tx) {
            Ok(r) => r,
            Err(e) => {
                let _ = sender_init.send_blocking(AppMsg::InitError(format!("Audio Init Failed: {}", e)));
                return;
            }
        };

        let transcriber = TranscriptionManager::new(std::path::Path::new("."));
        if let Err(e) = transcriber.load_model() {
            let _ = sender_init.send_blocking(AppMsg::InitError(format!("Model Load Failed: {}", e)));
            return;
        }

        let _ = sender_init.send_blocking(AppMsg::InitSuccess(Arc::new(Mutex::new(recorder)), transcriber));
    });

    // UI Event Loop
    let state_clone = state.clone();
    let button_clone = record_button.clone();
    let buffer_clone = buffer.clone();
    let sender_clone = sender.clone();
    let clipboard = gtk4::prelude::WidgetExt::display(&window).clipboard();

    glib::MainContext::default().spawn_local(async move {
        while let Ok(msg) = receiver.recv().await {
            match msg {
                AppMsg::InitSuccess(recorder, transcriber) => {
                    *state_clone.lock().unwrap() = Some(AppState {
                        recorder,
                        transcriber,
                        is_recording: false,
                    });
                    button_clone.set_label("Start Recording");
                    button_clone.set_sensitive(true);
                }
                AppMsg::InitError(e) => {
                    button_clone.set_label("Init Failed");
                    buffer_clone.set_text(&e);
                }
                AppMsg::TranscriptionSuccess(text) => {
                    button_clone.set_label("Start Recording");
                    button_clone.set_sensitive(true);
                    
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        buffer_clone.set_text(trimmed);
                        clipboard.set_text(trimmed);
                    } else {
                        // Ignore empty transcriptions (often noise)
                        buffer_clone.set_text("... (no speech detected)");
                    }
                }
                AppMsg::TranscriptionError(e) => {
                    button_clone.set_label("Start Recording");
                    button_clone.set_sensitive(true);
                    buffer_clone.set_text(&format!("Error: {}", e));
                }
                AppMsg::AudioStopped(samples) => {
                    // Mise à jour de l'état interne (important pour le bouton)
                    let mut guard = state_clone.lock().unwrap();
                    if let Some(app_state) = guard.as_mut() {
                        app_state.is_recording = false;
                    }
                    
                    // UI Update
                    button_clone.set_label("Processing...");
                    button_clone.set_sensitive(false);

                    if samples.is_empty() {
                        button_clone.set_label("Start Recording");
                        button_clone.set_sensitive(true);
                        buffer_clone.set_text("No audio recorded.");
                    } else {
                        // Start Transcription
                        if let Some(app_state) = guard.as_ref() {
                            let transcriber = app_state.transcriber.clone();
                            let sender_trans = sender_clone.clone();
                            thread::spawn(move || {
                                match transcriber.transcribe(&samples) {
                                    Ok(text) => { let _ = sender_trans.send_blocking(AppMsg::TranscriptionSuccess(text)); }
                                    Err(e) => { let _ = sender_trans.send_blocking(AppMsg::TranscriptionError(e.to_string())); }
                                }
                            });
                        }
                    }
                }
                AppMsg::AudioStartError(e) => {
                     buffer_clone.set_text(&format!("Start Error: {}", e));
                }
            }
        }
    });

    // Button Handler
    let state_clone = state.clone();
    let button_clone = record_button.clone();
    let buffer_clone = buffer.clone();
    let sender_clone = sender.clone();

    record_button.connect_clicked(move |_| {
        let mut guard = state_clone.lock().unwrap();
        if let Some(app_state) = guard.as_mut() {
            if app_state.is_recording {
                // STOP (Manuel)
                let recorder = app_state.recorder.clone();
                let sender_stop = sender_clone.clone();
                
                // On lance le stop dans un thread pour ne pas bloquer l'UI pendant le lock/channel
                thread::spawn(move || {
                     let res = recorder.lock().unwrap().stop_recording();
                     match res {
                         Ok(samples) => { let _ = sender_stop.send_blocking(AppMsg::AudioStopped(samples)); }
                         Err(e) => { let _ = sender_stop.send_blocking(AppMsg::TranscriptionError(format!("Stop Error: {}", e))); }
                     }
                });
            } else {
                // START
                if let Err(e) = app_state.recorder.lock().unwrap().start_recording() {
                    let _ = sender_clone.send_blocking(AppMsg::AudioStartError(e.to_string()));
                } else {
                    app_state.is_recording = true;
                    button_clone.set_label("Stop Recording");
                    buffer_clone.set_text("Recording...");
                }
            }
        }
    });
}