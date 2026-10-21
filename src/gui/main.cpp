// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

#include <QCommandLineParser>
#include <QFont>
#include <QFontDatabase>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QStandardPaths>
#include <QUrl>

#include <rc_qt/src/lifecycle.cxx.h>
#include <rc_qt/src/logging.cxx.h>

#include <cstdlib>

#ifdef ROCKETCHATQML_SPIX
#include <Spix/AnyRpcServer.h>
#include <Spix/QtQmlBot.h>

#include <memory>
#endif

int main(int argc, char *argv[])
{
    rc_qt::init_logging();

    QGuiApplication app(argc, argv);

    // The UI font, bundled so the GUI looks the same everywhere. The point size stays the system's:
    // Theme.fontSizeBase follows it.
    for (const char *file : {"InstrumentSans.ttf", "InstrumentSans-Italic.ttf"}) {
        if (QFontDatabase::addApplicationFont(QStringLiteral(":/qt/qml/rocketchat/fonts/") + QLatin1String(file)) < 0) {
            qWarning("Can't load the bundled font %s", file);
        }
    }
    QFont font = app.font();
    font.setFamily(QStringLiteral("Instrument Sans"));
    app.setFont(font);

    QCommandLineParser parser;
    parser.addHelpOption();
#ifdef ROCKETCHATQML_SPIX
    const QCommandLineOption spixPortOption("spix-port", "Start the Spix UI test server on <port>.", "port");
    parser.addOption(spixPortOption);
#endif
#ifdef ROCKETCHATQML_DEVELOPER_MODE
    // Never in release builds.
    const QCommandLineOption devOption(
        "dev", "Fill in the login page for the local test server (RC_DEV_SERVER, RC_DEV_USER and RC_DEV_PASS override it).");
    parser.addOption(devOption);
#endif
    parser.process(app);

#ifdef ROCKETCHATQML_SPIX
    int spixPort = 0;
    if (parser.isSet(spixPortOption)) {
        bool ok = false;
        spixPort = parser.value(spixPortOption).toInt(&ok);
        if (!ok || spixPort < 1 || spixPort > 65535) {
            QCommandLineParser::showMessageAndExit(QCommandLineParser::MessageType::Error,
                                                   QStringLiteral("Invalid --spix-port: %1\n").arg(parser.value(spixPortOption)),
                                                   EXIT_FAILURE);
        }
    }
#endif

    // After QGuiApplication, so the core's events can reach the GUI thread; before the QML is
    // loaded, which creates the view models that listen to it.
    const QByteArray cacheDir = QStandardPaths::writableLocation(QStandardPaths::CacheLocation).toUtf8();
    rc_qt::start_core(rust::Str(cacheDir.constData(), static_cast<size_t>(cacheDir.size())));
#ifdef ROCKETCHATQML_DEVELOPER_MODE
    if (parser.isSet(devOption)) {
        rc_qt::enable_dev_login();
    }
#endif

    QQmlApplicationEngine engine;
    QObject::connect(
        &engine, &QQmlApplicationEngine::objectCreationFailed, &app, [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    engine.loadFromModule("rocketchat", "Main");

#ifdef ROCKETCHATQML_SPIX
    // Started after loading Main.qml, so a client that gets an answer finds the window. Declared
    // after the bot, so the server (whose thread uses the bot's command queue) is destroyed first.
    std::unique_ptr<spix::QtQmlBot> spixBot;
    std::unique_ptr<spix::AnyRpcServer> spixServer;
    if (spixPort != 0) {
        spixBot = std::make_unique<spix::QtQmlBot>();
        spixServer = std::make_unique<spix::AnyRpcServer>(spixPort);
        spixBot->runTestServer(*spixServer);
    }
#endif

    const int exitCode = QGuiApplication::exec();

    // The core lives in a static, so nothing else drops it: this lets a pending logout finish.
    rc_qt::stop_core();
    return exitCode;
}
