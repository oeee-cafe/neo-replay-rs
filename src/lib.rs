pub mod renderer;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ActionValue {
    String(String),
    Number(f64),
    Integer(i64),
}

#[derive(Debug, Clone)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Debug, Clone)]
pub struct PchHeader {
    pub magic: [u8; 4], // "NEO "
    pub width: u16,
    pub height: u16,
    pub reserved: [u8; 4],
}

#[derive(Debug, Clone)]
pub struct PchFile {
    pub header: PchHeader,
    pub actions: Vec<Vec<ActionValue>>,
}

#[derive(Debug, Clone)]
pub enum LineType {
    None = 0,
    Pen = 1,
    Eraser = 2,
    Brush = 3,
    Tone = 4,
    Dodge = 5,
    Burn = 6,
    Blur = 7,
}

#[derive(Debug, Clone, Copy)]
pub enum AlphaType {
    Pen,
    Brush,
    Fill,
}

#[derive(Debug, Clone)]
pub enum MaskType {
    None = 0,
    Normal = 1,
    Reverse = 2,
    Add = 3,
    Sub = 4,
}

#[derive(Debug, Clone, Copy)]
pub enum FillType {
    Rect = 20,
    RectFill = 21,
    Ellipse = 22,
    EllipseFill = 23,
}

#[derive(Debug, Clone)]
pub struct DrawingState {
    pub current_color: Color,
    pub current_mask: Color,
    pub current_width: f64,
    pub current_mask_type: MaskType,
    pub aerr: f64, // For alpha error accumulation
}

impl PchFile {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let data = fs::read(path)?;
        Self::from_bytes(&data)
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 12 {
            bail!("PCH file too short");
        }

        // Parse header
        let header = PchHeader {
            magic: [data[0], data[1], data[2], data[3]],
            width: u16::from_le_bytes([data[4], data[5]]),
            height: u16::from_le_bytes([data[6], data[7]]),
            reserved: [data[8], data[9], data[10], data[11]],
        };

        // Verify magic
        if &header.magic != b"NEO " {
            bail!("Invalid PCH file magic");
        }

        // Decompress data using lz_str
        let compressed = &data[12..];
        let decompressed = lz_str::decompress_from_uint8_array(compressed)
            .ok_or_else(|| anyhow::anyhow!("Failed to decompress PCH data"))?;

        // Convert Vec<u16> to String - properly handle Unicode
        let decompressed_string: String = decompressed.into_iter()
            .filter_map(|c| std::char::from_u32(c as u32))
            .collect();
        
        // Parse JSON
        let actions: Vec<Vec<ActionValue>> = serde_json::from_str(&decompressed_string)?;

        Ok(PchFile { header, actions })
    }

    pub fn fix_actions(&mut self) {
        // Fix eraseAll actions as per original JavaScript logic
        let mut i = 0;
        while i < self.actions.len() {
            let action = &self.actions[i];
            
            // Find "eraseAll" in the action
            let mut erase_all_index = None;
            for (idx, value) in action.iter().enumerate() {
                if let ActionValue::String(s) = value {
                    if s == "eraseAll" && idx > 0 {
                        erase_all_index = Some(idx);
                        break;
                    }
                }
            }

            if let Some(index) = erase_all_index {
                let before = action[..index].to_vec();
                let after = action[index..].to_vec();
                
                self.actions[i] = before;
                self.actions.insert(i, after);
                i += 1; // Skip the newly inserted action
            }
            i += 1;
        }
    }
}

impl Default for DrawingState {
    fn default() -> Self {
        Self {
            current_color: Color { r: 0, g: 0, b: 0, a: 255 },
            current_mask: Color { r: 0, g: 0, b: 0, a: 0 },
            current_width: 1.0,
            current_mask_type: MaskType::None,
            aerr: 0.0,
        }
    }
}

impl From<i64> for LineType {
    fn from(value: i64) -> Self {
        match value {
            1 => LineType::Pen,
            2 => LineType::Eraser,
            3 => LineType::Brush,
            4 => LineType::Tone,
            5 => LineType::Dodge,
            6 => LineType::Burn,
            7 => LineType::Blur,
            _ => LineType::None,
        }
    }
}

impl From<i64> for MaskType {
    fn from(value: i64) -> Self {
        match value {
            1 => MaskType::Normal,
            2 => MaskType::Reverse,
            3 => MaskType::Add,
            4 => MaskType::Sub,
            _ => MaskType::None,
        }
    }
}