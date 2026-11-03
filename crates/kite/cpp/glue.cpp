// C++ glue between QtWebEngine and Kite's Rust core:
//  * URL scheme handlers for moz-extension:// (WebExtension resources) and kite:
//  * the profile-wide request interceptor (content blocking, DNR, webRequest)
//  * cookie store mirroring (cookies API)
// Everything here forwards to Rust via a small C ABI (src/ffi.rs).
#include <QBuffer>
#include <QFile>
#include <QHash>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QNetworkCookie>
#include <QPointer>
#include <QQmlApplicationEngine>
#include <QTimer>
#include <QUrl>
#include <QWebEngineCookieStore>
#include <QWebEngineUrlRequestInfo>
#include <QWebEngineUrlRequestInterceptor>
#include <QWebEngineUrlRequestJob>
#include <QWebEngineUrlScheme>
#include <QWebEngineUrlSchemeHandler>

#if __has_include(<QtWebEngineQuick/qquickwebengineprofile.h>)
#include <QtWebEngineQuick/qquickwebengineprofile.h>
#else
// Some distributions ship QtWebEngineQuick without its development headers.
// These are the (exported, stable) members of QQuickWebEngineProfile we use.
class QQuickWebEngineProfile : public QObject {
public:
    void installUrlSchemeHandler(const QByteArray &scheme, QWebEngineUrlSchemeHandler *);
    void setUrlRequestInterceptor(QWebEngineUrlRequestInterceptor *interceptor);
    QWebEngineCookieStore *cookieStore() const;
    void clearHttpCache();
};
#endif

extern "C" {
struct KiteBuf { unsigned char *ptr; size_t len; size_t cap; };
void kite_buf_free(KiteBuf b);
KiteBuf kite_serve(const char *url, const char *initiator, int *status, KiteBuf *mime);
KiteBuf kite_intercept(const char *url, const char *firstParty, const char *initiator, const char *type, const char *method);
void kite_cookie_event(const char *json, int removed, int initial);
int kite_rpc(const char *url, const char *initiator, const unsigned char *body, size_t bodyLen, quint64 job, int *status, KiteBuf *out);
void kite_rpc_job_gone(quint64 job);
KiteBuf kite_user_agent_for(const char *url);
}

static QByteArray takeBuf(KiteBuf b)
{
    QByteArray out(reinterpret_cast<const char *>(b.ptr), qsizetype(b.len));
    kite_buf_free(b);
    return out;
}

static QPointer<QQuickWebEngineProfile> g_profile;
static QPointer<QObject> g_offTheRecordProfile;
static bool g_initialCookiesDone = false;

// Held long-poll requests of sub-frame extension pages (see src/rpc.rs).
static QHash<quint64, QPointer<QWebEngineUrlRequestJob>> g_rpcJobs;
static quint64 g_nextJob = 1;

static void replyJson(QWebEngineUrlRequestJob *job, int status, const QByteArray &body)
{
    if (status == 403) { job->fail(QWebEngineUrlRequestJob::RequestDenied); return; }
    if (status >= 400) { job->fail(QWebEngineUrlRequestJob::RequestFailed); return; }
    auto *buffer = new QBuffer(job);
    buffer->setData(body);
    buffer->open(QIODevice::ReadOnly);
    QMultiMap<QByteArray, QByteArray> headers;
    headers.insert("Cache-Control", "no-store");
    // Content scripts in sub-frames call from the web page's origin; they
    // are authenticated by token (src/rpc.rs).
    headers.insert("Access-Control-Allow-Origin", "*");
    job->setAdditionalResponseHeaders(headers);
    job->reply("application/json", buffer);
}

extern "C" void kite_cpp_rpc_complete(quint64 id, const unsigned char *data, size_t len)
{
    const auto job = g_rpcJobs.take(id);
    if (job) replyJson(job, 200, QByteArray(reinterpret_cast<const char *>(data), qsizetype(len)));
}

