// The profile picker, shown by `kite --pick-profile` (see main.rs): its own
// small process, as the profile has to be known before QtWebEngine starts.
#include <QApplication>
#include <QBuffer>
#include <QCheckBox>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDragEnterEvent>
#include <QFileDialog>
#include <QFileInfo>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QIcon>
#include <QImageReader>
#include <QInputDialog>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QKeyEvent>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QMenu>
#include <QMessageBox>
#include <QMimeData>
#include <QMouseEvent>
#include <QPainter>
#include <QPainterPath>
#include <QPushButton>
#include <QScrollBar>
#include <QSlider>
#include <QStandardPaths>
#include <QStyledItemDelegate>
#include <QTimer>
#include <QToolButton>
#include <QVBoxLayout>
#include <QWheelEvent>
#include <cmath>
#include <functional>

extern "C" {
struct KiteBuf { unsigned char *ptr; size_t len; size_t cap; };
void kite_buf_free(KiteBuf b);
KiteBuf kite_profiles_json();
KiteBuf kite_profile_create(const char *name, int child, const char *password);
KiteBuf kite_parental_verify(const char *password);
KiteBuf kite_profile_delete(const char *name);
KiteBuf kite_profile_rename(const char *from, const char *to);
KiteBuf kite_profile_set_avatar(const char *name, const unsigned char *png, size_t len);
KiteBuf kite_profile_avatar_path(const char *name);
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
// Asks for the parent password (parental controls) until it's given or the dialog is
// closed. A wrong one says so and stays: the wait after several is Kite's to say.
bool askParentPassword(QWidget *parent, const QString &title, const QString &text)
{
    QDialog box(parent);
    box.setWindowTitle(title);
    auto *icon = new QLabel;
    icon->setPixmap(QIcon::fromTheme(QStringLiteral("dialog-password")).pixmap(48, 48));
    icon->setAlignment(Qt::AlignTop);
    auto *label = new QLabel(text);
    label->setWordWrap(true);
    auto *edit = new QLineEdit;
    edit->setEchoMode(QLineEdit::Password);
    edit->setPlaceholderText(QStringLiteral("Parent password"));
    auto *error = new QLabel;
    error->setWordWrap(true);
    error->setVisible(false);
    auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
    auto *right = new QVBoxLayout;
    right->addWidget(label);
    right->addWidget(edit);
    right->addWidget(error);
    auto *top = new QHBoxLayout;
    top->addWidget(icon);
    top->addLayout(right, 1);
    auto *layout = new QVBoxLayout(&box);
    layout->addLayout(top);
    layout->addWidget(buttons);
    // (As high as what it shows: it grows when a wrong password is answered.)
    label->setFixedWidth(340);
    error->setFixedWidth(340);
    layout->setSizeConstraint(QLayout::SetFixedSize);
    QObject::connect(buttons, &QDialogButtonBox::rejected, &box, &QDialog::reject);
    QObject::connect(buttons, &QDialogButtonBox::accepted, &box, [&] {
        const QString problem = take(kite_parental_verify(edit->text().toUtf8().constData()));
        if (problem.isEmpty()) { box.accept(); return; }
        error->setText(problem);
        error->setVisible(true);
        edit->selectAll();
        edit->setFocus();
    });
    edit->setFocus();
    return box.exec() == QDialog::Accepted;
}

// ---- Profile pictures ----
// A profile's picture is a square PNG file in its directory (kite-core `profiles`),
// shown in a circle. It's made here: an image file, moved and zoomed under a round
// opening, and what's in the opening is what's kept.

// The side of a kept picture, in pixels: enough for the largest it's shown at on a
// screen that draws at twice the size.
const int PictureSide = 256;

QColor mix(const QColor &a, const QColor &b, qreal t)
{
    return QColor::fromRgbF(a.redF() + (b.redF() - a.redF()) * t, a.greenF() + (b.greenF() - a.greenF()) * t, a.blueF() + (b.blueF() - a.blueF()) * t);
}

// The picture in a circle of `size`; without one, a figure in the colour scheme's
// accent (drawn, not an icon of the theme: those differ, and few have one in colour).
QPixmap avatarPixmap(const QImage &picture, int size, qreal ratio, const QPalette &palette)
{
    QPixmap out(QSize(size, size) * ratio);
    out.setDevicePixelRatio(ratio);
    out.fill(Qt::transparent);
    QPainter p(&out);
    p.setRenderHint(QPainter::Antialiasing);
    p.setRenderHint(QPainter::SmoothPixmapTransform);
    const QRectF circle(0.5, 0.5, size - 1, size - 1);
    QPainterPath round;
    round.addEllipse(circle);
    p.setClipPath(round);
    // (An active window's colours, also when it's drawn while another window has the
    // keyboard: it's kept, and would stay as pale as that.)
    const QColor accent = palette.color(QPalette::Active, QPalette::Highlight);
    const QColor window = palette.color(QPalette::Active, QPalette::Window);
    if (picture.isNull()) {
        p.fillRect(QRectF(0, 0, size, size), mix(window, accent, 0.25));
        p.setPen(Qt::NoPen);
        p.setBrush(accent);
        p.drawEllipse(QPointF(size * 0.5, size * 0.39), size * 0.17, size * 0.17);
        p.drawEllipse(QPointF(size * 0.5, size * 0.98), size * 0.33, size * 0.33);
    } else {
        // (Scaled once, with a filter that takes all of its pixels: a painter's own
        // scaling of one this much larger looks rough.)
        const QImage scaled = picture.scaled(out.size(), Qt::IgnoreAspectRatio, Qt::SmoothTransformation);
        p.drawImage(QRectF(0, 0, size, size), scaled);
    }
    p.setClipping(false);
    p.setBrush(Qt::NoBrush);
    p.setPen(QPen(mix(window, palette.color(QPalette::Active, QPalette::WindowText), 0.25), 1));
    p.drawEllipse(circle);
    return out;
}

// A profile's kept picture. (It's a file anyone could have put there: only a PNG
// file of about the size Kite writes is read.)
QImage keptPicture(const QString &path)
{
    if (path.isEmpty()) return QImage();
    QImageReader reader(path, "png");
    const QSize size = reader.size();
    if (!size.isValid() || size.width() > 4 * PictureSide || size.height() > 4 * PictureSide) return QImage();
    return reader.read();
}

// An image file for a picture: turned as its camera held it, and no larger than it's
// of use here (photos are several times that).
QImage openImage(const QString &path)
{
    QImageReader reader(path);
    reader.setDecideFormatFromContent(true);
    reader.setAutoTransform(true);
    const QSize size = reader.size();
    const int most = 1536;
    if (size.isValid() && !size.isEmpty()) {
        const int shorter = qMin(size.width(), size.height());
        // (A drawing has no size to speak of: it's drawn at one that looks sharp.)
        if (shorter > most || reader.format().startsWith("svg")) reader.setScaledSize(size.scaled(most, most, Qt::KeepAspectRatioByExpanding));
    }
    const QImage image = reader.read();
    return image.isNull() ? image : image.convertToFormat(QImage::Format_ARGB32_Premultiplied);
}

// The image under a round opening: dragged to move it, zoomed with the wheel (or the
// slider beside it). It always fills the opening, so what's kept has no empty part.
class CropCanvas : public QWidget {
public:
    static constexpr int Side = 300;
    static constexpr int Opening = 260;
    // The zoom changed here (the wheel, a key): the slider follows.
    std::function<void()> zoomed;
    // An image file was dropped here.
    std::function<void(const QString &)> dropped;

    CropCanvas()
    {
        setFixedSize(Side, Side);
        setFocusPolicy(Qt::StrongFocus);
        setAcceptDrops(true);
    }
    void setImage(const QImage &image)
    {
        m_image = image;
        m_zoom = 1;
        m_center = QPointF(image.width() / 2.0, image.height() / 2.0);
        setCursor(image.isNull() ? Qt::ArrowCursor : Qt::OpenHandCursor);
        update();
    }
    bool hasImage() const { return !m_image.isNull(); }
    // 1: the image just fills the opening. Up to `maxZoom`, where a part of it some
    // 48 of its pixels wide does.
    qreal zoom() const { return m_zoom; }
    qreal maxZoom() const { return hasImage() ? qBound(1.0, qMin(m_image.width(), m_image.height()) / 48.0, 8.0) : 1.0; }
    void setZoom(qreal zoom) { zoomAt(zoom, QPointF(Side / 2.0, Side / 2.0)); }
    // What's in the opening (the square around it: it's shown in a circle anyway).
    QImage cropped() const
    {
        if (!hasImage()) return QImage();
        const qreal half = Opening / (2 * scale());
        const QRect part = QRectF(m_center.x() - half, m_center.y() - half, 2 * half, 2 * half).toRect().intersected(m_image.rect());
        return m_image.copy(part).scaled(PictureSide, PictureSide, Qt::IgnoreAspectRatio, Qt::SmoothTransformation).convertToFormat(QImage::Format_ARGB32);
    }

protected:
    void paintEvent(QPaintEvent *) override
    {
        QPainter p(this);
        p.setRenderHint(QPainter::Antialiasing);
        p.setRenderHint(QPainter::SmoothPixmapTransform);
        const QRectF all(0, 0, Side, Side);
        const QRectF opening((Side - Opening) / 2.0, (Side - Opening) / 2.0, Opening, Opening);
        QPainterPath frame;
        frame.addRoundedRect(all, 5, 5);
        p.setClipPath(frame);
        // (Dark behind an image, as what's outside the opening is dimmed.)
        p.fillRect(all, hasImage() ? QColor(24, 24, 24) : palette().color(QPalette::Base));
        if (!hasImage()) {
            // No picture: what's shown for the profile then.
            const int size = 128;
            p.drawPixmap(QPointF((Side - size) / 2.0, (Side - size) / 2.0), avatarPixmap(QImage(), size, devicePixelRatioF(), palette()));
            return;
        }
        p.save();
        p.translate(all.center());
        p.scale(scale(), scale());
        p.translate(-m_center);
        p.drawImage(QPointF(0, 0), m_image);
        p.restore();
        // What's outside the opening is dimmed: it isn't kept.
        QPainterPath outside;
        outside.addRect(all);
        outside.addEllipse(opening);
        p.fillPath(outside, QColor(0, 0, 0, 150));
        p.setPen(QPen(QColor(255, 255, 255, 210), 1.5));
        p.drawEllipse(opening);
        if (hasFocus()) {
            p.setClipping(false);
            p.setPen(QPen(palette().color(QPalette::Highlight), 1));
            p.drawRoundedRect(all.adjusted(0.5, 0.5, -0.5, -0.5), 5, 5);
        }
    }
    void mousePressEvent(QMouseEvent *e) override
    {
        if (e->button() != Qt::LeftButton || !hasImage()) return;
        m_dragFrom = e->position();
        m_dragging = true;
        setCursor(Qt::ClosedHandCursor);
    }
    void mouseMoveEvent(QMouseEvent *e) override
    {
        if (!m_dragging) return;
        move(m_dragFrom - e->position());
        m_dragFrom = e->position();
    }
    void mouseReleaseEvent(QMouseEvent *e) override
    {
        if (e->button() != Qt::LeftButton || !m_dragging) return;
        m_dragging = false;
        setCursor(Qt::OpenHandCursor);
    }
    void wheelEvent(QWheelEvent *e) override
    {
        if (!hasImage()) return;
        // (A notch of a wheel is 120: a fifth closer or further. Under the pointer.)
        zoomAt(m_zoom * std::pow(1.0015, e->angleDelta().y()), e->position());
        e->accept();
    }
    void keyPressEvent(QKeyEvent *e) override
    {
        const qreal step = e->modifiers() & Qt::ShiftModifier ? 1 : 10;
        switch (hasImage() ? e->key() : 0) {
        case Qt::Key_Left: move(QPointF(-step, 0)); break;
        case Qt::Key_Right: move(QPointF(step, 0)); break;
        case Qt::Key_Up: move(QPointF(0, -step)); break;
        case Qt::Key_Down: move(QPointF(0, step)); break;
        case Qt::Key_Plus: case Qt::Key_Equal: setZoom(m_zoom * 1.1); break;
        case Qt::Key_Minus: setZoom(m_zoom / 1.1); break;
        default: QWidget::keyPressEvent(e); return;
        }
    }
    void focusInEvent(QFocusEvent *) override { update(); }
    void focusOutEvent(QFocusEvent *) override { update(); }
    void dragEnterEvent(QDragEnterEvent *e) override
    {
        if (!droppedFile(e->mimeData()).isEmpty()) e->acceptProposedAction();
    }
    void dropEvent(QDropEvent *e) override
    {
        const QString file = droppedFile(e->mimeData());
        if (file.isEmpty()) return;
        e->acceptProposedAction();
        if (dropped) dropped(file);
    }

private:
    static QString droppedFile(const QMimeData *data)
    {
        const QList<QUrl> urls = data->urls();
        return urls.size() == 1 && urls.first().isLocalFile() ? urls.first().toLocalFile() : QString();
    }
    // Pixels here for one of the image's.
    qreal scale() const { return m_zoom * Opening / qreal(qMin(m_image.width(), m_image.height())); }
    // (`by`: in pixels here; the image goes the other way.)
    void move(const QPointF &by)
    {
        m_center += by / scale();
        settle();
    }
    // Zooms with the image's point at `anchor` staying there.
    void zoomAt(qreal zoom, const QPointF &anchor)
    {
        if (!hasImage()) return;
        const QPointF off = anchor - QPointF(Side / 2.0, Side / 2.0);
        const QPointF point = m_center + off / scale();
        m_zoom = qBound(1.0, zoom, maxZoom());
        m_center = point - off / scale();
        settle();
        if (zoomed) zoomed();
    }
    // The opening stays on the image.
    void settle()
    {
        const qreal half = Opening / (2 * scale());
        m_center.setX(qBound(half, m_center.x(), qMax(half, m_image.width() - half)));
        m_center.setY(qBound(half, m_center.y(), qMax(half, m_image.height() - half)));
        update();
    }
    QImage m_image;
    qreal m_zoom = 1;
    QPointF m_center;
    QPointF m_dragFrom;
    bool m_dragging = false;
};

// A profile's picture: chosen from a file (or dropped in), put in place, or removed.
class PictureDialog : public QDialog {
public:
    // `current`: the one it has. `askFirst`: with none to show, ask for a file straight away.
    PictureDialog(QWidget *parent, const QString &profile, const QImage &current, bool askFirst) : QDialog(parent)
    {
        setWindowTitle(profile.isEmpty() ? QStringLiteral("Profile Picture") : QStringLiteral("Picture of “%1”").arg(profile));
        canvas = new CropCanvas;
        slider = new QSlider(Qt::Horizontal);
        slider->setRange(0, Steps);
        slider->setToolTip(QStringLiteral("Zoom"));
        auto *smaller = new QLabel;
        smaller->setPixmap(QIcon::fromTheme(QStringLiteral("zoom-out")).pixmap(16, 16));
        auto *larger = new QLabel;
        larger->setPixmap(QIcon::fromTheme(QStringLiteral("zoom-in")).pixmap(16, 16));
        auto *zoom = new QHBoxLayout;
        zoom->addWidget(smaller);
        zoom->addWidget(slider, 1);
        zoom->addWidget(larger);
        // (Two lines always, whatever it says: the dialog keeps its size.)
        hint = new QLabel;
        hint->setWordWrap(true);
        hint->setAlignment(Qt::AlignHCenter | Qt::AlignTop);
        hint->setFixedSize(CropCanvas::Side, 2 * hint->fontMetrics().lineSpacing());
        auto *pick = new QPushButton(QIcon::fromTheme(QStringLiteral("document-open")), QStringLiteral("Choose Image…"));
        removeBtn = new QPushButton(QIcon::fromTheme(QStringLiteral("edit-delete")), QStringLiteral("Remove Picture"));
        pick->setAutoDefault(false);
        removeBtn->setAutoDefault(false);
        auto *buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
        auto *row = new QHBoxLayout;
        row->addWidget(pick);
        row->addWidget(removeBtn);
        row->addStretch();
        auto *layout = new QVBoxLayout(this);
        layout->addWidget(canvas, 0, Qt::AlignHCenter);
        layout->addLayout(zoom);
        layout->addWidget(hint, 0, Qt::AlignHCenter);
        layout->addLayout(row);
        layout->addWidget(buttons);
        layout->setSizeConstraint(QLayout::SetFixedSize);
        connect(buttons, &QDialogButtonBox::accepted, this, &QDialog::accept);
        connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
        connect(pick, &QPushButton::clicked, this, [this] { choose(); });
        connect(removeBtn, &QPushButton::clicked, this, [this] { display(QImage()); });
        // (The slider is even to the eye: each step zooms by the same share.)
        connect(slider, &QSlider::valueChanged, this, [this](int v) {
            if (!following) canvas->setZoom(std::pow(canvas->maxZoom(), v / qreal(Steps)));
        });
        canvas->zoomed = [this] { follow(); };
        canvas->dropped = [this](const QString &file) { load(file); };
        display(current);
        // None to show: what's wanted is a file. Not choosing one leaves it at that.
        if (askFirst && current.isNull()) QTimer::singleShot(0, this, [this] { if (!choose() && !canvas->hasImage()) reject(); });
    }
    // What's in the circle; none for no picture.
    QImage picture() const { return canvas->cropped(); }

private:
    static constexpr int Steps = 1000;
    void display(const QImage &image)
    {
        canvas->setImage(image);
        const bool any = canvas->hasImage();
        slider->setEnabled(any && canvas->maxZoom() > 1);
        removeBtn->setEnabled(any);
        hint->setText(any ? QStringLiteral("Drag the picture to move it, scroll to zoom. The part in the circle is kept.") : QStringLiteral("No picture. Choose an image, or drop one here."));
        follow();
        if (any) canvas->setFocus();
    }
    void follow()
    {
        following = true;
        slider->setValue(canvas->maxZoom() > 1 ? qRound(Steps * std::log(canvas->zoom()) / std::log(canvas->maxZoom())) : 0);
        following = false;
    }
    void load(const QString &file)
    {
        const QImage image = openImage(file);
        if (image.isNull()) {
            hint->setText(QStringLiteral("“%1” can't be opened as an image.").arg(hint->fontMetrics().elidedText(QFileInfo(file).fileName(), Qt::ElideMiddle, CropCanvas::Side / 2)));
            return;
        }
        display(image);
    }
    // False if no file was chosen.
    bool choose()
    {
        QStringList patterns;
        for (const QByteArray &format : QImageReader::supportedImageFormats()) patterns << QStringLiteral("*.") + QString::fromLatin1(format);
        const QString pictures = QStandardPaths::writableLocation(QStandardPaths::PicturesLocation);
        const QString file = QFileDialog::getOpenFileName(this, QStringLiteral("Choose an Image"), pictures, QStringLiteral("Images (%1);;All files (*)").arg(patterns.join(QLatin1Char(' '))));
        if (file.isEmpty()) return false;
        load(file);
        return true;
    }
    CropCanvas *canvas;
    QSlider *slider;
    QLabel *hint;
    QPushButton *removeBtn;
    bool following = false;
};

// Keeps `picture` as the profile's (none: it has no picture). "" if that went well.
QString keepPicture(const QString &profile, const QImage &picture)
{
    QByteArray png;
    if (!picture.isNull()) {
        QBuffer buffer(&png);
        buffer.open(QIODevice::WriteOnly);
        picture.save(&buffer, "PNG");
    }
    return take(kite_profile_set_avatar(profile.toUtf8().constData(), reinterpret_cast<const unsigned char *>(png.constData()), size_t(png.size())));
}

// A new profile: its name, and whether it's a child's. A child's takes the parent
// password: set here with the first one, asked for with later ones.
class NewProfile : public QDialog {
public:
    NewProfile(QWidget *parent, bool hasPassword, bool parentPresent, bool guarded, int minPassword) : QDialog(parent), m_hasPassword(hasPassword), m_parentPresent(parentPresent), m_guarded(guarded), m_min(minPassword)
    {
        setWindowTitle(QStringLiteral("New Profile"));
        // (A fixed width: the text under the checkbox wraps to it, rather than widening the dialog.)
        setFixedWidth(480);
        auto *form = new QFormLayout;
        // (Before the name, which has the keyboard: Tab goes on from it to what follows.)
        pictureBtn = new QToolButton;
        pictureBtn->setIconSize(QSize(PictureShown, PictureShown));
        pictureBtn->setAutoRaise(true);
        pictureBtn->setToolTip(QStringLiteral("Choose a picture for the profile"));
        auto *pictureRow = new QHBoxLayout;
        pictureRow->addWidget(pictureBtn);
        auto *pictureHint = new QLabel(QStringLiteral("Click to choose one; it can be changed later."));
        pictureHint->setWordWrap(true);
        pictureRow->addWidget(pictureHint, 1);
        form->addRow(QStringLiteral("Picture:"), pictureRow);
        connect(pictureBtn, &QToolButton::clicked, this, [this] { choosePicture(); });
        showPicture();
        name = new QLineEdit;
        form->addRow(QStringLiteral("Name:"), name);
        child = new QCheckBox(QStringLiteral("This is a child's profile"));
        form->addRow(QString(), child);
        about = new QLabel(QStringLiteral("Adult sites are blocked in it, search engines are kept to safe search, and it can't install extensions or plugins. Leaving it for another profile takes the parent password. What it may do and see is set in Settings, under Parental Controls."));
        about->setWordWrap(true);
        form->addRow(QString(), about);
        password = new QLineEdit;
        password->setEchoMode(QLineEdit::Password);
        passwordLabel = new QLabel(hasPassword ? QStringLiteral("Parent password:") : QStringLiteral("New parent password:"));
        form->addRow(passwordLabel, password);
        repeat = new QLineEdit;
        repeat->setEchoMode(QLineEdit::Password);
        repeatLabel = new QLabel(QStringLiteral("Repeat it:"));
        form->addRow(repeatLabel, repeat);
        error = new QLabel;
        error->setWordWrap(true);
        form->addRow(QString(), error);
        buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
        buttons->button(QDialogButtonBox::Ok)->setText(QStringLiteral("Create"));
        auto *layout = new QVBoxLayout(this);
        layout->addLayout(form);
        layout->addWidget(buttons);
        connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
        connect(buttons, &QDialogButtonBox::accepted, this, [this] { create(); });
        connect(child, &QCheckBox::toggled, this, [this] { update(); });
        update();
        name->setFocus();
    }
    QString created;

private:
    static constexpr int PictureShown = 48;
    void showPicture() { pictureBtn->setIcon(QIcon(avatarPixmap(picture, PictureShown, devicePixelRatioF(), palette()))); }
    void choosePicture()
    {
        PictureDialog dialog(this, name->text().trimmed(), picture, true);
        if (dialog.exec() != QDialog::Accepted) return;
        picture = dialog.picture();
        showPicture();
    }
    void update()
    {
        const bool on = child->isChecked();
        // (The parent who gave the password in the picker isn't asked again.)
        const bool ask = on && !(m_hasPassword && m_parentPresent);
        about->setVisible(on);
        password->setVisible(ask);
        passwordLabel->setVisible(ask);
        repeat->setVisible(ask && !m_hasPassword);
        repeatLabel->setVisible(ask && !m_hasPassword);
        error->setVisible(!error->text().isEmpty());
        adjustSize();
    }
    void fail(const QString &text)
    {
        error->setText(text);
        update();
    }
    void create()
    {
        const QString n = name->text().trimmed();
        if (n.isEmpty()) { fail(QStringLiteral("The profile needs a name.")); return; }
        const bool on = child->isChecked();
        if (on && !m_hasPassword) {
            if (password->text().size() < m_min) { fail(QStringLiteral("The password needs at least %1 characters.").arg(m_min)); return; }
            if (password->text() != repeat->text()) { fail(QStringLiteral("The two passwords differ.")); return; }
        }
        // Another kind of profile, once one is a child's, is the parent's to make.
        if (!on && m_guarded) {
            if (!askParentPassword(this, QStringLiteral("New Profile"), QStringLiteral("Profiles are managed with the parent password, as one of them is a child's."))) return;
            m_guarded = false;
            m_parentPresent = true;
        }
        const QString problem = take(kite_profile_create(n.toUtf8().constData(), on ? 1 : 0, password->text().toUtf8().constData()));
        if (!problem.isEmpty()) { fail(problem); return; }
        created = n;
        // (The profile is there now; a picture that can't be kept isn't worth undoing that.)
        if (!picture.isNull()) keepPicture(n, picture);
        accept();
    }
    const bool m_hasPassword;
    bool m_parentPresent;
    bool m_guarded;
    const int m_min;
    QLineEdit *name;
    QToolButton *pictureBtn;
    QImage picture;
    QCheckBox *child;
    QLabel *about;
    QLabel *passwordLabel;
    QLineEdit *password;
    QLabel *repeatLabel;
    QLineEdit *repeat;
    QLabel *error;
    QDialogButtonBox *buttons;
};

// The profiles as cards: the picture in a circle, the name beside it, and under the
// name what there is to say about it (a child's, the default one, open in Kite).
enum CardRole { NameRole = Qt::UserRole, AboutRole, PictureRole, PictureFileRole, ChildRole, DefaultRole };
const int CardPicture = 48;
const int CardPadding = 12;
// (A card and the gap around it; as wide as fits the list, see CardList.)
const int CardGap = 8;
const int CardCellHeight = CardPicture + 2 * CardPadding + CardGap;
const int CardCellWidth = 240;

class CardDelegate : public QStyledItemDelegate {
public:
    using QStyledItemDelegate::QStyledItemDelegate;
    QSize sizeHint(const QStyleOptionViewItem &option, const QModelIndex &) const override
    {
        const auto *view = qobject_cast<const QListView *>(option.widget);
        return view && view->gridSize().isValid() ? view->gridSize() : QSize(CardCellWidth, CardCellHeight);
    }
    void paint(QPainter *p, const QStyleOptionViewItem &option, const QModelIndex &index) const override
    {
        p->save();
        p->setRenderHint(QPainter::Antialiasing);
        const QPalette &pal = option.palette;
        const QColor base = pal.color(QPalette::Base), text = pal.color(QPalette::Text), accent = pal.color(QPalette::Highlight);
        const bool selected = option.state & QStyle::State_Selected;
        const bool hovered = option.state & QStyle::State_MouseOver;
        // Breeze's own: a frame a fifth of the way to the text's colour, the accent
        // for the one chosen (fainter under the pointer), corners of 5 pixels.
        const QRectF card = QRectF(option.rect).adjusted(CardGap / 2.0 + 0.5, CardGap / 2.0 + 0.5, -CardGap / 2.0 - 0.5, -CardGap / 2.0 - 0.5);
        p->setPen(QPen(selected ? accent : hovered ? mix(base, accent, 0.6) : mix(base, text, 0.2), 1));
        p->setBrush(selected ? mix(base, accent, 0.22) : hovered ? mix(base, accent, 0.08) : base);
        p->drawRoundedRect(card, 5, 5);

        const QPointF at(card.left() + CardPadding, card.center().y() - CardPicture / 2.0);
        p->drawPixmap(at, index.data(PictureRole).value<QPixmap>());

        QFont titleFont = option.font;
        titleFont.setBold(true);
        const QFontMetricsF titleMetrics(titleFont), aboutMetrics(option.font);
        const QString about = index.data(AboutRole).toString();
        const qreal left = at.x() + CardPicture + CardPadding;
        const qreal width = card.right() - CardPadding - left;
        const qreal gap = 2;
        const qreal height = titleMetrics.height() + (about.isEmpty() ? 0 : gap + aboutMetrics.height());
        qreal y = card.center().y() - height / 2;
        p->setFont(titleFont);
        p->setPen(text);
        p->drawText(QRectF(left, y, width, titleMetrics.height()), Qt::AlignLeft | Qt::AlignVCenter, titleMetrics.elidedText(index.data(NameRole).toString(), Qt::ElideRight, width));
        if (!about.isEmpty()) {
            y += titleMetrics.height() + gap;
            p->setFont(option.font);
            p->setPen(mix(text, base, 0.35));
            p->drawText(QRectF(left, y, width, aboutMetrics.height()), Qt::AlignLeft | Qt::AlignVCenter, aboutMetrics.elidedText(about, Qt::ElideRight, width));
        }
        p->restore();
    }
};

// Cards in rows, as many beside each other as fit, each as wide as its share of the row.
class CardList : public QListWidget {
public:
    CardList()
    {
        setViewMode(QListView::IconMode);
        setMovement(QListView::Static);
        setResizeMode(QListView::Adjust);
        setFlow(QListView::LeftToRight);
        setWrapping(true);
        setDragEnabled(false);
        setSelectionMode(QAbstractItemView::SingleSelection);
        setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
        setVerticalScrollMode(QAbstractItemView::ScrollPerPixel);
        setContextMenuPolicy(Qt::CustomContextMenu);
        setItemDelegate(new CardDelegate(this));
        // The cards lie on the window, not in a framed list of its own colour.
        setFrameShape(QFrame::NoFrame);
        viewport()->setBackgroundRole(QPalette::Window);
        // (For a card to know the pointer is over it.)
        setMouseTracking(true);
        viewport()->setAttribute(Qt::WA_Hover);
        setMinimumSize(2 * CardCellWidth + 40, 2 * CardCellHeight);
        setAccessibleName(QStringLiteral("Profiles"));
    }

protected:
    void resizeEvent(QResizeEvent *e) override
    {
        fit();
        QListWidget::resizeEvent(e);
    }

public:
    // The cards' width, for the list's and the number of cards. (Qt lays them out in
    // the width that's left beside a scroll bar, shown or not, unless there's never
    // one: so there is none to be had while all the cards fit.)
    void fit()
    {
        const QSize room = maximumViewportSize();
        const int rows = (count() + qMax(1, room.width() / CardCellWidth) - 1) / qMax(1, room.width() / CardCellWidth);
        const bool scrolls = rows * CardCellHeight > room.height();
        setVerticalScrollBarPolicy(scrolls ? Qt::ScrollBarAsNeeded : Qt::ScrollBarAlwaysOff);
        const int width = room.width() - (scrolls ? style()->pixelMetric(QStyle::PM_ScrollBarExtent, nullptr, verticalScrollBar()) : 0) - 1;
        const QSize cell(width / qMax(1, width / CardCellWidth), CardCellHeight);
        if (cell != gridSize()) setGridSize(cell);
    }
};

class Picker : public QDialog {
public:
    Picker()
    {
        setWindowTitle(QStringLiteral("Choose a Profile — Kite"));
        auto *layout = new QVBoxLayout(this);
        auto *intro = new QLabel(QStringLiteral("Each profile has its own bookmarks, history, logins and add-ons. A child's profile blocks adult sites, and leaving it takes the parent password."));
        intro->setWordWrap(true);
        layout->addWidget(intro);
        list = new CardList;
        // (The cards have half the gap between them around them: their edges line up
        // with the text above and the buttons below.)
        auto *cards = new QHBoxLayout;
        cards->setContentsMargins(-CardGap / 2, 0, -CardGap / 2, 0);
        cards->addWidget(list);
        layout->addLayout(cards, 1);
        auto *row = new QHBoxLayout;
        auto *add = new QPushButton(QIcon::fromTheme(QStringLiteral("list-add")), QStringLiteral("New Profile…"));
        pictureBtn = new QPushButton(QIcon::fromTheme(QStringLiteral("edit-image-face-add"), QIcon::fromTheme(QStringLiteral("insert-image"))), QStringLiteral("Picture…"));
        renameBtn = new QPushButton(QIcon::fromTheme(QStringLiteral("edit-rename")), QStringLiteral("Rename…"));
        remove = new QPushButton(QIcon::fromTheme(QStringLiteral("edit-delete")), QStringLiteral("Delete…"));
        row->addWidget(add);
        row->addWidget(pictureBtn);
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
        connect(pictureBtn, &QPushButton::clicked, this, [this] { changePicture(); });
        connect(renameBtn, &QPushButton::clicked, this, [this] { renameCurrent(); });
        connect(remove, &QPushButton::clicked, this, [this] { removeCurrent(); });
        connect(list, &QListWidget::itemDoubleClicked, this, [this](QListWidgetItem *i) { if (i->flags() & Qt::ItemIsEnabled) accept(); });
        connect(list, &QListWidget::currentItemChanged, this, [this] { updateButtons(); });
        connect(list, &QListWidget::customContextMenuRequested, this, [this](const QPoint &at) { cardMenu(at); });
        reload(QString());
        // Two cards beside each other, three rows of them: more scroll, or the window is made larger.
        resize(2 * CardCellWidth + 80, sizeHint().height() + CardCellHeight);
    }
    QString chosen() const { return list->currentItem() ? list->currentItem()->data(NameRole).toString() : QString(); }

private:
    void reload(const QString &select)
    {
        list->clear();
        const QJsonObject all = QJsonDocument::fromJson(take(kite_profiles_json()).toUtf8()).object();
        const QString wanted = select.isEmpty() ? all[QStringLiteral("last")].toString() : select;
        hasPassword = all[QStringLiteral("hasPassword")].toBool();
        guarded = all[QStringLiteral("guarded")].toBool();
        parentPresent = all[QStringLiteral("parentPresent")].toBool();
        minPassword = all[QStringLiteral("minPassword")].toInt(4);
        QListWidgetItem *current = nullptr;
        for (const QJsonValue v : all[QStringLiteral("profiles")].toArray()) {
            const QJsonObject p = v.toObject();
            const QString name = p[QStringLiteral("name")].toString();
            const bool inUse = p[QStringLiteral("inUse")].toBool();
            const bool child = p[QStringLiteral("child")].toBool();
            QStringList about;
            if (child) about << QStringLiteral("Child's profile");
            const bool isDefault = p[QStringLiteral("isDefault")].toBool();
            if (isDefault) about << QStringLiteral("Default profile");
            if (inUse) about << QStringLiteral("Open");
            const QString file = p[QStringLiteral("avatar")].toString();
            auto *item = new QListWidgetItem(list);
            item->setData(NameRole, name);
            item->setData(AboutRole, about.join(QStringLiteral(" · ")));
            item->setData(PictureRole, avatarPixmap(keptPicture(file), CardPicture, devicePixelRatioF(), palette()));
            item->setData(PictureFileRole, file);
            item->setData(ChildRole, child);
            item->setData(DefaultRole, isDefault);
            // (What a screen reader says of the card.)
            item->setData(Qt::AccessibleTextRole, about.isEmpty() ? name : name + QStringLiteral(", ") + about.join(QStringLiteral(", ")));
            // (One that's open can be picked too: the Kite that has it opens a window.)
            if (name == wanted || !current) current = item;
        }
        list->setCurrentItem(current);
        list->fit();
        updateButtons();
    }
    void updateButtons()
    {
        const bool any = list->currentItem();
        // (The default profile is always there: it takes another name, but can't be deleted.)
        buttons->button(QDialogButtonBox::Open)->setEnabled(any);
        pictureBtn->setEnabled(any);
        renameBtn->setEnabled(any);
        remove->setEnabled(any && !list->currentItem()->data(DefaultRole).toBool());
    }
    // Once a profile is a child's, making, renaming and deleting profiles is the
    // parent's: the password is asked once in this picker.
    bool parentAllows(const QString &title)
    {
        if (!guarded) return true;
        if (!askParentPassword(this, title, QStringLiteral("Profiles are managed with the parent password, as one of them is a child's."))) return false;
        guarded = false;
        parentPresent = true;
        return true;
    }
    void create()
    {
        // (A child's profile asks for the password itself; another kind needs it first.)
        NewProfile dialog(this, hasPassword, parentPresent, guarded, minPassword);
        if (dialog.exec() != QDialog::Accepted) { reload(chosen()); return; }
        reload(dialog.created);
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
    // A child's profile's picture is anyone's to change (it's theirs to make their
    // own); another's is the parent's, like its name.
    void changePicture()
    {
        QListWidgetItem *item = list->currentItem();
        if (!item) return;
        const QString name = chosen();
        if (!item->data(ChildRole).toBool() && !parentAllows(QStringLiteral("Profile Picture"))) return;
        PictureDialog dialog(this, name, keptPicture(item->data(PictureFileRole).toString()), true);
        if (dialog.exec() != QDialog::Accepted) return;
        const QString error = keepPicture(name, dialog.picture());
        if (!error.isEmpty()) QMessageBox::warning(this, QStringLiteral("Profile Picture"), error);
        reload(name);
    }
    void cardMenu(const QPoint &at)
    {
        QListWidgetItem *item = list->itemAt(at);
        if (!item) return;
        list->setCurrentItem(item);
        QMenu menu(this);
        menu.addAction(QIcon::fromTheme(QStringLiteral("document-open")), QStringLiteral("Open"), this, [this] { accept(); });
        menu.addSeparator();
        menu.addAction(pictureBtn->icon(), pictureBtn->text(), this, [this] { changePicture(); });
        menu.addAction(renameBtn->icon(), renameBtn->text(), this, [this] { renameCurrent(); })->setEnabled(renameBtn->isEnabled());
        menu.addAction(remove->icon(), remove->text(), this, [this] { removeCurrent(); })->setEnabled(remove->isEnabled());
        menu.exec(list->viewport()->mapToGlobal(at));
    }
    void renameCurrent()
    {
        const QString name = chosen();
        if (name.isEmpty()) return;
        if (!parentAllows(QStringLiteral("Rename Profile"))) return;
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
        if (name.isEmpty() || list->currentItem()->data(DefaultRole).toBool()) return;
        if (!parentAllows(QStringLiteral("Delete Profile"))) return;
        if (!confirmDelete(name)) return;
        const QString error = take(kite_profile_delete(name.toUtf8().constData()));
        if (!error.isEmpty()) QMessageBox::warning(this, QStringLiteral("Delete Profile"), error);
        reload(QString());
    }
    bool hasPassword = false;
    bool guarded = false;
    bool parentPresent = false;
    int minPassword = 4;
    CardList *list;
    QPushButton *pictureBtn;
    QPushButton *renameBtn;
    QPushButton *remove;
    QDialogButtonBox *buttons;
};
}

static void setUpApp()
{
    QApplication::setApplicationName(QStringLiteral("kite"));
    QApplication::setApplicationDisplayName(QStringLiteral("Kite"));
    QApplication::setDesktopFileName(QStringLiteral("io.github.n4n0gh.kite"));
    QApplication::setWindowIcon(QIcon::fromTheme(QStringLiteral("io.github.n4n0gh.kite"), QIcon(QStringLiteral(":/qt/qml/org/kde/kite/icons/io.github.n4n0gh.kite.svg"))));
}

// `kite --parent-password <profile>` (see main.rs): 0 if the parent password was given.
// (`changed`: the parental controls were changed outside Kite; no profile opens without it.)
extern "C" int kite_ask_parent_password(int argc, char **argv, const char *profile, bool changed)
{
    static int s_argc = argc;
    QApplication app(s_argc, argv);
    setUpApp();
    const QString name = QString::fromUtf8(profile);
    const QString text = changed
        ? QStringLiteral("The parental controls were changed outside Kite. It takes the parent password to open Kite again; have a look at Settings, under Parental Controls, afterwards.")
        : name.isEmpty()
        ? QStringLiteral("A private window isn't held to a child's profile, so opening one takes the parent password.")
        : QStringLiteral("The profile “%1” isn't a child's, so opening it takes the parent password.").arg(name);
    return askParentPassword(nullptr, QStringLiteral("Parent Password — Kite"), text) ? 0 : 1;
}

// A profile's picture as the picker shows it, for Kite's own windows (bootstrap.cpp):
// in a circle of `size` pixels, the figure if it has none.
QPixmap kiteProfilePicture(const QString &profile, int size, const QPalette &palette)
{
    return avatarPixmap(keptPicture(take(kite_profile_avatar_path(profile.toUtf8().constData()))), size, 1, palette);
}

extern "C" int kite_pick_profile(int argc, char **argv)
{
    static int s_argc = argc;
    QApplication app(s_argc, argv);
    setUpApp();
    Picker picker;
    if (picker.exec() != QDialog::Accepted) return 0;
    kite_profile_chosen(picker.chosen().toUtf8().constData());
    return 0;
}
