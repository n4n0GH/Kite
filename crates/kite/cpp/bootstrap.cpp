// Minimal C++ bootstrap: QtWebEngine must be initialised before the
// application object exists, URL schemes must be registered before that, and
// the KDE desktop style needs a QApplication. Everything else lives in Rust
// (src/) and QML (qml/).
#include <QApplication>
#include <QBuffer>
#include <QIcon>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickImageProvider>

// The QtWebEngineQuick headers are not always packaged (openSUSE ships the
// runtime only), so declare the single exported entry point we need.
namespace QtWebEngineQuick { void initialize(); }

void kite_register_schemes();
void kite_install_profile_hooks(QObject *profile, bool isPrimary);
extern "C" void kite_log(int level, const char *category, const char *message);
extern "C" void kite_favicon_ready(const char *pageUrl, const unsigned char *png, size_t len);

static QQmlApplicationEngine *g_engine = nullptr;

// Store a page's favicon in Kite's icon cache (for the speed dial). The
// image comes from QtWebEngine's own "favicon" image provider, which
// already holds the icon the page loaded, so the site isn't contacted again.
extern "C" void kite_cpp_save_favicon(const char *pageUrl, const char *iconUrl)
{
    static const QString prefix = QStringLiteral("image://favicon/");
    const QString icon = QString::fromUtf8(iconUrl);
    if (!g_engine || !icon.startsWith(prefix)) return;
    auto *provider = dynamic_cast<QQuickAsyncImageProvider *>(g_engine->imageProvider(QStringLiteral("favicon")));
    if (!provider) return;
    QQuickImageResponse *response = provider->requestImageResponse(icon.mid(prefix.size()), QSize(64, 64));
    if (!response) return;
    const QByteArray page(pageUrl);
    // The response may finish on another thread; handle it on the GUI thread.
    QObject::connect(response, &QQuickImageResponse::finished, g_engine, [response, page] {
        QImage image;
        if (response->errorString().isEmpty()) {
            if (QQuickTextureFactory *factory = response->textureFactory()) {
                image = factory->image();
                delete factory;
            }
        }
        response->deleteLater();
        if (image.isNull()) return;
        QByteArray png;
        QBuffer buffer(&png);
        buffer.open(QIODevice::WriteOnly);
        image.save(&buffer, "PNG");
        kite_favicon_ready(page.constData(), reinterpret_cast<const unsigned char *>(png.constData()), size_t(png.size()));
    }, Qt::QueuedConnection);
}

// All Qt and QML output (including web page consoles, see Main.qml
// logConsole) goes to Kite's logger, filtered with RUST_LOG. Qt's default
// handler would send it to the journal whenever stderr isn't a terminal.
static void kiteMessageHandler(QtMsgType type, const QMessageLogContext &context, const QString &message)
{
    int level = 4; // debug
    switch (type) {
    case QtInfoMsg: level = 3; break;
    case QtWarningMsg: level = 2; break;
    case QtCriticalMsg:
    case QtFatalMsg: level = 1; break;
    default: break;
    }
    const QByteArray text = message.toUtf8();
    kite_log(level, context.category ? context.category : "default", text.constData());
}

extern "C" int kite_bootstrap(int argc, char **argv)
{
    qInstallMessageHandler(kiteMessageHandler);
    kite_register_schemes();
    QCoreApplication::setAttribute(Qt::AA_ShareOpenGLContexts);
    if (qEnvironmentVariableIsEmpty("QT_QUICK_CONTROLS_STYLE"))
        qputenv("QT_QUICK_CONTROLS_STYLE", "org.kde.desktop");
    // Chromium flags: allow our moz-extension pages to use the QWebChannel
    // transport and keep GPU rasterization on.
    if (qEnvironmentVariableIsEmpty("QTWEBENGINE_CHROMIUM_FLAGS"))
        qputenv("QTWEBENGINE_CHROMIUM_FLAGS", "--enable-gpu-rasterization");
    QtWebEngineQuick::initialize();

    static int s_argc = argc;
    QApplication app(s_argc, argv);
    QApplication::setApplicationName(QStringLiteral("kite"));
    QApplication::setOrganizationName(QStringLiteral("kde"));
    QApplication::setApplicationDisplayName(QStringLiteral("Kite"));
    QApplication::setApplicationVersion(QStringLiteral("0.1.0"));
    QApplication::setDesktopFileName(QStringLiteral("org.kde.kite"));
    QApplication::setWindowIcon(QIcon::fromTheme(QStringLiteral("org.kde.kite"), QIcon::fromTheme(QStringLiteral("internet-web-browser"))));
    QApplication::setQuitOnLastWindowClosed(false);

    QQmlApplicationEngine engine;
    g_engine = &engine;
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app,
                     [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    // Install network hooks on each profile as soon as QML creates it.
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreated, &app, [](QObject *obj, const QUrl &) {
        if (!obj) return;
        if (auto *p = obj->property("profile").value<QObject *>()) kite_install_profile_hooks(p, true);
        if (auto *p = obj->property("privateProfile").value<QObject *>()) kite_install_profile_hooks(p, false);
        QMetaObject::invokeMethod(obj, "start", Qt::QueuedConnection);
    });
    engine.loadFromModule("org.kde.kite", "Main");
    const int code = app.exec();
    g_engine = nullptr;
    return code;
}