// ---------------------------------------------------------------------------
class KiteSchemeHandler : public QWebEngineUrlSchemeHandler {
public:
    using QWebEngineUrlSchemeHandler::QWebEngineUrlSchemeHandler;
    void requestStarted(QWebEngineUrlRequestJob *job) override
    {
        const QByteArray url = job->requestUrl().toString(QUrl::FullyEncoded).toUtf8();
        const QByteArray initiator = job->initiator().isEmpty() ? QByteArray() : job->initiator().toString().toUtf8();
        if (job->requestUrl().scheme() == QLatin1String("moz-extension") && job->requestUrl().path().startsWith(QLatin1String("/__kite__/")) &&
            (job->requestUrl().path().endsWith(QLatin1String("/rpc")) || job->requestUrl().path().endsWith(QLatin1String("/poll")))) {
            QByteArray body;
            if (QIODevice *dev = job->requestBody()) {
                if (dev->isOpen() || dev->open(QIODevice::ReadOnly)) body = dev->readAll();
            }
            const quint64 id = g_nextJob++;
            int status = 0;
            KiteBuf out{nullptr, 0, 0};
            if (kite_rpc(url.constData(), initiator.constData(), reinterpret_cast<const unsigned char *>(body.constData()), size_t(body.size()), id, &status, &out) == 1) {
                g_rpcJobs.insert(id, job);
                QObject::connect(job, &QObject::destroyed, [id] { g_rpcJobs.remove(id); kite_rpc_job_gone(id); });
                return;
            }
            replyJson(job, status, takeBuf(out));
            return;
        }
        int status = 0;
        KiteBuf mimeBuf{nullptr, 0, 0};
        QByteArray body = takeBuf(kite_serve(url.constData(), initiator.constData(), &status, &mimeBuf));
        QByteArray mime = takeBuf(mimeBuf);
        if (status == 404) { job->fail(QWebEngineUrlRequestJob::UrlNotFound); return; }
        if (status == 403) { job->fail(QWebEngineUrlRequestJob::RequestDenied); return; }
        if (status >= 400) { job->fail(QWebEngineUrlRequestJob::RequestFailed); return; }
        auto *buffer = new QBuffer(job);
        buffer->setData(body);
        buffer->open(QIODevice::ReadOnly);
        QMultiMap<QByteArray, QByteArray> headers;
        headers.insert("Access-Control-Allow-Origin", "*");
        headers.insert("Cache-Control", "no-cache");
        job->setAdditionalResponseHeaders(headers);
        job->reply(mime, buffer);
    }
};

// ---------------------------------------------------------------------------
static const char *resourceTypeName(QWebEngineUrlRequestInfo::ResourceType t)
{
    switch (t) {
    case QWebEngineUrlRequestInfo::ResourceTypeMainFrame: return "mainFrame";
    case QWebEngineUrlRequestInfo::ResourceTypeSubFrame: return "subFrame";
    case QWebEngineUrlRequestInfo::ResourceTypeStylesheet: return "stylesheet";
    case QWebEngineUrlRequestInfo::ResourceTypeScript: return "script";
    case QWebEngineUrlRequestInfo::ResourceTypeImage: return "image";
    case QWebEngineUrlRequestInfo::ResourceTypeFontResource: return "fontResource";
    case QWebEngineUrlRequestInfo::ResourceTypeSubResource: return "other";
    case QWebEngineUrlRequestInfo::ResourceTypeObject: return "object";
    case QWebEngineUrlRequestInfo::ResourceTypeMedia: return "media";
    case QWebEngineUrlRequestInfo::ResourceTypeWorker: return "worker";
    case QWebEngineUrlRequestInfo::ResourceTypeSharedWorker: return "sharedWorker";
    case QWebEngineUrlRequestInfo::ResourceTypePrefetch: return "prefetch";
    case QWebEngineUrlRequestInfo::ResourceTypeFavicon: return "favicon";
    case QWebEngineUrlRequestInfo::ResourceTypeXhr: return "xhr";
    case QWebEngineUrlRequestInfo::ResourceTypePing: return "ping";
    case QWebEngineUrlRequestInfo::ResourceTypeServiceWorker: return "serviceWorker";
    case QWebEngineUrlRequestInfo::ResourceTypeCspReport: return "cspReport";
    case QWebEngineUrlRequestInfo::ResourceTypePluginResource: return "pluginResource";
    case QWebEngineUrlRequestInfo::ResourceTypeNavigationPreloadMainFrame: return "mainFrame";
    case QWebEngineUrlRequestInfo::ResourceTypeNavigationPreloadSubFrame: return "subFrame";
    case QWebEngineUrlRequestInfo::ResourceTypeWebSocket: return "websocket";
    default: return "other";
    }
}

