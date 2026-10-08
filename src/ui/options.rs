//! **Opciones** persistentes (`options.json`): video, juego y teclas.
//!
//! Logica pura y serializable (fuera del mundo). `engine::app` las carga al
//! arrancar, las aplica y las guarda al cambiar. La parte de **reasignacion de
//! teclas** resuelve conflictos **intercambiando** (no deja teclas duplicadas en
//! silencio).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::world::{FogMode, ViewSettings};

/// Version del archivo de opciones (para migrar si cambia el esquema).
pub const OPTIONS_VERSION: u32 = 1;

/// Archivo de opciones (global, fuera de los mundos).
pub const OPTIONS_FILE: &str = "options.json";

fn one() -> u32 {
    OPTIONS_VERSION
}

/// Una accion reasignable del teclado.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Binding {
    /// Identificador de la accion (`forward`, `jump`, ...).
    pub action: String,
    /// Tecla (nombre de `KeyCode`, p.ej. `KeyW`, `Space`).
    pub key: String,
}

/// Opciones del juego (contenido de `options.json`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Options {
    #[serde(default = "one")]
    pub version: u32,
    pub render_radius: i32,
    pub sim_radius: i32,
    /// Modo de niebla (`off`/`far`/`normal`/`short`).
    pub fog: String,
    pub fov_deg: f32,
    pub mouse_sensitivity: f32,
    /// Idioma (`es`/`en`).
    pub lang: String,
    /// Intervalo de autoguardado, en segundos.
    pub autosave_secs: f32,
    /// Mostrar el overlay F3 al arrancar.
    pub show_f3: bool,
    #[serde(default)]
    pub bindings: Vec<Binding>,
}

/// Acciones por defecto y su tecla.
const DEFAULT_BINDINGS: &[(&str, &str)] = &[
    ("forward", "KeyW"),
    ("back", "KeyS"),
    ("left", "KeyA"),
    ("right", "KeyD"),
    ("jump", "Space"),
    ("fly", "KeyF"),
    ("inventory", "KeyE"),
];

impl Default for Options {
    fn default() -> Self {
        Self {
            version: OPTIONS_VERSION,
            render_radius: 12,
            sim_radius: 6,
            fog: "normal".to_string(),
            fov_deg: 70.0,
            mouse_sensitivity: 0.12,
            lang: "es".to_string(),
            autosave_secs: 300.0,
            show_f3: false,
            bindings: DEFAULT_BINDINGS
                .iter()
                .map(|(a, k)| Binding {
                    action: (*a).to_string(),
                    key: (*k).to_string(),
                })
                .collect(),
        }
    }
}

impl Options {
    /// Ajusta valores fuera de rango (por si el archivo venia editado a mano).
    pub fn clamp(&mut self) {
        self.render_radius = self.render_radius.clamp(2, 32);
        self.sim_radius = self.sim_radius.clamp(1, 12);
        self.fov_deg = self.fov_deg.clamp(30.0, 110.0);
        self.mouse_sensitivity = self.mouse_sensitivity.clamp(0.02, 0.5);
        self.autosave_secs = self.autosave_secs.clamp(30.0, 3600.0);
    }

    /// Niebla como enum (con `normal` por defecto).
    pub fn fog_mode(&self) -> FogMode {
        match self.fog.as_str() {
            "off" => FogMode::Off,
            "far" => FogMode::Far,
            "short" => FogMode::Short,
            _ => FogMode::Normal,
        }
    }

    /// Ajustes de vista derivados de las opciones.
    pub fn view_settings(&self) -> ViewSettings {
        ViewSettings {
            render_radius: self.render_radius,
            simulation_radius: self.sim_radius,
            fog: self.fog_mode(),
        }
    }

    /// Siguiente modo de niebla (para un boton ciclar).
    pub fn cycle_fog(&mut self) {
        self.fog = match self.fog.as_str() {
            "off" => "far",
            "far" => "normal",
            "normal" => "short",
            _ => "off",
        }
        .to_string();
    }

    /// Alterna el idioma.
    pub fn cycle_lang(&mut self) {
        self.lang = if self.lang == "en" { "es" } else { "en" }.to_string();
    }

