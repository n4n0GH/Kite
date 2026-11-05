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
#include <QPointingDevice>
#include <QKeySequence>
#include <QMimeData>
#include <QElapsedTimer>
#include <QTimer>
#include <QPointer>
#include <QWheelEvent>
#include <atomic>
#include <QStyleHints>

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
//   title bar opens the window manager's menu, a click Qt Quick never sees);
// * mouse gestures: see holdForGesture() below.
// Scroll distance per mouse wheel notch on pages ("web.wheelScrollPixels",
// set from Rust; 0 = the system's). QtWebEngine scrolls a notch by the
// system's wheel scroll lines × 20 px, so a notch is scaled to the chosen
// distance before it reaches the page.
static std::atomic<int> g_wheelPixels{0};
extern "C" void kite_cpp_set_wheel_pixels(int px) { g_wheelPixels = px; }
static int systemWheelPixels() { return QGuiApplication::styleHints()->wheelScrollLines() * 20; }
extern "C" int kite_cpp_system_wheel_pixels() { return qApp ? systemWheelPixels() : 60; }

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
            if (!m_replaying) {
                auto *m = static_cast<QMouseEvent *>(e);
                if (m->button() == m_swallowRelease) m_swallowRelease = Qt::NoButton;
                if (m_held) {
                    // Another button ends it. The page never saw this one go down.
                    m_swallowRelease = m_heldButton;
                    endGesture(false);
                } else if (holdForGesture(win, m)) {
                    return true;
                }
            }
            if (win->property("menuBarPeek").toBool()) {
                const QPointF p = static_cast<QMouseEvent *>(e)->scenePosition();
                QMetaObject::invokeMethod(win, "windowPressed", Q_ARG(QVariant, p.x()), Q_ARG(QVariant, p.y()));
            }
            break;
        }
        case QEvent::Wheel: {
            m_altAlone = false;
            auto *w = static_cast<QWheelEvent *>(e);
            const int px = g_wheelPixels;
            // Mouse wheels only (touchpads scroll by pixels), not Ctrl+wheel
            // (zoom), and only over pages.
            if (m_rescaling || px <= 0 || !w->pixelDelta().isNull() || (w->modifiers() & Qt::ControlModifier)) break;
            const double f = double(px) / systemWheelPixels();
            if (qFuzzyCompare(f, 1.0) || !overWebView(win, w->scenePosition())) break;
            const QPoint a = w->angleDelta();
            QWheelEvent scaled(w->position(), w->globalPosition(), w->pixelDelta(), QPoint(qRound(a.x() * f), qRound(a.y() * f)),
                               w->buttons(), w->modifiers(), w->phase(), w->inverted(), w->source(), w->pointingDevice());
            m_rescaling = true;
            QCoreApplication::sendEvent(win, &scaled);
            m_rescaling = false;
            return true;
        }
        case QEvent::MouseButtonRelease: {
            auto *m = static_cast<QMouseEvent *>(e);
            if (m->button() == m_swallowRelease) {
                m_swallowRelease = Qt::NoButton;
                return true;
            }
            if (m_held && m_heldWindow == win && m->button() == m_heldButton) {
                if (m_gesturing) endGesture(true);
                else replayClick();
                return true;
            }
            break;
        }
        case QEvent::ContextMenu:
            // The menu's own event, from a held press: the replayed click opens it.
            if (m_held && m_heldWindow == win) return true;
            break;
        case QEvent::MouseMove: {
            auto *m = static_cast<QMouseEvent *>(e);
            if (m_held && m_heldWindow == win) {
                const QPointF at = m->scenePosition();
                if (!m_gesturing && (at - m_heldScene).manhattanLength() >= 12) {
                    m_gesturing = true;
                    QMetaObject::invokeMethod(win, "gestureStarted", Q_ARG(double, m_heldScene.x()), Q_ARG(double, m_heldScene.y()));
                }
                if (m_gesturing) QMetaObject::invokeMethod(win, "gestureMoved", Q_ARG(double, at.x()), Q_ARG(double, at.y()));
                return true;
            }
            if (m->buttons() == Qt::NoButton && overlayScrollbars())
                revealScrollbar(win, m->scenePosition());
            break;
        }
        case QEvent::WindowDeactivate:
            if (m_held && m_heldWindow == win) endGesture(false);
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
    // Mouse gestures. A page acts on a button as soon as it is pressed (Chromium
    // even opens its menu then), before anyone can tell a click from a gesture.
    // So a press of the window's "gestureButton" on a tab's page is held back.
    // If the pointer travels, it is a gesture: the page never hears of it, and
    // the window is told. If not, the release replays the click for the page.
    // Shift, or a menu being open, leaves the press to do what it always does.
    bool holdForGesture(QQuickWindow *win, QMouseEvent *m)
    {
        const int wanted = win->property("gestureButton").toInt();
        if (!wanted || int(m->button()) != wanted || m->buttons() != m->button()) return false;
        if (m->modifiers() & Qt::ShiftModifier) return false;
        if (popupOpen(win)) return false;
        QQuickItem *view = viewAt(win, m->scenePosition());
        if (!view || !view->property("kiteTabPage").toBool()) return false;
        m_held = true;
        m_gesturing = false;
        m_heldWindow = win;
        m_heldButton = m->button();
        m_heldLocal = m->position();
        m_heldScene = m->scenePosition();
        m_heldGlobal = m->globalPosition();
        m_heldModifiers = m->modifiers();
        m_heldDevice = m->pointingDevice();
        return true;
    }
    void endGesture(bool finished)
    {
        const bool told = m_gesturing;
        m_held = m_gesturing = false;
        if (told && m_heldWindow) QMetaObject::invokeMethod(m_heldWindow, finished ? "gestureFinished" : "gestureCancelled");
    }
    void replayClick()
    {
        m_held = false;
        if (!m_heldWindow) return;
        QMouseEvent press(QEvent::MouseButtonPress, m_heldLocal, m_heldScene, m_heldGlobal, m_heldButton, m_heldButton, m_heldModifiers, m_heldDevice);
        QMouseEvent release(QEvent::MouseButtonRelease, m_heldLocal, m_heldScene, m_heldGlobal, m_heldButton, Qt::NoButton, m_heldModifiers, m_heldDevice);
        m_replaying = true;
        QCoreApplication::sendEvent(m_heldWindow, &press);
        if (m_heldWindow) QCoreApplication::sendEvent(m_heldWindow, &release);
        m_replaying = false;
    }
    static bool popupOpen(QQuickWindow *win)
    {
        for (QQuickItem *layer : win->contentItem()->childItems()) {
            if (!layer->inherits("QQuickOverlay") || !layer->isVisible()) continue;
            for (QQuickItem *item : layer->childItems()) {
                QObject *popup = item->parent();
                if (item->isVisible() && !(popup && popup->inherits("QQuickToolTip"))) return true;
            }
        }
        return false;
    }
    bool m_replaying = false;
    bool m_held = false;
    bool m_gesturing = false;
    QPointer<QQuickWindow> m_heldWindow;
    Qt::MouseButton m_heldButton = Qt::NoButton;
    Qt::MouseButton m_swallowRelease = Qt::NoButton;
    QPointF m_heldLocal, m_heldScene, m_heldGlobal;
    Qt::KeyboardModifiers m_heldModifiers;
    const QPointingDevice *m_heldDevice = nullptr;

    bool m_rescaling = false;
    // Is a page (WebEngineView) the topmost item under this point?
    static bool overWebView(QQuickWindow *win, QPointF scenePos) { return viewAt(win, scenePos); }
    static QQuickItem *viewAt(QQuickWindow *win, QPointF scenePos)
    {
        QQuickItem *it = win->contentItem()->parentItem() ? win->contentItem()->parentItem() : win->contentItem();
        while (it) {
            if (it->inherits("QQuickWebEngineView")) return it;
            it = childUnder(it, scenePos);
        }
        return nullptr;
    }
    // Popups' layer covers the window while anything shows in it, even a tooltip
    // elsewhere: where it has nothing, what is below it counts.
    static QQuickItem *childUnder(QQuickItem *item, QPointF scenePos)
    {
        const QPointF p = item->mapFromScene(scenePos);
        QQuickItem *child = item->childAt(p.x(), p.y());
        if (!child || !child->inherits("QQuickOverlay")) return child;
        const QPointF inLayer = child->mapFromScene(scenePos);
        if (child->childAt(inLayer.x(), inLayer.y())) return child;
        const QList<QQuickItem *> kids = item->childItems();
        for (auto k = kids.crbegin(); k != kids.crend(); ++k) {
            if (*k != child && (*k)->isVisible() && (*k)->contains((*k)->mapFromScene(scenePos))) return *k;
        }
        return nullptr;
    }
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

// Keyboard shortcuts (Settings → Shortcuts): a pressed key as a portable
// sequence ("Ctrl+Shift+T"), and a stored sequence as shown to the user.
// The returned text stays valid until the next call.
extern "C" const char *kite_cpp_key_text(int key, int modifiers)
{
    static QByteArray buf;
    const auto mods = Qt::KeyboardModifiers(modifiers) & (Qt::ControlModifier | Qt::ShiftModifier | Qt::AltModifier | Qt::MetaModifier);
    buf = QKeySequence(QKeyCombination(mods, Qt::Key(key))).toString(QKeySequence::PortableText).toUtf8();
    return buf.constData();
}

extern "C" const char *kite_cpp_key_native(const char *sequence)
{
    static QByteArray buf;
    buf = QKeySequence::fromString(QString::fromUtf8(sequence), QKeySequence::PortableText).toString(QKeySequence::NativeText).toUtf8();
    return buf.constData();
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

extern "C" int kite_bootstrap(int argc, char **argv, const char *version)
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
    QApplication::setApplicationVersion(QString::fromUtf8(version));
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
