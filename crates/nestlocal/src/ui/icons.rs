//! Icons by Home Assistant's names for them (utils/icons.py builds the set).

use std::collections::HashMap;
use std::sync::OnceLock;

static DEFLATED: &[u8] = include_bytes!("icons.deflate");

/// Brand marks used by browser folders (Spotify / Apple / Podcast). Not in the
/// word-filtered `icons.deflate` set — kept here from @mdi/js (Apache-2.0).
const BRANDS: &[(&str, &str)] = &[
    // mdiSpotify
    ("mdi:spotify", "M17.9,10.9C14.7,9 9.35,8.8 6.3,9.75C5.8,9.9 5.3,9.6 5.15,9.15C5,8.65 5.3,8.15 5.75,8C9.3,6.95 15.15,7.15 18.85,9.35C19.3,9.6 19.45,10.2 19.2,10.65C18.95,11 18.35,11.15 17.9,10.9M17.8,13.7C17.55,14.05 17.1,14.2 16.75,13.95C14.05,12.3 9.95,11.8 6.8,12.8C6.4,12.9 5.95,12.7 5.85,12.3C5.75,11.9 5.95,11.45 6.35,11.35C10,10.25 14.5,10.8 17.6,12.7C17.9,12.85 18.05,13.35 17.8,13.7M16.6,16.45C16.4,16.75 16.05,16.85 15.75,16.65C13.4,15.2 10.45,14.9 6.95,15.7C6.6,15.8 6.3,15.55 6.2,15.25C6.1,14.9 6.35,14.6 6.65,14.5C10.45,13.65 13.75,14 16.35,15.6C16.7,15.75 16.75,16.15 16.6,16.45M12,2A10,10 0 0,0 2,12A10,10 0 0,0 12,22A10,10 0 0,0 22,12A10,10 0 0,0 12,2Z"),
    // mdiApple
    ("mdi:apple", "M18.71,19.5C17.88,20.74 17,21.95 15.66,21.97C14.32,22 13.89,21.18 12.37,21.18C10.84,21.18 10.37,21.95 9.1,22C7.79,22.05 6.8,20.68 5.96,19.47C4.25,17 2.94,12.45 4.7,9.39C5.57,7.87 7.13,6.91 8.82,6.88C10.1,6.86 11.32,7.75 12.11,7.75C12.89,7.75 14.37,6.68 15.92,6.84C16.57,6.87 18.39,7.1 19.56,8.82C19.47,8.88 17.39,10.1 17.41,12.63C17.44,15.65 20.06,16.66 20.09,16.67C20.06,16.74 19.67,18.11 18.71,19.5M13,3.5C13.73,2.67 14.94,2.04 15.94,2C16.07,3.17 15.6,4.35 14.9,5.19C14.21,6.04 13.07,6.7 11.95,6.61C11.8,5.46 12.36,4.26 13,3.5Z"),
    // mdiPodcast
    ("mdi:podcast", "M17,18.25V21.5H7V18.25C7,16.87 9.24,15.75 12,15.75C14.76,15.75 17,16.87 17,18.25M12,5.5A6.5,6.5 0 0,1 18.5,12C18.5,13.25 18.15,14.42 17.54,15.41L16,14.04C16.32,13.43 16.5,12.73 16.5,12C16.5,9.5 14.5,7.5 12,7.5C9.5,7.5 7.5,9.5 7.5,12C7.5,12.73 7.68,13.43 8,14.04L6.46,15.41C5.85,14.42 5.5,13.25 5.5,12A6.5,6.5 0 0,1 12,5.5M12,1.5A10.5,10.5 0 0,1 22.5,12C22.5,14.28 21.77,16.39 20.54,18.11L19.04,16.76C19.96,15.4 20.5,13.76 20.5,12A8.5,8.5 0 0,0 12,3.5A8.5,8.5 0 0,0 3.5,12C3.5,13.76 4.04,15.4 4.96,16.76L3.46,18.11C2.23,16.39 1.5,14.28 1.5,12A10.5,10.5 0 0,1 12,1.5M12,9.5A2.5,2.5 0 0,1 14.5,12A2.5,2.5 0 0,1 12,14.5A2.5,2.5 0 0,1 9.5,12A2.5,2.5 0 0,1 12,9.5Z"),
    // Tidal has no MDI glyph — four diamonds approximating the mark (24×24).
    ("mdi:tidal", "M1,12L4.5,8.5L8,12L4.5,15.5L1,12M6,12L9.5,8.5L13,12L9.5,15.5L6,12M11,12L14.5,8.5L18,12L14.5,15.5L11,12M16,12L19.5,8.5L23,12L19.5,15.5L16,12"),
];