    /// Tecla asignada a una accion.
    pub fn key_for(&self, action: &str) -> Option<&str> {
        self.bindings
            .iter()
            .find(|b| b.action == action)
            .map(|b| b.key.as_str())
    }

    /// Reasigna una tecla a una accion. Si la tecla ya estaba en otra accion,
    /// **intercambia** las teclas (no deja duplicados). `action` debe existir.
    /// Devuelve la accion con la que se intercambio, si la hubo.
    pub fn rebind(&mut self, action: &str, key: &str) -> Option<String> {
        let old_key = self.key_for(action)?.to_string();
        // ¿Que otra accion usa ya esta tecla?
        let conflict = self
            .bindings
            .iter()
            .find(|b| b.action != action && b.key == key)
            .map(|b| b.action.clone());
        if let Some(other) = &conflict {
            for b in self.bindings.iter_mut() {
                if &b.action == other {
                    b.key = old_key.clone();
                }
            }
        }
        for b in self.bindings.iter_mut() {
            if b.action == action {
                b.key = key.to_string();
            }
        }
        conflict
    }

    /// Carga `options.json`; si falta o esta corrupto, devuelve los defaults
    /// (no crashea). Siempre aplica `clamp`.
    pub fn load(path: &Path) -> Self {
        let mut opts = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str::<Options>(&s).ok())
            .unwrap_or_default();
        opts.clamp();
        opts
    }

    /// Guarda `options.json` de forma **atomica** (`.tmp` -> rename).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&tmp, json)?;
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("solaria_options_{tag}_{}.json", std::process::id()))
    }

    #[test]
    fn ida_y_vuelta() {
        let path = temp_path("roundtrip");
        let _ = std::fs::remove_file(&path);
        let o = Options {
            render_radius: 20,
            fov_deg: 90.0,
            fog: "short".into(),
            ..Default::default()
        };
        o.save(&path).unwrap();
        let back = Options::load(&path);
        assert_eq!(back.render_radius, 20);
        assert_eq!(back.fov_deg, 90.0);
        assert_eq!(back.fog_mode(), FogMode::Short);
        assert_eq!(back.bindings, o.bindings);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn corrupto_o_ausente_da_defaults() {
        let path = temp_path("corrupt");
        std::fs::write(&path, b"{ no es json").unwrap();
        let o = Options::load(&path);
        assert_eq!(o, Options::default());
        let _ = std::fs::remove_file(&path);
        // Ausente tambien.
        assert_eq!(Options::load(&temp_path("nope")), Options::default());
    }

    #[test]
    fn clamp_acota_los_valores() {
        let mut o = Options {
            render_radius: 9999,
            fov_deg: 5.0,
            mouse_sensitivity: -1.0,
            ..Default::default()
        };
        o.clamp();
        assert_eq!(o.render_radius, 32);
        assert_eq!(o.fov_deg, 30.0);
        assert_eq!(o.mouse_sensitivity, 0.02);
    }

    #[test]
    fn reasignar_tecla_con_conflicto_intercambia() {
        let mut o = Options::default();
        // "forward" usa W; reasignamos W a "back": deben intercambiarse.
        let swapped = o.rebind("back", "KeyW");
        assert_eq!(swapped.as_deref(), Some("forward"));
        assert_eq!(o.key_for("back"), Some("KeyW"));
        assert_eq!(o.key_for("forward"), Some("KeyS"));
        // Sin conflicto, simple.
        assert_eq!(o.rebind("jump", "KeyJ"), None);
        assert_eq!(o.key_for("jump"), Some("KeyJ"));
        // Accion inexistente no cambia nada.
        assert_eq!(o.rebind("no.existe", "KeyZ"), None);
    }

    #[test]
    fn los_modos_ciclan() {
        let mut o = Options::default();
        assert_eq!(o.fog, "normal");
        o.cycle_fog();
        assert_eq!(o.fog, "short");
        o.cycle_fog();
        assert_eq!(o.fog, "off");
        o.cycle_lang();
        assert_eq!(o.lang, "en");
    }

    #[test]
    fn las_opciones_dan_views_settings() {
        let o = Options::default();
        let v = o.view_settings();
        assert_eq!(v.render_radius, 12);
        assert_eq!(v.simulation_radius, 6);
        assert_eq!(v.fog, FogMode::Normal);
    }
}
