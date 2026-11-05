// Printing: Qt's print dialog, then the page as a PDF made by Chromium for the
// chosen paper (Main.qml), which goes to CUPS with lp. QtWebEngine's QML view
// has no way to print to a printer itself; this keeps the text sharp too.
#include <QFile>
#include <QGuiApplication>
#include <QJsonDocument>
#include <QJsonObject>
#include <QPageRanges>
#include <QPageSize>
#include <QPrintDialog>
#include <QPrinter>
#include <QProcess>
#include <QTemporaryDir>
#include <QWindow>
#include <QtPrintSupport/qprintengine.h>

extern "C" void kite_print_chosen(const char *json);
extern "C" void kite_print_done(int ok, const char *message);

// The choice between the dialog and the PDF being ready.
static QPrinter *g_printer = nullptr;
static QTemporaryDir *g_tempDir = nullptr;
static QString g_title;

static void finishJob()
{
    delete g_printer;
    g_printer = nullptr;
    delete g_tempDir; // (with the PDF)
    g_tempDir = nullptr;
}

extern "C" void kite_cpp_print_dialog(const char *title)
{
    finishJob();
    g_title = QString::fromUtf8(title);
    g_printer = new QPrinter(QPrinter::HighResolution);
    g_printer->setDocName(g_title);
    auto *dialog = new QPrintDialog(g_printer);
    dialog->setWindowTitle(QStringLiteral("Print — %1").arg(g_title));
    dialog->setOption(QAbstractPrintDialog::PrintToFile, true);
    dialog->setOption(QAbstractPrintDialog::PrintPageRange, true);
    dialog->setOption(QAbstractPrintDialog::PrintCollateCopies, true);
    dialog->setAttribute(Qt::WA_DeleteOnClose);
    QWindow *from = QGuiApplication::focusWindow();
    QObject::connect(dialog, &QDialog::finished, dialog, [](int result) {
        if (result != QDialog::Accepted) {
            finishJob();
            kite_print_chosen("");
            return;
        }
        g_tempDir = new QTemporaryDir(); // (only the user can read it)
        const QPageLayout layout = g_printer->pageLayout();
        const QPageSize::PageSizeId size = layout.pageSize().id();
        QJsonObject o{{QStringLiteral("path"), g_tempDir->filePath(QStringLiteral("page.pdf"))},
                      {QStringLiteral("pageSize"), int(size == QPageSize::Custom ? QPageSize::A4 : size)},
                      {QStringLiteral("landscape"), layout.orientation() == QPageLayout::Landscape}};
        kite_print_chosen(QJsonDocument(o).toJson(QJsonDocument::Compact).constData());
    });
    dialog->winId();
    if (from && dialog->windowHandle()) dialog->windowHandle()->setTransientParent(from);
    dialog->open();
}

// The options lp gets for the dialog's choices.
static QStringList lpArguments(const QPrinter &printer, const QString &path)
{
    QStringList args{QStringLiteral("-d"), printer.printerName(), QStringLiteral("-t"), g_title};
    if (printer.copyCount() > 1) {
        args << QStringLiteral("-n") << QString::number(printer.copyCount());
        if (printer.collateCopies()) args << QStringLiteral("-o") << QStringLiteral("collate=true");
    }
    QStringList options;
    switch (printer.duplex()) {
    case QPrinter::DuplexNone: options << QStringLiteral("sides=one-sided"); break;
    case QPrinter::DuplexLongSide: options << QStringLiteral("sides=two-sided-long-edge"); break;
    case QPrinter::DuplexShortSide: options << QStringLiteral("sides=two-sided-short-edge"); break;
    default: break;
    }
    if (printer.colorMode() == QPrinter::GrayScale) options << QStringLiteral("print-color-mode=monochrome");
    const QString media = printer.pageLayout().pageSize().key();
    if (!media.isEmpty()) options << QStringLiteral("media=%1").arg(media);
    // The rest of the dialog's printer properties (pairs of name and value,
    // Qt's QCUPSSupport keeps them under this key). The PDF is already turned
    // for landscape.
    const QPrintEngine::PrintEnginePropertyKey cupsOptions = QPrintEngine::PrintEnginePropertyKey(0xfe00);
    const QStringList pairs = printer.printEngine()->property(cupsOptions).toStringList();
    for (qsizetype i = 0; i + 1 < pairs.size(); i += 2) {
        if (pairs[i] == QStringLiteral("landscape")) continue;
        const QString name = pairs[i] + QLatin1Char('=');
        options.erase(std::remove_if(options.begin(), options.end(), [&](const QString &o) { return o.startsWith(name); }), options.end());
        options << name + pairs[i + 1];
    }
    for (const QString &o : options) args << QStringLiteral("-o") << o;
    if (!printer.pageRanges().isEmpty()) args << QStringLiteral("-P") << printer.pageRanges().toString();
    args << path;
    return args;
}

// The PDF is there: to the file or printer chosen.
extern "C" void kite_cpp_print_submit()
{
    if (!g_printer || !g_tempDir) return;
    const QString pdf = g_tempDir->filePath(QStringLiteral("page.pdf"));
    const QString file = g_printer->outputFileName();
    if (!file.isEmpty()) {
        QFile::remove(file);
        const bool ok = QFile::copy(pdf, file);
        const QByteArray message = (ok ? QStringLiteral("Saved %1") : QStringLiteral("Could not write %1")).arg(file).toUtf8();
        finishJob();
        kite_print_done(ok, message.constData());
        return;
    }
    auto *lp = new QProcess;
    const QString printerName = g_printer->printerName();
    QObject::connect(lp, &QProcess::errorOccurred, lp, [lp](QProcess::ProcessError e) {
        if (e != QProcess::FailedToStart) return;
        finishJob();
        kite_print_done(0, "Printing needs CUPS' lp command, which wasn't found");
        lp->deleteLater();
    });
    QObject::connect(lp, &QProcess::finished, lp, [lp, printerName](int code, QProcess::ExitStatus) {
        const QByteArray message = code == 0 ? QStringLiteral("Sent to %1").arg(printerName).toUtf8()
                                             : QStringLiteral("Printing failed: %1").arg(QString::fromLocal8Bit(lp->readAllStandardError()).trimmed()).toUtf8();
        finishJob();
        kite_print_done(code == 0, message.constData());
        lp->deleteLater();
    });
    lp->start(QStringLiteral("lp"), lpArguments(*g_printer, pdf));
}

extern "C" void kite_cpp_print_cancel()
{
    finishJob();
}
