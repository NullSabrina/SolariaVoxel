//! **Libreria de mundos multiples**: `saves/<slug>/` con `level.json` legible.
//!
//! Cada mundo vive en su carpeta: `world.vf` (binario existente), `world.vf.bak`
//! (copia previa) y `level.json` (metadatos legibles: nombre, semilla, modo,
//! fechas...). Los metadatos van **fuera** del binario para poder listar mundos
//! sin abrir cada `world.vf`.
//!
//! Escritura **atomica**: se escribe `.tmp` y se renombra encima. Carga de
//! `level.json` tolerante: si falta o esta corrupto se marca el mundo como
//! `corrupt` (no crashea la lista).
//!
//! Base por defecto: junto al ejecutable; `SOLARIA_HOME` la sobreescribe.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Carpeta de mundos.
pub const SAVES_DIR: &str = "saves";
/// Archivo binario del mundo.
pub const WORLD_FILE: &str = "world.vf";
/// Copia previa del binario.
pub const WORLD_BAK: &str = "world.vf.bak";
/// Metadatos legibles.
pub const LEVEL_FILE: &str = "level.json";
/// Longitud maxima del slug.
pub const MAX_SLUG_LEN: usize = 48;

fn default_game_mode() -> String {
    "creative".to_string()
}

fn default_generator() -> String {
    "legacy16".to_string()
}

/// Metadatos de un mundo (contenido de `level.json`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorldMeta {
    /// Nombre visible ("Mundo nuevo").
    pub display_name: String,
    /// Semilla del generador.
    pub seed: u32,
    /// Modo de juego ("creative").
    #[serde(default = "default_game_mode")]
    pub game_mode: String,
    /// Creacion (epoch UNIX).
    pub created_at: u64,
    /// Ultima vez jugado (epoch UNIX).
    pub last_played: u64,
    /// Version del motor que creo el mundo.
    #[serde(default)]
    pub engine_version: String,
    /// Tipo de generador ("legacy16").
    #[serde(default = "default_generator")]
    pub generator_kind: String,
}

/// Un mundo en disco: su slug (carpeta) y sus metadatos.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldEntry {
    pub slug: String,
    pub meta: WorldMeta,
    /// `true` si faltaba/corrompio `level.json` (el mundo puede seguir jugable).
    pub corrupt: bool,
}

impl WorldEntry {
    /// Carpeta del mundo dentro de `base`.
    pub fn dir(&self, base: &Path) -> PathBuf {
        saves_dir(base).join(&self.slug)
    }

    /// Ruta del binario del mundo.
    pub fn world_path(&self, base: &Path) -> PathBuf {
        self.dir(base).join(WORLD_FILE)
    }
}

/// Directorio base (junto al ejecutable, o `SOLARIA_HOME`).
pub fn base_dir_from_env() -> PathBuf {
    std::env::var("SOLARIA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Carpeta `saves/` dentro de `base`.
pub fn saves_dir(base: &Path) -> PathBuf {
    base.join(SAVES_DIR)
}

/// Segundos desde el epoch UNIX.
pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Semilla a partir de un texto no numerico, con un hash **estable entre
/// versiones** (FNV-1a 64, truncado a 32 bits). Si el texto es un entero, se usa
/// tal cual. Determinista: el mismo texto da siempre la misma semilla.
pub fn seed_from_text(text: &str) -> u32 {
    let trimmed = text.trim();
    if let Ok(n) = trimmed.parse::<u32>() {
        return n;
    }
    // FNV-1a de 64 bits, iniciado con el offset basis.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in trimmed.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (hash ^ (hash >> 32)) as u32
}

/// Nombres reservados de Windows (no se pueden usar como carpeta).
fn is_reserved(s: &str) -> bool {
    let up = s.to_ascii_uppercase();
    matches!(up.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (up.len() == 4
            && (up.starts_with("COM") || up.starts_with("LPT"))
            && up[3..].chars().all(|c| c.is_ascii_digit()))
}

/// Convierte un nombre en un slug de carpeta valido en Windows/Linux: minusculas,
/// solo `[a-z0-9_-]`, sin nombres reservados, longitud acotada.
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            // Espacios, acentos y caracteres invalidos -> guion.
            out.push('-');
        }
    }
    // Colapsa guiones y recorta extremos.
    let mut collapsed = String::with_capacity(out.len());
    let mut prev_dash = false;
    for c in out.chars() {
        if c == '-' {
            if !prev_dash && !collapsed.is_empty() {
                collapsed.push('-');
            }
            prev_dash = true;
        } else {
            collapsed.push(c);
            prev_dash = false;
        }
    }
    while collapsed.ends_with('-') {
        collapsed.pop();
    }
    if collapsed.is_empty() {
        collapsed.push_str("mundo");
    }
    collapsed.truncate(MAX_SLUG_LEN);
    while collapsed.ends_with('-') {
        collapsed.pop();
    }
    if is_reserved(&collapsed) {
        collapsed.push_str("-mundo");
    }
    collapsed
}

