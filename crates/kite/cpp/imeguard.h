// Keys that an input method turned into text for a page with nothing to type into.
//
// With an input method running (IBus, Fcitx; on Wayland, the compositor's), a key
// reaches a page as a key press only while the input method is off for the window.
// While it's on, a letter arrives as text already typed, which a page without a
// focused field drops: its own keys (Vim keys' j and k, a site's shortcuts) then do
// nothing. Qt turns it on and off by asking what has the keyboard, and QtWebEngine
// answers for a page from the text field last focused in its tab. Just after a page
// has taken over from one with the keyboard in a field (its search box, the start
// page's), that is still the old page's field, and nothing tells Qt when it no
// longer is. The input method then stays on for the new page. (WebTab.qml's
// syncInputMethod has Qt ask again when a page gets the keyboard and when it has
// loaded, which is too early for this.)
//
// So here: a single character typed at a page with no field to take it is passed on
// as the key it was, and Qt is made to ask again, which turns the input method off
// for the keys after it.
#pragma once

#include <QCoreApplication>
#include <QElapsedTimer>
#include <QGuiApplication>
#include <QInputMethod>
#include <QInputMethodEvent>
#include <QInputMethodQueryEvent>
#include <QKeyEvent>
#include <QPointer>
#include <QQuickItem>
#include <QQuickWindow>
#include <QTimer>

class KiteImeGuard : public QObject {
public:
    using QObject::QObject;

protected:
    bool eventFilter(QObject *obj, QEvent *e) override
    {
        if (e->type() != QEvent::InputMethod || m_sending) return false;
        auto *item = qobject_cast<QQuickItem *>(obj);
        if (!item || !item->inherits("QtWebEngineCore::RenderWidgetHostViewQtDelegateItem")) return false;
        auto *typed = static_cast<QInputMethodEvent *>(e);
        // Text being put together (an accent, Chinese), and what ends it: not a key.
        if (!typed->preeditString().isEmpty()) {
            m_composing = item;
            return false;
        }
        const bool composed = m_composing == item;
        m_composing = nullptr;
        const QString text = typed->commitString();
        if (composed || text.size() != 1 || typed->replacementLength() != 0) return false;
        // Only what a key of a Latin keyboard types.
        const char16_t c = text.at(0).unicode();
        if (!((c >= 0x20 && c < 0x7f) || (c >= 0xa0 && c <= 0xff))) return false;

        // Is there a field? The page's answer can still be the last page's (see above),
        // but its item's own flag is set only by a field of this page. A password
        // field, which doesn't set the flag, says so in its hints.
        QInputMethodQueryEvent ask(Qt::ImEnabled | Qt::ImHints);
        QCoreApplication::sendEvent(item, &ask);
        const bool wanted = ask.value(Qt::ImEnabled).toBool();
        const bool hidden = Qt::InputMethodHints(ask.value(Qt::ImHints).toInt()).testFlag(Qt::ImhHiddenText);
        if (wanted && (hidden || item->flags().testFlag(QQuickItem::ItemAcceptsInputMethod))) return false;
        QPointer<QQuickWindow> win(item->window());
        if (!win) return false;

        // (Qt's keys of letters are their capitals; a few have theirs elsewhere.)
        const char16_t upper = text.at(0).toUpper().unicode();
        const int key = upper <= 0xff ? upper : c;
        // Shift from the letter itself, not from what Qt thinks is held: it takes that
        // from a press sent like this too, until the next real key or pointer event.
        const Qt::KeyboardModifiers held = text.at(0).isUpper() ? Qt::ShiftModifier : Qt::NoModifier;
        QKeyEvent press(QEvent::KeyPress, key, held, text);
        QKeyEvent release(QEvent::KeyRelease, key, held, text);
        m_sending = true;
        QCoreApplication::sendEvent(win, &press);
        if (win) QCoreApplication::sendEvent(win, &release);
        m_sending = false;

        // Once the page itself says there's no field, Qt asking again turns the input
        // method off. (Not from in here: this is the input method's own event.)
        if (!wanted) {
            QTimer::singleShot(0, qApp, [] { QGuiApplication::inputMethod()->update(Qt::ImEnabled); });
            if (!m_said.isValid() || m_said.elapsed() > 10000) {
                m_said.start();
                qWarning("The input method was still on for a page with nothing to type into: its keys were passed on to the page, and it was turned off.");
            }
        }
        return true;
    }

private:
    QPointer<QQuickItem> m_composing;
    QElapsedTimer m_said;
    bool m_sending = false;
};
