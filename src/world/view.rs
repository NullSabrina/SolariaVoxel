//! Ajustes de distancia de vista: render, simulacion y niebla.
//!
//! Inspirado en como Minecraft separa la **distancia de renderizado** de la de
//! **simulacion**. Estos valores viven fuera del mundo (pensados para
//! persistirse en `options` y exponerse en el menu de opciones). Son puros y
//! serializables; el `World` los lee para cargar/descargar y simular.
//!
//! Notas de diseno:
//! - La carga es **circular** (radio euclideo): ~21 % menos columnas que un
//!   cuadrado del mismo radio.
//! - La **histéresis** (`unload_radius = render_radius + 2`) evita cargar y
//!   descargar en bucle cuando el jugador camina por un borde.
//! - `generation_radius` coincide con `render_radius`: la niebla termina en
//!   `(render_radius - 1)` chunks, asi que el anillo exterior nunca se ve y no
//!   hace falta generar una corona extra mas alla de la malla.

use crate::world::chunk::CHUNK_SIZE;

/// Modo de niebla del menu (mapea a un ratio de inicio y a un factor de final).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FogMode {
    /// Sin niebla (la distancia de culling sigue cubriendo el area cargada).
    Off,
    /// Lejana: casi no oscurece el borde.
    Far,
    /// Normal (por defecto).
    Normal,
    /// Corta: ambiente cerrado/niebla densa.
    Short,
}

impl FogMode {
    /// Ratio `fog_start / fog_end`.
    pub fn start_ratio(self) -> f32 {
        match self {
            FogMode::Off => 1.0,
            FogMode::Far => 0.88,
            FogMode::Normal => 0.80,
            FogMode::Short => 0.55,
        }
    }

    /// Factor que acorta `fog_end` respecto al borde de carga.
    pub fn end_factor(self) -> f32 {
        match self {
            FogMode::Off => 1.0,
            FogMode::Far => 1.0,
            FogMode::Normal => 1.0,
            FogMode::Short => 0.7,
        }
    }

    /// Nombre para el overlay F3.
    pub fn name(self) -> &'static str {
        match self {
            FogMode::Off => "OFF",
            FogMode::Far => "FAR",
            FogMode::Normal => "NORMAL",
            FogMode::Short => "SHORT",
        }
    }
}

/// Ajustes de distancia de vista y simulacion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewSettings {
    /// Columnas con malla dibujada (radio euclideo).
    pub render_radius: i32,
    /// Radio donde corre el automata de fluidos (fuera queda congelado).
    pub simulation_radius: i32,
    /// Modo de niebla.
    pub fog: FogMode,
}

/// Radio de render por defecto.
pub const DEFAULT_RENDER_RADIUS: i32 = 12;
/// Radio de simulacion por defecto.
pub const DEFAULT_SIM_RADIUS: i32 = 6;

impl Default for ViewSettings {
    fn default() -> Self {
        Self {
            render_radius: DEFAULT_RENDER_RADIUS,
            simulation_radius: DEFAULT_SIM_RADIUS,
            fog: FogMode::Normal,
        }
    }
}

impl ViewSettings {
    /// Ajustes con un radio de render dado (usado por tests y por el ajuste
    /// `SOLARIA_VIEW_RADIUS`); simulacion igual al render, niebla normal.
    pub fn from_render_radius(render_radius: i32) -> Self {
        let render_radius = render_radius.clamp(0, 32);
        Self {
            render_radius,
            simulation_radius: render_radius,
            fog: FogMode::Normal,
        }
    }

    /// Radio de generacion/carga. Igual a `render_radius` (ver la nota del modulo).
    pub fn generation_radius(&self) -> i32 {
        self.render_radius
    }

    /// Radio a partir del cual se descarga (histéresis).
    pub fn unload_radius(&self) -> i32 {
        self.render_radius + 2
    }

    /// Distancia (bloques) donde empieza la niebla.
    pub fn fog_start(&self) -> f32 {
        self.fog_end() * self.fog.start_ratio()
    }

    /// Distancia (bloques) donde la niebla es total (o el borde cargado si Off).
    pub fn fog_end(&self) -> f32 {
        let edge = (self.render_radius as f32 - 1.0).max(1.0) * CHUNK_SIZE as f32;
        edge * self.fog.end_factor()
    }

    /// Distancia de **culling**: cubre todo el area cargada (radio * 16).
    pub fn cull_distance(&self) -> f32 {
        (self.render_radius as f32).max(1.0) * CHUNK_SIZE as f32
    }

    /// Aplica overrides de entorno sobre estos ajustes (para demos/capturas):
    /// `SOLARIA_VIEW_RADIUS`, `SOLARIA_SIM_RADIUS` y `SOLARIA_FOG`.
    pub fn with_env_overrides(mut self) -> Self {
        if let Some(r) = std::env::var("SOLARIA_VIEW_RADIUS")
            .ok()
            .and_then(|s| s.parse::<i32>().ok())
        {
            self.render_radius = r.clamp(2, 32);
        }
        if let Some(r) = std::env::var("SOLARIA_SIM_RADIUS")
            .ok()
            .and_then(|s| s.parse::<i32>().ok())
        {
            self.simulation_radius = r.clamp(1, 12);
        }
        if let Ok(m) = std::env::var("SOLARIA_FOG") {
            self.fog = match m.to_ascii_lowercase().as_str() {
                "off" | "0" => FogMode::Off,
                "far" | "lejana" => FogMode::Far,
                "short" | "corta" => FogMode::Short,
                _ => FogMode::Normal,
            };
        }
        self
    }

    /// Lee los ajustes del entorno (defaults + overrides).
    ///
    /// - `SOLARIA_VIEW_RADIUS`: radio de render (alias historico).
    /// - `SOLARIA_SIM_RADIUS`: radio de simulacion.
    /// - `SOLARIA_FOG`: `off|far|normal|short`.
    pub fn from_env() -> Self {
        Self::default().with_env_overrides()
    }

    /// Preset `low`: poca distancia.
    pub fn preset_low() -> Self {
        Self {
            render_radius: 6,
            simulation_radius: 4,
            fog: FogMode::Normal,
        }
    }

    /// Preset `medium` (por defecto).
    pub fn preset_medium() -> Self {
        Self::default()
    }

    /// Preset `high`: mucha distancia.
    pub fn preset_high() -> Self {
        Self {
            render_radius: 20,
            simulation_radius: 8,
            fog: FogMode::Far,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_niebla_termina_dentro_del_area_cargada() {
        let v = ViewSettings::from_render_radius(12);
        assert!(v.fog_end() < v.cull_distance());
        assert!(v.fog_start() < v.fog_end());
    }

    #[test]
    fn el_modo_off_desactiva_la_niebla() {
        let mut v = ViewSettings::from_render_radius(12);
        v.fog = FogMode::Off;
        assert_eq!(v.fog_start(), v.fog_end());
    }

    #[test]
    fn la_histeresis_deja_un_margen() {
        let v = ViewSettings::from_render_radius(8);
        assert_eq!(v.unload_radius(), 10);
        assert!(v.unload_radius() > v.render_radius);
    }

    #[test]
    fn los_presets_estan_ordenados() {
        assert!(
            ViewSettings::preset_low().render_radius < ViewSettings::preset_medium().render_radius
        );
        assert!(
            ViewSettings::preset_medium().render_radius < ViewSettings::preset_high().render_radius
        );
    }
}
