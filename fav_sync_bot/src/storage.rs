use log::info;
use std::sync::OnceLock;

pub fn output_dir() -> &'static str {
    static OUTPUT_DIR: OnceLock<String> = OnceLock::new();
    OUTPUT_DIR.get_or_init(|| {
        let output_dir = std::env::args().nth(1).unwrap_or(".".to_string());
        info!("Output dir: {}", output_dir);
        std::fs::create_dir_all(&output_dir).expect("Failed to create output dir");
        output_dir
    })
}

pub fn fav_dir() -> &'static str {
    static FAV_DIR: OnceLock<String> = OnceLock::new();
    FAV_DIR.get_or_init(|| {
        let fav_dir = format!("{}/favorite", output_dir());
        info!("Favorite dir: {}", fav_dir);
        std::fs::create_dir_all(&fav_dir).expect("Failed to create favorite dir");
        fav_dir
    })
}
