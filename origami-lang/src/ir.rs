use crate::{Grade, KEPT, LINE};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, DeserializeSeed, Error as _, SeqAccess, Visitor},
};
use std::{fmt, mem, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Video,
    Sound,
    Image,
    Text,
    Color,
    Rect,
    Line,
    Path,
    Group,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Val {
    Fixed(f64),
    Frames(u32, Vec<f32>),
}

struct Shape;

impl<'a> Visitor<'a> for Shape {
    type Value = Val;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a number or per-frame numbers")
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Val, E> {
        Ok(Val::Fixed(v as f64))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Val, E> {
        Ok(Val::Fixed(v as f64))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Val, E> {
        Ok(Val::Fixed(v))
    }

    fn visit_seq<A: SeqAccess<'a>>(self, mut s: A) -> Result<Val, A::Error> {
        match (s.next_element()?, s.next_element_seed(Run)?) {
            (Some(line), Some(run)) if !run.is_empty() => Ok(Val::Frames(line, run)),
            _ => Err(A::Error::custom("no such run")),
        }
    }
}

impl<'a> Deserialize<'a> for Val {
    fn deserialize<D: Deserializer<'a>>(d: D) -> Result<Val, D::Error> {
        d.deserialize_any(Shape)
    }
}

struct Run;

impl<'a> Visitor<'a> for Run {
    type Value = Vec<f32>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("per-frame numbers")
    }

    fn visit_u64<E: de::Error>(self, i: u64) -> Result<Vec<f32>, E> {
        Ok(KEPT.with_borrow_mut(|k| k.get_mut(i as usize).map(mem::take)).unwrap_or_default())
    }

    fn visit_seq<A: SeqAccess<'a>>(self, mut s: A) -> Result<Vec<f32>, A::Error> {
        let mut out = vec![];
        while let Some(v) = s.next_element::<f64>()? {
            out.push(v as f32);
        }
        Ok(out)
    }
}

impl<'a> DeserializeSeed<'a> for Run {
    type Value = Vec<f32>;

    fn deserialize<D: Deserializer<'a>>(self, d: D) -> Result<Vec<f32>, D::Error> {
        d.deserialize_any(self)
    }
}

impl Val {
    pub fn at(&self, i: usize) -> f64 {
        match self {
            Self::Fixed(v) => *v,
            Self::Frames(_, s) => s[i.min(s.len() - 1)] as f64,
        }
    }

    pub fn max(&self) -> f64 {
        match self {
            Self::Fixed(v) => *v,
            Self::Frames(_, s) => s.iter().fold(f32::MIN, |a, b| a.max(*b)) as f64,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Fx {
    Lens(Val),
    Blur(Val),
    Levels(f64, f64, bool),
    Noise(f64, Val),
    Fractal { scale: f64, seed: Val },
    Roughen { border: f64, scale: f64, sharp: f64, seed: Val },
    Displace { amount: Val, size: f64, seed: Val },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub from: usize,
    pub to: usize,
    pub opacity: Val,
    pub x: Val,
    pub y: Val,
    pub scale: Val,
    pub rotate: Val,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub kind: Kind,
    pub src: String,
    pub w: u32,
    pub h: u32,
    pub start: f64,
    pub end: f64,
    pub src_in: f64,
    pub speed: f64,
    pub z: u32,
    pub scale: Val,
    pub pos: (Option<Val>, Option<Val>),
    pub gain: f64,
    pub fade: (f64, f64),
    pub label: String,
    #[serde(deserialize_with = "line")]
    pub line: u32,
    #[serde(default)]
    pub home: u32,
    #[serde(default)]
    pub total: f64,
    pub video: bool,
    pub audio: bool,
    pub size: f64,
    pub color: String,
    #[serde(default)]
    pub font: String,
    #[serde(default)]
    pub radius: f64,
    #[serde(default)]
    pub ends: [f64; 4],
    #[serde(default)]
    pub background: String,
    #[serde(default)]
    pub align: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grade: Option<Grade>,
    pub opacity: Val,
    pub rotate: Val,
    pub shown: Val,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<Part>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub blend: String,
    #[serde(default)]
    pub step: f64,
    #[serde(default)]
    pub id: u32,
    #[serde(default)]
    pub into: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matte: Option<(u32, String)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fx: Vec<Fx>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<(Val, Val)>,
    #[serde(default)]
    pub curve: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub native: bool,
}

impl Clip {
    pub fn vals(&self) -> impl Iterator<Item = (&'static str, &Val)> {
        [
            ("scale", Some(&self.scale)),
            ("x", self.pos.0.as_ref()),
            ("y", self.pos.1.as_ref()),
            ("opacity", Some(&self.opacity)),
            ("rotate", Some(&self.rotate)),
            ("shown", Some(&self.shown)),
        ]
        .into_iter()
        .filter_map(|(n, v)| Some((n, v?)))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Timeline {
    pub fps: u32,
    pub size: (u32, u32),
    pub duration: f64,
    #[serde(default)]
    pub preview: String,
    #[serde(default)]
    pub proxy: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    pub clips: Vec<Clip>,
    #[serde(skip)]
    pub files: Vec<(u32, PathBuf)>,
}

fn line<'a, D: Deserializer<'a>>(d: D) -> Result<u32, D::Error> {
    let n = u32::deserialize(d)?;
    LINE.set(n);
    Ok(n)
}
