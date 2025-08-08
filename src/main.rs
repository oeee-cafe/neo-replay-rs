use anyhow::Result;
use neo_replay_rs::{PchFile, renderer::Renderer};
use std::env;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <pch_file>", args[0]);
        std::process::exit(1);
    }

    let pch_path = &args[1];
    println!("Loading PCH file: {}", pch_path);

    // Load and parse PCH file
    let mut pch = PchFile::from_file(pch_path)?;
    println!("PCH dimensions: {}x{}", pch.header.width, pch.header.height);
    println!("Number of actions: {}", pch.actions.len());

    // Fix actions as per original logic
    pch.fix_actions();
    println!("Actions after fixing: {}", pch.actions.len());

    // Create renderer
    let mut renderer = Renderer::new(pch.header.width as u32, pch.header.height as u32);

    // Render frame by frame
    println!("Rendering frames...");
    let frames = renderer.render_frame_by_frame(&pch)?;
    println!("Generated {} frames", frames.len());

    // Save all frames with separate layers
    let output_dir = "output_frames";
    std::fs::create_dir_all(output_dir)?;

    for (i, frame_set) in frames.iter().enumerate() {
        // Save layer 0
        let layer0_filename = format!("{}/frame_{:06}_layer_0.png", output_dir, i);
        frame_set.layer0.save(&layer0_filename)?;
        
        // Save layer 1
        let layer1_filename = format!("{}/frame_{:06}_layer_1.png", output_dir, i);
        frame_set.layer1.save(&layer1_filename)?;
        
        // Save composite
        let composite_filename = format!("{}/frame_{:06}_composite.png", output_dir, i);
        frame_set.composite.save(&composite_filename)?;

        if i % 100 == 0 || i == frames.len() - 1 {
            println!("Saved frame {}/{} (layer0, layer1, composite)", i + 1, frames.len());
        }
    }

    println!("All frames saved to {}/", output_dir);
    println!("Each frame includes: frame_XXXXXX_layer_0.png, frame_XXXXXX_layer_1.png, frame_XXXXXX_composite.png");

    Ok(())
}