// The profile picker, shown by `kite --pick-profile` (see main.rs): its own
// small process, as the profile has to be known before QtWebEngine starts.
#include <QApplication>
#include <QCheckBox>
#include <QDialog>
#include <QDialogButtonBox>
#include <QHBoxLayout>
#include <QIcon>
#include <QInputDialog>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLabel>
#include <QListWidget>
#include <QMessageBox>
#include <QPushButton>
#include <QVBoxLayout>

extern "C" {
struct KiteBuf { unsigned char *ptr; size_t len; size_t cap; };
void kite_buf_free(KiteBuf b);
KiteBuf kite_profiles_json();
KiteBuf kite_profile_create(const char *name);
KiteBuf kite_profile_delete(const char *name);
KiteBuf kite_profile_rename(const char *from, const char *to);
void kite_profile_chosen(const char *name);
int kite_profiles_ask();
void kite_profiles_set_ask(int ask);
}

static QString take(KiteBuf b)
{
    const QString s = QString::fromUtf8(reinterpret_cast<const char *>(b.ptr), qsizetype(b.len));
    kite_buf_free(b);
    return s;
}

namespace {
class Picker : public QDialog {
public:
    Picker()
    {
        setWindowTitle(QStringLiteral("Choose a Profile — Kite"));
        setMinimumWidth(420);
        auto *layout = new QVBoxLayout(this);
        layout->addWidget(new QLabel(QStringLiteral("Each profile has its own bookmarks, history, logins and add-ons.")));
        list = new QListWidget;
        layout->addWidget(list);
        auto *row = new QHBoxLayout;
        auto *add = new QPushButton(QIcon::fromTheme(QStringLiteral("list-add")), QStringLiteral("New Profile…"));
        renameBtn = new QPushButton(QIcon::fromTheme(QStringLiteral("edit-rename")), QStringLiteral("Rename…"));
        remove = new QPushButton(QIcon::fromTheme(QStringLiteral("edit-delete")), QStringLiteral("Delete…"));
        row->addWidget(add);
        row->addWidget(renameBtn);
        row->addWidget(remove);
        row->addStretch();
        layout->addLayout(row);
        auto *ask = new QCheckBox(QStringLiteral("Ask at startup when there are several profiles"));
        ask->setChecked(kite_profiles_ask());
        layout->addWidget(ask);
        buttons = new QDialogButtonBox(QDialogButtonBox::Open | QDialogButtonBox::Cancel);
        buttons->button(QDialogButtonBox::Open)->setText(QStringLiteral("Open"));
        layout->addWidget(buttons);
        connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
        connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
        connect(ask, &QCheckBox::toggled, this, [](bool on) { kite_profiles_set_ask(on); });
        connect(add, &QPushButton::clicked, this, [this] { create(); });
        connect(renameBtn, &QPushButton::clicked, this, [this] { renameCurrent(); });
        connect(remove, &QPushButton::clicked, this, [this] { removeCurrent(); });
        connect(list, &QListWidget::itemDoubleClicked, this, [this](QListWidgetItem *i) { if (i->flags() & Qt::ItemIsEnabled) accept(); });
        connect(list, &QListWidget::currentItemChanged, this, [this] { updateButtons(); });
        reload(QString());
    }
    QString chosen() const { return list->currentItem() ? list->currentItem()->data(Qt::UserRole).toString() : QString(); }

private:
    void reload(const QString &select)
    {
        list->clear();
        const QJsonObject all = QJsonDocument::fromJson(take(kite_profiles_json()).toUtf8()).object();
        const QString wanted = select.isEmpty() ? all[QStringLiteral("last")].toString() : select;
        QListWidgetItem *current = nullptr;
        for (const QJsonValue v : all[QStringLiteral("profiles")].toArray()) {
            const QJsonObject p = v.toObject();
            const QString name = p[QStringLiteral("name")].toString();
            const bool inUse = p[QStringLiteral("inUse")].toBool();
            auto *item = new QListWidgetItem(QIcon::fromTheme(QStringLiteral("user-identity")), inUse ? QStringLiteral("%1 (open)").arg(name) : name, list);
            item->setData(Qt::UserRole, name);
            // (One that's open can be picked too: the Kite that has it opens a window.)
            if (name == wanted || !current) current = item;
        }
        list->setCurrentItem(current);
        updateButtons();
    }
    void updateButtons()
    {
        const bool any = list->currentItem();
        const bool notDefault = any && chosen() != QStringLiteral("default");
        buttons->button(QDialogButtonBox::Open)->setEnabled(any);
        renameBtn->setEnabled(notDefault);
        remove->setEnabled(notDefault);
    }
    void create()
    {
        bool ok = false;
        const QString name = QInputDialog::getText(this, QStringLiteral("New Profile"), QStringLiteral("Name:"), QLineEdit::Normal, QString(), &ok).trimmed();
        if (!ok || name.isEmpty()) return;
        const QString error = take(kite_profile_create(name.toUtf8().constData()));
        if (!error.isEmpty()) { QMessageBox::warning(this, QStringLiteral("New Profile"), error); return; }
        reload(name);
    }
    // This erases data, so Enter must never do it. Not a QMessageBox: its button
    // box picks a default button of its own when shown, which here was the
    // destructive one. Delete is only reached by a click, or Tab and Space.
    bool confirmDelete(const QString &name)
    {
        QDialog box(this);
        box.setWindowTitle(QStringLiteral("Delete Profile"));
        auto *icon = new QLabel;
        icon->setPixmap(QIcon::fromTheme(QStringLiteral("dialog-warning")).pixmap(48, 48));
        icon->setAlignment(Qt::AlignTop);
        auto *text = new QLabel(QStringLiteral("<b>Delete the profile “%1”?</b><br><br>Its bookmarks, history, saved logins, add-ons and cookies are deleted for good. Logins kept in KDE Wallet stay there.").arg(name.toHtmlEscaped()));
        text->setWordWrap(true);
        text->setMinimumWidth(320);
        auto *cancel = new QPushButton(QIcon::fromTheme(QStringLiteral("dialog-cancel")), QStringLiteral("Cancel"));
        auto *confirm = new QPushButton(QIcon::fromTheme(QStringLiteral("edit-delete")), QStringLiteral("Delete Profile"));
        confirm->setAutoDefault(false);
        cancel->setDefault(true);
        connect(cancel, &QPushButton::clicked, &box, &QDialog::reject);
        connect(confirm, &QPushButton::clicked, &box, &QDialog::accept);
        auto *top = new QHBoxLayout;
        top->addWidget(icon);
        top->addWidget(text, 1);
        auto *row = new QHBoxLayout;
        row->addStretch();
        row->addWidget(confirm);
        row->addWidget(cancel);
        auto *layout = new QVBoxLayout(&box);
        layout->addLayout(top);
        layout->addLayout(row);
        cancel->setFocus();
        return box.exec() == QDialog::Accepted;
    }
    void renameCurrent()
    {
        const QString name = chosen();
        if (name.isEmpty() || name == QStringLiteral("default")) return;
        bool ok = false;
        const QString to = QInputDialog::getText(this, QStringLiteral("Rename Profile"), QStringLiteral("New name:"), QLineEdit::Normal, name, &ok).trimmed();
        if (!ok || to.isEmpty() || to == name) return;
        const QString error = take(kite_profile_rename(name.toUtf8().constData(), to.toUtf8().constData()));
        if (!error.isEmpty()) { QMessageBox::warning(this, QStringLiteral("Rename Profile"), error); return; }
        reload(to);
    }
    void removeCurrent()
    {
        const QString name = chosen();
        if (name.isEmpty()) return;
        if (!confirmDelete(name)) return;
        const QString error = take(kite_profile_delete(name.toUtf8().constData()));
        if (!error.isEmpty()) QMessageBox::warning(this, QStringLiteral("Delete Profile"), error);
        reload(QString());
    }
    QListWidget *list;
    QPushButton *renameBtn;
    QPushButton *remove;
    QDialogButtonBox *buttons;
};
}

extern "C" int kite_pick_profile(int argc, char **argv)
{
    static int s_argc = argc;
    QApplication app(s_argc, argv);
    QApplication::setApplicationName(QStringLiteral("kite"));
    QApplication::setApplicationDisplayName(QStringLiteral("Kite"));
    QApplication::setDesktopFileName(QStringLiteral("org.kde.kite"));
    QApplication::setWindowIcon(QIcon::fromTheme(QStringLiteral("org.kde.kite"), QIcon(QStringLiteral(":/qt/qml/org/kde/kite/icons/org.kde.kite.svg"))));
    Picker picker;
    if (picker.exec() != QDialog::Accepted) return 0;
    kite_profile_chosen(picker.chosen().toUtf8().constData());
    return 0;
}
