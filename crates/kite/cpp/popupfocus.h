// A page's dropdown (<select>) that closed as soon as it opened, on Wayland.
//
// QtWebEngine shows a dropdown's list in a window of its own, a popup, with a
// page item of its own in it. On X11 such a window never has the keyboard: the
// browser window keeps it. On Wayland the compositor hands the keyboard to a
// popup that grabs input, and Qt Quick, as the popup's window gets it, takes the
// focus from the item in it and gives it back at once. QtWebEngine closes a popup
// by telling it that it lost the keyboard, and takes the first half of that for
// the same thing: the list was gone a few milliseconds after the click. (This is
// QtWebEngine's and Qt's own: a window with nothing but a WebEngineView does it
// too.)
//
// So here: a popup's page item isn't told that it lost the keyboard while its
// window is the one that has it. A popup that really loses the keyboard (a click
// into another window, which has the compositor dismiss it) is told as before:
// its window no longer has it then.
#pragma once

#include <QFocusEvent>
#include <QGuiApplication>
#include <QQuickItem>
#include <QQuickWindow>

class KitePopupFocus : public QObject {
public:
    using QObject::QObject;

protected:
    bool eventFilter(QObject *obj, QEvent *e) override
    {
        if (e->type() != QEvent::FocusOut) return false;
        auto *item = qobject_cast<QQuickItem *>(obj);
        if (!item || !item->inherits("QtWebEngineCore::RenderWidgetHostViewQtDelegateItem")) return false;
        const QQuickWindow *own = item->window();
        return own && own->type() == Qt::Popup && own->isVisible() && QGuiApplication::focusWindow() == own;
    }
};