class KiteInterceptor : public QWebEngineUrlRequestInterceptor {
public:
    using QWebEngineUrlRequestInterceptor::QWebEngineUrlRequestInterceptor;
    void interceptRequest(QWebEngineUrlRequestInfo &info) override
    {
        const QByteArray url = info.requestUrl().toString(QUrl::FullyEncoded).toUtf8();
        const QByteArray fp = info.firstPartyUrl().toString(QUrl::FullyEncoded).toUtf8();
        const QByteArray ini = info.initiator().toString().toUtf8();
        // Make addons.mozilla.org treat Kite as Firefox.
        {
            QByteArray ua = takeBuf(kite_user_agent_for(url.constData()));
            if (!ua.isEmpty()) info.setHttpHeader("User-Agent", ua);
        }
        const QByteArray verdict = takeBuf(kite_intercept(url.constData(), fp.constData(), ini.constData(),
                                                          resourceTypeName(info.resourceType()), info.requestMethod().constData()));
        if (verdict.isEmpty()) return;
        if (verdict == "block") { info.block(true); return; }
        if (verdict.startsWith("redirect:")) { info.redirect(QUrl(QString::fromUtf8(verdict.mid(9)))); return; }
        if (verdict.startsWith("headers:")) {
            const auto arr = QJsonDocument::fromJson(verdict.mid(8)).array();
            for (const auto &h : arr) {
                const auto p = h.toArray();
                info.setHttpHeader(p.at(0).toString().toUtf8(), p.at(1).toString().toUtf8());
            }
        }
    }
};

// ---------------------------------------------------------------------------
static QByteArray cookieJson(const QNetworkCookie &c)
{
    QJsonObject o;
    o["name"] = QString::fromUtf8(c.name());
    o["value"] = QString::fromUtf8(c.value());
    o["domain"] = c.domain();
    o["path"] = c.path().isEmpty() ? QStringLiteral("/") : c.path();
    o["secure"] = c.isSecure();
    o["httpOnly"] = c.isHttpOnly();
    o["session"] = c.isSessionCookie();
    o["expirationDate"] = c.isSessionCookie() ? 0.0 : double(c.expirationDate().toSecsSinceEpoch());
    switch (c.sameSitePolicy()) {
    case QNetworkCookie::SameSite::Strict: o["sameSite"] = "strict"; break;
    case QNetworkCookie::SameSite::Lax: o["sameSite"] = "lax"; break;
    case QNetworkCookie::SameSite::None: o["sameSite"] = "no_restriction"; break;
    default: o["sameSite"] = "lax"; break;
    }
    return QJsonDocument(o).toJson(QJsonDocument::Compact);
}

static QNetworkCookie cookieFromJson(const QJsonObject &o, QUrl *originOut)
{
    QNetworkCookie c(o["name"].toString().toUtf8(), o["value"].toString().toUtf8());
    const QUrl url(o["url"].toString());
    if (o.contains("domain") && !o["domain"].toString().isEmpty()) c.setDomain(o["domain"].toString());
    c.setPath(o.contains("path") ? o["path"].toString() : QStringLiteral("/"));
    c.setSecure(o["secure"].toBool());
    c.setHttpOnly(o["httpOnly"].toBool());
    if (o.contains("expirationDate")) c.setExpirationDate(QDateTime::fromSecsSinceEpoch(qint64(o["expirationDate"].toDouble())));
    const QString ss = o["sameSite"].toString();
    if (ss == "strict") c.setSameSitePolicy(QNetworkCookie::SameSite::Strict);
    else if (ss == "lax") c.setSameSitePolicy(QNetworkCookie::SameSite::Lax);
    else if (ss == "no_restriction") c.setSameSitePolicy(QNetworkCookie::SameSite::None);
    if (originOut) *originOut = url.isValid() ? QUrl(url.scheme() + "://" + url.host()) : QUrl();
    return c;
}

