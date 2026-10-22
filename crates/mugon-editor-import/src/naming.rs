//! Name-based heuristics: pack ids, tags and fallback categories.

/// `SIMPLE_Racer_Unity_2020_3_v1_5.unitypackage` -> (`Simple_Racer`, `Simple`, `Racer`)
pub fn pack_identity(file_name: &str) -> (String, String, String) {
    let stem = file_name.rsplit_once('.').map(|(s, _)| s).unwrap_or(file_name);
    let mut parts: Vec<&str> = Vec::new();
    for p in stem.split('_') {
        let l = p.to_ascii_lowercase();
        if l == "unity" || l == "sourcefiles" || l == "source" || l == "files" {
            break;
        }
        parts.push(p);
    }
    if parts.is_empty() {
        return (stem.to_string(), String::new(), stem.to_string());
    }
    let family = title(parts[0]);
    let name = parts[1..].join("_");
    let id = if name.is_empty() { family.clone() } else { format!("{family}_{name}") };
    (id, family, name)
}

fn title(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
        None => String::new(),
    }
}

/// Splits `SM_Prop_BarrelOil_01` into lowercase word tokens, dropping prefixes and numbers.
pub fn tags_from_name(name: &str, drop_prefixes: usize) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for (i, token) in name.split('_').enumerate() {
        if i < drop_prefixes || token.is_empty() || token.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let mut word = String::new();
        let mut prev_lower = false;
        for c in token.chars() {
            if c.is_ascii_uppercase() && prev_lower {
                push_word(&mut tags, &mut word);
            }
            if c.is_alphabetic() {
                word.push(c.to_ascii_lowercase());
            } else {
                push_word(&mut tags, &mut word);
            }
            prev_lower = c.is_lowercase();
        }
        push_word(&mut tags, &mut word);
    }
    tags
}

fn push_word(tags: &mut Vec<String>, word: &mut String) {
    if word.len() > 1 && !tags.contains(word) {
        tags.push(std::mem::take(word));
    } else {
        word.clear();
    }
}

/// Fallback category from the type token of Synty names (`SM_Prop_...`, `SR_Bld_...`, `SK_Chr_...`).
pub fn category_from_name(name: &str) -> String {
    for token in name.split('_').skip(1).take(2) {
        let c = match token.to_ascii_lowercase().as_str() {
            "prop" | "props" => "Props",
            "bld" | "bldg" | "building" | "buildings" => "Buildings",
            "env" | "environment" => "Environment",
            "veh" | "vehicle" | "vehicles" => "Vehicles",
            "chr" | "char" | "character" | "characters" => "Characters",
            "fx" => "FX",
            "wep" | "weapon" | "weapons" => "Weapons",
            "item" | "items" => "Items",
            "icon" | "icons" => "Icons",
            "gen" | "generic" => "Generic",
            _ => continue,
        };
        return c.to_string();
    }
    "Uncategorized".to_string()
}

/// `Assets/X/Prefabs/Props/Sub/Foo.prefab` -> `Props/Sub`
pub fn category_from_prefab_path(path: &str) -> Option<String> {
    let rest = path.split("/Prefabs/").nth(1).or_else(|| path.split("/Prefab/").nth(1))?;
    let mut parts: Vec<&str> = rest.split('/').collect();
    parts.pop(); // file name
    if parts.is_empty() {
        return None;
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_ids() {
        assert_eq!(pack_identity("SIMPLE_Racer_Unity_2020_3_v1_5.unitypackage").0, "Simple_Racer");
        assert_eq!(pack_identity("POLYGON_SciFi_Space_Unity_2022_3_v1_14_1.unitypackage").0, "Polygon_SciFi_Space");
        assert_eq!(pack_identity("Simple_Cars_SourceFiles.zip").0, "Simple_Cars");
    }

    #[test]
    fn tags() {
        assert_eq!(tags_from_name("SM_Prop_BarrelOil_01", 2), vec!["barrel", "oil"]);
    }

    #[test]
    fn categories() {
        assert_eq!(category_from_name("SR_Bld_Stands"), "Buildings");
        assert_eq!(category_from_prefab_path("Assets/SimpleRacer/Prefabs/Props/X.prefab").unwrap(), "Props");
    }
}
