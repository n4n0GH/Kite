// Letting another program's window come to the front, on Wayland.
//
// A Wayland compositor doesn't let a window raise itself: the program the
// user is working in asks for a token (xdg-activation-v1) and hands it to
// the one that is to show up. Qt does this by itself for what it opens. For
// what Kite asks over D-Bus (Show in Folder: kite_core::filemanager), Kite
// gets the token here.
#pragma once

#include <QObject>
#include <QString>
#include <QtQml/qqmlregistration.h>

class WindowActivation : public QObject
{
    Q_OBJECT
    QML_ELEMENT
    QML_SINGLETON

public:
    using QObject::QObject;

    // A token for the program about to be asked to show a window, good for
    // the click or key that led here. "" when there's none to be had (not on
    // Wayland, or the compositor has none to give): the window may then open
    // behind Kite's.
    Q_INVOKABLE QString token();
};
