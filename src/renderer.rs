use crate::{ActionValue, Color, DrawingState, LineType, MaskType, PchFile, AlphaType, FillType};
use anyhow::{bail, Result};
use image::{ImageBuffer, Rgb, RgbImage, Rgba, RgbaImage};
use ab_glyph::{FontRef, PxScale, point, Font};
use font_kit::family_name::FamilyName;
use font_kit::properties::Properties;
use font_kit::source::SystemSource;

pub struct Canvas {
    pub layers: [RgbaImage; 2], // Two layers with alpha support
    pub width: u32,
    pub height: u32,
    pub current_layer: usize,
    pub visible: [bool; 2],
}

pub struct Renderer {
    pub canvas: Canvas,
    pub state: DrawingState,
    pub round_data: Vec<Vec<u8>>, // Circular brush masks for each radius (1-30)
    pub tone_data: Vec<Vec<u8>>, // 4x4 dithering patterns for tone brush (16 levels)
    pub arial_font: Option<FontRef<'static>>, // Arial font for text rendering
    pub clipboard: Option<Vec<u32>>, // Temporary storage for copy/paste operations (RGBA data)
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            layers: [
                ImageBuffer::new(width, height),
                ImageBuffer::new(width, height),
            ],
            width,
            height,
            current_layer: 0,
            visible: [true, true],
        }
    }

    pub fn clear(&mut self) {
        for layer in &mut self.layers {
            for pixel in layer.pixels_mut() {
                *pixel = Rgba([0, 0, 0, 0]); // Fully transparent background
            }
        }
    }

    pub fn clear_layer(&mut self, layer: usize) {
        if layer < 2 {
            for pixel in self.layers[layer].pixels_mut() {
                *pixel = Rgba([0, 0, 0, 0]); // Fully transparent
            }
        }
    }

    pub fn composite(&self) -> RgbImage {
        let mut result = ImageBuffer::new(self.width, self.height);
        
        // Start with white background for final composite
        for pixel in result.pixels_mut() {
            *pixel = Rgb([255, 255, 255]);
        }

        // Composite visible layers in order (Layer 0, then Layer 1)
        for (layer_idx, layer) in self.layers.iter().enumerate() {
            if self.visible[layer_idx] {
                for (x, y, pixel) in layer.enumerate_pixels() {
                    if pixel.0[3] > 0 { // If foreground has alpha
                        let bg = result.get_pixel(x, y);
                        let fg = pixel;
                        
                        let alpha = fg.0[3] as f32 / 255.0;
                        let inv_alpha = 1.0 - alpha;
                        
                        let r = (fg.0[0] as f32 * alpha + bg.0[0] as f32 * inv_alpha) as u8;
                        let g = (fg.0[1] as f32 * alpha + bg.0[1] as f32 * inv_alpha) as u8;
                        let b = (fg.0[2] as f32 * alpha + bg.0[2] as f32 * inv_alpha) as u8;
                        
                        result.put_pixel(x, y, Rgb([r, g, b]));
                    }
                }
            }
        }

        result
    }

    pub fn get_layer(&self, layer: usize) -> Option<&RgbaImage> {
        if layer < 2 {
            Some(&self.layers[layer])
        } else {
            None
        }
    }
    
    pub fn get_layer_as_rgb(&self, layer: usize) -> Option<RgbImage> {
        if layer < 2 {
            let rgba_layer = &self.layers[layer];
            let mut rgb_layer = ImageBuffer::new(self.width, self.height);
            
            // Start with white background for individual layer outputs
            for pixel in rgb_layer.pixels_mut() {
                *pixel = Rgb([255, 255, 255]);
            }
            
            // Composite layer onto white background
            for (x, y, pixel) in rgba_layer.enumerate_pixels() {
                if pixel.0[3] > 0 { // If pixel has alpha
                    let alpha = pixel.0[3] as f32 / 255.0;
                    let inv_alpha = 1.0 - alpha;
                    
                    // Blend with white background
                    let r = (pixel.0[0] as f32 * alpha + 255.0 * inv_alpha) as u8;
                    let g = (pixel.0[1] as f32 * alpha + 255.0 * inv_alpha) as u8;
                    let b = (pixel.0[2] as f32 * alpha + 255.0 * inv_alpha) as u8;
                    
                    rgb_layer.put_pixel(x, y, Rgb([r, g, b]));
                }
            }
            
            Some(rgb_layer)
        } else {
            None
        }
    }
}

pub struct FrameSet {
    pub layer0: RgbImage,
    pub layer1: RgbImage,
    pub composite: RgbImage,
}

impl Renderer {
    pub fn new(width: u32, height: u32) -> Self {
        let mut renderer = Self {
            canvas: Canvas::new(width, height),
            state: DrawingState::default(),
            round_data: Vec::new(),
            tone_data: Vec::new(),
            arial_font: Self::load_arial_font(),
            clipboard: None,
        };
        renderer.init_round_data();
        renderer.init_tone_data();
        renderer
    }

    fn init_round_data(&mut self) {
        // Initialize round data for brush sizes 1-30
        self.round_data = vec![Vec::new(); 31]; // Index 0 unused, 1-30 for brush sizes
        
        for r in 1..=30 {
            let mut mask = vec![0u8; r * r];
            let mut index = 0;
            
            for x in 0..r {
                for y in 0..r {
                    let xx = x as f64 + 0.5 - r as f64 / 2.0;
                    let yy = y as f64 + 0.5 - r as f64 / 2.0;
                    let distance_squared = xx * xx + yy * yy;
                    let radius_squared = (r * r) as f64 / 4.0;
                    
                    mask[index] = if distance_squared <= radius_squared { 1 } else { 0 };
                    index += 1;
                }
            }
            
            self.round_data[r] = mask;
        }
        
        // Apply the specific pixel adjustments from the original code
        if self.round_data.len() > 3 {
            let mask = &mut self.round_data[3];
            if mask.len() > 8 {
                mask[0] = 0;
                mask[2] = 0;
                mask[6] = 0;
                mask[8] = 0;
            }
        }
        
        if self.round_data.len() > 5 {
            let mask = &mut self.round_data[5];
            if mask.len() > 23 {
                mask[1] = 0;
                mask[3] = 0;
                mask[5] = 0;
                mask[9] = 0;
                mask[15] = 0;
                mask[19] = 0;
                mask[21] = 0;
                mask[23] = 0;
            }
        }
    }
    
    fn load_arial_font() -> Option<FontRef<'static>> {
        // Try to load Arial from the system
        let source = SystemSource::new();
        
        // Try Arial first, then fallback to other sans-serif fonts
        let family_names = [
            FamilyName::Title("Arial".into()),
            FamilyName::SansSerif,
        ];
        