/// Slug unico: si `base` ya esta en `taken`, anade `-2`, `-3`...
pub fn unique_slug(base: &str, taken: &[String]) -> String {
    if !taken.contains(&base.to_string()) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}-{n}");
        if !taken.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Escribe `level.json` de forma **atomica** (`.tmp` -> rename).
pub fn save_meta(dir: &Path, meta: &WorldMeta) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{LEVEL_FILE}.tmp"));
    let final_path = dir.join(LEVEL_FILE);
    let json = serde_json::to_string_pretty(meta)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&tmp, json)?;
    // Windows: `rename` no sobreescribe; borramos el destino primero.
    if final_path.exists() {
        std::fs::remove_file(&final_path)?;
    }
    std::fs::rename(&tmp, &final_path)?;
    Ok(())
}

/// Carga `level.json` de una carpeta (o `None` si falta/corrupto).
pub fn load_meta(dir: &Path) -> Option<WorldMeta> {
    let data = std::fs::read_to_string(dir.join(LEVEL_FILE)).ok()?;
    serde_json::from_str(&data).ok()
}

/// Lista los mundos de `base`, ordenados por ultima vez jugado (desc).
pub fn list_worlds(base: &Path) -> Vec<WorldEntry> {
    let dir = saves_dir(base);
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let slug = entry.file_name().to_string_lossy().to_string();
        match load_meta(&entry.path()) {
            Some(meta) => out.push(WorldEntry {
                slug,
                meta,
                corrupt: false,
            }),
            None => out.push(WorldEntry {
                slug: slug.clone(),
                meta: WorldMeta {
                    display_name: slug,
                    seed: 0,
                    game_mode: default_game_mode(),
                    created_at: 0,
                    last_played: 0,
                    engine_version: String::new(),
                    generator_kind: default_generator(),
                },
                corrupt: true,
            }),
        }
    }
    out.sort_by_key(|w| std::cmp::Reverse(w.meta.last_played));
    out
}

/// Crea un mundo nuevo (generador legacy) y devuelve su entrada.
pub fn create_world(base: &Path, name: &str, seed: u32, now: u64) -> std::io::Result<WorldEntry> {
    create_world_kind(base, name, seed, now, "legacy16")
}

/// Crea un mundo nuevo con el **tipo de generador** pedido (`legacy16`/`graph`).
pub fn create_world_kind(
    base: &Path,
    name: &str,
    seed: u32,
    now: u64,
    generator: &str,
) -> std::io::Result<WorldEntry> {
    let taken: Vec<String> = list_worlds(base).into_iter().map(|w| w.slug).collect();
    let slug = unique_slug(&slugify(name), &taken);
    let meta = WorldMeta {
        display_name: name.to_string(),
        seed,
        game_mode: default_game_mode(),
        created_at: now,
        last_played: now,
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        generator_kind: generator.to_string(),
    };
    save_meta(&saves_dir(base).join(&slug), &meta)?;
    Ok(WorldEntry {
        slug,
        meta,
        corrupt: false,
    })
}

