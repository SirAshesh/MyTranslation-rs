use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct TranslateForm {
    #[serde(default)]
    pub src: String,
    #[serde(rename = "sourceLang", default = "default_source_lang")]
    pub source_lang: String,
    #[serde(rename = "targetLang", default = "default_target_lang")]
    pub target_lang: String,
    #[serde(rename = "withoutImage", default)]
    pub without_image: Option<String>,
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    #[serde(rename = "saveToFile", default)]
    pub save_to_file: Option<String>,
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub callback: Option<String>,
}

pub fn default_source_lang() -> String {
    "en".to_string()
}

pub fn default_target_lang() -> String {
    "id".to_string()
}

#[derive(Debug, Deserialize)]
pub struct TranslateRegionForm {
    #[serde(default)]
    pub base64: String,
    #[serde(rename = "sourceLang", default = "default_source_lang")]
    pub source_lang: String,
    #[serde(rename = "targetLang", default = "default_target_lang")]
    pub target_lang: String,
    #[serde(default)]
    pub callback: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TranslateResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub img: Option<String>,
    #[serde(rename = "imgMap", skip_serializing_if = "Option::is_none")]
    pub img_map: Option<ImgMap>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ImgMap {
    pub boxes: Vec<BoxItem>,
}

#[derive(Debug, Serialize, Clone)]
pub struct BoxItem {
    pub text: String,
    pub target: String,
    pub geometry: Geometry,
    #[serde(rename = "targetGeometry")]
    pub target_geometry: Geometry,
}

#[derive(Debug, Serialize, Clone)]
pub struct Geometry {
    #[serde(rename = "X")]
    pub x: i32,
    #[serde(rename = "Y")]
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Serialize)]
pub struct RegionMapResponse {
    pub success: bool,
    #[serde(rename = "regionMap")]
    pub region_map: RegionMapData,
}

#[derive(Debug, Serialize)]
pub struct RegionMapData {
    pub source: String,
    pub target: Vec<TargetEngine>,
}

#[derive(Debug, Serialize)]
pub struct TargetEngine {
    pub engine: String,
    pub text: String,
}

#[derive(Debug, Serialize)]
pub struct ListInstance {
    pub name: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    pub running: bool,
}

#[derive(Debug, Deserialize)]
pub struct BatchRequest {
    pub images: Vec<String>,
    #[serde(rename = "sourceLang", default = "default_source_lang")]
    pub source_lang: String,
    #[serde(rename = "targetLang", default = "default_target_lang")]
    pub target_lang: String,
}

#[derive(Debug, Serialize)]
pub struct BatchResponse {
    pub success: bool,
    pub total: usize,
    pub elapsed_sec: f64,
    pub results: Vec<TranslateResponse>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OcrLineItem {
    pub r#box: [[f32; 2]; 4],
    pub text: String,
    pub score: f32,
}

#[derive(Debug, Clone)]
pub struct LineInfo {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub cx: f32,
    pub cy: f32,
    pub w: f32,
    pub h: f32,
    pub text: String,
    pub score: f32,
}

#[derive(Debug, Clone)]
pub struct Bubble {
    pub id: usize,
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub w: f32,
    pub h: f32,
    pub source_text: String,
    pub lines: Vec<LineInfo>,
}
