//! SPEC-CONFIG gates: T-CFG-ORDER, T-CFG-COLD, T-CFG-ONEFILE, T-CFG-BIND.

use rill_orchestrate::config::*;
use std::cell::Cell;
use std::rc::Rc;

fn table(pairs: &[(&str, &str)]) -> toml::value::Table {
    let mut t = toml::value::Table::new();
    for (k, v) in pairs {
        t.insert((*k).to_string(), toml::Value::String((*v).to_string()));
    }
    t
}

// ------------------------------------------------------------ T-CFG-ORDER

/// Highest layer wins. The oracle reads the resolved value, not a flag the
/// test set itself.
///
/// Required mutation: `RILL_MUTATE=resolution_order_reversed`.
#[test]
fn t_cfg_order_highest_precedence_layer_wins() {
    let flag = table(&[("theme", "from-flag")]);
    let default = table(&[("theme", "from-default"), ("font-size", "16")]);
    let layers: Vec<(ConfigLayer, Rc<dyn Fn() -> toml::value::Table>)> = vec![
        (ConfigLayer::Flag, Rc::new(move || flag.clone())),
        (
            ConfigLayer::ShippedDefault,
            Rc::new(move || default.clone()),
        ),
    ];
    let resolved = resolve_once(layers, &["theme", "font-size"]);
    assert_eq!(
        resolved.get("theme"),
        Some(toml::Value::String("from-flag".into())),
        "flag layer did not win over the shipped default"
    );
    assert_eq!(
        resolved.get("font-size"),
        Some(toml::Value::String("16".into())),
        "a key only the default layer has should still resolve"
    );
}

// ------------------------------------------------------------- T-CFG-COLD

/// Resolution reads each source exactly once, regardless of how many times
/// the result is queried afterward.
///
/// Required mutation: `RILL_MUTATE=resolve_reads_per_query`.
#[test]
fn t_cfg_cold_resolution_reads_sources_exactly_once() {
    let reads = Rc::new(Cell::new(0u32));
    let reads2 = reads.clone();
    let layers: Vec<(ConfigLayer, Rc<dyn Fn() -> toml::value::Table>)> = vec![(
        ConfigLayer::UserFile,
        Rc::new(move || {
            reads2.set(reads2.get() + 1);
            table(&[("theme", "counted")])
        }),
    )];
    let resolved = resolve_once(layers, &["theme"]);
    assert_eq!(
        reads.get(),
        1,
        "the source must be read exactly once at resolve time"
    );

    for _ in 0..5 {
        let _ = resolved.get("theme");
    }
    assert_eq!(
        reads.get(),
        1,
        "querying a resolved config re-read its source — this is the per-key \
         cost SPEC-CONFIG §3 forbids"
    );
}

// ---------------------------------------------------------- T-CFG-ONEFILE

/// A settings write lands in the user's own file with untouched keys
/// preserved, and creates no second store.
///
/// Required mutation: `RILL_MUTATE=settings_write_shadow_store`.
#[test]
fn t_cfg_onefile_write_preserves_untouched_keys_no_shadow() {
    let dir = std::env::temp_dir().join(format!(
        "rill-cfg-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    std::fs::write(&path, "theme = \"latte\"\nfont-size = 16\n").unwrap();

    let mut updates = toml::value::Table::new();
    updates.insert("font-size".into(), toml::Value::Integer(18));
    write_user_config(&path, &updates).expect("write");

    let shadow = path.with_extension("shadow.toml");
    assert!(
        !shadow.exists(),
        "a second, parallel store was created next to the user's file"
    );
    let text = std::fs::read_to_string(&path).expect("real file must contain the update");
    let doc: toml::value::Table = toml::from_str(&text).unwrap();
    assert_eq!(
        doc.get("theme"),
        Some(&toml::Value::String("latte".into())),
        "an untouched key was lost by the write"
    );
    assert_eq!(
        doc.get("font-size"),
        Some(&toml::Value::Integer(18)),
        "the update was not applied to the user's file"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ------------------------------------------------------------- T-CFG-BIND

/// A binding on Ctrl+C is reported at load time, not discovered when a raw
/// child never receives SIGINT.
///
/// Required mutation: `RILL_MUTATE=skip_swallow_check`.
#[test]
fn t_cfg_bind_reports_control_character_swallow_at_load() {
    let bindings = vec![
        Binding {
            chord: KeyChord {
                modifiers: CTRL,
                key: "c".into(),
            },
            action: "app.quit".into(),
        },
        Binding {
            chord: KeyChord {
                modifiers: 0,
                key: "k".into(),
            },
            action: "app.clear".into(),
        },
    ];
    let warnings = detect_swallowed_control_chars(&bindings);
    assert_eq!(
        warnings.len(),
        1,
        "expected exactly one swallow warning, for Ctrl+C bound to app.quit"
    );
    assert_eq!(warnings[0].bound_to, "app.quit");
}
