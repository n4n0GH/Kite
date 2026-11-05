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
#include <QWebEngineGlobalSettings>
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
KiteBuf kite_serve(const char *url, const char *initiator, int inPrivate, int *status, KiteBuf *mime);
KiteBuf kite_intercept(const char *url, const char *firstParty, const char *initiator, const char *type, const char *method, int inPrivate);
void kite_cookie_event(const char *json, int removed, int initial);
int kite_rpc(const char *url, const char *initiator, const unsigned char *body, size_t bodyLen, quint64 job, int *status, KiteBuf *out);
void kite_rpc_job_gone(quint64 job);
KiteBuf kite_user_agent_for(const char *url);
bool kite_do_not_track();
bool kite_gpc();
bool kite_send_referer();
int kite_cookie_policy();
int kite_cookie_rule(const char *host);
KiteBuf kite_session_cookie_hosts();
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
    KiteSchemeHandler(bool inPrivate, QObject *parent) : QWebEngineUrlSchemeHandler(parent), m_private(inPrivate) {}
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
        QByteArray body = takeBuf(kite_serve(url.constData(), initiator.constData(), m_private ? 1 : 0, &status, &mimeBuf));
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
private:
    const bool m_private;
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
    KiteInterceptor(bool inPrivate, QObject *parent) : QWebEngineUrlRequestInterceptor(parent), m_private(inPrivate) {}
    void interceptRequest(QWebEngineUrlRequestInfo &info) override
    {
        const QByteArray url = info.requestUrl().toString(QUrl::FullyEncoded).toUtf8();
        const QByteArray fp = info.firstPartyUrl().toString(QUrl::FullyEncoded).toUtf8();
        const QByteArray ini = info.initiator().toString().toUtf8();
        if (kite_do_not_track()) info.setHttpHeader("DNT", "1");
        if (kite_gpc()) info.setHttpHeader("Sec-GPC", "1");
        // An empty Referer set here removes the request's referrer.
        if (!kite_send_referer()) info.setHttpHeader("Referer", QByteArray());
        // Make addons.mozilla.org treat Kite as Firefox.
        {
            QByteArray ua = takeBuf(kite_user_agent_for(url.constData()));
            if (!ua.isEmpty()) info.setHttpHeader("User-Agent", ua);
        }
        const QByteArray verdict = takeBuf(kite_intercept(url.constData(), fp.constData(), ini.constData(),
                                                          resourceTypeName(info.resourceType()), info.requestMethod().constData(), m_private ? 1 : 0));
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
private:
    const bool m_private;
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

// All cookies of the primary profile (for deleting a site's cookies).
static QList<QNetworkCookie> g_cookies;

// Delete the cookies of the given hosts and their subdomains (a JSON array).
extern "C" void kite_cpp_delete_cookies_of(const char *hostsJson)
{
    if (!g_profile) return;
    const auto hosts = QJsonDocument::fromJson(QByteArray(hostsJson)).array();
    if (hosts.isEmpty()) return;
    const QList<QNetworkCookie> all = g_cookies;
    for (const QNetworkCookie &c : all) {
        const QString domain = c.domain().startsWith(QLatin1Char('.')) ? c.domain().mid(1) : c.domain();
        for (const auto &h : hosts) {
            const QString host = h.toString();
            if (domain == host || domain.endsWith(QLatin1Char('.') + host)) {
                g_profile->cookieStore()->deleteCookie(c);
                break;
            }
        }
    }
}

// DNS over HTTPS ("privacy.dnsOverHttps"): 0 = system DNS, 1 = secure DNS
// with fallback to the system's, 2 = secure DNS only. Applies at once.
extern "C" void kite_cpp_apply_dns(int mode, const char *server)
{
    QWebEngineGlobalSettings::DnsMode dns;
    const QString templ = QString::fromUtf8(server).trimmed();
    if (mode > 0 && templ.startsWith(QLatin1String("https://"))) {
        dns.secureMode = mode == 2 ? QWebEngineGlobalSettings::SecureDnsMode::SecureOnly : QWebEngineGlobalSettings::SecureDnsMode::SecureWithFallback;
        dns.serverTemplates = {templ};
    }
    if (!QWebEngineGlobalSettings::setDnsMode(dns))
        qWarning("Kite: DNS over HTTPS server not accepted: %s", server);
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

    // Stand-in responses for blocked requests (see kite-core standin.rs):
    // a redirect target web pages may load, like moz-extension.
    QWebEngineUrlScheme res("kite-res");
    res.setSyntax(QWebEngineUrlScheme::Syntax::Path);
    res.setFlags(QWebEngineUrlScheme::SecureScheme | QWebEngineUrlScheme::CorsEnabled |
                 QWebEngineUrlScheme::FetchApiAllowed | QWebEngineUrlScheme::ContentSecurityPolicyIgnored);
    QWebEngineUrlScheme::registerScheme(res);

    QWebEngineUrlScheme kite("kite");
    kite.setSyntax(QWebEngineUrlScheme::Syntax::Path);
    kite.setFlags(QWebEngineUrlScheme::SecureScheme | QWebEngineUrlScheme::LocalAccessAllowed);
    QWebEngineUrlScheme::registerScheme(kite);
}

// Called from bootstrap once the QML profile object exists.
// kind: 0 = the main profile, 1 = private browsing, 2 = a container (only
// the main profile's cookies are tracked, for the extension cookies API).
void kite_install_profile_hooks(QObject *profileObject, int kind)
{
    auto *profile = static_cast<QQuickWebEngineProfile *>(profileObject);
    auto *schemes = new KiteSchemeHandler(kind == 1, profileObject);
    profile->installUrlSchemeHandler("moz-extension", schemes);
    profile->installUrlSchemeHandler("kite", schemes);
    profile->installUrlSchemeHandler("kite-res", schemes);
    profile->setUrlRequestInterceptor(new KiteInterceptor(kind == 1, profileObject));
    // A site's own cookie rule (Site Settings, "privacy.siteCookieRules"),
    // else "privacy.cookies": 0 = all, 1 = no third-party, 2 = none.
    profile->cookieStore()->setCookieFilter([](const QWebEngineCookieStore::FilterRequest &request) {
        const int rule = kite_cookie_rule(request.origin.host().toUtf8().constData());
        if (rule == 1 || rule == 3) return true;
        if (rule == 2) return false;
        const int policy = kite_cookie_policy();
        return policy == 0 || (policy == 1 && !request.thirdParty);
    });
    if (kind == 1) { g_offTheRecordProfile = profileObject; return; }
    if (kind == 2) return;

    g_profile = profile;
    auto *store = profile->cookieStore();
    QObject::connect(store, &QWebEngineCookieStore::cookieAdded, profileObject, [](const QNetworkCookie &c) {
        g_cookies.removeIf([&c](const QNetworkCookie &o) { return o.hasSameIdentifier(c); });
        g_cookies.append(c);
        kite_cookie_event(cookieJson(c).constData(), 0, g_initialCookiesDone ? 0 : 1);
    });
    QObject::connect(store, &QWebEngineCookieStore::cookieRemoved, profileObject, [](const QNetworkCookie &c) {
        g_cookies.removeIf([&c](const QNetworkCookie &o) { return o.hasSameIdentifier(c); });
        kite_cookie_event(cookieJson(c).constData(), 1, 0);
    });
    store->loadAllCookies();
    QTimer::singleShot(1500, profileObject, [] {
        g_initialCookiesDone = true;
        // Cookies of sites set to "until Kite closes" left over from a
        // session that didn't end normally.
        kite_cpp_delete_cookies_of(takeBuf(kite_session_cookie_hosts()).constData());
    });
}