        for family_name in &family_names {
            if let Ok(handle) = source.select_best_match(&[family_name.clone()], &Properties::new()) {
                if let Ok(font_data) = handle.load() {
                    // Convert font data to static lifetime by leaking it
                    // This is acceptable since we only load one font for the entire program
                    if let Some(data_vec) = font_data.copy_font_data() {
                        let font_bytes: &'static [u8] = Box::leak((*data_vec).clone().into_boxed_slice());
                        if let Ok(font) = FontRef::try_from_slice(font_bytes) {
                            return Some(font);
                        }
                    }
                }
            }
        }
        
        None
    }

    fn init_tone_data(&mut self) {
        // Initialize 4x4 dithering patterns (16 levels)
        // Pattern from original JavaScript: [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5]
        let pattern = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
        
        self.tone_data = Vec::with_capacity(16);
        
        for i in 0..16 {
            let mut tone_pattern = vec![0u8; 16]; // 4x4 = 16 pixels
            
            for j in 0..16 {
                tone_pattern[j] = if i >= pattern[j] { 1 } else { 0 };
            }
            
            self.tone_data.push(tone_pattern);
        }
    }

    fn get_tone_data(&self, alpha: u8) -> &Vec<u8> {
        // Alpha table from original JavaScript
        let alpha_table = [23, 47, 69, 92, 114, 114, 114, 138, 161, 184, 184, 207, 230, 230, 253];
        
        for i in 0..alpha_table.len() {
            if alpha < alpha_table[i] {
                return &self.tone_data[i];
            }
        }
        
        // Return last pattern if alpha is >= all thresholds
        &self.tone_data[alpha_table.len()]
    }

    pub fn get_alpha(&mut self, alpha_type: AlphaType) -> f64 {
        let mut a1 = self.state.current_color.a as f64 / 255.0;
        
        match alpha_type {
            AlphaType::Pen => {
                if a1 > 0.5 {
                    a1 = 1.0 / 16.0 + ((a1 - 0.5) * 30.0) / 16.0;
                } else {
                    a1 = (2.0_f64 * a1).sqrt() / 16.0;
                }
                a1 = a1.min(1.0).max(0.0);
            }
            AlphaType::Fill => {
                a1 = -0.00056 * a1 + 0.0042 / (1.0 - a1) - 0.0042;
                a1 = (a1 * 10.0).min(1.0).max(0.0);
            }
            AlphaType::Brush => {
                a1 = -0.00056 * a1 + 0.0042 / (1.0 - a1) - 0.0042;
                a1 = a1.min(1.0).max(0.0);
            }
        }
        
        // Alpha error accumulation for very small alphas
        if a1 < 1.0 / 255.0 {
            self.state.aerr += a1;
            a1 = 0.0;
            while self.state.aerr > 1.0 / 255.0 {
                a1 = 1.0 / 255.0;
                self.state.aerr -= 1.0 / 255.0;
            }
        }
        
        a1
    }

    pub fn render_frame_by_frame(&mut self, pch: &PchFile) -> Result<Vec<FrameSet>> {
        let mut frames = Vec::new();
        
        // Clear canvas initially
        self.canvas.clear();
        frames.push(FrameSet {
            layer0: self.canvas.get_layer_as_rgb(0).unwrap(),
            layer1: self.canvas.get_layer_as_rgb(1).unwrap(),
            composite: self.canvas.composite(),
        });

        for action in &pch.actions {
            self.execute_action(action)?;
            frames.push(FrameSet {
                layer0: self.canvas.get_layer_as_rgb(0).unwrap(),
                layer1: self.canvas.get_layer_as_rgb(1).unwrap(),
                composite: self.canvas.composite(),
            });
        }

        Ok(frames)
    }

    fn execute_action(&mut self, action: &[ActionValue]) -> Result<()> {
        if action.is_empty() {
            return Ok(());
        }

        let command = match &action[0] {
            ActionValue::String(s) => s.as_str(),
            _ => return Ok(()), // Skip non-string commands
        };

        match command {
            "clearCanvas" => self.clear_canvas(),
            "eraseAll" => self.erase_all(action)?,
            "freeHand" => self.free_hand(action)?,
            "line" => self.draw_line(action)?,
            "bezier" => self.draw_bezier(action)?,
            "fill" => self.fill(action)?,
            "floodFill" => self.flood_fill(action)?,
            "text" => self.draw_text(action)?,
            "copy" => self.copy(action)?,
            "paste" => self.paste(action)?,
            "merge" => self.merge(action)?,
            "restore" => self.restore(action)?,
            _ => {
                // Unknown command, skip
                println!("Unknown command: {}", command);
            }
        }

        Ok(())
    }

    fn clear_canvas(&mut self) {
        self.canvas.clear();
    }

    fn erase_all(&mut self, action: &[ActionValue]) -> Result<()> {
        if action.len() >= 2 {
            if let ActionValue::Number(layer) = action[1] {
                self.canvas.clear_layer(layer as usize);
            }
        }
        Ok(())
    }

    fn free_hand(&mut self, action: &[ActionValue]) -> Result<()> {
        if action.len() < 12 {
            return Ok(());
        }

        // Parse layer and drawing state
        let layer = match action[1] {
            ActionValue::Number(n) => n as usize,
            _ => return Ok(()),
        };

        if layer >= 2 {
            return Ok(());
        }

        // Update drawing state from action
        self.update_drawing_state_from_action(action);

        // Parse line type and coordinates
        let line_type = match action.get(11) {
            Some(ActionValue::Number(n)) => LineType::from(*n as i64),
            _ => LineType::Pen,
        };

        // Draw points from action data
        let mut i = 12;
        while i + 3 < action.len() {
            let x0 = self.get_number(&action[i])?;
            let y0 = self.get_number(&action[i + 1])?;
            let x1 = self.get_number(&action[i + 2])?;
            let y1 = self.get_number(&action[i + 3])?;

            self.draw_line_segment(layer, x0 as u32, y0 as u32, x1 as u32, y1 as u32, &line_type);
            i += 2;
        }

        Ok(())
    }

    fn draw_line(&mut self, action: &[ActionValue]) -> Result<()> {
        if action.len() < 16 {
            return Ok(());
        }

        let layer = match action[1] {
            ActionValue::Number(n) => n as usize,
            _ => return Ok(()),
        };

        if layer >= 2 {
            return Ok(());
        }

        self.update_drawing_state_from_action(action);

        let line_type = match action.get(11) {
            Some(ActionValue::Number(n)) => LineType::from(*n as i64),
            _ => LineType::Pen,
        };

        let x0 = self.get_number(&action[12])? as u32;
        let y0 = self.get_number(&action[13])? as u32;
        let x1 = self.get_number(&action[14])? as u32;
        let y1 = self.get_number(&action[15])? as u32;

        self.draw_line_segment(layer, x0, y0, x1, y1, &line_type);
        Ok(())
    }

    fn draw_bezier(&mut self, _action: &[ActionValue]) -> Result<()> {
        // Simplified bezier - just skip for now
        println!("Bezier curves not implemented yet");
        Ok(())
    }

    fn fill(&mut self, action: &[ActionValue]) -> Result<()> {
        if action.len() < 16 {
            return Err(anyhow::anyhow!("Fill action requires at least 16 parameters"));
        }

        let layer = match action[1] {
            ActionValue::Number(n) => n as usize,
            _ => return Ok(()),
        };

        // Update current state from action parameters (indices 2-10)
        if let (Ok(r), Ok(g), Ok(b), Ok(a)) = (
            self.get_number(&action[2]),
            self.get_number(&action[3]),
            self.get_number(&action[4]),
            self.get_number(&action[5]),
        ) {
            self.state.current_color = Color {
                r: r as u8,
                g: g as u8,
                b: b as u8,
                a: a as u8,
            };
        }

        if let (Ok(mr), Ok(mg), Ok(mb)) = (
            self.get_number(&action[6]),
            self.get_number(&action[7]),
            self.get_number(&action[8]),
        ) {
            self.state.current_mask = Color {
                r: mr as u8,
                g: mg as u8,
                b: mb as u8,
                a: 255,
            };
        }

        if let Ok(width) = self.get_number(&action[9]) {
            self.state.current_width = width;
        }

        if let Ok(mask_type) = self.get_number(&action[10]) {
            self.state.current_mask_type = MaskType::from(mask_type as i64);
        }

        // Skip color/mask parameters (indices 2-10)
        let x = self.get_number(&action[11])? as u32;
        let y = self.get_number(&action[12])? as u32;
        let width = self.get_number(&action[13])? as u32;
        let height = self.get_number(&action[14])? as u32;
        let fill_type = self.get_number(&action[15])? as u32;

        self.do_fill(layer, x, y, width, height, fill_type)
    }

    fn flood_fill(&mut self, action: &[ActionValue]) -> Result<()> {
        if action.len() < 5 {
            return Err(anyhow::anyhow!("Flood fill action requires at least 5 parameters"));
        }

        let layer = match action[1] {
            ActionValue::Number(n) => n as usize,
            _ => return Ok(()),
        };

        let x = self.get_number(&action[2])? as i32;
        let y = self.get_number(&action[3])? as i32;
        let fill_color = self.get_number(&action[4])? as u32;

        self.do_flood_fill(layer, x, y, fill_color)
    }

    fn draw_text(&mut self, action: &[ActionValue]) -> Result<()> {
        // Text action format: ["text", layer, x, y, color, alpha, string, size, family]
        if action.len() < 9 {
            return Ok(());
        }

        let layer = match action[1] {
            ActionValue::Number(n) => n as usize,
            _ => return Ok(()),
        };

        if layer >= 2 {
            return Ok(());
        }

        let x = self.get_number(&action[2])? as u32;
        let y = self.get_number(&action[3])? as u32;
        let color = self.get_number(&action[4])? as u32;
        let alpha = self.get_number(&action[5])? as f64;
        
        let text = match &action[6] {
            ActionValue::String(s) => s.clone(),
            _ => return Ok(()),
        };
        
        let size = self.parse_font_size(&action[7])? as u32;
        
        // Use Arial font if available, otherwise fallback to bitmap
        if let Some(font) = self.arial_font.clone() {
            self.draw_arial_text(layer, x, y, &text, color, alpha, size, font);
        } else {
            self.draw_simple_text(layer, x, y, &text, color, alpha, size);
        }
        
        Ok(())
    }

    fn copy(&mut self, action: &[ActionValue]) -> Result<()> {
        if action.len() < 6 {
            return Ok(());
        }

        let layer = match action[1] {
            ActionValue::Number(n) => n as usize,
            _ => return Ok(()),
        };

        let x = self.get_number(&action[2])? as u32;
        let y = self.get_number(&action[3])? as u32;
        let width = self.get_number(&action[4])? as u32;
        let height = self.get_number(&action[5])? as u32;

        self.do_copy(layer, x, y, width, height)
    }

    fn paste(&mut self, action: &[ActionValue]) -> Result<()> {
        if action.len() < 8 {
            return Ok(());
        }

        let layer = match action[1] {
            ActionValue::Number(n) => n as usize,
            _ => return Ok(()),
        };

        let x = self.get_number(&action[2])? as u32;
        let y = self.get_number(&action[3])? as u32;
        let width = self.get_number(&action[4])? as u32;
        let height = self.get_number(&action[5])? as u32;
        let dx = self.get_number(&action[6])? as i32;
        let dy = self.get_number(&action[7])? as i32;

        self.do_paste(layer, x, y, width, height, dx, dy)
    }

    fn merge(&mut self, action: &[ActionValue]) -> Result<()> {
        if action.len() < 6 {
            return Ok(());
        }

        let layer = match action[1] {
            ActionValue::Number(n) => n as usize,
            _ => return Ok(()),
        };

        let x = self.get_number(&action[2])? as u32;
        let y = self.get_number(&action[3])? as u32;
        let width = self.get_number(&action[4])? as u32;
        let height = self.get_number(&action[5])? as u32;

        self.do_merge(layer, x, y, width, height)
    }

    fn restore(&mut self, _action: &[ActionValue]) -> Result<()> {
        println!("Restore not implemented yet");
        Ok(())
    }

    fn update_drawing_state_from_action(&mut self, action: &[ActionValue]) {
        // Extract color, mask, width, and mask type from action (indices 2-10)
        if action.len() >= 11 {
            if let (Ok(r), Ok(g), Ok(b), Ok(a)) = (
                self.get_number(&action[2]),
                self.get_number(&action[3]),
                self.get_number(&action[4]),
                self.get_number(&action[5]),
            ) {
                self.state.current_color = Color {
                    r: r as u8,
                    g: g as u8,
                    b: b as u8,
                    a: a as u8,
                };
            }

            if let (Ok(r), Ok(g), Ok(b)) = (
                self.get_number(&action[6]),
                self.get_number(&action[7]),
                self.get_number(&action[8]),
            ) {
                self.state.current_mask = Color {
                    r: r as u8,
                    g: g as u8,
                    b: b as u8,
                    a: 255,
                };
            }

            if let Ok(width) = self.get_number(&action[9]) {
                self.state.current_width = width;
            }

            if let Ok(mask_type) = self.get_number(&action[10]) {
                self.state.current_mask_type = MaskType::from(mask_type as i64);
            }
        }
    }

    fn draw_line_segment(&mut self, layer: usize, x0: u32, y0: u32, x1: u32, y1: u32, line_type: &LineType) {
        // Simple line drawing using Bresenham's algorithm
        let mut curr_x = x0 as i32;
        let mut curr_y = y0 as i32;
        let end_x = x1 as i32;
        let end_y = y1 as i32;
        let stroke_x = x0; // Store original stroke start coordinates
        let stroke_y = y0;

        let dx = (end_x - curr_x).abs();
        let dy = (end_y - curr_y).abs();
        let sx = if curr_x < end_x { 1 } else { -1 };
        let sy = if curr_y < end_y { 1 } else { -1 };
        let mut err = dx - dy;

        loop {
            if curr_x >= 0 && curr_y >= 0 && curr_x < self.canvas.width as i32 && curr_y < self.canvas.height as i32 {
                self.draw_point_with_origin(layer, curr_x as u32, curr_y as u32, stroke_x, stroke_y, line_type);
            }

            if curr_x == end_x && curr_y == end_y {
                break;
            }

            let e2 = 2 * err;
            if e2 > -dy {
                err -= dy;
                curr_x += sx;
            }
            if e2 < dx {
                err += dx;
                curr_y += sy;
            }
        }
    }

    fn draw_point(&mut self, layer: usize, x: u32, y: u32, line_type: &LineType) {
        // For backward compatibility, use current point as stroke origin
        self.draw_point_with_origin(layer, x, y, x, y, line_type);
    }
    
    pub fn draw_point_with_origin(&mut self, layer: usize, x: u32, y: u32, x0: u32, y0: u32, line_type: &LineType) {
        match line_type {
            LineType::Pen => self.set_pen_point(layer, x, y),
            LineType::Brush => self.set_brush_point(layer, x, y),
            LineType::Tone => self.set_tone_point(layer, x, y, x0, y0),
            LineType::Eraser => self.set_eraser_point(layer, x, y),
            _ => {
                // For other line types, use pen for now
                self.set_pen_point(layer, x, y);
            }
        }
    }
    
    fn set_pen_point(&mut self, layer: usize, x: u32, y: u32) {
        let d = self.state.current_width as usize;
        let d = d.clamp(1, 30);
        let r = (d as f64 / 2.0).floor() as usize;
        
        if d >= self.round_data.len() || self.round_data[d].is_empty() {
            return;
        }
        
        let start_x = x as i32 - r as i32;
        let start_y = y as i32 - r as i32;
        
        let r1 = self.state.current_color.r as f64;
        let g1 = self.state.current_color.g as f64;
        let b1 = self.state.current_color.b as f64;
        let a1 = self.get_alpha(AlphaType::Pen);
        
        let shape = self.round_data[d].clone();
        let mut shape_index = 0;
        
        if a1 == 0.0 {
            return;
        }
        
        for i in 0..d {
            for j in 0..d {
                if shape_index < shape.len() && shape[shape_index] == 1 {
                    let pixel_x = start_x + j as i32;
                    let pixel_y = start_y + i as i32;
                    
                    if pixel_x >= 0 && pixel_y >= 0 && 
                       (pixel_x as u32) < self.canvas.width && (pixel_y as u32) < self.canvas.height {
                        
                        let current_pixel = self.canvas.layers[layer].get_pixel(pixel_x as u32, pixel_y as u32);
                        let r0 = current_pixel.0[0] as f64;
                        let g0 = current_pixel.0[1] as f64;
                        let b0 = current_pixel.0[2] as f64;
                        let a0 = current_pixel.0[3] as f64 / 255.0;
                        
                        // Alpha blending calculation from setPenPoint
                        let a = a0 + a1 - a0 * a1;
                        let (r, g, b) = if a > 0.0 {
                            let a1x = a1.max(1.0 / 255.0);
                            
                            let r = (r1 * a1x + r0 * a0 * (1.0 - a1x)) / a;
                            let g = (g1 * a1x + g0 * a0 * (1.0 - a1x)) / a;
                            let b = (b1 * a1x + b0 * a0 * (1.0 - a1x)) / a;
                            
                            let r = if r1 > r0 { r.ceil() } else { r.floor() };
                            let g = if g1 > g0 { g.ceil() } else { g.floor() };
                            let b = if b1 > b0 { b.ceil() } else { b.floor() };
                            
                            (r, g, b)
                        } else {
                            (r0, g0, b0)
                        };
                        
                        let final_alpha = (a * 255.0).ceil().min(255.0) as u8;
                        let final_r = r.clamp(0.0, 255.0) as u8;
                        let final_g = g.clamp(0.0, 255.0) as u8;
                        let final_b = b.clamp(0.0, 255.0) as u8;
                        
                        self.canvas.layers[layer].put_pixel(
                            pixel_x as u32, 
                            pixel_y as u32, 
                            Rgba([final_r, final_g, final_b, final_alpha])
                        );
                    }
                }
                shape_index += 1;
            }
        }
    }
    
    fn set_brush_point(&mut self, layer: usize, x: u32, y: u32) {
        let d = self.state.current_width as usize;
        let d = d.clamp(1, 30);
        let r = (d as f64 / 2.0).floor() as usize;
        
        if d >= self.round_data.len() || self.round_data[d].is_empty() {
            return;
        }
        
        let start_x = x as i32 - r as i32;
        let start_y = y as i32 - r as i32;
        
        let r1 = self.state.current_color.r as f64;
        let g1 = self.state.current_color.g as f64;
        let b1 = self.state.current_color.b as f64;
        let a1 = self.get_alpha(AlphaType::Brush);
        
        let shape = self.round_data[d].clone();
        let mut shape_index = 0;
        
        if a1 == 0.0 {
            return;
        }
        
        for i in 0..d {
            for j in 0..d {
                if shape_index < shape.len() && shape[shape_index] == 1 {
                    let pixel_x = start_x + j as i32;
                    let pixel_y = start_y + i as i32;
                    
                    if pixel_x >= 0 && pixel_y >= 0 && 
                       (pixel_x as u32) < self.canvas.width && (pixel_y as u32) < self.canvas.height {
                        
                        let current_pixel = self.canvas.layers[layer].get_pixel(pixel_x as u32, pixel_y as u32);
                        let r0 = current_pixel.0[0] as f64;
                        let g0 = current_pixel.0[1] as f64;
                        let b0 = current_pixel.0[2] as f64;
                        let a0 = current_pixel.0[3] as f64 / 255.0;
                        
                        // Alpha blending calculation from setBrushPoint (different formula)
                        let a = a0 + a1 - a0 * a1;
                        let (r, g, b) = if a > 0.0 {
                            let a1x = a1.max(1.0 / 255.0);
                            
                            let r = (r1 * a1x + r0 * a0) / (a0 + a1x);
                            let g = (g1 * a1x + g0 * a0) / (a0 + a1x);
                            let b = (b1 * a1x + b0 * a0) / (a0 + a1x);
                            
                            let r = if r1 > r0 { r.ceil() } else { r.floor() };
                            let g = if g1 > g0 { g.ceil() } else { g.floor() };
                            let b = if b1 > b0 { b.ceil() } else { b.floor() };
                            
                            (r, g, b)
                        } else {
                            (r0, g0, b0)
                        };
                        
                        let final_alpha = (a * 255.0).ceil().min(255.0) as u8;
                        let final_r = r.clamp(0.0, 255.0) as u8;
                        let final_g = g.clamp(0.0, 255.0) as u8;
                        let final_b = b.clamp(0.0, 255.0) as u8;
                        
                        self.canvas.layers[layer].put_pixel(
                            pixel_x as u32, 
                            pixel_y as u32, 
                            Rgba([final_r, final_g, final_b, final_alpha])
                        );
                    }
                }
                shape_index += 1;
            }
        }
    }
    
    fn set_tone_point(&mut self, layer: usize, x: u32, y: u32, _x0: u32, _y0: u32) {
        let d = self.state.current_width as usize;
        let d = d.clamp(1, 30);
        let r = (d as f64 / 2.0).floor() as usize;
        
        if d >= self.round_data.len() || self.round_data[d].is_empty() {
            return;
        }
        
        let start_x = x as i32 - r as i32;
        let start_y = y as i32 - r as i32;
        
        let shape = self.round_data[d].clone();
        let mut shape_index = 0;
        
        let r1 = self.state.current_color.r;
        let g1 = self.state.current_color.g;
        let b1 = self.state.current_color.b;
        let a = self.state.current_color.a;
        
        let tone_data = self.get_tone_data(a).clone();
        
        for i in 0..d {
            for j in 0..d {
                if shape_index < shape.len() && shape[shape_index] == 1 {
                    let pixel_x = start_x + j as i32;
                    let pixel_y = start_y + i as i32;
                    
                    if pixel_x >= 0 && pixel_y >= 0 && 
                       (pixel_x as u32) < self.canvas.width && (pixel_y as u32) < self.canvas.height {
                        
                        // Calculate dithering pattern position based on stroke-relative coordinates
                        // Use original stroke position plus brush offset (like JavaScript)
                        let offset_x = pixel_x - start_x;
                        let offset_y = pixel_y - start_y;
                        let pattern_x = ((x as i32 + offset_x) as usize) % 4;
                        let pattern_y = ((y as i32 + offset_y) as usize) % 4;
                        let pattern_index = pattern_y * 4 + pattern_x;
                        
                        // Apply tone if the dithering pattern allows it
                        if pattern_index < tone_data.len() && tone_data[pattern_index] == 1 {
                            self.canvas.layers[layer].put_pixel(
                                pixel_x as u32, 
                                pixel_y as u32, 
                                Rgba([r1, g1, b1, 255])
                            );
                        }
                    }
                }
                shape_index += 1;
            }
        }
    }
    
    fn set_eraser_point(&mut self, layer: usize, x: u32, y: u32) {
        let d = self.state.current_width as usize;
        let d = d.clamp(1, 30);
        let r = (d as f64 / 2.0).floor() as usize;
        
        if d >= self.round_data.len() || self.round_data[d].is_empty() {
            return;
        }
        
        let start_x = x as i32 - r as i32;
        let start_y = y as i32 - r as i32;
        
        let shape = self.round_data[d].clone();
        let mut shape_index = 0;
        
        for i in 0..d {
            for j in 0..d {
                if shape_index < shape.len() && shape[shape_index] == 1 {
                    let pixel_x = start_x + j as i32;
                    let pixel_y = start_y + i as i32;
                    
                    if pixel_x >= 0 && pixel_y >= 0 && 
                       (pixel_x as u32) < self.canvas.width && (pixel_y as u32) < self.canvas.height {
                        
                        // Eraser sets pixel to transparent
                        self.canvas.layers[layer].put_pixel(
                            pixel_x as u32, 
                            pixel_y as u32, 
                            Rgba([0, 0, 0, 0])
                        );
                    }
                }
                shape_index += 1;
            }
        }
    }
    
    pub fn draw_simple_text(&mut self, layer: usize, x: u32, y: u32, text: &str, color: u32, alpha: f64, size: u32) {
        // Extract RGB from color
        let r = (color & 0xff) as u8;
        let g = ((color & 0xff00) >> 8) as u8;
        let b = ((color & 0xff0000) >> 16) as u8;
        let final_alpha = (alpha * 255.0).clamp(0.0, 255.0) as u8;
        
        // Simple 8x8 bitmap font for basic ASCII characters
        // Each character is represented as an 8x8 bitmap
        let font_data = self.get_simple_font_data();
        
        let char_width = 8;
        let _char_height = 8;
        let scale = (size as f32 / 8.0).max(1.0) as u32;
        
        let mut char_x = x;
        
        for ch in text.chars() {
            if let Some(bitmap) = font_data.get(&ch) {
                self.draw_character_bitmap(layer, char_x, y, bitmap, r, g, b, final_alpha, scale);
            }
            char_x += char_width * scale + 1; // Add 1 pixel spacing between chars
            
            // Stop if we're going off the canvas
            if char_x >= self.canvas.width {
                break;
            }
        }
    }
    
    fn draw_arial_text(&mut self, layer: usize, x: u32, y: u32, text: &str, color: u32, alpha: f64, size: u32, font: FontRef<'static>) {
        // Extract RGB from color
        let r = (color & 0xff) as u8;
        let g = ((color & 0xff00) >> 8) as u8;
        let b = ((color & 0xff0000) >> 16) as u8;
        let final_alpha = (alpha * 255.0).clamp(0.0, 255.0) as u8;
        
        // Create scale for the font
        let scale = PxScale::from(size as f32);
        
        // Calculate text layout manually
        let mut glyphs = Vec::new();
        let mut cursor = point(x as f32, y as f32);
        
        for ch in text.chars() {
            let glyph_id = font.glyph_id(ch);
            let glyph = glyph_id.with_scale_and_position(scale, cursor);
            glyphs.push(glyph);
            
            // Advance cursor
            cursor.x += font.h_advance_unscaled(glyph_id) * scale.x / font.units_per_em().unwrap_or(1000.0);
        }
        
        // Render each glyph
        for glyph in glyphs {
            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                
                // Create a small image for the glyph
                let glyph_width = bounds.width().ceil() as u32;
                let glyph_height = bounds.height().ceil() as u32;
                
                if glyph_width == 0 || glyph_height == 0 {
                    continue;
                }
                
                // Draw the glyph using binary coverage (no antialiasing)
                outlined.draw(|glyph_x, glyph_y, coverage| {
                    let pixel_x = bounds.min.x as i32 + glyph_x as i32;
                    let pixel_y = bounds.min.y as i32 + glyph_y as i32;
                    
                    if pixel_x >= 0 && pixel_y >= 0 && 
                       (pixel_x as u32) < self.canvas.width && (pixel_y as u32) < self.canvas.height {
                        
                        // Binary threshold - only draw if coverage is above 0.5 (no antialiasing)
                        if coverage > 0.5 {
                            // Get current pixel
                            let current_pixel = self.canvas.layers[layer].get_pixel(pixel_x as u32, pixel_y as u32);
                            
                            // Alpha blend with full opacity (no sub-pixel alpha)
                            let alpha_f = final_alpha as f32 / 255.0;
                            let inv_alpha = 1.0 - alpha_f;
                            
                            let new_r = (r as f32 * alpha_f + current_pixel[0] as f32 * inv_alpha) as u8;
                            let new_g = (g as f32 * alpha_f + current_pixel[1] as f32 * inv_alpha) as u8;
                            let new_b = (b as f32 * alpha_f + current_pixel[2] as f32 * inv_alpha) as u8;
                            let new_a = ((final_alpha as f32 + current_pixel[3] as f32 * inv_alpha).min(255.0)) as u8;
                            
                            self.canvas.layers[layer].put_pixel(
                                pixel_x as u32, 
                                pixel_y as u32, 
                                Rgba([new_r, new_g, new_b, new_a])
                            );
                        }
                    }
                });
            }
        }
    }
    
    fn do_fill(&mut self, layer: usize, x: u32, y: u32, width: u32, height: u32, fill_type: u32) -> Result<()> {
        if layer >= self.canvas.layers.len() {
            return Ok(());
        }

        let r1 = self.state.current_color.r;
        let g1 = self.state.current_color.g;
        let b1 = self.state.current_color.b;
        let a1 = self.get_alpha(AlphaType::Fill);

        // Clamp fill area to canvas bounds
        let canvas_width = self.canvas.width;
        let canvas_height = self.canvas.height;
        let end_x = (x + width).min(canvas_width);
        let end_y = (y + height).min(canvas_height);

        for j in y..end_y {
            for i in x..end_x {
                let local_x = i - x;
                let local_y = j - y;
                
                if self.apply_fill_mask(local_x, local_y, width, height, fill_type) {
                    // Get current pixel
                    let current = self.canvas.layers[layer].get_pixel(i, j);
                    let r0 = current[0];
                    let g0 = current[1];
                    let b0 = current[2];
                    let a0 = current[3] as f64 / 255.0;

                    // Apply the same complex alpha blending as in the original
                    let a = a0 + a1 - a0 * a1;

                    let (r, g, b) = if a > 0.0 {
                        let a1x = a1;
                        let ax = 1.0 + a0 * (1.0 - a1x);

                        let r = (r1 as f64 + r0 as f64 * a0 * (1.0 - a1x)) / ax;
                        let g = (g1 as f64 + g0 as f64 * a0 * (1.0 - a1x)) / ax;
                        let b = (b1 as f64 + b0 as f64 * a0 * (1.0 - a1x)) / ax;

                        // Apply ceiling/floor based on comparison like in original
                        let r = if r1 > r0 { r.ceil() } else { r.floor() } as u8;
                        let g = if g1 > g0 { g.ceil() } else { g.floor() } as u8;
                        let b = if b1 > b0 { b.ceil() } else { b.floor() } as u8;

                        (r, g, b)
                    } else {
                        (r0, g0, b0)
                    };

                    let new_alpha = (a * 255.0).ceil() as u8;

                    self.canvas.layers[layer].put_pixel(i, j, Rgba([r, g, b, new_alpha]));
                }
            }
        }

        Ok(())
    }

    fn apply_fill_mask(&self, x: u32, y: u32, width: u32, height: u32, fill_type: u32) -> bool {
        match fill_type {
            20 => self.rect_mask(x, y, width, height),      // TOOLTYPE_RECT
            21 => self.rect_fill_mask(x, y, width, height), // TOOLTYPE_RECTFILL
            22 => self.ellipse_mask(x, y, width, height),   // TOOLTYPE_ELLIPSE
            23 => self.ellipse_fill_mask(x, y, width, height), // TOOLTYPE_ELLIPSEFILL
            _ => false,
        }
    }

    fn rect_fill_mask(&self, _x: u32, _y: u32, _width: u32, _height: u32) -> bool {
        true // Fill entire rectangle
    }

    fn rect_mask(&self, x: u32, y: u32, width: u32, height: u32) -> bool {
        let d = self.state.current_width as u32;
        x < d || x > width.saturating_sub(1 + d) || y < d || y > height.saturating_sub(1 + d)
    }

    fn ellipse_fill_mask(&self, x: u32, y: u32, width: u32, height: u32) -> bool {
        let cx = (width - 1) as f64 / 2.0;
        let cy = (height - 1) as f64 / 2.0;
        let x_norm = (x as f64 - cx) / (cx + 1.0);
        let y_norm = (y as f64 - cy) / (cy + 1.0);

        x_norm * x_norm + y_norm * y_norm < 1.0
    }

    fn ellipse_mask(&self, x: u32, y: u32, width: u32, height: u32) -> bool {
        let d = self.state.current_width;
        let cx = (width - 1) as f64 / 2.0;
        let cy = (height - 1) as f64 / 2.0;

        if cx <= d || cy <= d {
            return self.ellipse_fill_mask(x, y, width, height);
        }

        let x2_norm = (x as f64 - cx) / (cx - d + 1.0);
        let y2_norm = (y as f64 - cy) / (cy - d + 1.0);

        let x_norm = (x as f64 - cx) / (cx + 1.0);
        let y_norm = (y as f64 - cy) / (cy + 1.0);

        if x_norm * x_norm + y_norm * y_norm < 1.0 {
            if x2_norm * x2_norm + y2_norm * y2_norm >= 1.0 {
                return true;
            }
        }
        false
    }

    fn do_flood_fill(&mut self, layer: usize, x: i32, y: i32, fill_color: u32) -> Result<()> {
        if layer >= self.canvas.layers.len() {
            return Ok(());
        }

        // Round coordinates and check bounds
        let x = x as u32;
        let y = y as u32;
        let width = self.canvas.width;
        let height = self.canvas.height;

        if x >= width || y >= height {
            return Ok(());
        }

        // Get base color at the starting point
        let base_pixel = self.canvas.layers[layer].get_pixel(x, y);
        let base_color = pixel_to_u32(&base_pixel);

        // Convert fill_color to RGBA components
        let fill_r = (fill_color & 0xff) as u8;
        let fill_g = ((fill_color & 0xff00) >> 8) as u8;
        let fill_b = ((fill_color & 0xff0000) >> 16) as u8;
        let fill_a = ((fill_color & 0xff000000) >> 24) as u8;

        // Don't fill if the area is already the target color or if base color is fully transparent
        if (base_color & 0xff000000) == 0 || base_color == fill_color {
            return Ok(());
        }

        // Stack-based flood fill algorithm
        let mut stack = Vec::new();
        stack.push((x, y));
        const MAX_STACK_SIZE: usize = 1_000_000;

        while let Some((px, py)) = stack.pop() {
            if stack.len() > MAX_STACK_SIZE {
                break; // Prevent stack overflow like in original
            }

            // Skip if out of bounds
            if px >= width || py >= height {
                continue;
            }

            let current_pixel = self.canvas.layers[layer].get_pixel(px, py);
            let current_color = pixel_to_u32(&current_pixel);

            // Skip if already filled or not the base color
            if current_color == fill_color || current_color != base_color {
                continue;
            }

            // Find horizontal line extent
            let mut x0 = px;
            let mut x1 = px;

            // Extend left
            while x0 > 0 {
                let left_pixel = self.canvas.layers[layer].get_pixel(x0 - 1, py);
                let left_color = pixel_to_u32(&left_pixel);
                if left_color != base_color {
                    break;
                }
                x0 -= 1;
            }

            // Extend right
            while x1 < width - 1 {
                let right_pixel = self.canvas.layers[layer].get_pixel(x1 + 1, py);
                let right_color = pixel_to_u32(&right_pixel);
                if right_color != base_color {
                    break;
                }
                x1 += 1;
            }

            // Fill horizontal line
            for fill_x in x0..=x1 {
                self.canvas.layers[layer].put_pixel(fill_x, py, Rgba([fill_r, fill_g, fill_b, fill_a]));
            }

            // Add adjacent lines to stack
            if py + 1 < height {
                for scan_x in x0..=x1 {
                    stack.push((scan_x, py + 1));
                }
            }
            if py > 0 {
                for scan_x in x0..=x1 {
                    stack.push((scan_x, py - 1));
                }
            }
        }

        Ok(())
    }

    fn do_copy(&mut self, layer: usize, x: u32, y: u32, width: u32, height: u32) -> Result<()> {
        if layer >= self.canvas.layers.len() {
            return Ok(());
        }

        // Clear existing clipboard
        self.clipboard = None;

        // Clamp region to canvas bounds
        let canvas_width = self.canvas.width;
        let canvas_height = self.canvas.height;
        let end_x = (x + width).min(canvas_width);
        let end_y = (y + height).min(canvas_height);

        if x >= canvas_width || y >= canvas_height || end_x <= x || end_y <= y {
            return Ok(()); // Nothing to copy
        }

        let actual_width = end_x - x;
        let actual_height = end_y - y;

        // Copy pixel data to clipboard
        let mut clipboard_data = Vec::with_capacity((actual_width * actual_height) as usize);
        
        for py in y..end_y {
            for px in x..end_x {
                let pixel = self.canvas.layers[layer].get_pixel(px, py);
                let packed = pixel_to_u32(&pixel);
                clipboard_data.push(packed);
            }
        }

        self.clipboard = Some(clipboard_data);
        Ok(())
    }

    fn do_paste(&mut self, layer: usize, x: u32, y: u32, width: u32, height: u32, dx: i32, dy: i32) -> Result<()> {
        if layer >= self.canvas.layers.len() {
            return Ok(());
        }

        let Some(ref clipboard_data) = self.clipboard else {
            return Ok(()); // No data to paste
        };

        // Calculate destination position
        let dest_x = (x as i32 + dx) as u32;
        let dest_y = (y as i32 + dy) as u32;

        // Clamp destination to canvas bounds
        let canvas_width = self.canvas.width;
        let canvas_height = self.canvas.height;
        let end_x = (dest_x + width).min(canvas_width);
        let end_y = (dest_y + height).min(canvas_height);

        if dest_x >= canvas_width || dest_y >= canvas_height || end_x <= dest_x || end_y <= dest_y {
            return Ok(()); // Nothing to paste
        }

        // Paste pixel data
        let mut clipboard_index = 0;
        for py in dest_y..end_y {
            for px in dest_x..end_x {
                if clipboard_index < clipboard_data.len() {
                    let packed_color = clipboard_data[clipboard_index];
                    let r = (packed_color & 0xff) as u8;
                    let g = ((packed_color >> 8) & 0xff) as u8;
                    let b = ((packed_color >> 16) & 0xff) as u8;
                    let a = ((packed_color >> 24) & 0xff) as u8;

                    self.canvas.layers[layer].put_pixel(px, py, Rgba([r, g, b, a]));
                    clipboard_index += 1;
                }
            }
        }

        // Clear clipboard after paste (like original)
        self.clipboard = None;
        Ok(())
    }

    fn do_merge(&mut self, layer: usize, x: u32, y: u32, width: u32, height: u32) -> Result<()> {
        if layer >= self.canvas.layers.len() {
            return Ok(());
        }

        // Clamp region to canvas bounds
        let canvas_width = self.canvas.width;
        let canvas_height = self.canvas.height;
        let end_x = (x + width).min(canvas_width);
        let end_y = (y + height).min(canvas_height);

        if x >= canvas_width || y >= canvas_height || end_x <= x || end_y <= y {
            return Ok(()); // Nothing to merge
        }

        // Determine destination and source layers
        let dst = layer;
        let src = if dst == 1 { 0 } else { 1 };

        // Merge pixels from both layers
        for py in y..end_y {
            for px in x..end_x {
                let pixel0 = self.canvas.layers[0].get_pixel(px, py);
                let pixel1 = self.canvas.layers[1].get_pixel(px, py);

                let r0 = pixel0[0] as f64;
                let g0 = pixel0[1] as f64;
                let b0 = pixel0[2] as f64;
                let a0 = pixel0[3] as f64 / 255.0;

                let r1 = pixel1[0] as f64;
                let g1 = pixel1[1] as f64;
                let b1 = pixel1[2] as f64;
                let a1 = pixel1[3] as f64 / 255.0;

                // Alpha composition like in original
                let a = a0 + a1 - a0 * a1;
                let (r, g, b) = if a > 0.0 {
                    let r = (r1 * a1 + r0 * a0 * (1.0 - a1)) / a;
                    let g = (g1 * a1 + g0 * a0 * (1.0 - a1)) / a;
                    let b = (b1 * a1 + b0 * a0 * (1.0 - a1)) / a;
                    ((r + 0.5) as u8, (g + 0.5) as u8, (b + 0.5) as u8)
                } else {
                    (0, 0, 0)
                };

                // Clear source layer
                self.canvas.layers[src].put_pixel(px, py, Rgba([0, 0, 0, 0]));
                
                // Set merged result in destination layer
                self.canvas.layers[dst].put_pixel(px, py, Rgba([r, g, b, ((a * 255.0 + 0.5) as u8)]));
            }
        }

        Ok(())
    }

    fn draw_character_bitmap(&mut self, layer: usize, x: u32, y: u32, bitmap: &[u8; 8], r: u8, g: u8, b: u8, alpha: u8, scale: u32) {
        for row in 0..8 {
            let byte = bitmap[row];
            for col in 0..8 {
                if (byte >> (7 - col)) & 1 == 1 {
                    // Draw scaled pixel
                    for sy in 0..scale {
                        for sx in 0..scale {
                            let px = x + col as u32 * scale + sx;
                            let py = y + row as u32 * scale + sy;
                            
                            if px < self.canvas.width && py < self.canvas.height {
                                self.canvas.layers[layer].put_pixel(px, py, Rgba([r, g, b, alpha]));
                            }
                        }
                    }
                }
            }
        }
    }
    
    fn get_simple_font_data(&self) -> std::collections::HashMap<char, [u8; 8]> {
        use std::collections::HashMap;
        let mut font = HashMap::new();
        
        // Basic 8x8 font data for common characters
        font.insert(' ', [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        font.insert('A', [0x18, 0x24, 0x42, 0x42, 0x7E, 0x42, 0x42, 0x00]);
        font.insert('B', [0x7C, 0x42, 0x42, 0x7C, 0x42, 0x42, 0x7C, 0x00]);
        font.insert('C', [0x3C, 0x42, 0x40, 0x40, 0x40, 0x42, 0x3C, 0x00]);
        font.insert('D', [0x78, 0x44, 0x42, 0x42, 0x42, 0x44, 0x78, 0x00]);
        font.insert('E', [0x7E, 0x40, 0x40, 0x7C, 0x40, 0x40, 0x7E, 0x00]);
        font.insert('F', [0x7E, 0x40, 0x40, 0x7C, 0x40, 0x40, 0x40, 0x00]);
        font.insert('G', [0x3C, 0x42, 0x40, 0x4E, 0x42, 0x42, 0x3C, 0x00]);
        font.insert('H', [0x42, 0x42, 0x42, 0x7E, 0x42, 0x42, 0x42, 0x00]);
        font.insert('I', [0x3E, 0x08, 0x08, 0x08, 0x08, 0x08, 0x3E, 0x00]);
        font.insert('J', [0x02, 0x02, 0x02, 0x02, 0x02, 0x42, 0x3C, 0x00]);
        font.insert('K', [0x44, 0x48, 0x50, 0x60, 0x50, 0x48, 0x44, 0x00]);
        font.insert('L', [0x40, 0x40, 0x40, 0x40, 0x40, 0x40, 0x7E, 0x00]);
        font.insert('M', [0x42, 0x66, 0x5A, 0x42, 0x42, 0x42, 0x42, 0x00]);
        font.insert('N', [0x42, 0x62, 0x52, 0x4A, 0x46, 0x42, 0x42, 0x00]);
        font.insert('O', [0x3C, 0x42, 0x42, 0x42, 0x42, 0x42, 0x3C, 0x00]);
        font.insert('P', [0x7C, 0x42, 0x42, 0x7C, 0x40, 0x40, 0x40, 0x00]);
        font.insert('Q', [0x3C, 0x42, 0x42, 0x42, 0x4A, 0x44, 0x3A, 0x00]);
        font.insert('R', [0x7C, 0x42, 0x42, 0x7C, 0x48, 0x44, 0x42, 0x00]);
        font.insert('S', [0x3C, 0x42, 0x40, 0x3C, 0x02, 0x42, 0x3C, 0x00]);
        font.insert('T', [0x7F, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x00]);
        font.insert('U', [0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x3C, 0x00]);
        font.insert('V', [0x42, 0x42, 0x42, 0x42, 0x24, 0x18, 0x18, 0x00]);
        font.insert('W', [0x42, 0x42, 0x42, 0x42, 0x5A, 0x66, 0x42, 0x00]);
        font.insert('X', [0x42, 0x24, 0x18, 0x18, 0x24, 0x42, 0x42, 0x00]);
        font.insert('Y', [0x41, 0x22, 0x14, 0x08, 0x08, 0x08, 0x08, 0x00]);
        font.insert('Z', [0x7E, 0x04, 0x08, 0x10, 0x20, 0x40, 0x7E, 0x00]);
        
        // Numbers
        font.insert('0', [0x3C, 0x46, 0x4A, 0x52, 0x62, 0x62, 0x3C, 0x00]);
        font.insert('1', [0x18, 0x28, 0x08, 0x08, 0x08, 0x08, 0x3E, 0x00]);
        font.insert('2', [0x3C, 0x42, 0x02, 0x0C, 0x30, 0x40, 0x7E, 0x00]);
        font.insert('3', [0x3C, 0x42, 0x02, 0x1C, 0x02, 0x42, 0x3C, 0x00]);
        font.insert('4', [0x08, 0x18, 0x28, 0x48, 0x7E, 0x08, 0x08, 0x00]);
        font.insert('5', [0x7E, 0x40, 0x7C, 0x02, 0x02, 0x42, 0x3C, 0x00]);
        font.insert('6', [0x3C, 0x40, 0x40, 0x7C, 0x42, 0x42, 0x3C, 0x00]);
        font.insert('7', [0x7E, 0x02, 0x04, 0x08, 0x10, 0x20, 0x20, 0x00]);
        font.insert('8', [0x3C, 0x42, 0x42, 0x3C, 0x42, 0x42, 0x3C, 0x00]);
        font.insert('9', [0x3C, 0x42, 0x42, 0x3E, 0x02, 0x02, 0x3C, 0x00]);
        
        // Some lowercase letters
        font.insert('a', [0x00, 0x00, 0x3C, 0x02, 0x3E, 0x42, 0x3E, 0x00]);
        font.insert('e', [0x00, 0x00, 0x3C, 0x42, 0x7E, 0x40, 0x3C, 0x00]);
        font.insert('i', [0x08, 0x00, 0x18, 0x08, 0x08, 0x08, 0x1C, 0x00]);
        font.insert('l', [0x30, 0x10, 0x10, 0x10, 0x10, 0x10, 0x38, 0x00]);
        font.insert('o', [0x00, 0x00, 0x3C, 0x42, 0x42, 0x42, 0x3C, 0x00]);
        font.insert('r', [0x00, 0x00, 0x5C, 0x62, 0x40, 0x40, 0x40, 0x00]);
        font.insert('s', [0x00, 0x00, 0x3E, 0x40, 0x3C, 0x02, 0x7C, 0x00]);
        font.insert('t', [0x10, 0x10, 0x7C, 0x10, 0x10, 0x12, 0x0C, 0x00]);
        font.insert('u', [0x00, 0x00, 0x42, 0x42, 0x42, 0x46, 0x3A, 0x00]);
        
        // Basic punctuation
        font.insert('.', [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x00]);
        font.insert(',', [0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x18, 0x30]);
        font.insert('!', [0x18, 0x18, 0x18, 0x18, 0x00, 0x00, 0x18, 0x00]);
        font.insert('?', [0x3C, 0x42, 0x04, 0x08, 0x08, 0x00, 0x08, 0x00]);
        
        font
    }

    fn get_number(&self, value: &ActionValue) -> Result<f64> {
        match value {
            ActionValue::Number(n) => Ok(*n),
            ActionValue::Integer(i) => Ok(*i as f64),
            _ => {
                eprintln!("Expected number but got: {:?}", value);
                bail!("Expected number")
            },
        }
    }
    
    fn parse_font_size(&self, value: &ActionValue) -> Result<f64> {
        match value {
            ActionValue::Number(n) => Ok(*n),
            ActionValue::Integer(i) => Ok(*i as f64),
            ActionValue::String(s) => {
                // Parse font size strings like "27px", "16pt", etc.
                let size_str = s.trim_end_matches("px")
                    .trim_end_matches("pt")
                    .trim_end_matches("em")
                    .trim();
                
                size_str.parse::<f64>().or_else(|_| {
                    eprintln!("Could not parse font size: {}", s);
                    Ok(12.0) // Default font size
                })
            }
        }
    }
}

fn pixel_to_u32(pixel: &Rgba<u8>) -> u32 {
    ((pixel[3] as u32) << 24) | // Alpha
    ((pixel[2] as u32) << 16) | // Blue  
    ((pixel[1] as u32) << 8) |  // Green
    (pixel[0] as u32)           // Red
}