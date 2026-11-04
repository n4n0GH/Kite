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
#include <QQuickWindow>
#include <QSet>
#include <QQuickItem>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QMimeData>
#include <QElapsedTimer>
#include <QTimer>
#include <QPointer>
#include <QWheelEvent>

// The QtWebEngineQuick headers are not always packaged (openSUSE ships the
// runtime only), so declare the single exported entry point we need.
namespace QtWebEngineQuick { void initialize(); }

void kite_register_schemes();
void kite_install_profile_hooks(QObject *profile, int kind);
extern "C" void kite_log(int level, const char *category, const char *message);
extern "C" void kite_favicon_ready(const char *pageUrl, const unsigned char *png, size_t len);

static QQmlApplicationEngine *g_engine = nullptr;

// Input the QML side can't see because web pages consume it first:
// * tapping Alt alone shows the menu bar briefly (like Firefox and Falkon):
//   BrowserWindow.altTapped();
// * presses while that menu bar is shown: BrowserWindow.windowPressed(x, y),
//   so a click elsewhere hides it again;
// * a window losing focus closes its open menus (e.g. right-clicking the
//   title bar opens the window manager's menu, a click Qt Quick never sees).
class KiteInputFilter : public QObject {
public:
    using QObject::QObject;
protected:
    bool eventFilter(QObject *obj, QEvent *e) override
    {
        // Every event reaches the window first; ignore its re-deliveries.
        auto *win = qobject_cast<QQuickWindow *>(obj);
        if (!win) return false;
        switch (e->type()) {
        case QEvent::KeyPress: {
            auto *k = static_cast<QKeyEvent *>(e);
            m_altAlone = k->key() == Qt::Key_Alt && !k->isAutoRepeat() && (k->modifiers() & ~Qt::AltModifier) == Qt::NoModifier;
            break;
        }
        case QEvent::KeyRelease: {
            auto *k = static_cast<QKeyEvent *>(e);
            // Only browser windows have a menu bar (not Preferences, …).
            if (k->key() == Qt::Key_Alt && !k->isAutoRepeat() && m_altAlone && win->metaObject()->indexOfMethod("altTapped()") >= 0)
                QMetaObject::invokeMethod(win, "altTapped", Qt::QueuedConnection);
            m_altAlone = false;
            break;
        }
        case QEvent::MouseButtonPress: {
            m_altAlone = false;
            if (win->property("menuBarPeek").toBool()) {
                const QPointF p = static_cast<QMouseEvent *>(e)->scenePosition();
                QMetaObject::invokeMethod(win, "windowPressed", Q_ARG(QVariant, p.x()), Q_ARG(QVariant, p.y()));
            }
            break;
        }
        case QEvent::Wheel:
            m_altAlone = false;
            break;
        case QEvent::MouseMove: {
            auto *m = static_cast<QMouseEvent *>(e);
            if (m->buttons() == Qt::NoButton && overlayScrollbars())
                revealScrollbar(win, m->scenePosition());
            break;
        }
        case QEvent::WindowDeactivate:
            closeMenus(win);
            break;
        // A drag over a browser window (from a page, or another app): the
        // window reveals a hidden tab bar to drop on (BrowserWindow.qml) if
        // the tab bar can take what is dragged (TabStrip.qml: URLs, text).
        // Pointer moves of pages that capture the mouse (games) are mouse
        // events, never drags; a page's own drag of custom data (e.g. game
        // pieces) is not shown the tab bar.
        case QEvent::DragEnter: {
            const QMimeData *data = static_cast<QDragEnterEvent *>(e)->mimeData();
            const bool usable = data && (!data->urls().isEmpty() || !data->text().trimmed().isEmpty());
            if (usable && win->metaObject()->indexOfMethod("dragEntered()") >= 0)
                QMetaObject::invokeMethod(win, "dragEntered", Qt::QueuedConnection);
            break;
        }
        case QEvent::DragLeave:
        case QEvent::Drop:
            if (win->metaObject()->indexOfMethod("dragLeft()") >= 0)
                QMetaObject::invokeMethod(win, "dragLeft", Qt::QueuedConnection);
            break;
        default:
            break;
        }
        return false;
    }
private:
    // Overlay page scrollbars (main.rs: Chromium's FluentOverlayScrollbar)
    // hide while idle, and Chromium doesn't bring them back when the pointer
    // comes near (it only shows them for scrolling input). So: when the
    // pointer moves near a page's right edge, the page gets a tiny wheel
    // nudge, 1/120 of a notch down and back (half a pixel each way, the
    // page ends where it was), which shows the scrollbar there.
    static bool overlayScrollbars()
    {
        static const bool on = qEnvironmentVariable("QTWEBENGINE_CHROMIUM_FLAGS").contains(QLatin1String("FluentOverlayScrollbar"));
        return on;
    }
    static QQuickItem *webViewAt(QQuickWindow *win, const QPointF &scenePos)
    {
        QQuickItem *item = win->contentItem();
        while (item) {
            if (item->inherits("QQuickWebEngineView")) return item;
            QQuickItem *child = item->childAt(item->mapFromScene(scenePos).x(), item->mapFromScene(scenePos).y());
            if (!child) return nullptr;
            item = child;
        }
        return nullptr;
    }
    void revealScrollbar(QQuickWindow *win, const QPointF &scenePos)
    {
        constexpr qreal zone = 32;      // px from the page's right edge
        constexpr qint64 every = 700;   // ms between nudges (the bar shows ~1 s)
        QQuickItem *view = webViewAt(win, scenePos);
        const bool near = view && view->mapFromScene(scenePos).x() >= view->width() - zone;
        if (!near) { m_scrollbarNear = false; return; }
        if (m_scrollbarNear && m_lastNudge.isValid() && m_lastNudge.elapsed() < every) return;
        m_scrollbarNear = true;
        m_lastNudge.start();
        QPointer<QQuickWindow> w(win);
        // A complete scroll gesture (begin … end), so Chromium doesn't wait
        // for the end of a scroll that never comes (keeping the bar shown).
        auto send = [w, scenePos](int angle, Qt::ScrollPhase phase) {
            if (!w) return;
            QWheelEvent ev(scenePos, w->mapToGlobal(scenePos), QPoint(), QPoint(0, angle), Qt::NoButton, Qt::NoModifier, phase, false);
            QCoreApplication::sendEvent(w, &ev);
        };
        // After the move event is through.
        QTimer::singleShot(0, win, [send] {
            send(0, Qt::ScrollBegin);
            send(-1, Qt::ScrollUpdate);
        });
        QTimer::singleShot(30, win, [send] {
            send(1, Qt::ScrollUpdate);
            send(0, Qt::ScrollEnd);
        });
    }
    bool m_scrollbarNear = false;
    QElapsedTimer m_lastNudge;
    // Menus are shown in the window's popup overlay; their items' QObject
    // parent is the QQuickMenu. (Dialogs stay open.)
    static void closeMenus(QQuickWindow *win)
    {
        for (QQuickItem *layer : win->contentItem()->childItems()) {
            if (!layer->inherits("QQuickOverlay")) continue;
            for (QQuickItem *item : layer->childItems()) {
                QObject *popup = item->parent();
                if (popup && popup->inherits("QQuickMenu") && popup->property("opened").toBool())
                    QMetaObject::invokeMethod(popup, "close");
            }
        }
    }
    bool m_altAlone = false;
};