extern "C" void kite_cpp_set_cookie(const char *json)
{
    if (!g_profile) return;
    QUrl origin;
    const auto c = cookieFromJson(QJsonDocument::fromJson(json).object(), &origin);
    g_profile->cookieStore()->setCookie(c, origin);
}

extern "C" void kite_cpp_delete_cookie(const char *json)
{
    if (!g_profile) return;
    QUrl origin;
    const auto c = cookieFromJson(QJsonDocument::fromJson(json).object(), &origin);
    g_profile->cookieStore()->deleteCookie(c, origin);
}

extern "C" void kite_cpp_delete_all_cookies()
{
    if (g_profile) g_profile->cookieStore()->deleteAllCookies();
}

extern "C" void kite_cpp_clear_http_cache()
{
    if (g_profile) g_profile->clearHttpCache();
}

extern "C" const char *kite_cpp_qwebchannel_js()
{
    static QByteArray js;
    if (js.isEmpty()) {
        QFile f(QStringLiteral(":/qtwebchannel/qwebchannel.js"));
        if (f.open(QIODevice::ReadOnly)) js = f.readAll();
        if (js.isEmpty()) qWarning("Kite: qwebchannel.js resource not found; extensions will not work");
    }
    return js.constData();
}

// Called from bootstrap before QApplication exists.
void kite_register_schemes()
{
    QWebEngineUrlScheme ext("moz-extension");
    ext.setSyntax(QWebEngineUrlScheme::Syntax::Host);
    ext.setFlags(QWebEngineUrlScheme::SecureScheme | QWebEngineUrlScheme::CorsEnabled |
                 QWebEngineUrlScheme::FetchApiAllowed | QWebEngineUrlScheme::ContentSecurityPolicyIgnored);
    QWebEngineUrlScheme::registerScheme(ext);

    QWebEngineUrlScheme kite("kite");
    kite.setSyntax(QWebEngineUrlScheme::Syntax::Path);
    kite.setFlags(QWebEngineUrlScheme::SecureScheme | QWebEngineUrlScheme::LocalAccessAllowed);
    QWebEngineUrlScheme::registerScheme(kite);
}

// Called from bootstrap once the QML profile object exists.
void kite_install_profile_hooks(QObject *profileObject, bool isPrimary)
{
    auto *profile = static_cast<QQuickWebEngineProfile *>(profileObject);
    auto *schemes = new KiteSchemeHandler(profileObject);
    profile->installUrlSchemeHandler("moz-extension", schemes);
    profile->installUrlSchemeHandler("kite", schemes);
    profile->setUrlRequestInterceptor(new KiteInterceptor(profileObject));
    if (!isPrimary) { g_offTheRecordProfile = profileObject; return; }

    g_profile = profile;
    auto *store = profile->cookieStore();
    QObject::connect(store, &QWebEngineCookieStore::cookieAdded, profileObject, [](const QNetworkCookie &c) {
        kite_cookie_event(cookieJson(c).constData(), 0, g_initialCookiesDone ? 0 : 1);
    });
    QObject::connect(store, &QWebEngineCookieStore::cookieRemoved, profileObject, [](const QNetworkCookie &c) {
        kite_cookie_event(cookieJson(c).constData(), 1, 0);
    });
    store->loadAllCookies();
    QTimer::singleShot(1500, profileObject, [] { g_initialCookiesDone = true; });
}