/// Renombra el **nombre visible** (mantiene el slug para no romper la carpeta).
pub fn rename_world(base: &Path, slug: &str, new_name: &str) -> std::io::Result<()> {
    let dir = saves_dir(base).join(slug);
    let mut meta = load_meta(&dir)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "sin level.json"))?;
    meta.display_name = new_name.to_string();
    save_meta(&dir, &meta)
}

/// Duplica un mundo (copia binario + metadatos) con un slug nuevo.
pub fn duplicate_world(base: &Path, slug: &str, now: u64) -> std::io::Result<WorldEntry> {
    let src = saves_dir(base).join(slug);
    let mut meta = load_meta(&src)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "sin level.json"))?;
    let taken: Vec<String> = list_worlds(base).into_iter().map(|w| w.slug).collect();
    let new_slug = unique_slug(&format!("{slug}-copia"), &taken);
    let dst = saves_dir(base).join(&new_slug);
    std::fs::create_dir_all(&dst)?;
    let world = src.join(WORLD_FILE);
    if world.exists() {
        std::fs::copy(&world, dst.join(WORLD_FILE))?;
    }
    meta.display_name = format!("{} (copia)", meta.display_name);
    meta.created_at = now;
    meta.last_played = now;
    save_meta(&dst, &meta)?;
    Ok(WorldEntry {
        slug: new_slug,
        meta,
        corrupt: false,
    })
}

/// Elimina un mundo (carpeta completa).
pub fn delete_world(base: &Path, slug: &str) -> std::io::Result<()> {
    std::fs::remove_dir_all(saves_dir(base).join(slug))
}

/// Marca `last_played = now` (al guardar/salir de un mundo).
pub fn touch_last_played(base: &Path, slug: &str, now: u64) -> std::io::Result<()> {
    let dir = saves_dir(base).join(slug);
    let mut meta = load_meta(&dir)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "sin level.json"))?;
    meta.last_played = now;
    save_meta(&dir, &meta)
}

