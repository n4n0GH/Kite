use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    fn files(dir: &std::path::Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).expect("resource dir").filter_map(|e| e.ok()) {
            let p = e.path();
            if p.is_dir() { files(&p, out) } else { out.push(p.to_string_lossy().into_owned()) }
        }
    }
    let mut qml: Vec<String> = std::fs::read_dir("qml")
        .expect("qml dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "qml"))
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    // Plugins' QML files are part of the module too, under plugins/<id>/ (so
    // their names must differ from all others).
    let plugin_files = {
        let mut out = Vec::new();
        files(std::path::Path::new("plugins"), &mut out);
        out
    };
    qml.extend(plugin_files.iter().filter(|f| f.ends_with(".qml")).cloned());
    qml.sort();
    for f in &qml {
        println!("cargo::rerun-if-changed={f}");
    }
    println!("cargo::rerun-if-changed=qml");
    println!("cargo::rerun-if-changed=plugins");
    // Icons Kite ships itself (qrc:/qt/qml/org/kde/kite/icons/…), including
    // its fallback icon theme (icons/kite/, see bootstrap.cpp).
    let mut icons = Vec::new();
    files(std::path::Path::new("icons"), &mut icons);
    icons.sort();
    // Plain JavaScript the QML files import (tested with node, not QML types).
    for js in ["qml/tabtree.js", "qml/fuzzy.js", "qml/palette.js", "qml/contextmenu.js"] {
        icons.push(js.to_owned());
    }
    icons.extend(plugin_files.iter().filter(|f| f.ends_with(".js") && !f.ends_with(".test.js")).cloned());
    println!("cargo::rerun-if-changed=icons");
    // (icons/org.kde.kite.svg is a link to the application's icon.)
    println!("cargo::rerun-if-changed=../../data/org.kde.kite.svg");

    // Wayland's client headers (cpp/pointerlock.cpp), wherever pkg-config says.
    let wayland_includes: Vec<String> = std::process::Command::new("pkg-config")
        .args(["--cflags-only-I", "wayland-client"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).split_whitespace().filter_map(|f| f.strip_prefix("-I")).map(str::to_owned).collect())
        .unwrap_or_else(|| vec!["/usr/include/wayland".to_owned()]);
    println!("cargo::rerun-if-changed=cpp/wayland");

    let builder = CxxQtBuilder::new_qml_module(QmlModule::new("org.kde.kite").qml_files(qml));
    // SAFETY: only adds include directories.
    let builder = unsafe {
        builder.cc_builder(|cc| {
            for dir in &wayland_includes {
                cc.include(dir);
            }
        })
    };
    builder
        .qrc_resources(icons)
        .qt_module("Network")
        .qt_module("Widgets")
        .qt_module("PrintSupport")
        .qt_module("WebEngineCore")
        .qt_module("Quick")
        .cpp_file("cpp/bootstrap.cpp")
        .cpp_file("cpp/glue.cpp")
        .cpp_file("cpp/print.cpp")
        .cpp_file("cpp/profiles.cpp")
        // (The header has the QML singleton PointerLock, for moc.)
        .cpp_file("cpp/pointerlock.h")
        .cpp_file("cpp/pointerlock.cpp")
        .files(["src/browser.rs", "src/extensions.rs", "src/passwords.rs", "src/formfill.rs"])
        .build();

    // QtWebEngineQuick::initialize() and QQuickWebEngineProfile — link the
    // runtime library directly since a -devel package may be absent.
    println!("cargo::rustc-link-arg=-l:libQt6WebEngineQuick.so.6");
    // Pointer lock talks to the Wayland compositor itself (cpp/pointerlock.cpp).
    println!("cargo::rustc-link-arg=-l:libwayland-client.so.0");
}
