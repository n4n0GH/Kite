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

    CxxQtBuilder::new_qml_module(QmlModule::new("org.kde.kite").qml_files(qml))
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
        .files(["src/browser.rs", "src/extensions.rs", "src/passwords.rs", "src/formfill.rs"])
        .build();

    // QtWebEngineQuick::initialize() and QQuickWebEngineProfile — link the
    // runtime library directly since a -devel package may be absent.
    println!("cargo::rustc-link-arg=-l:libQt6WebEngineQuick.so.6");
}
