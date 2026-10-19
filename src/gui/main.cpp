// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

#include <QCommandLineParser>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QUrl>

#include <cstdlib>

#ifdef ROCKETCHATQML_SPIX
#include <Spix/AnyRpcServer.h>
#include <Spix/QtQmlBot.h>

#include <memory>
#endif

int main(int argc, char *argv[])
{
    QGuiApplication app(argc, argv);

    QCommandLineParser parser;
    parser.addHelpOption();
#ifdef ROCKETCHATQML_SPIX
    const QCommandLineOption spixPortOption("spix-port", "Start the Spix UI test server on <port>.", "port");
    parser.addOption(spixPortOption);
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

    return QGuiApplication::exec();
}
