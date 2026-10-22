//! Component definitions (`[[editor.component]]` in the game's `mugon.toml`).

use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    Bool,
    I32,
    F32,
    String,
    Vec2,
    Vec3,
    Color,
    Enum,
    Asset,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FieldDef {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: FieldType,
    #[serde(default)]
    pub default: Option<toml::Value>,
    #[serde(default)]
    pub options: Vec<String>,
    /// Rust enum type used when exporting an `enum` field.
    #[serde(default)]
    pub rust_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ComponentDef {
    pub name: String,
    #[serde(default)]
    pub fields: Vec<FieldDef>,
    /// `[r, g, b]` (0..1; a fourth value is ignored): markers carrying this component are drawn in this colour.
    #[serde(default)]
    pub marker_color: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ComponentRegistry {
    #[serde(default, rename = "component")]
    pub components: Vec<ComponentDef>,
}

impl ComponentRegistry {
    pub fn from_defs(components: Vec<ComponentDef>) -> Self {
        Self { components }
    }

    pub fn get(&self, name: &str) -> Option<&ComponentDef> {
        self.components.iter().find(|c| c.name == name)
    }

    /// Colour for a marker with these components: the first one that defines a valid `marker_color`.
    pub fn marker_color<'a>(&self, names: impl IntoIterator<Item = &'a str>) -> Option<[f32; 3]> {
        names.into_iter().find_map(|name| match self.get(name)?.marker_color.as_deref()? {
            [r, g, b] | [r, g, b, _] => Some([*r, *g, *b]),
            _ => None,
        })
    }
}

impl FieldDef {
    /// The value a new component instance starts with.
    pub fn default_value(&self) -> Value {
        if let Some(d) = &self.default {
            if let Ok(v) = serde_json::to_value(d) {
                return v;
            }
        }
        match self.ty {
            FieldType::Bool => json!(false),
            FieldType::I32 => json!(0),
            FieldType::F32 => json!(0.0),
            FieldType::String | FieldType::Asset => json!(""),
            FieldType::Vec2 => json!([0.0, 0.0]),
            FieldType::Vec3 => json!([0.0, 0.0, 0.0]),
            FieldType::Color => json!([1.0, 1.0, 1.0, 1.0]),
            FieldType::Enum => json!(self.options.first().cloned().unwrap_or_default()),
        }
    }

    /// Rust expression for `value` as written inside `bsn!`.
    pub fn rust_expr(&self, value: &Value) -> String {
        let f = |v: &Value| v.as_f64().unwrap_or(0.0);
        let float = |x: f64| {
            let s = format!("{x}");
            if s.contains('.') || s.contains('e') {
                s
            } else {
                format!("{s}.0")
            }
        };
        let arr = |n: usize| -> Vec<f64> { (0..n).map(|i| value.get(i).map(f).unwrap_or(0.0)).collect() };
        match self.ty {
            FieldType::Bool => value.as_bool().unwrap_or(false).to_string(),
            FieldType::I32 => value.as_i64().unwrap_or(0).to_string(),
            FieldType::F32 => float(f(value)),
            FieldType::String | FieldType::Asset => format!("String::from({:?})", value.as_str().unwrap_or("")),
            FieldType::Vec2 => {
                let a = arr(2);
                format!("Vec2::new({}, {})", float(a[0]), float(a[1]))
            }
            FieldType::Vec3 => {
                let a = arr(3);
                format!("Vec3::new({}, {}, {})", float(a[0]), float(a[1]), float(a[2]))
            }
            FieldType::Color => {
                let a = arr(4);
                format!("Color::srgba({}, {}, {}, {})", float(a[0]), float(a[1]), float(a[2]), float(a[3]))
            }
            FieldType::Enum => {
                let variant = value.as_str().unwrap_or("");
                match &self.rust_type {
                    Some(t) => format!("{t}::{variant}"),
                    None => variant.to_string(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_example_config() {
        let reg = crate::game::example_registry();
        assert_eq!(reg.components.len(), 4);
        let spawner = reg.get("Spawner").unwrap();
        assert_eq!(spawner.fields[2].default_value(), json!([0.0, 0.0, 0.0]));
        assert_eq!(spawner.fields[1].rust_expr(&json!(3)), "3");
        let collider = reg.get("Collider").unwrap();
        assert_eq!(collider.fields[0].rust_expr(&json!("Sphere")), "ColliderShape::Sphere");
    }
}