/// Importa el `world.vf` antiguo (junto al ejecutable) a `saves/` si existe y no
/// hay mundos. **Copia** (no mueve). Devuelve la entrada importada.
pub fn import_legacy(base: &Path, now: u64) -> std::io::Result<Option<WorldEntry>> {
    let legacy = base.join(WORLD_FILE);
    if !legacy.exists() || !list_worlds(base).is_empty() {
        return Ok(None);
    }
    let taken: Vec<String> = Vec::new();
    let slug = unique_slug(&slugify("Mundo importado"), &taken);
    let dst = saves_dir(base).join(&slug);
    std::fs::create_dir_all(&dst)?;
    std::fs::copy(&legacy, dst.join(WORLD_FILE))?;
    // Lee la semilla real del header del binario importado (si se puede).
    let seed = super::save::load_and_migrate(&legacy)
        .map(|s| s.header.seed)
        .unwrap_or(0);
    let meta = WorldMeta {
        display_name: "Mundo importado".to_string(),
        seed,
        game_mode: default_game_mode(),
        created_at: now,
        last_played: now,
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        generator_kind: default_generator(),
    };
    save_meta(&dst, &meta)?;
    Ok(Some(WorldEntry {
        slug,
        meta,
        corrupt: false,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_base(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "solaria_lib_{tag}_{}_{}",
            std::process::id(),
            now_unix()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn slugify_sanitiza_y_acota() {
        assert_eq!(slugify("Mi Mundo"), "mi-mundo");
        assert_eq!(slugify("a<>:\"/\\|?*b"), "a-b");
        assert_eq!(slugify("  ---  "), "mundo");
        assert_eq!(slugify(""), "mundo");
        assert_eq!(slugify("CON"), "con-mundo");
        assert_eq!(slugify("COM1"), "com1-mundo");
        let long = "x".repeat(100);
        assert!(slugify(&long).len() <= MAX_SLUG_LEN);
    }

    #[test]
    fn unique_slug_anade_sufijo() {
        let taken = vec!["mundo".to_string(), "mundo-2".to_string()];
        assert_eq!(unique_slug("mundo", &taken), "mundo-3");
        assert_eq!(unique_slug("otro", &taken), "otro");
    }

    #[test]
    fn crear_listar_renombrar_y_borrar() {
        let base = temp_base("crud");
        let a = create_world(&base, "Mundo A", 42, 1000).unwrap();
        assert_eq!(a.slug, "mundo-a");
        let b = create_world(&base, "Mundo A", 7, 2000).unwrap();
        assert_eq!(b.slug, "mundo-a-2", "segundo mundo con mismo nombre");

        let list = list_worlds(&base);
        assert_eq!(list.len(), 2);
        // Ordenado por last_played desc: b (2000) antes que a (1000).
        assert_eq!(list[0].slug, "mundo-a-2");

        rename_world(&base, &a.slug, "Renombrado").unwrap();
        let list = list_worlds(&base);
        let renamed = list.iter().find(|w| w.slug == "mundo-a").unwrap();
        assert_eq!(renamed.meta.display_name, "Renombrado");

        delete_world(&base, &b.slug).unwrap();
        assert_eq!(list_worlds(&base).len(), 1);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn duplicar_copia_el_binario() {
        let base = temp_base("dup");
        let a = create_world(&base, "Original", 1, 10).unwrap();
        std::fs::write(a.dir(&base).join(WORLD_FILE), b"datos").unwrap();
        let copy = duplicate_world(&base, &a.slug, 20).unwrap();
        assert!(copy.meta.display_name.contains("copia"));
        assert_eq!(
            std::fs::read(copy.dir(&base).join(WORLD_FILE)).unwrap(),
            b"datos"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn un_level_json_corrupto_no_pierde_la_lista() {
        let base = temp_base("corrupt");
        let a = create_world(&base, "Bueno", 1, 10).unwrap();
        let bad_dir = saves_dir(&base).join("roto");
        std::fs::create_dir_all(&bad_dir).unwrap();
        std::fs::write(bad_dir.join(LEVEL_FILE), b"{ esto no es json").unwrap();
        let list = list_worlds(&base);
        assert_eq!(list.len(), 2);
        let bad = list.iter().find(|w| w.slug == "roto").unwrap();
        assert!(bad.corrupt);
        assert!(!list.iter().find(|w| w.slug == a.slug).unwrap().corrupt);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn el_guardado_de_meta_es_atomico_y_legible() {
        let base = temp_base("atomic");
        let a = create_world(&base, "Atom", 5, 10).unwrap();
        let dir = a.dir(&base);
        assert!(dir.join(LEVEL_FILE).exists());
        assert!(
            !dir.join(format!("{LEVEL_FILE}.tmp")).exists(),
            "tmp limpiado"
        );
        let raw = std::fs::read_to_string(dir.join(LEVEL_FILE)).unwrap();
        assert!(raw.contains("display_name"));
        assert!(raw.contains("seed"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn importa_el_world_vf_antiguo_una_sola_vez() {
        let base = temp_base("import");
        std::fs::write(base.join(WORLD_FILE), b"antiguo").unwrap();
        let first = import_legacy(&base, 100).unwrap().unwrap();
        assert_eq!(first.slug, "mundo-importado");
        assert!(first.dir(&base).join(WORLD_FILE).exists());
        // La segunda vez ya hay mundos: no reimporta.
        assert!(import_legacy(&base, 200).unwrap().is_none());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn la_semilla_de_texto_es_determinista_y_estable() {
        // Un entero se usa tal cual.
        assert_eq!(seed_from_text("42"), 42);
        assert_eq!(seed_from_text("  13371  "), 13_371);
        // Un texto da una semilla estable y distinta segun el texto.
        let a = seed_from_text("solaria");
        assert_eq!(a, seed_from_text("solaria"));
        assert_ne!(a, seed_from_text("solaria2"));
        // Valor fijado (FNV-1a 64 truncado): si cambia el algoritmo, falla.
        assert_eq!(seed_from_text("stone"), 869_850_871);
    }
}
