//! Saved, explicitly chosen keyboard lighting; never contains HID commands or paths.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Command {
    pub id: String,
    pub value: serde_json::Value,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preference {
    pub key: String,
    pub model: String,
    pub commands: Vec<Command>,
}
pub fn validate(entries: &[Preference]) -> Result<(), String> {
    if entries.len() > 64 {
        return Err("Too many saved keyboards.".into());
    }
    for entry in entries {
        if entry.key.len() != 9
            || entry.key.as_bytes()[4] != b':'
            || !entry
                .key
                .bytes()
                .enumerate()
                .all(|(i, c)| i == 4 || c.is_ascii_hexdigit())
            || entry.model.is_empty()
            || entry.model.len() > 160
            || entry.commands.len() > 4
        {
            return Err("Invalid saved keyboard identity.".into());
        }
        for command in &entry.commands {
            let valid = match command.id.as_str() {
                "keyboard.color" => command.value.as_str().is_some_and(|v| {
                    [
                        "#ffffff", "#ff0000", "#ff8000", "#ffff00", "#00ff00", "#00ffff",
                        "#0000ff", "#8000ff", "#ff00ff",
                    ]
                    .contains(&v)
                }),
                "keyboard.effect" => command.value.as_str().is_some_and(|v| {
                    ["Off", "Static", "Cycling", "Wave", "Breathing single"].contains(&v)
                }),
                "keyboard.brightness" => command.value.as_u64().is_some_and(|v| v <= 65535),
                "keyboard.illumination" => command.value.is_boolean(),
                _ => false,
            };
            if !valid {
                return Err("Invalid saved keyboard lighting choice.".into());
            }
        }
    }
    Ok(())
}
fn path() -> Result<std::path::PathBuf, String> {
    dirs::config_dir()
        .map(|p| p.join("openmouse/keyboard-lighting.json"))
        .ok_or("No configuration folder is available.".into())
}
pub fn load() -> Result<Vec<Preference>, String> {
    let bytes = match std::fs::read(path()?) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };
    if bytes.len() > 128 * 1024 {
        return Err("Saved keyboard lighting is too large.".into());
    }
    let entries = serde_json::from_slice::<Vec<Preference>>(&bytes).map_err(|e| e.to_string())?;
    validate(&entries)?;
    Ok(entries)
}
pub fn save(entries: &[Preference]) -> Result<(), String> {
    validate(entries)?;
    crate::games::custom::save_value(&path()?, entries)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn only_known_lighting_choices_can_be_saved_and_reloaded() {
        let mut entry = Preference {
            key: "046d:c359".into(),
            model: "G915 X".into(),
            commands: vec![Command {
                id: "keyboard.color".into(),
                value: json!("#ff0000"),
            }],
        };
        validate(&[entry.clone()]).unwrap();
        let encoded = serde_json::to_vec(&vec![entry.clone()]).unwrap();
        let decoded: Vec<Preference> = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, vec![entry.clone()]);
        entry.commands[0].id = "arbitrary-hid-command".into();
        assert!(validate(&[entry.clone()]).is_err());
        entry.commands[0].id = "keyboard.color".into();
        entry.commands[0].value = json!("not a color");
        assert!(validate(&[entry]).is_err());
    }
}
