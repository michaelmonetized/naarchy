use std::{fs, path::Path};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = omadesign::project::load_from(Path::new("launch/motion/naarchy-reveal.oma"))?;
    fs::write("launch/motion/naarchy-reveal.svg", omadesign::svg::export_animated(&doc)?)?;
    fs::write("launch/motion/naarchy-reveal.json", omadesign::motion::export_lottie(&doc)?)?;
    fs::create_dir_all("launch/artifacts/motion-frames")?;
    for i in 0..120 {
        let png = omadesign::anim_export::render_frame(&doc, i as f32 / 30.0, 2.0, true)?;
        fs::write(format!("launch/artifacts/motion-frames/{i:04}.png"), png)?;
    }
    Ok(())
}
