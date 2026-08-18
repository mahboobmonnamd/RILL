//! SPEC-CONFIG: canonical schema resolution (ADR 0025).
//!
//! This module owns the resolution *mechanism* — precedence order, a cold
//! single-read snapshot, one-file writeback, and keybinding conflict
//! detection. It does not own the look/theme schema itself
//! (`host-surface.toml`, ADR 0017); that already exists and is Proven
//! elsewhere, and this module must not compete with it.

use std::path::Path;
use std::rc::Rc;

/// Precedence, highest first (SPEC-CONFIG §3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigLayer {
    Flag,
    Env,
    ProjectTrusted,
    UserFile,
    ShippedDefault,
}

/// One key found in more than one layer, dropped rather than guessed
/// (SPEC-CONFIG §1, T-LOOK-UNKNOWN's rule generalized).
#[derive(Clone, Debug)]
pub struct UnknownKey {
    pub layer: ConfigLayer,
    pub key: String,
}

struct LayeredTable<'a> {
    layers: Vec<(ConfigLayer, &'a toml::value::Table)>,
}

impl<'a> LayeredTable<'a> {
    /// Resolve every key the shipped-default schema declares. A key present
    /// in a layer but not in `schema_keys` is unknown: reported via the
    /// second return value, dropped from the resolved table.
    fn resolve(&self, schema_keys: &[&str]) -> (toml::value::Table, Vec<UnknownKey>) {
        let order: Vec<usize> = if cfg!(feature = "mutate")
            && std::env::var("RILL_MUTATE").as_deref() == Ok("resolution_order_reversed")
        {
            (0..self.layers.len()).rev().collect()
        } else {
            (0..self.layers.len()).collect()
        };

        let mut table = toml::value::Table::new();
        for key in schema_keys {
            for &i in &order {
                let (_, t) = &self.layers[i];
                if let Some(v) = t.get(*key) {
                    table.insert((*key).to_string(), v.clone());
                    break;
                }
            }
        }
        let mut unknown = Vec::new();
        for (layer, t) in &self.layers {
            for k in t.keys() {
                if !schema_keys.contains(&k.as_str()) {
                    unknown.push(UnknownKey {
                        layer: *layer,
                        key: k.clone(),
                    });
                }
            }
        }
        (table, unknown)
    }
}

/// A resolved configuration: built once from its layers (cold), then read
/// any number of times without re-consulting a source (SPEC-CONFIG §3 —
/// "MUST NOT be consulted per frame or per key").
pub struct ResolvedConfig {
    values: toml::value::Table,
    unknown: Vec<UnknownKey>,
    #[cfg(feature = "mutate")]
    schema_keys: Vec<String>,
    #[cfg(feature = "mutate")]
    hot_readers: Option<Vec<(ConfigLayer, Rc<dyn Fn() -> toml::value::Table>)>>,
}

/// Build a `ResolvedConfig` from ordered layer readers (highest precedence
/// first). Each reader is invoked exactly once here; `ResolvedConfig::get`
/// never invokes a reader again.
pub fn resolve_once(
    layers: Vec<(ConfigLayer, Rc<dyn Fn() -> toml::value::Table>)>,
    schema_keys: &[&str],
) -> ResolvedConfig {
    let tables: Vec<(ConfigLayer, toml::value::Table)> =
        layers.iter().map(|(l, f)| (*l, f())).collect();
    let refs: Vec<(ConfigLayer, &toml::value::Table)> =
        tables.iter().map(|(l, t)| (*l, t)).collect();
    let (values, unknown) = LayeredTable { layers: refs }.resolve(schema_keys);

    #[cfg(feature = "mutate")]
    let hot_readers = if std::env::var("RILL_MUTATE").as_deref() == Ok("resolve_reads_per_query") {
        Some(layers)
    } else {
        None
    };

    ResolvedConfig {
        values,
        unknown,
        #[cfg(feature = "mutate")]
        schema_keys: schema_keys.iter().map(|s| s.to_string()).collect(),
        #[cfg(feature = "mutate")]
        hot_readers,
    }
}

impl ResolvedConfig {
    /// Cold read. Under the `resolve_reads_per_query` mutation this
    /// re-invokes every reader on every call instead — the regression
    /// SPEC-CONFIG §3 forbids.
    pub fn get(&self, key: &str) -> Option<toml::Value> {
        #[cfg(feature = "mutate")]
        if let Some(readers) = &self.hot_readers {
            let tables: Vec<(ConfigLayer, toml::value::Table)> =
                readers.iter().map(|(l, f)| (*l, f())).collect();
            let refs: Vec<(ConfigLayer, &toml::value::Table)> =
                tables.iter().map(|(l, t)| (*l, t)).collect();
            let schema: Vec<&str> = self.schema_keys.iter().map(|s| s.as_str()).collect();
            let (table, _) = LayeredTable { layers: refs }.resolve(&schema);
            return table.get(key).cloned();
        }
        self.values.get(key).cloned()
    }

    pub fn unknown_keys(&self) -> &[UnknownKey] {
        &self.unknown
    }
}

/// Write a partial update into the user's config file, in place — never a
/// second, parallel store (SPEC-CONFIG §2). Keys not in `updates` are
/// preserved exactly as read.
pub fn write_user_config(path: &Path, updates: &toml::value::Table) -> std::io::Result<()> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let mut doc: toml::value::Table = toml::from_str(&existing).unwrap_or_default();
    for (k, v) in updates {
        doc.insert(k.clone(), v.clone());
    }

    #[cfg(feature = "mutate")]
    if std::env::var("RILL_MUTATE").as_deref() == Ok("settings_write_shadow_store") {
        let shadow = path.with_extension("shadow.toml");
        let text = toml::to_string_pretty(&doc).unwrap_or_default();
        return std::fs::write(shadow, text);
    }

    let text = toml::to_string_pretty(&doc).map_err(std::io::Error::other)?;
    std::fs::write(path, text)
}

/// A keyboard chord: modifier bitflags plus a key name.
pub const CTRL: u8 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyChord {
    pub modifiers: u8,
    pub key: String,
}

#[derive(Clone, Debug)]
pub struct Binding {
    pub chord: KeyChord,
    pub action: String,
}

pub struct SwallowWarning {
    pub chord: KeyChord,
    pub bound_to: String,
}

/// Control characters a raw-mode child relies on (SIGINT, EOF, SIGTSTP,
/// SIGQUIT). Not exhaustive — enough to demonstrate the check.
fn reserved_control_chords() -> Vec<KeyChord> {
    ["c", "d", "z", "\\"]
        .iter()
        .map(|k| KeyChord {
            modifiers: CTRL,
            key: (*k).to_string(),
        })
        .collect()
}

/// Report — at load time, not at press time — any binding that would
/// swallow a control character a raw-mode child needs (SPEC-CONFIG §5).
pub fn detect_swallowed_control_chars(bindings: &[Binding]) -> Vec<SwallowWarning> {
    #[cfg(feature = "mutate")]
    if std::env::var("RILL_MUTATE").as_deref() == Ok("skip_swallow_check") {
        return Vec::new();
    }
    let reserved = reserved_control_chords();
    bindings
        .iter()
        .filter(|b| reserved.contains(&b.chord))
        .map(|b| SwallowWarning {
            chord: b.chord.clone(),
            bound_to: b.action.clone(),
        })
        .collect()
}
