mod gui;
mod audio;
mod transcription;

use gtk4::prelude::*;
use gtk4::Application;

fn main() {
    // Initialisation du logger
    env_logger::init();

    let app = Application::builder()
        .application_id("com.github.nspeech")
        .build();

    app.connect_activate(gui::build_ui);

    app.run();
}