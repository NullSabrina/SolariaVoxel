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
//! Base por defecto: `SOLARIA_HOME` si existe; si no, una biblioteca ya
//! existente en el directorio de trabajo (compatibilidad) y, en su defecto,
//! junto al ejecutable (estable aunque se lance desde un acceso directo). Ver
//! [`base_dir_from_env`].

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
/// Marca de que el `world.vf` legado ya se importo (evita reimportarlo tras
/// borrar todos los mundos, P1-2).
pub const LEGACY_MARKER: &str = ".legacy-imported";
/// Longitud maxima del slug.
pub const MAX_SLUG_LEN: usize = 48;

fn default_game_mode() -> String {
    "creative".to_string()
}

fn default_generator() -> String {
    "legacy16".to_string()
}

/// Los mundos creados antes de P0.0 no tienen `generator_config`: se asume la
/// version 1 del pipeline (la historica).
fn default_generator_config() -> u32 {
    super::generator::LEGACY_GENERATOR_CONFIG
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
    /// Version del **pipeline** del generador (1 = historico, 2 = P0.1).
    /// Los `level.json` antiguos no lo traen: [`default_generator_config`]
    /// devuelve 1 y se avisa una vez por consola.
    #[serde(default = "default_generator_config")]
    pub generator_config: u32,
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

/// Directorio base de la biblioteca.
///
/// Precedencia (P1-3):
/// 1. `SOLARIA_HOME`, si esta definido.
/// 2. Una biblioteca `saves/` **ya existente** en el directorio de trabajo
///    (compatibilidad: no se pierden mundos de versiones anteriores).
/// 3. El directorio del ejecutable (estable al lanzar desde un acceso directo;
///    antes dependia del cwd y la biblioteca "desaparecia").
/// 4. El directorio de trabajo si el del ejecutable no admite escritura.
pub fn base_dir_from_env() -> PathBuf {
    if let Ok(home) = std::env::var("SOLARIA_HOME") {
        return PathBuf::from(home);
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let chosen = resolve_base_dir(cwd.clone(), exe_dir);
    // Si el directorio del ejecutable no es escribible (p. ej. Program Files),
    // no se puede crear la biblioteca: se usa el de trabajo como ultimo recurso.
    if chosen != cwd && std::fs::create_dir_all(saves_dir(&chosen)).is_err() {
        return cwd;
    }
    chosen
}

/// Politica pura de [`base_dir_from_env`] (testeable, sin efectos laterales).
pub fn resolve_base_dir(cwd: PathBuf, exe_dir: Option<PathBuf>) -> PathBuf {
    let Some(exe) = exe_dir else {
        return cwd;
    };
    if exe == cwd {
        return exe;
    }
    // Compatibilidad: si ya hay una biblioteca en el directorio de trabajo y no
    // junto al ejecutable, se conserva para no "perder" los mundos existentes.
    let cwd_has = saves_dir(&cwd).is_dir();
    let exe_has = saves_dir(&exe).is_dir();
    if cwd_has && !exe_has {
        return cwd;
    }
    exe
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

/// ¿El slug es seguro para unirse a `saves/`? Solo `[a-z0-9_-]`, sin `.`/`..`
/// ni separadores, longitud acotada y sin nombres reservados (P2-13).
pub fn is_safe_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= MAX_SLUG_LEN
        && slug != "."
        && slug != ".."
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        && !is_reserved(slug)
}

/// Valida un slug o devuelve `InvalidInput`.
fn check_slug(slug: &str) -> std::io::Result<()> {
    if is_safe_slug(slug) {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("slug no valido: {slug:?}"),
        ))
    }
}

/// Escribe `level.json` de forma **atomica** (`.tmp` -> rename).
pub fn save_meta(dir: &Path, meta: &WorldMeta) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{LEVEL_FILE}.tmp"));
    let final_path = dir.join(LEVEL_FILE);
    let bak = dir.join(format!("{LEVEL_FILE}.bak"));
    let json = serde_json::to_string_pretty(meta)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    // P2-10: sincronizar el temporal antes de rotar/renombrar.
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
    }
    // Windows: `rename` no sobreescribe. Rotamos el actual a `.bak` (en vez de
    // borrarlo): si algo falla entre medias, `load_meta` recupera el `.bak`.
    if final_path.exists() {
        let _ = std::fs::remove_file(&bak);
        std::fs::rename(&final_path, &bak)?;
    }
    std::fs::rename(&tmp, &final_path)?;
    sync_dir(dir);
    Ok(())
}

