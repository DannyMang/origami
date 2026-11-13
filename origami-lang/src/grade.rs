use serde::{Deserialize, Serialize};

type Points = Vec<(f64, f64)>;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Grade {
    pub input_lut: Option<String>,
    pub temperature: f64,
    pub tint: f64,
    pub exposure: f64,
    pub contrast: f64,
    pub highlights: f64,
    pub shadows: f64,
    pub whites: f64,
    pub blacks: f64,
    pub saturation: Option<f64>,
    pub look: Option<(String, f64)>,
    pub faded_film: f64,
    pub vibrance: f64,
    pub shadow_tint: (f64, f64),
    pub highlight_tint: (f64, f64),
    pub tint_balance: f64,
    pub curve: Curves,
    pub hue_vs_sat: Points,
    pub hue_vs_hue: Points,
    pub hue_vs_luma: Points,
    pub luma_vs_sat: Points,
    pub sat_vs_sat: Points,
    pub wheel: Wheels,
    pub hsl_secondary: Option<Secondary>,
    pub vignette: Option<Vignette>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Curves {
    pub luma: Points,
    pub red: Points,
    pub green: Points,
    pub blue: Points,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Wheels {
    pub shadows: (f64, f64, f64),
    pub midtones: (f64, f64, f64),
    pub highlights: (f64, f64, f64),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Secondary {
    pub color: String,
    pub range: f64,
    pub soft: f64,
    pub temperature: f64,
    pub tint: f64,
    pub contrast: f64,
    pub saturation: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vignette {
    pub amount: f64,
    pub midpoint: f64,
    pub roundness: f64,
    pub feather: f64,
}
