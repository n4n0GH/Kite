use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    let qml: Vec<String> = std::fs::read_dir("qml")
        .expect("qml dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "qml"))
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let mut qml = qml;
    qml.sort();
    for f in &qml {
        println!("cargo::rerun-if-changed={f}");
    }
    println!("cargo::rerun-if-changed=qml");

    CxxQtBuilder::new_qml_module(QmlModule::new("org.kde.kite").qml_files(qml))
        .qt_module("Network")
        .qt_module("Widgets")
        .qt_module("WebEngineCore")
        .cpp_file("cpp/bootstrap.cpp")
        .cpp_file("cpp/glue.cpp")
        .files(["src/browser.rs", "src/extensions.rs", "src/passwords.rs"])
        .build();

    // QtWebEngineQuick::initialize() and QQuickWebEngineProfile — link the
    // runtime library directly since a -devel package may be absent.
    println!("cargo::rustc-link-arg=-l:libQt6WebEngineQuick.so.6");
}