// Store a page's favicon in Kite's icon cache (for the speed dial). The
// image comes from QtWebEngine's own "favicon" image provider, which
// already holds the icon the page loaded, so the site isn't contacted again.
// Qt's favicon image provider answers a request from the first web view
// showing that icon. An unloaded (discarded) view still shows its icon but
// has no page behind it, and reading the icon from it crashes. Kite takes
// unloaded views out of the provider's list and puts them back once they
// are loaded again. FaviconProviderHelper is private to QtWebEngineQuick;
// these members are exported.
class QQuickWebEngineView : public QQuickItem {};
class FaviconProviderHelper : public QObject {
public:
    static FaviconProviderHelper *instance();
    void attach(QPointer<QQuickWebEngineView> view);
    void detach(QPointer<QQuickWebEngineView> view);
};

static QQuickItem *findItem(QQuickItem *in, const QString &name)
{
    for (QQuickItem *c : in->childItems()) {
        if (c->objectName() == name) return c;
        if (QQuickItem *f = findItem(c, name)) return f;
    }
    return nullptr;
}

extern "C" void kite_cpp_favicon_track(const char *viewName, bool track)
{
    const QString name = QString::fromUtf8(viewName);
    for (QWindow *w : QGuiApplication::topLevelWindows()) {
        auto *q = qobject_cast<QQuickWindow *>(w);
        if (!q) continue;
        QQuickItem *item = findItem(q->contentItem(), name);
        if (!item || !item->inherits("QQuickWebEngineView")) continue;
        QPointer<QQuickWebEngineView> view(static_cast<QQuickWebEngineView *>(item));
        if (track) FaviconProviderHelper::instance()->attach(view);
        else FaviconProviderHelper::instance()->detach(view);
        return;
    }
}

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

