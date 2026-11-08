// Pointer lock for web pages (element.requestPointerLock()), on Wayland.
//
// QtWebEngine refuses pointer lock in Qt Quick without asking, and Qt can't
// hold the pointer still on Wayland anyway (it would warp it, which Wayland
// doesn't allow). So Kite does it itself: the compositor locks the pointer
// (pointer-constraints-unstable-v1) and reports how the mouse moves
// (relative-pointer-unstable-v1); WebTab passes that on to the page, where
// js/pointerlock.js plays the part of the browser's pointer lock.
#pragma once

#include <QObject>
#include <QPointer>
#include <QTimer>
#include <QtQml/qqmlregistration.h>

class QWindow;
struct wl_pointer;
struct wl_surface;
struct zwp_pointer_constraints_v1;
struct zwp_relative_pointer_manager_v1;
struct zwp_relative_pointer_v1;
struct zwp_locked_pointer_v1;

class PointerLock : public QObject
{
    Q_OBJECT
    QML_ELEMENT
    QML_SINGLETON
    // The tab holding (or asking for) the lock, or null.
    Q_PROPERTY(QObject *owner READ owner NOTIFY ownerChanged)

public:
    explicit PointerLock(QObject *parent = nullptr);
    ~PointerLock() override;

    QObject *owner() const { return m_owner; }

    // Ask the compositor to lock the pointer where it is, in `window`, for
    // `owner`. "" when asked (locked() or unlocked() follows), else why not.
    // `unadjusted`: movement without the pointer acceleration.
    Q_INVOKABLE QString lock(QObject *window, QObject *owner, bool unadjusted);
    // End the lock (or the request), if there is one.
    Q_INVOKABLE void unlock();

    // Wayland callbacks (pointerlock.cpp).
    void pointerEntered(wl_surface *surface) { m_surface = surface; }
    void pointerLeft(wl_surface *surface) { if (m_surface == surface) m_surface = nullptr; }
    void lockActivated();
    void lockDeactivated();
    void relativeMotion(double dx, double dy, double dxRaw, double dyRaw);
    void global(struct wl_registry *registry, uint32_t name, const char *interface, uint32_t version);

Q_SIGNALS:
    void ownerChanged();
    // The compositor holds the pointer now: the owner's request succeeded.
    void locked(QObject *owner);
    // The lock ended (or never started). `why`: "" when it was only released
    // (Esc, the page, focus moving away), else what failed.
    void unlocked(QObject *owner, const QString &why);
    // How far the mouse moved since the last time, in the window's pixels.
    void moved(double dx, double dy);

protected:
    bool eventFilter(QObject *watched, QEvent *event) override;

private:
    QString setUp();
    void end(const QString &why);
    void flush();

    bool m_setUp = false;
    QString m_unavailable;
    wl_pointer *m_pointer = nullptr;
    wl_surface *m_surface = nullptr;
    zwp_pointer_constraints_v1 *m_constraints = nullptr;
    zwp_relative_pointer_manager_v1 *m_relativeManager = nullptr;
    zwp_relative_pointer_v1 *m_relative = nullptr;
    zwp_locked_pointer_v1 *m_locked = nullptr;

    QPointer<QObject> m_owner;
    QPointer<QWindow> m_window;
    bool m_active = false;
    bool m_unadjusted = false;
    bool m_cursorHidden = false;
    double m_dx = 0, m_dy = 0;
    QTimer m_flush;
    QTimer m_timeout;
};