/// An icon: its SVG path and the side of its square viewBox, 24 (MDI's
/// 0 0 24 24) or 960 (Material Symbols' 0 -960 960 960).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Icon {
    pub path: &'static str,
    pub box_size: f32,
}

/// Brand accent for a browser folder icon (Spotify green, …).
pub fn brand_colour(name: &str) -> Option<[u8; 3]> {
    match name {
        "mdi:spotify" => Some([29, 185, 84]),
        "mdi:tidal" => Some([0, 255, 255]),
        "mdi:apple" => Some([250, 36, 60]),
        "mdi:podcast" => Some([155, 89, 182]),
        _ => None,
    }
}

fn all() -> &'static HashMap<&'static str, Icon> {
    static ICONS: OnceLock<HashMap<&'static str, Icon>> = OnceLock::new();
    ICONS.get_or_init(|| {
        let text = miniz_oxide::inflate::decompress_to_vec(DEFLATED).unwrap_or_default();
        let text: &'static str = Box::leak(String::from_utf8(text).unwrap_or_default().into_boxed_str());
        let mut map: HashMap<&str, Icon> = text.lines().filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let (name, size, path) = (parts.next()?, parts.next()?, parts.next()?);
            Some((name, Icon { path, box_size: size.parse().ok()? }))
        }).collect();
        for &(name, path) in BRANDS {
            map.insert(name, Icon { path, box_size: 24.0 });
        }
        map
    })
}

/// Home Assistant's icon `name` if it is one this carries, else the
/// default for the entity's domain (light, switch, ...).
pub fn lookup(name: &str, entity: &str) -> Option<Icon> {
    let icons = all();
    if let Some(icon) = icons.get(name) {
        return Some(*icon);
    }
    // A filled Material Symbol this lacks: its outline.
    if let Some((prefix, rest)) = name.split_once(':') {
        if prefix.starts_with("m3") && prefix.ends_with('f') {
            if let Some(icon) = icons.get(format!("{}:{rest}", &prefix[..prefix.len() - 1]).as_str()) {
                return Some(*icon);
            }
        }
    }
    let domain = entity.split('.').next().unwrap_or_default();
    let fallback = match domain {
        "light" => "mdi:lightbulb",
        "switch" | "input_boolean" => "mdi:toggle-switch-variant",
        "fan" => "mdi:fan",
        "climate" => "mdi:thermostat",
        "media_player" => "mdi:speaker",
        "lights" => "mdi:lightbulb-group",
        "settings" => "mdi:cog",
        "back" => "mdi:arrow-left",
        _ => return None,
    };
    icons.get(fallback).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_home_assistants_names_and_falls_back_by_domain() {
        let group = lookup("mdi:lightbulb-group", "light.bedroom").unwrap();
        assert_eq!(group.box_size, 24.0);
        let lamp = lookup("m3s:table-lamp", "light.reading_lamp").unwrap();
        assert_eq!(lamp.box_size, 960.0);
        assert!(lookup("m3sf:table-lamp", "light.x").is_some());
        assert_eq!(lookup("mdi:no-such-icon", "light.x"), lookup("mdi:lightbulb", ""));
        assert!(lookup("", "switch.kettle").is_some());
        assert!(lookup("", "settings").is_some() && lookup("", "back").is_some());
        assert_eq!(lookup("", "sensor.x"), None);
        assert!(lookup("mdi:spotify", "").is_some());
        assert_eq!(brand_colour("mdi:spotify"), Some([29, 185, 84]));
    }
}