// Container tabs' profiles are created later (Main.qml containerProfile()):
// hook the ones not hooked yet.
extern "C" void kite_cpp_hook_container_profiles()
{
    static QSet<QObject *> hooked;
    if (!g_engine || g_engine->rootObjects().isEmpty()) return;
    const QVariantMap profiles = g_engine->rootObjects().first()->property("containerProfiles").toMap();
    for (const QVariant &v : profiles) {
        QObject *p = v.value<QObject *>();
        if (p && !hooked.contains(p)) {
            hooked.insert(p);
            kite_install_profile_hooks(p, 2);
        }
    }
}

extern "C" void kite_init_dns();

extern "C" int kite_bootstrap(int argc, char **argv)
{
    qInstallMessageHandler(kiteMessageHandler);
    kite_register_schemes();
    QCoreApplication::setAttribute(Qt::AA_ShareOpenGLContexts);
    if (qEnvironmentVariableIsEmpty("QT_QUICK_CONTROLS_STYLE"))
        qputenv("QT_QUICK_CONTROLS_STYLE", "org.kde.desktop");
    // Chromium flags are set by main.rs (they depend on settings).
    QtWebEngineQuick::initialize();

    static int s_argc = argc;
    QApplication app(s_argc, argv);
    QApplication::setApplicationName(QStringLiteral("kite"));
    QApplication::setOrganizationName(QStringLiteral("kde"));
    QApplication::setApplicationDisplayName(QStringLiteral("Kite"));
    // Kite's own icons (crates/kite/icons/kite/, names start with "kite-"):
    // an icon theme in the resources used after the user's theme (it
    // inherits Breeze, the usual fallback), for QIcon lookups; Kirigami's
    // lookup (QML icons) only searches the user's theme and the fallback
    // search paths, so the 22 px icons are there too.
    QIcon::setThemeSearchPaths(QIcon::themeSearchPaths() << QStringLiteral(":/qt/qml/org/kde/kite/icons"));
    QIcon::setFallbackThemeName(QStringLiteral("kite"));
    QIcon::setFallbackSearchPaths(QIcon::fallbackSearchPaths() << QStringLiteral(":/qt/qml/org/kde/kite/icons/kite/22"));
    QApplication::setApplicationVersion(QStringLiteral("0.1.0"));
    QApplication::setDesktopFileName(QStringLiteral("org.kde.kite"));
    QApplication::setWindowIcon(QIcon::fromTheme(QStringLiteral("org.kde.kite"), QIcon::fromTheme(QStringLiteral("internet-web-browser"))));
    QApplication::setQuitOnLastWindowClosed(false);

    QQmlApplicationEngine engine;
    g_engine = &engine;
    kite_init_dns();
    app.installEventFilter(new KiteInputFilter(&app));
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app,
                     [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    // Install network hooks on each profile as soon as QML creates it.
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreated, &app, [](QObject *obj, const QUrl &) {
        if (!obj) return;
        if (auto *p = obj->property("profile").value<QObject *>()) kite_install_profile_hooks(p, 0);
        if (auto *p = obj->property("privateProfile").value<QObject *>()) kite_install_profile_hooks(p, 1);
        QMetaObject::invokeMethod(obj, "start", Qt::QueuedConnection);
    });
    engine.loadFromModule("org.kde.kite", "Main");
    const int code = app.exec();
    g_engine = nullptr;
    return code;
}