/// `fsync` del directorio (durabilidad de los `rename`), solo en Unix.
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    {
        if let Ok(file) = std::fs::File::open(dir) {
            let _ = file.sync_all();
        }
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
    }
}

/// Carga `level.json` de una carpeta (o `None` si falta/corrupto). Si el
/// principal falta o esta corrupto, prueba el `.bak` de la ultima escritura.
pub fn load_meta(dir: &Path) -> Option<WorldMeta> {
    let read = |p: std::path::PathBuf| -> Option<WorldMeta> {
        let data = std::fs::read_to_string(p).ok()?;
        let value: serde_json::Value = serde_json::from_str(&data).ok()?;
        // No migrar en silencio: un `level.json` sin el campo es un mundo
        // historico (v1). Se avisa una sola vez por proceso.
        if value.get("generator_config").is_none() {
            static WARNED: std::sync::Once = std::sync::Once::new();
            WARNED.call_once(|| {
                eprintln!("[world] level.json sin generator_config: se asume v1 (mundo existente)");
            });
        }
        serde_json::from_value(value).ok()
    };
    read(dir.join(LEVEL_FILE)).or_else(|| read(dir.join(format!("{LEVEL_FILE}.bak"))))
}

/// Lista los mundos de `base`, ordenados por ultima vez jugado (desc).
///
/// Si no se puede listar la carpeta **no** se finge una lista vacia: se registra
/// el error y se devuelve vacio solo como ultimo recurso (P2-11). Los llamadores
/// que necesitan distinguir el error usan [`list_worlds_checked`].
pub fn list_worlds(base: &Path) -> Vec<WorldEntry> {
    match list_worlds_checked(base) {
        Ok(worlds) => worlds,
        Err(e) => {
            eprintln!(
                "[world] no se pudo listar {}: {e}",
                saves_dir(base).display()
            );
            Vec::new()
        }
    }
}

/// Version que **propaga** los errores de listado (salvo `NotFound`, que es una
/// biblioteca aun vacia).
pub fn list_worlds_checked(base: &Path) -> std::io::Result<Vec<WorldEntry>> {
    let dir = saves_dir(base);
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                eprintln!("[world] entrada ilegible en {}: {e}", dir.display());
                continue;
            }
        };
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
                    generator_config: default_generator_config(),
                },
                corrupt: true,
            }),
        }
    }
    out.sort_by_key(|w| std::cmp::Reverse(w.meta.last_played));
    Ok(out)
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
    if name.trim().is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "el nombre del mundo no puede estar vacio",
        ));
    }
    // P2-11: un fallo de listado no puede llevar a reutilizar un slug existente.
    let mut taken: Vec<String> = list_worlds_checked(base)?
        .into_iter()
        .map(|w| w.slug)
        .collect();
    if let Ok(entries) = std::fs::read_dir(saves_dir(base)) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                taken.push(entry.file_name().to_string_lossy().to_string());
            }
        }
    }
    let slug = unique_slug(&slugify(name), &taken);
    let meta = WorldMeta {
        display_name: name.to_string(),
        seed,
        game_mode: default_game_mode(),
        created_at: now,
        last_played: now,
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        generator_kind: generator.to_string(),
        // Mundo nuevo: usa el pipeline vigente (v2 a partir de P0.0).
        generator_config: super::generator::CURRENT_GENERATOR_CONFIG,
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
    check_slug(slug)?;
    if new_name.trim().is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "el nombre del mundo no puede estar vacio",
        ));
    }
    let dir = saves_dir(base).join(slug);
    let mut meta = load_meta(&dir)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "sin level.json"))?;
    meta.display_name = new_name.to_string();
    save_meta(&dir, &meta)
}

/// Duplica un mundo (copia binario + metadatos) con un slug nuevo.
pub fn duplicate_world(base: &Path, slug: &str, now: u64) -> std::io::Result<WorldEntry> {
    check_slug(slug)?;
    let src = saves_dir(base).join(slug);
    let mut meta = load_meta(&src)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "sin level.json"))?;
    let taken: Vec<String> = list_worlds_checked(base)?
        .into_iter()
        .map(|w| w.slug)
        .collect();
    let new_slug = unique_slug(&format!("{slug}-copia"), &taken);
    let dst = saves_dir(base).join(&new_slug);
    if dst.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "ya existe la carpeta de la copia",
        ));
    }
    // P2-12: se construye en una carpeta temporal y se renombra al final; si
    // algo falla no queda una carpeta a medias visible en la lista.
    let tmp = saves_dir(base).join(format!(".{new_slug}.tmp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let build = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(&tmp)?;
        for file in [WORLD_FILE, WORLD_BAK] {
            let from = src.join(file);
            if from.exists() {
                std::fs::copy(&from, tmp.join(file))?;
            }
        }
        meta.display_name = format!("{} (copia)", meta.display_name);
        meta.created_at = now;
        meta.last_played = now;
        save_meta(&tmp, &meta)?;
        std::fs::rename(&tmp, &dst)?;
        Ok(())
    })();
    if let Err(e) = build {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    Ok(WorldEntry {
        slug: new_slug,
        meta,
        corrupt: false,
    })
}

/// Elimina un mundo (carpeta completa).
pub fn delete_world(base: &Path, slug: &str) -> std::io::Result<()> {
    check_slug(slug)?;
    std::fs::remove_dir_all(saves_dir(base).join(slug))
}

/// Marca `last_played = now` (al guardar/salir de un mundo).
pub fn touch_last_played(base: &Path, slug: &str, now: u64) -> std::io::Result<()> {
    check_slug(slug)?;
    let dir = saves_dir(base).join(slug);
    let mut meta = load_meta(&dir)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "sin level.json"))?;
    meta.last_played = now;
    save_meta(&dir, &meta)
}

/// Importa el `world.vf` antiguo (junto al ejecutable) a `saves/` si existe y no
/// hay mundos. **Copia** (no mueve) y deja una marca (`.legacy-imported`) para
/// no reimportarlo al borrar todos los mundos (P1-2). Devuelve la entrada
/// importada.
pub fn import_legacy(base: &Path, now: u64) -> std::io::Result<Option<WorldEntry>> {
    let legacy = base.join(WORLD_FILE);
    if !legacy.exists() {
        return Ok(None);
    }
    // Ya se importo una vez: no debe "resucitar" aunque la lista este vacia.
    if base.join(LEGACY_MARKER).exists() {
        return Ok(None);
    }
    if !list_worlds_checked(base)?.is_empty() {
        return Ok(None);
    }
    let mut taken: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(saves_dir(base)) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                taken.push(entry.file_name().to_string_lossy().to_string());
            }
        }
    }
    let slug = unique_slug(&slugify("Mundo importado"), &taken);
    let dst = saves_dir(base).join(&slug);
    std::fs::create_dir_all(&dst)?;
    std::fs::copy(&legacy, dst.join(WORLD_FILE))?;
    let legacy_bak = base.join(WORLD_BAK);
    if legacy_bak.exists() {
        let _ = std::fs::copy(&legacy_bak, dst.join(WORLD_BAK));
    }
    // P2-14: solo se lee la cabecera (no se descomprime el mundo entero).
    let seed = match super::save::WorldSave::peek_header(&legacy) {
        Ok(header) => header.seed,
        Err(e) => {
            eprintln!("[world] no se pudo leer la cabecera del mundo legado: {e}");
            0
        }
    };
    let meta = WorldMeta {
        display_name: "Mundo importado".to_string(),
        seed,
        game_mode: default_game_mode(),
        created_at: now,
        last_played: now,
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        generator_kind: default_generator(),
        // El `world.vf` legado se genero con el pipeline historico.
        generator_config: super::generator::LEGACY_GENERATOR_CONFIG,
    };
    save_meta(&dst, &meta)?;
    // Marca de importacion: borrar todos los mundos no debe resucitar el legado.
    std::fs::write(base.join(LEGACY_MARKER), b"1")?;
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
    fn un_mundo_nuevo_conserva_semilla_y_generador() {
        // Contrato del que depende el arreglo de "no puedo entrar en un mundo
        // nuevo": un mundo recien creado solo tiene `level.json`, y la entrada al
        // mundo usa la semilla/generador de esos metadatos.
        let base = temp_base("seedgen");
        let w = create_world_kind(&base, "Con semilla", 123_456, 10, "larion").unwrap();
        let meta = load_meta(&w.dir(&base)).unwrap();
        assert_eq!(meta.seed, 123_456);
        assert_eq!(meta.generator_kind, "larion");
        assert_eq!(
            crate::world::GeneratorKind::from_name(&meta.generator_kind),
            crate::world::GeneratorKind::Larion
        );
        // Aun no hay `world.vf`: es justo el caso que antes rechazaba la entrada.
        assert!(!w.dir(&base).join(WORLD_FILE).exists());
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
    fn el_legado_no_resucita_tras_borrar_todos_los_mundos() {
        // P1-2: importar deja marca; borrar todos no lo reimporta al reiniciar.
        let base = temp_base("legacy_resurrect");
        std::fs::write(base.join(WORLD_FILE), b"antiguo").unwrap();
        let first = import_legacy(&base, 10).unwrap().unwrap();
        assert!(base.join(LEGACY_MARKER).exists());
        delete_world(&base, &first.slug).unwrap();
        assert!(list_worlds(&base).is_empty());
        assert!(
            import_legacy(&base, 20).unwrap().is_none(),
            "no debe reimportar el legado"
        );
        assert!(!saves_dir(&base).join("mundo-importado").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn la_politica_de_directorio_base_es_estable() {
        // P1-3: sin biblioteca previa usa el ejecutable; conserva la del cwd.
        let cwd = temp_base("base_cwd");
        let exe = temp_base("base_exe");
        let _ = std::fs::remove_dir_all(saves_dir(&cwd));
        let _ = std::fs::remove_dir_all(saves_dir(&exe));
        assert_eq!(resolve_base_dir(cwd.clone(), Some(exe.clone())), exe);

        std::fs::create_dir_all(saves_dir(&cwd)).unwrap();
        assert_eq!(resolve_base_dir(cwd.clone(), Some(exe.clone())), cwd);

        std::fs::create_dir_all(saves_dir(&exe)).unwrap();
        assert_eq!(resolve_base_dir(cwd.clone(), Some(exe.clone())), exe);
        assert_eq!(resolve_base_dir(cwd.clone(), None), cwd);
        let _ = std::fs::remove_dir_all(&cwd);
        let _ = std::fs::remove_dir_all(&exe);
    }

    #[test]
    fn duplicar_es_atomico_y_copia_el_backup() {
        // P2-12: sin carpetas parciales y con el .bak incluido.
        let base = temp_base("dup_atomic");
        let a = create_world(&base, "Original", 1, 10).unwrap();
        std::fs::write(a.dir(&base).join(WORLD_FILE), b"main").unwrap();
        std::fs::write(a.dir(&base).join(WORLD_BAK), b"bak").unwrap();
        let copy = duplicate_world(&base, &a.slug, 20).unwrap();
        assert_eq!(
            std::fs::read(copy.dir(&base).join(WORLD_FILE)).unwrap(),
            b"main"
        );
        assert_eq!(
            std::fs::read(copy.dir(&base).join(WORLD_BAK)).unwrap(),
            b"bak"
        );
        // No quedan temporales de la duplicacion.
        let leftovers: Vec<_> = std::fs::read_dir(saves_dir(&base))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "quedaron temporales");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn los_slugs_inseguros_se_rechazan() {
        // P2-13: `..` no debe poder borrar la biblioteca entera.
        let base = temp_base("slug_guard");
        let a = create_world(&base, "Seguro", 1, 10).unwrap();
        assert!(delete_world(&base, "..").is_err());
        assert!(delete_world(&base, "../x").is_err());
        assert!(rename_world(&base, &a.slug, "   ").is_err());
        assert!(duplicate_world(&base, "../x", 1).is_err());
        assert!(touch_last_played(&base, "a/b", 1).is_err());
        assert!(is_safe_slug(&a.slug));
        assert!(!is_safe_slug(".."));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn crear_no_reutiliza_una_carpeta_existente() {
        // P2-11: aunque la carpeta no tenga metadatos, no se pisa.
        let base = temp_base("slug_collision");
        std::fs::create_dir_all(saves_dir(&base).join("mundo-x")).unwrap();
        let w = create_world(&base, "Mundo X", 1, 1).unwrap();
        assert_eq!(w.slug, "mundo-x-2");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn los_metadatos_antiguos_asumen_generator_config_1() {
        // P0.0: un level.json sin el campo es un mundo historico. No se migra en
        // silencio; `load_meta` aplica 1.
        let base = temp_base("gencfg_old");
        let dir = saves_dir(&base).join("viejo");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(LEVEL_FILE),
            r#"{"display_name":"Viejo","seed":7,"created_at":1,"last_played":1}"#,
        )
        .unwrap();
        let meta = load_meta(&dir).expect("metadatos legibles");
        assert_eq!(meta.generator_config, 1);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn un_mundo_nuevo_nace_con_generator_config_actual() {
        let base = temp_base("gencfg_new");
        let w = create_world_kind(&base, "Nuevo", 1, 1, "larion").unwrap();
        assert_eq!(
            w.meta.generator_config,
            crate::world::generator::CURRENT_GENERATOR_CONFIG
        );
        let meta = load_meta(&w.dir(&base)).unwrap();
        assert_eq!(
            meta.generator_config,
            crate::world::generator::CURRENT_GENERATOR_CONFIG
        );
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
